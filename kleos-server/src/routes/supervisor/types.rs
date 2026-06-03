use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub(super) struct InjectBody {
    pub session_id: String,
    pub rule_id: String,
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct PendingQuery {
    pub session_id: String,
    /// Patch 45 (VOCSAP): optional long-poll override. `wait=0` -> immediate
    /// return with no long-poll (used by the fast PreToolUse drain hook, which
    /// cannot block 30s); `wait=n` -> cap the long-poll at n seconds; absent ->
    /// default long-poll (upstream behavior preserved).
    #[serde(default)]
    pub wait: Option<u64>,
}

#[derive(Debug, Serialize)]
pub(super) struct InjectionRow {
    pub id: i64,
    pub session_id: String,
    pub rule_id: String,
    pub severity: String,
    pub message: String,
    pub created_at: String,
}
