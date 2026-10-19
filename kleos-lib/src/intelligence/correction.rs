//! Memory correction -- replace a memory with corrected content while preserving lineage.

use crate::db::Database;
use crate::embeddings::EmbeddingProvider;
use crate::memory;
use crate::memory::types::{Memory, UpdateRequest};
use crate::Result;

/// Correct a memory: create a new version with corrected content, link it to
/// the original via 'supersedes', and mark the original as superseded.
#[tracing::instrument(skip(db, corrected_content, reason))]
pub async fn correct_memory(
    db: &Database,
    user_id: i64,
    memory_id: i64,
    corrected_content: &str,
    reason: Option<&str>,
) -> Result<Memory> {
    correct_memory_with_embedder(db, None, user_id, memory_id, corrected_content, reason).await
}

/// [`correct_memory`] with the server embedder, so the corrected text gets its
/// own vector instead of none.
#[tracing::instrument(skip(db, embedder, corrected_content, reason))]
pub async fn correct_memory_with_embedder(
    db: &Database,
    embedder: Option<&dyn EmbeddingProvider>,
    user_id: i64,
    memory_id: i64,
    corrected_content: &str,
    reason: Option<&str>,
) -> Result<Memory> {
    // Fetch the original (validates ownership)
    let original = memory::get(db, memory_id, user_id).await?;

    // The corrected text is the next version of the original, chained and
    // carrying its metadata. memory::store would start an unrelated version 1,
    // keep the original latest, and could fold a close correction back into
    // the original as a duplicate. update refuses an original that is no
    // longer the latest version, before anything below is written.
    let req = UpdateRequest {
        content: Some(corrected_content.to_string()),
        category: None,
        importance: None,
        tags: None,
        is_static: None,
        status: None,
        embedding: None,
        chunk_embeddings: None,
    };
    let corrected = match embedder {
        Some(embedder) => {
            memory::update_with_chunks(db, embedder, memory_id, req, user_id, false).await?
        }
        None => memory::update(db, memory_id, req, user_id, false).await?,
    };

    let new_id = corrected.id;

    // Create 'supersedes' link from new to old
    memory::insert_link(db, new_id, memory_id, 1.0, "supersedes", user_id).await?;

    // Mark original as superseded
    db.write(move |conn| {
        conn.execute(
            "UPDATE memories SET is_superseded = 1, updated_at = datetime('now') \
             WHERE id = ?1",
            rusqlite::params![memory_id],
        )?;
        Ok(())
    })
    .await?;
    memory::search::invalidate_search_cache(user_id);

    // Record the correction in reconsolidations table
    let reason_text = reason.unwrap_or("manual correction").to_string();
    let old_content = original.content.clone();
    let new_content = corrected_content.to_string();
    db.write(move |conn| {
        conn.execute(
            "INSERT INTO reconsolidations \
             (memory_id, old_content, new_content, reason, user_id, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
            rusqlite::params![memory_id, old_content, new_content, reason_text, user_id],
        )?;
        Ok(())
    })
    .await?;

    // Return the new memory
    memory::get(db, new_id, user_id).await
}
