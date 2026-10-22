//! Operator opt-out of the in-memory approval wait for hook-driven agents.
//!
//! A PreToolUse hook cannot usefully hold a request: the harness kills it at
//! its own timeout and lets the tool run. Agents listed in
//! `KLEOS_GATE_NO_WAIT_AGENTS` get their allowed result back immediately;
//! `requires_approval` stays set so the client can escalate to its own prompt.
//! The list is server-side on purpose: a client must not exempt itself.
//! The agent name is declared by the client, so a skipped approval is flagged
//! with `approval_waived: true` for clients that cannot prompt a human.

use kleos_lib::gate::GateCheckResult;
use serde_json::{json, Value};

const ENV_VAR: &str = "KLEOS_GATE_NO_WAIT_AGENTS";

/// Body of the early /gate/check return for a listed agent, or `None` when
/// the handler must go on to the approval wait (or to its final deny).
pub(super) fn early_response(result: &GateCheckResult, agent: &str) -> Option<Value> {
    let raw = std::env::var(ENV_VAR).unwrap_or_default();
    early_response_for(result, list_contains(&raw, agent))
}

fn early_response_for(result: &GateCheckResult, agent_listed: bool) -> Option<Value> {
    if !result.allowed || !agent_listed {
        return None;
    }
    let mut body = json!(result);
    if result.requires_approval {
        body["approval_waived"] = Value::Bool(true);
    }
    Some(body)
}

fn list_contains(raw: &str, agent: &str) -> bool {
    let agent = agent.trim();
    !agent.is_empty()
        && raw
            .split(',')
            .map(str::trim)
            .any(|entry| entry.eq_ignore_ascii_case(agent))
}

#[cfg(test)]
mod tests {
    use super::{early_response_for, list_contains};
    use kleos_lib::gate::GateCheckResult;

    fn result(allowed: bool, requires_approval: bool) -> GateCheckResult {
        GateCheckResult {
            allowed,
            reason: None,
            resolved_command: Some("systemctl status x".to_string()),
            gate_id: 7,
            requires_approval,
            enrichment: None,
        }
    }

    #[test]
    fn listed_agent_with_blocked_command_gets_no_early_allow() {
        assert!(early_response_for(&result(false, false), true).is_none());
    }

    #[test]
    fn listed_agent_with_required_approval_returns_waived_at_once() {
        let body = early_response_for(&result(true, true), true).expect("early return");
        assert_eq!(body["allowed"], true);
        assert_eq!(body["requires_approval"], true);
        assert_eq!(body["approval_waived"], true);
    }

    #[test]
    fn listed_agent_without_required_approval_is_not_marked_waived() {
        let body = early_response_for(&result(true, false), true).expect("early return");
        assert!(body.get("approval_waived").is_none());
    }

    #[test]
    fn unlisted_agent_goes_on_to_the_approval_wait() {
        assert!(early_response_for(&result(true, true), false).is_none());
    }

    #[test]
    fn empty_list_keeps_upstream_wait() {
        assert!(!list_contains("", "claude-code"));
        assert!(!list_contains(" , ", "claude-code"));
    }

    #[test]
    fn listed_agent_matches_trimmed_and_case_insensitive() {
        assert!(list_contains("codex, Claude-Code ", "claude-code"));
        assert!(list_contains("claude-code", " claude-code "));
    }

    #[test]
    fn unlisted_or_empty_agent_does_not_match() {
        assert!(!list_contains("codex", "claude-code"));
        assert!(!list_contains("claude-code", ""));
        assert!(!list_contains("claude-code-extra", "claude-code"));
    }
}
