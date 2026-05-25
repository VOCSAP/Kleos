use kleos_lib::conversations::AddMessageRequest;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct ListConversationsParams {
    pub limit: Option<usize>,
    pub agent: Option<String>,
    /// Patch 33 -- limit listing to a named space.
    #[serde(default)]
    pub space: Option<String>,
    /// Patch 33 -- limit listing to an explicit numeric space id.
    #[serde(default)]
    pub space_id: Option<i64>,
    /// Patch 33 -- include the user's default space when filtering.
    /// Defaults to server-side `true` (inclusive).
    #[serde(default)]
    pub include_unscoped: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct GetConversationParams {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(super) enum MessageBody {
    Single(AddMessageRequest),
    Batch(Vec<AddMessageRequest>),
}

#[derive(Debug, Deserialize)]
pub(super) struct ListMessagesParams {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}
