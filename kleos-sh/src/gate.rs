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
) -> Result<GateOutcome, String> {
    let url = format!("{}/gate/check", server_url.trim_end_matches('/'));

    let result = send_request(client, &url, api_key, req).await?;

    if result.allowed {
        let command = result
            .resolved_command
            .unwrap_or_else(|| req.command.clone());
        if result.requires_approval {
            return Ok(GateOutcome::Ask {
                command,
                reason: result.reason.unwrap_or_else(|| {
                    "Kleos gate: this command matches a require-approval pattern".to_string()
                }),
                enrichment: result.enrichment,
                gate_id: result.gate_id,
            });
        }
        Ok(GateOutcome::Allow {
            command,
            enrichment: result.enrichment,
            gate_id: result.gate_id,
        })
    } else {
        let reason = result
            .reason
            .unwrap_or_else(|| "denied by gate (no reason given)".to_string());
        Ok(GateOutcome::Deny {
            reason,
            gate_id: result.gate_id,
        })
    }
}

// Windows used to spawn curl.exe here: reqwest timed out on this host while
// return packets to low source ports were dropped by the router. With that
// rule fixed, one in-process client serves every platform and saves a process
// spawn per tool call.
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
