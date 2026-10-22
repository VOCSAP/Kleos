use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct GateCheckRequest {
    pub command: String,
    pub agent: String,
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GateCheckResult {
    pub allowed: bool,
    pub reason: Option<String>,
    pub resolved_command: Option<String>,
    pub gate_id: i64,
    pub requires_approval: bool,
    pub enrichment: Option<String>,
    /// Set by the server only when a human approved the request; absent from
    /// older servers, which therefore read as not granted.
    #[serde(default)]
    pub approval_granted: bool,
}

pub enum GateOutcome {
    Allow {
        command: String,
        enrichment: Option<String>,
        gate_id: i64,
    },
    /// Allowed by the gate but flagged by a require_approval pattern. Only a
    /// server that skips its approval wait for this agent returns it before a
    /// human decided; after a real approval the flag is also still set.
    Ask {
        command: String,
        reason: String,
        enrichment: Option<String>,
        gate_id: i64,
    },
    Deny {
        reason: String,
        #[allow(dead_code)]
        gate_id: i64,
    },
}

pub async fn check_remote(
    client: &reqwest::Client,
    server_url: &str,
    api_key: &str,
    req: &GateCheckRequest,
    claude_hook: bool,
) -> Result<GateOutcome, String> {
    let url = format!("{}/gate/check", server_url.trim_end_matches('/'));

    let result = send_request(client, &url, api_key, req).await?;
    Ok(outcome_from_result(result, &req.command, claude_hook))
}

/// Outside the Claude Code hook nobody can answer an "ask", so an approval
/// the server does not report as granted becomes a deny.
fn outcome_from_result(
    result: GateCheckResult,
    request_command: &str,
    claude_hook: bool,
) -> GateOutcome {
    if result.allowed {
        let command = result
            .resolved_command
            .unwrap_or_else(|| request_command.to_string());
        if result.requires_approval && !result.approval_granted && !claude_hook {
            return GateOutcome::Deny {
                reason: "approval required but no human approval was granted; kleos-sh \
                         cannot ask for one, so run this command where a human confirms it \
                         (Claude Code permission prompt or a manual shell)"
                    .to_string(),
                gate_id: result.gate_id,
            };
        }
        if result.requires_approval {
            return GateOutcome::Ask {
                command,
                reason: result.reason.unwrap_or_else(|| {
                    "Kleos gate: this command matches a require-approval pattern".to_string()
                }),
                enrichment: result.enrichment,
                gate_id: result.gate_id,
            };
        }
        GateOutcome::Allow {
            command,
            enrichment: result.enrichment,
            gate_id: result.gate_id,
        }
    } else {
        let reason = result
            .reason
            .unwrap_or_else(|| "denied by gate (no reason given)".to_string());
        GateOutcome::Deny {
            reason,
            gate_id: result.gate_id,
        }
    }
}

async fn send_request(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    req: &GateCheckRequest,
) -> Result<GateCheckResult, String> {
    let resp = client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .timeout(gate_timeout())
        .json(req)
        .send()
        .await
        .map_err(|e| format!("gate check request failed: {}", e))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("gate check returned {}: {}", status, body));
    }

    resp.json::<GateCheckResult>()
        .await
        .map_err(|e| format!("failed to parse gate response: {}", e))
}

/// Upper bound for one /gate/check round trip. A server that holds the
/// request for a human approval answers only when someone decides, so hook
/// callers keep this below their harness timeout and fail open on expiry.
fn gate_timeout() -> std::time::Duration {
    let secs = std::env::var("KLEOS_SH_APPROVAL_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(12);
    std::time::Duration::from_secs(secs)
}

#[cfg(test)]
mod tests {
    use super::{outcome_from_result, GateCheckResult, GateOutcome};

    fn result(allowed: bool, requires_approval: bool, approval_granted: bool) -> GateCheckResult {
        GateCheckResult {
            allowed,
            reason: Some("matches systemctl".to_string()),
            resolved_command: Some("systemctl status x".to_string()),
            gate_id: 7,
            requires_approval,
            enrichment: None,
            approval_granted,
        }
    }

    #[test]
    fn required_approval_maps_to_ask_in_hook_mode() {
        let outcome = outcome_from_result(result(true, true, false), "cmd", true);
        assert!(matches!(outcome, GateOutcome::Ask { .. }));
    }

    #[test]
    fn ungranted_approval_is_denied_outside_hook_mode() {
        let outcome = outcome_from_result(result(true, true, false), "cmd", false);
        match outcome {
            GateOutcome::Deny { reason, gate_id } => {
                assert_eq!(gate_id, 7);
                assert!(reason.contains("no human approval"), "reason: {reason}");
            }
            _ => panic!("ungranted approval must not reach exec or gate-only allow"),
        }
    }

    #[test]
    fn granted_approval_maps_to_ask_outside_hook_mode() {
        let outcome = outcome_from_result(result(true, true, true), "cmd", false);
        assert!(matches!(outcome, GateOutcome::Ask { .. }));
    }

    #[test]
    fn plain_allow_and_deny_are_unchanged() {
        assert!(matches!(
            outcome_from_result(result(true, false, false), "cmd", false),
            GateOutcome::Allow { .. }
        ));
        assert!(matches!(
            outcome_from_result(result(false, false, false), "cmd", false),
            GateOutcome::Deny { .. }
        ));
    }

    #[test]
    fn response_without_granted_field_is_denied_outside_hook_mode() {
        let json = r#"{"allowed":true,"reason":null,"resolved_command":null,"gate_id":1,"requires_approval":true}"#;
        let parsed: GateCheckResult = serde_json::from_str(json).expect("older server body");
        assert!(matches!(
            outcome_from_result(parsed, "cmd", false),
            GateOutcome::Deny { .. }
        ));
    }
}
