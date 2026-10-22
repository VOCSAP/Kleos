//! Operator opt-out of the in-memory approval wait for hook-driven agents.
//!
//! A PreToolUse hook cannot usefully hold a request: the harness kills it at
//! its own timeout and lets the tool run. Agents listed in
//! `KLEOS_GATE_NO_WAIT_AGENTS` get their allowed result back immediately;
//! `requires_approval` stays set so the client can escalate to its own prompt.
//! The list is server-side on purpose: a client must not exempt itself.
//! The agent name is declared by the client and several other returns also
//! skip the wait, so only a human approval is marked, with
//! `approval_granted: true`; a client that cannot prompt refuses the rest.

use kleos_lib::gate::GateCheckResult;
use serde_json::{json, Value};

const ENV_VAR: &str = "KLEOS_GATE_NO_WAIT_AGENTS";

pub(super) fn agent_skips_approval_wait(agent: &str) -> bool {
    let raw = std::env::var(ENV_VAR).unwrap_or_default();
    list_contains(&raw, agent)
}

/// Body of the /gate/check return once a human approved the pending request.
pub(super) fn approved_response(result: &GateCheckResult) -> Value {
    let mut body = json!(result);
    body["approval_granted"] = Value::Bool(true);
    body
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
    use super::{approved_response, list_contains};
    use kleos_lib::gate::GateCheckResult;
    use serde_json::json;

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
    fn human_approval_is_marked_granted() {
        let body = approved_response(&result(true, true));
        assert_eq!(body["allowed"], true);
        assert_eq!(body["requires_approval"], true);
        assert_eq!(body["approval_granted"], true);
    }

    #[test]
    fn result_returned_without_a_wait_carries_no_grant() {
        let body = json!(result(true, true));
        assert!(body.get("approval_granted").is_none());
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
