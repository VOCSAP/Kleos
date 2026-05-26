//! DTOs for /brain routes.
//!
//! Patch 36 (2026-05-26): introduces `BrainQueryRequest`, a thin wrapper
//! around `kleos_lib::services::brain::BrainQueryOptions` that lets the
//! caller request a per-space post-filter on the activated patterns
//! returned by `brain.query`. The plan (Patch 33 section 4 paragraphe
//! Brain) keeps the Hopfield substrate global; the filter is applied
//! entirely cote handler so the brain engine stays untouched. `inner`
//! is flattened during deserialization so existing payloads (just
//! `{query, top_k, beta, spread_hops}`) keep working unchanged.

use kleos_lib::services::brain::BrainQueryOptions;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct BrainQueryRequest {
    #[serde(flatten)]
    pub inner: BrainQueryOptions,
    #[serde(default)]
    pub space: Option<String>,
    #[serde(default)]
    pub space_id: Option<i64>,
    #[serde(default = "default_include_unscoped")]
    pub include_unscoped: bool,
}

fn default_include_unscoped() -> bool {
    true
}
