use serde::Deserialize;

/// Patch 49 Lot C: mirrors kleos-server/src/routes/memory/types.rs -- plain
/// `#[serde(default)]` on `Option<bool>` deserializes an absent field to
/// `None`, which downstream treats as strict, contradicting the documented
/// server-side default of `true`. An explicit `include_unscoped: false`
/// still deserializes to `Some(false)`.
fn default_include_unscoped() -> Option<bool> {
    Some(true)
}

/// Patch 49 task 2: `#[serde(default = ...)]` alone only fires when the KEY
/// is absent, not when it is present with an explicit `null`. Collapses
/// `null` into the same inclusive default as absent; an explicit
/// `true`/`false` is unaffected. Kept symmetric with the other 5
/// `include_unscoped` boundary structs even though this one is only ever
/// reached via `axum::extract::Query` (no `null` literal in a query
/// string), for a single consistent contract across all of them.
fn deserialize_include_unscoped<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<bool>::deserialize(deserializer)?.or_else(default_include_unscoped))
}

#[derive(Deserialize)]
pub(super) struct ObservationsQuery {
    pub limit: Option<usize>,
    /// Patch 33 -- limit observations to a named space (#3028 fix).
    #[serde(default)]
    pub space: Option<String>,
    /// Patch 33 -- limit observations to an explicit numeric space id.
    #[serde(default)]
    pub space_id: Option<i64>,
    /// Patch 33 -- include the user's default space and pre-v57 NULL
    /// rows when filtering by space. Defaults to server-side `true`.
    #[serde(
        default = "default_include_unscoped",
        deserialize_with = "deserialize_include_unscoped"
    )]
    pub include_unscoped: Option<bool>,
}

#[derive(Deserialize)]
pub(super) struct MaterializeBody {
    pub observation_id: i64,
}

#[derive(Deserialize)]
pub(super) struct ContextQuery {
    /// Keywords or current session topic to score observations against.
    pub q: Option<String>,
    /// Maximum number of scored observations to return (default 5, max 20).
    pub limit: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Patch 49 Lot C acceptance check, via the actual production extraction medium:
    // GET /growth/observations extracts `ObservationsQuery` through
    // `axum::extract::Query<ObservationsQuery>` (routes/growth/mod.rs), which axum
    // implements with `serde_urlencoded` (axum 0.8.9 depends on serde_urlencoded
    // directly, per its own Cargo.lock block) -- not serde_json. An absent
    // `include_unscoped` key must deserialize to the documented default `Some(true)`,
    // while an explicit `false` must still deserialize to `Some(false)`.
    #[test]
    fn observations_query_include_unscoped_default_is_true_but_explicit_false_stays_false() {
        let absent: ObservationsQuery = serde_urlencoded::from_str("").unwrap();
        assert_eq!(absent.include_unscoped, Some(true));

        let explicit_false: ObservationsQuery =
            serde_urlencoded::from_str("include_unscoped=false").unwrap();
        assert_eq!(explicit_false.include_unscoped, Some(false));

        let explicit_true: ObservationsQuery =
            serde_urlencoded::from_str("include_unscoped=true").unwrap();
        assert_eq!(explicit_true.include_unscoped, Some(true));
    }
}
