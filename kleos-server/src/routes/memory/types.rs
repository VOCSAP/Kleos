use serde::Deserialize;

/// Patch 49 Lot B: documented server-side default for `include_unscoped` is
/// `true` (scoped space + user's default space + legacy NULL rows). Plain
/// `#[serde(default)]` on `Option<bool>` defaults to `None`, which downstream
/// (kleos-lib) treats as strict -- the opposite of the documented contract.
/// This helper makes the field absent from the payload deserialize to
/// `Some(true)`; an explicit `include_unscoped: false` still deserializes to
/// `Some(false)` (serde defaults only apply when the key is missing).
/// Mirrors the existing pattern in kleos-server/src/routes/brain/types.rs.
fn default_include_unscoped() -> Option<bool> {
    Some(true)
}

/// Patch 49 task 2: `#[serde(default = ...)]` alone only fires when the KEY is
/// absent. A key present with an explicit JSON `null` (`"include_unscoped":
/// null`) deserializes through the normal path to `None`, which downstream
/// treats as strict -- the same bug as an absent key, just reachable a
/// different way. This collapses `null` into the same inclusive default;
/// an explicit `true`/`false` is unaffected (`Option::or` only substitutes
/// on `None`).
fn deserialize_include_unscoped<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<bool>::deserialize(deserializer)?.or_else(default_include_unscoped))
}

/// JSON body accepted by the hybrid memory search endpoints.
#[derive(Debug, Deserialize)]
pub(super) struct SearchBody {
    pub query: String,
    pub limit: Option<usize>,
    pub category: Option<String>,
    pub source: Option<String>,
    pub tags: Option<Vec<String>>,
    pub threshold: Option<f32>,
    pub tag: Option<String>,
    pub space_id: Option<i64>,
    /// Patch 33: free-form space name (resolved server-side).
    #[serde(default)]
    pub space: Option<String>,
    /// Patch 33: include the user's default space (and legacy NULL rows)
    /// when filtering by a named space. Defaults to `true`.
    #[serde(
        default = "default_include_unscoped",
        deserialize_with = "deserialize_include_unscoped"
    )]
    pub include_unscoped: Option<bool>,
    pub include_forgotten: Option<bool>,
    pub mode: Option<String>,
    pub question_type: Option<kleos_lib::memory::types::QuestionType>,
    pub expand_relationships: Option<bool>,
    pub include_links: Option<bool>,
    pub latest_only: Option<bool>,
    pub source_filter: Option<String>,
    pub budget: Option<kleos_lib::memory::types::SearchBudget>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RecallBody {
    pub context: Option<String>,
    pub query: Option<String>,
    pub limit: Option<usize>,
    pub space_id: Option<i64>,
    /// Patch 33: free-form space name (resolved server-side).
    #[serde(default)]
    pub space: Option<String>,
    /// Patch 33: include the user's default space when filtering by a
    /// named space. Defaults to `true`.
    #[serde(
        default = "default_include_unscoped",
        deserialize_with = "deserialize_include_unscoped"
    )]
    pub include_unscoped: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ListQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub category: Option<String>,
    pub source: Option<String>,
    pub space_id: Option<i64>,
    /// Patch 33: free-form space name (resolved server-side).
    #[serde(default)]
    pub space: Option<String>,
    /// Patch 33: include the user's default space when filtering by a
    /// named space. Defaults to `true`.
    #[serde(
        default = "default_include_unscoped",
        deserialize_with = "deserialize_include_unscoped"
    )]
    pub include_unscoped: Option<bool>,
    pub include_forgotten: Option<bool>,
    pub include_archived: Option<bool>,
    /// Inclusive lower bound on created_at (YYYY-MM-DD), or None.
    pub from: Option<String>,
    /// Exclusive upper bound on created_at (YYYY-MM-DD), or None.
    pub to: Option<String>,
}

