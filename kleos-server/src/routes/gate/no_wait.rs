//! Operator opt-out of the in-memory approval wait for hook-driven agents.
//!
//! A PreToolUse hook cannot usefully hold a request: the harness kills it at
//! its own timeout and lets the tool run. Agents listed in
//! `KLEOS_GATE_NO_WAIT_AGENTS` get their allowed result back immediately;
//! `requires_approval` stays set so the client can escalate to its own prompt.
//! The list is server-side on purpose: a client must not exempt itself.

const ENV_VAR: &str = "KLEOS_GATE_NO_WAIT_AGENTS";

pub(super) fn agent_skips_approval_wait(agent: &str) -> bool {
    let raw = std::env::var(ENV_VAR).unwrap_or_default();
    list_contains(&raw, agent)
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
    use super::list_contains;

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
