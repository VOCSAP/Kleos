use kleos_lib::conversations::AddMessageRequest;
use serde::Deserialize;

/// Patch 49 Lot C: mirrors kleos-server/src/routes/memory/types.rs -- plain
/// `#[serde(default)]` on `Option<bool>` deserializes an absent field to
/// `None`, which downstream treats as strict, contradicting the documented
/// server-side default of `true`. An explicit `include_unscoped: false`
/// still deserializes to `Some(false)`.
fn default_include_unscoped() -> Option<bool> {
    Some(true)
}

/// Patch 49 task 2: collapses an explicit `null` into the same inclusive
/// default as an absent key (cf. kleos-server/src/routes/memory/types.rs).
/// Kept symmetric with the other `include_unscoped` boundary structs even
/// though this one is only ever reached via `axum::extract::Query` (no
/// `null` literal in a query string).
fn deserialize_include_unscoped<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<bool>::deserialize(deserializer)?.or_else(default_include_unscoped))
}

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
    #[serde(
        default = "default_include_unscoped",
        deserialize_with = "deserialize_include_unscoped"
    )]
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

#[cfg(test)]
mod tests {
    use super::*;

    // Patch 49 Lot C acceptance check, via the actual production extraction medium:
    // GET /conversations extracts `ListConversationsParams` through
    // `axum::extract::Query<ListConversationsParams>` (routes/conversations/mod.rs:62),
    // which axum implements with `serde_urlencoded` (axum 0.8.9 depends on
    // serde_urlencoded directly, per its own Cargo.lock block) -- not serde_json. An
    // absent `include_unscoped` key must deserialize to the documented default
    // `Some(true)`, while an explicit `false` must still deserialize to `Some(false)`.
    #[test]
    fn list_conversations_params_include_unscoped_default_is_true_but_explicit_false_stays_false() {
        let absent: ListConversationsParams = serde_urlencoded::from_str("").unwrap();
        assert_eq!(absent.include_unscoped, Some(true));

        let explicit_false: ListConversationsParams =
            serde_urlencoded::from_str("include_unscoped=false").unwrap();
        assert_eq!(explicit_false.include_unscoped, Some(false));

        let explicit_true: ListConversationsParams =
            serde_urlencoded::from_str("include_unscoped=true").unwrap();
        assert_eq!(explicit_true.include_unscoped, Some(true));
    }
}