/// Query params for GET /memories/calendar.
#[derive(Debug, Deserialize)]
pub(super) struct CalendarQuery {
    /// Bucket granularity: "year", "month", or "day".
    pub granularity: String,
    /// Required for "month" and "day" granularity; ignored for "year".
    pub year: Option<i32>,
    /// Required for "day" granularity; ignored otherwise.
    pub month: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub(super) struct TrashListOptions {
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(super) struct SearchTagsBody {
    pub tags: Vec<String>,
    pub match_all: Option<bool>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(super) struct UpdateTagsBody {
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ForgetBody {
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Patch 49 Lot B acceptance check: an absent `include_unscoped` must
    // deserialize to the documented default `Some(true)`, while an explicit
    // `false` must still deserialize to `Some(false)` (serde defaults only
    // fire when the key is missing from the payload, not when it is present
    // and falsy).
    #[test]
    fn search_body_include_unscoped_default_is_true_but_explicit_false_stays_false() {
        let absent: SearchBody = serde_json::from_str(r#"{"query": "x"}"#).unwrap();
        assert_eq!(absent.include_unscoped, Some(true));

        let explicit_false: SearchBody =
            serde_json::from_str(r#"{"query": "x", "include_unscoped": false}"#).unwrap();
        assert_eq!(explicit_false.include_unscoped, Some(false));

        let explicit_true: SearchBody =
            serde_json::from_str(r#"{"query": "x", "include_unscoped": true}"#).unwrap();
        assert_eq!(explicit_true.include_unscoped, Some(true));
    }

    #[test]
    fn list_query_include_unscoped_default_is_true_but_explicit_false_stays_false() {
        let absent: ListQuery = serde_json::from_str(r#"{}"#).unwrap();
        assert_eq!(absent.include_unscoped, Some(true));

        let explicit_false: ListQuery =
            serde_json::from_str(r#"{"include_unscoped": false}"#).unwrap();
        assert_eq!(explicit_false.include_unscoped, Some(false));
    }

    // Patch 49 review finding: `ListQuery` is never deserialized from JSON in production --
    // GET /list extracts it via `axum::extract::Query<ListQuery>` (routes/memory/mod.rs),
    // which axum implements with `serde_urlencoded` (confirmed via axum 0.8.9's own
    // Cargo.lock dependency block: axum depends directly on serde_urlencoded, not
    // serde_json or serde_html_form). A JSON-only test exercises a different Deserializer
    // and can pass while the real HTTP query-string path still fails. This test proves the
    // same absent/explicit-false contract holds for the actual extraction medium.
    #[test]
    fn list_query_include_unscoped_via_query_string_default_is_true_but_explicit_false_stays_false()
    {
        let absent: ListQuery = serde_urlencoded::from_str("").unwrap();
        assert_eq!(absent.include_unscoped, Some(true));

        let explicit_false: ListQuery =
            serde_urlencoded::from_str("include_unscoped=false").unwrap();
        assert_eq!(explicit_false.include_unscoped, Some(false));

        let explicit_true: ListQuery = serde_urlencoded::from_str("include_unscoped=true").unwrap();
        assert_eq!(explicit_true.include_unscoped, Some(true));
    }

    // Patch 49 review finding #3: `#[serde(default = "...")]` only fires when the KEY is
    // absent, not when it is present with a null-ish value. serde_urlencoded has no `null`
    // literal (query strings are all-string key=value pairs), so this edge case is JSON-only
    // -- an explicit `"include_unscoped": null` in a JSON body (SearchBody/RecallBody) still
    // deserializes to `None`, which downstream (kleos-lib) treats as strict, contradicting
    // the documented "absent or null -> inclusive" contract. This is EXPECTED TO FAIL until
    // the dev's follow-up fix (Patch 49 task 2, e.g. a custom deserialize_with or a
    // post-deserialize `.or(Some(true))` normalization step). Left un-#[ignore]d so the red
    // is visible in the suite as the acceptance signal for that follow-up.
    #[test]
    fn search_body_include_unscoped_explicit_null_key_should_be_inclusive() {
        let explicit_null: SearchBody =
            serde_json::from_str(r#"{"query": "x", "include_unscoped": null}"#).unwrap();
        assert_eq!(
            explicit_null.include_unscoped,
            Some(true),
            "an explicit `include_unscoped: null` key must be treated the same as an absent \
             key (inclusive), not fall through to None/strict; currently RED pending the \
             dev's Patch 49 follow-up fix"
        );
    }
}
