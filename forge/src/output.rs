//! Output formatting for forge exec results.

use serde_json::Value;

use crate::dispatch::OutputHints;

/// Output format selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Detect from TTY: human if interactive, json if piped.
    Auto,
    /// Pretty-printed JSON.
    Json,
    /// Human-readable formatted output.
    Human,
    /// Compact single-line JSON for agent consumption.
    Agent,
}

// Parsing and resolution helpers for the output format enum.
impl Format {
    /// Parse a format string from the CLI.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(Self::Auto),
            "json" => Some(Self::Json),
            "human" => Some(Self::Human),
            "agent" => Some(Self::Agent),
            _ => None,
        }
    }

    /// Resolve Auto to a concrete format based on TTY detection.
    pub fn resolve(self) -> Self {
        if self == Self::Auto {
            if atty_stdout() {
                Self::Human
            } else {
                Self::Json
            }
        } else {
            self
        }
    }
}

/// Format a response value according to the chosen format and output hints.
pub fn format_output(
    value: &Value,
    format: Format,
    hints: &OutputHints,
    skill_name: &str,
) -> String {
    let resolved = format.resolve();
    match resolved {
        Format::Json | Format::Auto => {
            serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
        }
        Format::Agent => value.to_string(),
        Format::Human => format_human(value, hints, skill_name),
    }
}

/// Render a human-readable summary using output hints.
fn format_human(value: &Value, hints: &OutputHints, skill_name: &str) -> String {
    let mut out = String::new();

    // Header with count
    let count = hints
        .count_path
        .as_deref()
        .and_then(|p| json_pointer(value, p))
        .and_then(|v| v.as_u64());

    let query = value.get("query").and_then(|v| v.as_str());

    match (count, query) {
        (Some(n), Some(q)) => out.push_str(&format!("{skill_name}: {n} results for \"{q}\"\n\n")),
        (Some(n), None) => out.push_str(&format!("{skill_name}: {n} results\n\n")),
        (None, Some(q)) => out.push_str(&format!("{skill_name}: results for \"{q}\"\n\n")),
        (None, None) => out.push_str(&format!("{skill_name}:\n\n")),
    }

    // Results
    let default_fields = ["title".to_string(), "url".to_string()];
    let fields = hints.summary_fields.as_deref().unwrap_or(&default_fields);

    if let Some(results) = hints
        .results_path
        .as_deref()
        .and_then(|p| json_pointer(value, p))
        .and_then(|v| v.as_array())
    {
        for (i, result) in results.iter().enumerate() {
            out.push_str(&format!("  {}. ", i + 1));
            for (j, field) in fields.iter().enumerate() {
                if let Some(val) = result.get(field).and_then(|v| v.as_str()) {
                    if j == 0 {
                        out.push_str(val);
                        out.push('\n');
                    } else {
                        out.push_str(&format!("     {val}\n"));
                    }
                }
            }
            out.push('\n');
        }
    }

    // Suggestions
    if let Some(suggestions) = hints
        .suggestions_path
        .as_deref()
        .and_then(|p| json_pointer(value, p))
        .and_then(|v| v.as_array())
    {
        if !suggestions.is_empty() {
            let items: Vec<&str> = suggestions.iter().filter_map(|v| v.as_str()).collect();
            if !items.is_empty() {
                out.push_str(&format!("Suggestions: {}\n", items.join(", ")));
            }
        }
    }

    out
}

/// Resolve a JSON pointer like "/results" against a value.
fn json_pointer<'a>(value: &'a Value, pointer: &str) -> Option<&'a Value> {
    value.pointer(pointer)
}

/// Check if stdout is a TTY.
fn atty_stdout() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}
