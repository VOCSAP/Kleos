//! Toolbox: a searchable catalog of the tools a team already has.
//!
//! An agent running on someone's machine reads a repository (or a skill, plugin,
//! CLI, MCP server, doc site), writes a sheet describing what it is good for,
//! and sends it here. Later, from any AI CLI, "do we have something for X?" is
//! one hybrid search away, and the answer comes back with the places the caller
//! actually keeps that tool.
//!
//! Two design rules run through the whole module:
//!
//! * **The server only stores and retrieves.** No clone, no fetch, no directory
//!   walk happens on this side; the client does all the digestion.
//! * **Sheets are shared, locations are not.** [`store::upsert_tool`] writes one
//!   row per canonical key for everyone; [`store::upsert_location`] writes the
//!   caller's own copy of "where I keep it". Every read of the shared table is
//!   filtered by the keys the caller owns a location for, so the catalog can be
//!   shared without any user seeing another's inventory.
//!
//! The canonical key is recomputed server-side from the raw identity fields
//! (see [`key::normalize_tool_key`]), never trusted from the client.

pub mod embedding;
pub mod key;
pub mod search;
pub mod store;
pub mod types;

#[cfg(test)]
mod tests;

pub use key::normalize_tool_key;
pub use search::{find_tools, fts_query, rerank_find_results};
pub use store::{
    content_hash, delete_locations, embedding_text, get_tool_by_id, get_tool_by_key,
    list_user_keys, locations_for_keys, normalize_keywords, normalize_tags, set_embedding,
    tools_by_keys, tools_needing_embedding, upsert_location, upsert_tool,
};
pub use types::{
    CommitInfo, FindOptions, FindResult, KeyInput, KeyKind, ToolEntry, ToolLocation, UpsertOutcome,
    UpsertToolRequest, DEFAULT_FIND_LIMIT, MAX_FIND_LIMIT,
};
