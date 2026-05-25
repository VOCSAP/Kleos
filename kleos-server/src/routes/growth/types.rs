use serde::Deserialize;

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
    #[serde(default)]
    pub include_unscoped: Option<bool>,
}

#[derive(Deserialize)]
pub(super) struct MaterializeBody {
    pub observation_id: i64,
}
