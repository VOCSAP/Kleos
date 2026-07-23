use serde::Deserialize;

/// Query parameters for GET /prompt: output format, token budget, and free-form context.
#[derive(Deserialize)]
pub(super) struct PromptQuery {
    pub format: Option<String>,
    pub tokens: Option<usize>,
    pub context: Option<String>,
}

/// Body for POST /header: actor identity plus an optional context string and row limit.
#[derive(Deserialize)]
pub(super) struct HeaderBody {
    pub actor_model: Option<String>,
    pub actor_role: Option<String>,
    pub context: Option<String>,
    pub limit: Option<usize>,
}

/// Body for POST /prompt/generate: agent/task identity plus per-section opt-in
/// flags and limits. All flags are optional; the handler applies server defaults.
#[derive(Deserialize)]
pub(super) struct GeneratePromptRequest {
    pub agent: String,
    pub task: String,
    #[serde(default)]
    pub max_tokens: Option<usize>,
    #[serde(default)]
    pub include_personality: Option<bool>,
    #[serde(default)]
    pub include_memories: Option<bool>,
    #[serde(default)]
    pub memory_limit: Option<usize>,
    #[serde(default)]
    pub include_brain: Option<bool>,
    #[serde(default)]
    pub include_growth: Option<bool>,
    #[serde(default)]
    pub include_instincts: Option<bool>,
    #[serde(default)]
    pub brain_limit: Option<usize>,
    #[serde(default)]
    pub growth_limit: Option<usize>,
    /// Opt-in for the "## Recent Agent Activity" section fed from the Broca
    /// action log. Defaults to false so existing callers see no new section.
    #[serde(default)]
    pub include_activity: Option<bool>,
    /// Maximum Broca actions injected when `include_activity` is set.
    /// Server clamps to 1..=30; defaults to 10.
    #[serde(default)]
    pub activity_limit: Option<usize>,
}
