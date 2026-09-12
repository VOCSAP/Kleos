//! Wire and storage types for the toolbox catalog.
//!
//! Everything here is `Serialize + Deserialize`: the same shapes travel from the
//! indexing client to the server handlers and back out of a search. Two rules
//! shape the design:
//!
//! * no `space` / `space_id` anywhere -- the toolbox sits outside the spaces
//!   partitioning, and the MCP bridge injects those fields into every call, so
//!   request types must tolerate (ignore) unknown fields rather than reject them;
//! * the raw embedding never leaves the crate: [`ToolEntry`] exposes only
//!   `has_embedding` + `embedding_model`.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Which family of identity a canonical tool key was derived from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyKind {
    /// A git remote, normalized to `<host>/<owner>/<repo>`.
    Git,
    /// A plain URL, normalized to `<host>/<path>`.
    Url,
    /// A filesystem location, `local:<host>:<path>`.
    Local,
}

impl KeyKind {
    /// Storage/wire spelling (`"git"`, `"url"`, `"local"`).
    pub fn as_str(&self) -> &'static str {
        match self {
            KeyKind::Git => "git",
            KeyKind::Url => "url",
            KeyKind::Local => "local",
        }
    }
}

impl fmt::Display for KeyKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for KeyKind {
    type Err = crate::EngError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "git" => Ok(KeyKind::Git),
            "url" => Ok(KeyKind::Url),
            "local" => Ok(KeyKind::Local),
            other => Err(crate::EngError::InvalidInput(format!(
                "unknown toolbox key kind: {other}"
            ))),
        }
    }
}

/// Raw identity fields as sent by the client. The server always recomputes the
/// canonical key from these (see [`crate::toolbox::key::normalize_tool_key`]);
/// the client never sends a key directly.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KeyInput {
    /// `git remote get-url origin` output, in any of its usual spellings.
    #[serde(default)]
    pub git_remote: Option<String>,
    /// Canonical URL when the tool has no git remote (docs, hosted service).
    #[serde(default)]
    pub url: Option<String>,
    /// Absolute path on the indexing machine.
    #[serde(default)]
    pub local_path: Option<String>,
    /// Hostname of the indexing machine; part of a `local:` key, and recorded on
    /// the location row for every kind.
    #[serde(default)]
    pub host: Option<String>,
}

/// The commit the sheet was written from. `time` is the commit's unix seconds
/// and drives the overwrite policy (see [`crate::toolbox::store::upsert_tool`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub sha: String,
    #[serde(default)]
    pub time: Option<i64>,
}

/// One indexing request: the shared sheet plus the caller's own location for it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpsertToolRequest {
    #[serde(default)]
    pub key: KeyInput,
    /// Free-form family: `repo`, `skill`, `plugin`, `cli`, `mcp`, `doc`, ...
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    /// Short, keyword-dense description. This is what gets embedded.
    #[serde(default)]
    pub summary: String,
    /// Full markdown sheet.
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub canonical_url: Option<String>,
    #[serde(default)]
    pub commit: Option<CommitInfo>,
    /// Tags recorded on the caller's location row, not on the shared sheet.
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub notes: String,
    /// Overrule the "newer commit wins" policy in the cases where it allows it.
    #[serde(default)]
    pub force: bool,
}

/// A stored shared sheet. Never carries the raw embedding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEntry {
    pub id: i64,
    pub tool_key: String,
    pub key_kind: String,
    pub kind: String,
    pub name: String,
    pub summary: String,
    pub body: String,
    pub keywords: Vec<String>,
    pub canonical_url: Option<String>,
    pub indexed_commit: Option<String>,
    pub indexed_commit_time: Option<i64>,
    pub content_hash: String,
    pub has_embedding: bool,
    pub embedding_model: Option<String>,
    pub indexed_by_user_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

/// Where one user keeps one tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolLocation {
    pub id: i64,
    pub user_id: i64,
    pub tool_key: String,
    pub host: String,
    pub local_path: String,
    pub tags: Vec<String>,
    pub notes: String,
    pub last_seen_at: String,
    pub created_at: String,
}

/// What an upsert did to the *shared* sheet. The caller's location row is always
/// written, whatever the outcome here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpsertOutcome {
    /// No row existed for this key.
    Inserted,
    /// The stored sheet was replaced by the incoming one.
    Updated,
    /// The stored sheet won (older incoming commit, or no commit to compare).
    Kept,
    /// Same content hash: nothing worth writing.
    Unchanged,
}

impl UpsertOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            UpsertOutcome::Inserted => "inserted",
            UpsertOutcome::Updated => "updated",
            UpsertOutcome::Kept => "kept",
            UpsertOutcome::Unchanged => "unchanged",
        }
    }
}

impl fmt::Display for UpsertOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Default number of results returned by a find when the caller says nothing.
pub const DEFAULT_FIND_LIMIT: usize = 10;
/// Hard ceiling on a find's result count.
pub const MAX_FIND_LIMIT: usize = 50;

/// Filters and knobs for [`crate::toolbox::search::find_tools`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindOptions {
    /// Keep only tools of this `kind`.
    #[serde(default)]
    pub kind: Option<String>,
    /// Keep only tools the caller tagged with ALL of these.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Result count, clamped to `1..=MAX_FIND_LIMIT`; `0` means the default.
    #[serde(default)]
    pub limit: usize,
    /// Caller intent only: `find_tools` never reranks (kleos-lib has no access to
    /// the server's reranker). The handler honours it by calling
    /// [`crate::toolbox::search::rerank_find_results`].
    #[serde(default)]
    pub rerank: bool,
}

impl Default for FindOptions {
    fn default() -> Self {
        Self {
            kind: None,
            tags: Vec::new(),
            limit: DEFAULT_FIND_LIMIT,
            rerank: false,
        }
    }
}

impl FindOptions {
    /// The effective result count: `0` (unset) becomes the default, anything
    /// larger than [`MAX_FIND_LIMIT`] is clamped down.
    pub fn effective_limit(&self) -> usize {
        if self.limit == 0 {
            DEFAULT_FIND_LIMIT
        } else {
            self.limit.min(MAX_FIND_LIMIT)
        }
    }
}

/// One hit: the shared sheet, the caller's locations for it, and the score
/// breakdown. `fts_score` is the raw bm25 value (negative, lower is better),
/// `vector_score` the raw cosine similarity; `score` is the RRF fusion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindResult {
    pub tool: ToolEntry,
    pub locations: Vec<ToolLocation>,
    pub score: f64,
    pub fts_score: f64,
    pub vector_score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rerank_score: Option<f64>,
}
