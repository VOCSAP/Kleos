use super::{CheckType, Rule, Violation};
use regex::Regex;

pub fn check(entry: &serde_json::Value, rules: &[Rule]) -> Vec<Violation> {
    let mut violations = Vec::new();

    let text = extract_check_text(entry);
    if text.is_empty() {
        return violations;
    }

    // Patch 46 (Levier A): the Claude tool that produced this entry. Used to
    // scope rules so a command-oriented rule does not fire on the content of a
    // memory_store / Write that merely mentions a keyword.
    let tool_name = entry.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");

    for rule in rules {
        if !matches!(rule.check_type, CheckType::RuleMatch) {
            continue;
        }

        // Patch 46: tool-name scope. Empty list => the rule applies to every
        // tool (backward compatible with configs that omit `tools`).
        if !rule.tools.is_empty() && !rule.tools.iter().any(|t| t == tool_name) {
            continue;
        }

        if let Ok(re) = Regex::new(&rule.pattern) {
            if re.is_match(&text) {
                violations.push(Violation {
                    rule_id: rule.id.clone(),
                    severity: rule.severity.clone(),
                    message: rule.message.clone(),
                    context: truncate(&text, 200),
                    session_id: None,
                });
            }
        }
    }

    violations
}

fn extract_check_text(entry: &serde_json::Value) -> String {
    let obj = match entry.as_object() {
        Some(o) => o,
        None => return String::new(),
    };

    let mut parts = Vec::new();

    // Check tool_input.command (Bash commands)
    if let Some(input) = obj.get("tool_input").or(obj.get("input")) {
        if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
            parts.push(cmd.to_string());
        }
        if let Some(content) = input.get("content").and_then(|v| v.as_str()) {
            parts.push(content.to_string());
        }
    }

    // Check assistant text output
    if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
        parts.push(text.to_string());
    }
    if let Some(content) = obj.get("content").and_then(|v| v.as_str()) {
        parts.push(content.to_string());
    }

    // Check commit messages in git operations
    if let Some(msg) = obj.get("message").and_then(|v| v.as_str()) {
        parts.push(msg.to_string());
    }

    parts.join("\n")
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::{default_rules, Severity};
    use serde_json::json;

    // Patch 46 (Levier A): a non-Bash tool whose content merely mentions a
    // keyword must NOT trip a command-scoped rule.
    #[test]
    fn tool_scope_skips_non_bash_false_positive() {
        let rules = default_rules();
        let entry = json!({
            "tool_name": "mcp__kleos__memory_store",
            "tool_input": { "content": "documenting that systemctl poweroff is blocked" }
        });
        let v = check(&entry, &rules);
        assert!(
            v.iter().all(|x| x.rule_id != "no-reboot"),
            "no-reboot must not fire on a non-Bash tool, got {v:?}"
        );
    }

    // A real Bash command still fires the command-scoped rule.
    #[test]
    fn tool_scope_allows_real_bash() {
        let rules = default_rules();
        let entry = json!({
            "tool_name": "Bash",
            "tool_input": { "command": "systemctl poweroff" }
        });
        let v = check(&entry, &rules);
        assert!(
            v.iter().any(|x| x.rule_id == "no-reboot"),
            "no-reboot must fire on a real Bash command, got {v:?}"
        );
    }

    // Empty `tools` (default / legacy config) matches every tool.
    #[test]
    fn empty_tools_matches_any_tool() {
        let rules = vec![Rule {
            id: "any".into(),
            check_type: CheckType::RuleMatch,
            pattern: "danger".into(),
            severity: Severity::Warning,
            cooldown_secs: 0,
            message: "m".into(),
            tools: Vec::new(),
        }];
        let entry = json!({
            "tool_name": "SomeRandomTool",
            "tool_input": { "command": "enter the danger zone" }
        });
        assert_eq!(check(&entry, &rules).len(), 1);
    }
}
