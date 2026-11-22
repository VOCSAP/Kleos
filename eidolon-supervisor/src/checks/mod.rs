pub mod drift;
pub mod retry_loop;
pub mod rule_match;
pub mod scope;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub check_type: CheckType,
    pub pattern: String,
    pub severity: Severity,
    pub cooldown_secs: u64,
    pub message: String,
    /// Patch 46 (VOCSAP -- Levier A): restrict a RuleMatch rule to specific
    /// Claude tool names. Empty (the serde default) => the rule applies to
    /// every tool, which keeps existing `supervisor.json` configs working
    /// unchanged. Scoping `no-reboot`/`no-force-push` to `["Bash"]` stops a
    /// `memory_store` or `Write` whose content merely mentions a keyword from
    /// tripping a command-oriented rule.
    #[serde(default)]
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckType {
    RuleMatch,
    RetryLoop,
    ScopeViolation,
    Drift,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug)]
pub struct Violation {
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    pub context: String,
    /// Claude session id extracted from the JSONL line, used to route the
    /// supervisor's deny back to the same agent on the next PreToolUse via
    /// /supervisor/inject. None when the line had no sessionId field.
    pub session_id: Option<String>,
}

pub fn default_rules() -> Vec<Rule> {
    vec![
        Rule {
            id: "no-force-push".into(),
            check_type: CheckType::RuleMatch,
            pattern: r"git\s+push\s+.*--force".into(),
            severity: Severity::Critical,
            cooldown_secs: 300,
            message: "Force push detected".into(),
            // Patch 46: command rule -- only real Bash invocations.
            tools: vec!["Bash".into()],
        },
        Rule {
            id: "no-reboot".into(),
            check_type: CheckType::RuleMatch,
            pattern: r"reboot|shutdown|systemctl\s+(reboot|poweroff)".into(),
            severity: Severity::Critical,
            cooldown_secs: 600,
            message: "Reboot or shutdown command detected".into(),
            // Patch 46: command rule -- only real Bash invocations.
            tools: vec!["Bash".into()],
        },
        Rule {
            id: "retry-loop".into(),
            check_type: CheckType::RetryLoop,
            pattern: String::new(),
            severity: Severity::Warning,
            cooldown_secs: 120,
            message: "Agent stuck in retry loop (3+ identical failing commands)".into(),
            // RetryLoop runs through retry_tracker (not rule_match); tools is
            // unused here. Left empty for forward compatibility.
            tools: Vec::new(),
        },
        Rule {
            id: "em-dash-usage".into(),
            check_type: CheckType::RuleMatch,
            pattern: "\u{2014}".into(),
            severity: Severity::Info,
            cooldown_secs: 60,
            message: "Em dash used in output -- should use -- instead".into(),
            // Patch 46: em dashes matter in written content, not Bash commands.
            tools: vec!["Write".into(), "Edit".into(), "MultiEdit".into()],
        },
    ]
}
