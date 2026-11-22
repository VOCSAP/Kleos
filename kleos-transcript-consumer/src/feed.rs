use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

const SUPPORTED_SCHEMA_VERSION: u32 = 1;

/// One element of `GET /v1/feed`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct FeedItem {
    pub seq: i64,
    pub kind: String,
    pub schema_version: u32,
    pub payload: Value,
}

#[derive(Debug, thiserror::Error)]
pub enum FeedError {
    #[error("unknown feed kind {0:?}")]
    UnknownKind(String),
    #[error("unsupported schema_version {0}")]
    UnsupportedSchemaVersion(u32),
    #[error("payload carries the retired secret_patterns_version field")]
    LegacySecretPatternsVersion,
    #[error("invalid {kind} payload: {source}")]
    Payload {
        kind: String,
        #[source]
        source: serde_json::Error,
    },
}

/// A field that may be null but must be present: serde would otherwise turn
/// an absent `Option` into `None`.
fn nullable<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d)
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    Main,
    Subagent,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Label {
    pub name: String,
    pub value: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SessionMeta {
    pub session_id: String,
    pub file_key: String,
    pub host_id: String,
    pub kind: SessionKind,
    #[serde(deserialize_with = "nullable")]
    pub parent_session: Option<String>,
    pub labels: Vec<Label>,
    #[serde(deserialize_with = "nullable")]
    pub git_root: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub repo_name: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub project_key: Option<String>,
    pub lineage_id: String,
    pub branch_id: String,
    pub cc_version: String,
    pub entrypoint: String,
    #[serde(deserialize_with = "nullable")]
    pub agent: Option<String>,
    pub view_version: u32,
    pub secret_patterns_rev: u32,
    pub secret_patterns_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EpochReason {
    Initial,
    Rewrite,
    ViewUpgrade,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct EpochOpened {
    pub file_key: String,
    pub epoch: u32,
    pub epoch_reason: EpochReason,
    pub view_version: u32,
    pub secret_patterns_rev: u32,
    pub secret_patterns_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct EpochSuperseded {
    pub file_key: String,
    pub epoch: u32,
    pub by_epoch: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct MapVersion {
    pub model: String,
    pub prompt: String,
    pub reasoning_cutoff: String,
    pub view_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactStatus {
    Proposed,
    Decided,
    Measured,
    Abandoned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactCategory {
    Decision,
    Discovery,
    State,
    Issue,
    Task,
    Reference,
    General,
}

impl FactCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Decision => "decision",
            Self::Discovery => "discovery",
            Self::State => "state",
            Self::Issue => "issue",
            Self::Task => "task",
            Self::Reference => "reference",
            Self::General => "general",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Fact {
    pub fact_id: u32,
    pub version: u32,
    pub status: FactStatus,
    pub category: FactCategory,
    pub importance: i32,
    pub content: String,
    pub source_turns: Vec<String>,
    pub abandoned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct FactsUpdated {
    pub lineage_id: String,
    pub branch_id: String,
    pub session_id: String,
    pub secret_patterns_rev: u32,
    pub secret_patterns_hash: String,
    pub map_version: MapVersion,
    pub facts: Vec<Fact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LineageBranch {
    pub lineage_id: String,
    pub branch_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LineageMerged {
    pub from: String,
    pub into: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct BranchRootChanged {
    pub lineage_id: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RemapMotif {
    ModelChange,
    PromptChange,
    ViewUpgrade,
    PatternsUpgrade,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Remap {
    pub motif: RemapMotif,
    pub lineage_id: String,
    pub map_version: MapVersion,
    pub secret_patterns_rev: u32,
    pub secret_patterns_hash: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FeedEvent {
    SessionMeta(Box<SessionMeta>),
    EpochOpened(EpochOpened),
    EpochSuperseded(EpochSuperseded),
    FactsUpdated(FactsUpdated),
    LineageStabilized(LineageBranch),
    LineageMerged(LineageMerged),
    LineageReopened(LineageBranch),
    BranchRootChanged(BranchRootChanged),
    Remap(Remap),
}

impl FeedEvent {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::SessionMeta(_) => "session_meta",
            Self::EpochOpened(_) => "epoch_opened",
            Self::EpochSuperseded(_) => "epoch_superseded",
            Self::FactsUpdated(_) => "facts_updated",
            Self::LineageStabilized(_) => "lineage_stabilized",
            Self::LineageMerged(_) => "lineage_merged",
            Self::LineageReopened(_) => "lineage_reopened",
            Self::BranchRootChanged(_) => "branch_root_changed",
            Self::Remap(_) => "remap",
        }
    }
}

/// Fails closed: an unknown kind, a schema version other than 1, a missing
/// required field or an out-of-vocabulary string value is an error. Unknown
/// extra fields are ignored.
pub fn decode(item: &FeedItem) -> Result<FeedEvent, FeedError> {
    if item.schema_version != SUPPORTED_SCHEMA_VERSION {
        return Err(FeedError::UnsupportedSchemaVersion(item.schema_version));
    }
    if item
        .payload
        .as_object()
        .is_some_and(|o| o.contains_key("secret_patterns_version"))
    {
        return Err(FeedError::LegacySecretPatternsVersion);
    }
    let p = item.payload.clone();
    let parsed = match item.kind.as_str() {
        "session_meta" => serde_json::from_value(p).map(|v| FeedEvent::SessionMeta(Box::new(v))),
        "epoch_opened" => serde_json::from_value(p).map(FeedEvent::EpochOpened),
        "epoch_superseded" => serde_json::from_value(p).map(FeedEvent::EpochSuperseded),
        "facts_updated" => serde_json::from_value(p).map(FeedEvent::FactsUpdated),
        "lineage_stabilized" => serde_json::from_value(p).map(FeedEvent::LineageStabilized),
        "lineage_merged" => serde_json::from_value(p).map(FeedEvent::LineageMerged),
        "lineage_reopened" => serde_json::from_value(p).map(FeedEvent::LineageReopened),
        "branch_root_changed" => serde_json::from_value(p).map(FeedEvent::BranchRootChanged),
        "remap" => serde_json::from_value(p).map(FeedEvent::Remap),
        other => return Err(FeedError::UnknownKind(other.to_string())),
    };
    parsed.map_err(|source| FeedError::Payload {
        kind: item.kind.clone(),
        source,
    })
}
