use super::{CheckType, Rule, Violation};
use regex::Regex;

/// A rule whose pattern has been compiled exactly once at startup.
/// Rules with invalid patterns are stored as `None` (warned at compile time).
pub struct CompiledRule<'a> {
    /// Reference to the original rule definition.
    pub rule: &'a Rule,
    /// Pre-compiled regex; `None` when the pattern was invalid.
    pub regex: Option<Regex>,
}

/// Compile all RuleMatch-typed rules into `CompiledRule`s.
/// Invalid patterns emit a `tracing::warn` and produce a `None` regex so the
/// caller can detect and report silently-disabled rules.
pub fn compile_rules(rules: &[Rule]) -> Vec<CompiledRule<'_>> {
    rules
        .iter()
        .filter(|r| matches!(r.check_type, CheckType::RuleMatch))
        .map(|rule| {
            let regex = match Regex::new(&rule.pattern) {
                Ok(re) => Some(re),
                Err(err) => {
                    // Warn loudly -- a mis-typed pattern silently disables a
                    // security control; operators must know about it.
                    tracing::warn!(
                        rule_id = %rule.id,
                        pattern = %rule.pattern,
                        error = %err,
                        "rule pattern failed to compile -- rule will never match"
                    );
                    None
                }
            };
            CompiledRule { rule, regex }
        })
        .collect()
}

/// Check `entry` against the pre-compiled rule set.
/// Accepts `&[CompiledRule]` so regexes are compiled once (at startup) and
/// reused for every log entry rather than being rebuilt per call.
pub fn check(entry: &serde_json::Value, compiled: &[CompiledRule<'_>]) -> Vec<Violation> {
    let mut violations = Vec::new();

    let text = extract_check_text(entry);
    if text.is_empty() {
        return violations;
    }

    // Patch 46 (Levier A): the Claude tool that produced this entry. Used to
    // scope rules so a command-oriented rule does not fire on the content of a
    // memory_store / Write that merely mentions a keyword.
    let tool_name = entry.get("tool_name").and_then(|v| v.as_str()).unwrap_or("");

    for cr in compiled {
        // Skip rules with invalid patterns -- they were already warned about at
        // compile time. (check_type == RuleMatch is filtered in compile_rules.)
        let re = match &cr.regex {
            Some(r) => r,
            None => continue,
        };

        // Patch 46: tool-name scope. Empty list => the rule applies to every
        // tool (backward compatible with configs that omit `tools`).
        if !cr.rule.tools.is_empty() && !cr.rule.tools.iter().any(|t| t == tool_name) {
            continue;
        }

        if re.is_match(&text) {
            violations.push(Violation {
                rule_id: cr.rule.id.clone(),
                severity: cr.rule.severity.clone(),
                message: cr.rule.message.clone(),
                context: truncate(&text, 200),
                session_id: None,
            });
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
    if s.chars().count() <= max {
        s.to_string()
    } else {
        // Take whole chars, not a byte slice: untrusted JSONL may contain
        // multibyte text and `&s[..max]` panics on a non-char-boundary index.
        let truncated: String = s.chars().take(max).collect();
        format!("{}...", truncated)
    }
}

#[cfg(test)]
mod truncate_tests {
    use super::truncate;

    // Multibyte input must never panic on a non-char-boundary byte index and
    // must truncate by whole chars.
    #[test]
    fn truncate_multibyte_does_not_panic() {
        let s = "\u{65e5}\u{672c}\u{8a9e}\u{30c6}\u{30b9}\u{30c8}"; // 6 CJK/kana chars
        let out = truncate(s, 2);
        assert_eq!(out, "\u{65e5}\u{672c}...");
    }

    #[test]
    fn truncate_short_ascii_unchanged() {
        assert_eq!(truncate("hello", 16), "hello");
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
        let v = check(&entry, &compile_rules(&rules));
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
        let v = check(&entry, &compile_rules(&rules));
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
        assert_eq!(check(&entry, &compile_rules(&rules)).len(), 1);
    }
}
