//! Dispatch config types and fetch logic.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A single skill dispatch configuration as returned by the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchConfig {
    /// Human-readable key used in `forge exec <name>`.
    pub skill_name: String,
    /// Short description shown in `forge exec --list`.
    pub description: String,
    /// Whether this dispatch is currently available.
    pub enabled: bool,
    /// "internal" (Kleos server route) or "external" (future).
    pub target_type: String,
    /// Relative path on the Kleos server (e.g. "/search/web").
    pub endpoint: String,
    /// HTTP method.
    pub method: String,
    /// Parameter definitions keyed by param name.
    pub params_schema: HashMap<String, ParamSpec>,
    /// Hints for CLI output formatting.
    pub output_hints: OutputHints,
}

/// Specification for a single parameter accepted by a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamSpec {
    /// Data type: "string", "integer", or "boolean".
    #[serde(rename = "type")]
    pub param_type: String,
    /// Whether the parameter must be provided.
    #[serde(default)]
    pub required: bool,
    /// Default value applied when the parameter is omitted.
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    /// Help text shown in dynamic `--help` output.
    #[serde(default)]
    pub description: Option<String>,
    /// Allowed values (validated at the CLI layer before sending).
    #[serde(default, rename = "enum")]
    pub allowed_values: Option<Vec<String>>,
    /// Override for the CLI flag name (e.g. map API param "q" to "--query").
    #[serde(default)]
    pub cli_name: Option<String>,
}

/// Hints telling the output formatter where to find data in the response.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OutputHints {
    /// JSON pointer to the results array (e.g. "/results").
    pub results_path: Option<String>,
    /// Field names to show in human-readable output.
    pub summary_fields: Option<Vec<String>>,
    /// JSON pointer to the total count.
    pub count_path: Option<String>,
    /// JSON pointer to the suggestions array.
    pub suggestions_path: Option<String>,
}

/// Summary entry returned by the list endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSummary {
    /// Skill name.
    pub skill_name: String,
    /// Short description.
    pub description: String,
}
