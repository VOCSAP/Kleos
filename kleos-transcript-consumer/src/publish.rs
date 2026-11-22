use kleos_lib::memory::types::StoreRequest;

use crate::feed::FactCategory;

pub const SOURCE: &str = "kleos-transcript";
pub const MAX_IMPORTANCE: i32 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTurn {
    pub uuid: String,
    /// Timestamp of the source turn, RFC 3339.
    pub ts: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishFact {
    pub lineage_id: String,
    pub fact_id: u32,
    pub category: FactCategory,
    pub importance: i32,
    pub content: String,
    pub turn: SourceTurn,
    /// Memory this publication revises; `None` for a first publication.
    pub parent_memory_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishSession {
    pub session_id: String,
    pub project: String,
    /// Resolved space name, `None` when the session has no space.
    pub space: Option<String>,
}

pub fn store_request(fact: &PublishFact, session: &PublishSession) -> StoreRequest {
    let lineage8: String = fact.lineage_id.chars().take(8).collect();
    StoreRequest {
        content: fact.content.clone(),
        category: fact.category.as_str().to_string(),
        source: SOURCE.to_string(),
        importance: fact.importance.min(MAX_IMPORTANCE),
        tags: Some(vec![
            "transcript".to_string(),
            session.project.clone(),
            format!("turn:{}", fact.turn.uuid),
            // No branch_id: branch_root_changed rewrites it, which would change
            // the tag of an already published memory and republish it.
            format!("fact:{lineage8}-{}", fact.fact_id),
        ]),
        session_id: Some(session.session_id.clone()),
        space: session.space.clone(),
        parent_memory_id: fact.parent_memory_id,
        created_at: Some(fact.turn.ts.clone()),
        is_static: None,
        ..Default::default()
    }
}
