//! Hybrid retrieval over the toolbox catalog.
//!
//! Two channels, fused by reciprocal rank: FTS5 over the sheets, and a cosine
//! scan over the stored embeddings. Both are restricted, before anything else,
//! to the keys the calling user owns a location for -- the shared table is never
//! queried without that filter, which is what keeps one user's catalog out of
//! another's results.

use super::embedding;
use super::store::{self, MAX_IN_PARAMS};
use super::types::{FindOptions, FindResult, ToolEntry};
use crate::db::Database;
use crate::memory::scoring::rrf_score;
use crate::reranker::Reranker;
use crate::Result;
use std::collections::HashMap;

/// Rows pulled from each channel before fusion. The catalog is human-sized, so a
/// fixed pool is cheaper to reason about than a limit-derived one.
const CHANNEL_CANDIDATES: usize = 200;

/// RRF weights. The vector channel is trusted more: the whole point of the
/// toolbox is answering "do we have something for X?" in words that do not
/// appear literally in the sheet.
const FTS_WEIGHT: f64 = 1.0;
const VECTOR_WEIGHT: f64 = 1.5;

/// Build an FTS5 MATCH expression from free text.
///
/// Non-alphanumeric characters become separators (an unescaped `"` or `*` is a
/// syntax error in FTS5, and a syntax error is a 500 for the caller), tokens
/// shorter than two characters are dropped, and the rest are ORed: a natural
/// language question carries many words that the sheet will not contain, and
/// FTS5's implicit AND would answer nothing.
pub fn fts_query(query: &str) -> Option<String> {
    let tokens: Vec<String> = query
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .filter(|w| w.chars().count() >= 2)
        .map(|w| w.to_lowercase())
        .collect();
    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" OR "))
    }
}

/// Search the caller's tools.
///
/// `query_embedding` is the caller's query already embedded (kleos-lib has no
/// embedder of its own); `None` degrades cleanly to the FTS channel alone.
/// Returns at most `opts.effective_limit()` hits, best first, each carrying the
/// caller's locations for the tool.
pub async fn find_tools(
    shared: &Database,
    user_db: &Database,
    user_id: i64,
    query: &str,
    query_embedding: Option<&[f32]>,
    opts: &FindOptions,
) -> Result<Vec<FindResult>> {
    let keys = store::list_user_keys(user_db, user_id).await?;
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let locations = store::locations_for_keys(user_db, user_id, &keys).await?;

    // Tag filter: keep a key only when one of this user's locations for it
    // carries every requested tag.
    let wanted_tags = store::normalize_tags(&opts.tags);
    let keys: Vec<String> = if wanted_tags.is_empty() {
        keys
    } else {
        keys.into_iter()
            .filter(|k| {
                locations.get(k).is_some_and(|locs| {
                    locs.iter()
                        .any(|l| wanted_tags.iter().all(|t| l.tags.contains(t)))
                })
            })
            .collect()
    };
    if keys.is_empty() {
        return Ok(Vec::new());
    }

    let fts_hits = fts_channel(shared, query, &keys).await?;
    let vector_hits = match query_embedding {
        Some(q) if !q.is_empty() => vector_channel(shared, q, &keys).await?,
        _ => Vec::new(),
    };

    // Reciprocal rank fusion over the two channels.
    let mut fused: HashMap<i64, (f64, f64, f64)> = HashMap::new();
    for (rank, (id, bm25)) in fts_hits.iter().enumerate() {
        let e = fused.entry(*id).or_insert((0.0, 0.0, 0.0));
        e.0 += rrf_score(rank) * FTS_WEIGHT;
        e.1 = *bm25;
    }
    for (rank, (id, cos)) in vector_hits.iter().enumerate() {
        let e = fused.entry(*id).or_insert((0.0, 0.0, 0.0));
        e.0 += rrf_score(rank) * VECTOR_WEIGHT;
        e.2 = *cos;
    }
    if fused.is_empty() {
        return Ok(Vec::new());
    }

    let ids: Vec<i64> = fused.keys().copied().collect();
    let tools = store::tools_by_ids(shared, &ids).await?;
    let kind_filter = opts
        .kind
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty());

    let mut results: Vec<FindResult> = tools
        .into_iter()
        .filter(|t: &ToolEntry| kind_filter.is_none_or(|k| t.kind.eq_ignore_ascii_case(k)))
        .map(|t| {
            let (score, fts_score, vector_score) =
                fused.get(&t.id).copied().unwrap_or((0.0, 0.0, 0.0));
            let locs = locations.get(&t.tool_key).cloned().unwrap_or_default();
            FindResult {
                tool: t,
                locations: locs,
                score,
                fts_score,
                vector_score,
                rerank_score: None,
            }
        })
        .collect();

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.tool.name.cmp(&b.tool.name))
    });
    results.truncate(opts.effective_limit());
    Ok(results)
}

/// FTS channel: `(tool id, raw bm25)` best first (bm25 is negative, lower is
/// better, so the SQL `ORDER BY rank` already yields the right order).
async fn fts_channel(shared: &Database, query: &str, keys: &[String]) -> Result<Vec<(i64, f64)>> {
    let Some(match_expr) = fts_query(query) else {
        return Ok(Vec::new());
    };
    let mut hits: Vec<(i64, f64)> = Vec::new();
    for chunk in keys.chunks(MAX_IN_PARAMS) {
        let chunk: Vec<String> = chunk.to_vec();
        let expr = match_expr.clone();
        let mut rows = shared
            .read(move |conn| {
                let n = chunk.len();
                let sql = format!(
                    "SELECT t.id, bm25(toolbox_tools_fts) AS rank \
                     FROM toolbox_tools_fts f \
                     JOIN toolbox_tools t ON t.id = f.rowid \
                     WHERE toolbox_tools_fts MATCH ?{} AND t.tool_key IN ({}) \
                     ORDER BY rank LIMIT ?{}",
                    n + 1,
                    store::placeholders(n),
                    n + 2
                );
                let mut stmt = conn.prepare(&sql)?;
                let mut binds: Vec<Box<dyn rusqlite::ToSql>> = chunk
                    .iter()
                    .map(|k| Box::new(k.clone()) as Box<dyn rusqlite::ToSql>)
                    .collect();
                binds.push(Box::new(expr));
                binds.push(Box::new(CHANNEL_CANDIDATES as i64));
                let refs: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();
                let rows = stmt
                    .query_map(refs.as_slice(), |r| {
                        Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?;
        hits.append(&mut rows);
    }
    // bm25 values are comparable across batches (same index, same query).
    hits.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(CHANNEL_CANDIDATES);
    Ok(hits)
}

/// Vector channel: `(tool id, cosine)` best first. Rows whose stored vector has
/// a different dimension than the query (an embedder change without a reindex)
/// are skipped rather than scored against a mismatched space.
async fn vector_channel(
    shared: &Database,
    query_embedding: &[f32],
    keys: &[String],
) -> Result<Vec<(i64, f64)>> {
    let mut hits: Vec<(i64, f64)> = Vec::new();
    for chunk in keys.chunks(MAX_IN_PARAMS) {
        let chunk: Vec<String> = chunk.to_vec();
        let rows: Vec<(i64, Vec<u8>)> = shared
            .read(move |conn| {
                let sql = format!(
                    "SELECT id, embedding FROM toolbox_tools \
                     WHERE tool_key IN ({}) AND embedding IS NOT NULL",
                    store::placeholders(chunk.len())
                );
                let mut stmt = conn.prepare(&sql)?;
                let refs: Vec<&dyn rusqlite::ToSql> =
                    chunk.iter().map(|k| k as &dyn rusqlite::ToSql).collect();
                let rows = stmt
                    .query_map(refs.as_slice(), |r| {
                        Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?;
        for (id, blob) in rows {
            let stored = embedding::from_blob(&blob);
            if let Some(sim) = embedding::cosine(query_embedding, &stored) {
                hits.push((id, sim));
            }
        }
    }
    hits.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    hits.truncate(CHANNEL_CANDIDATES);
    Ok(hits)
}

/// Optional second pass: cross-encode the fused hits and re-sort them.
///
/// kleos-lib cannot reach the server's reranker, so the handler passes it in.
/// The [`Reranker`] trait only speaks `memory::types::SearchResult`, so each hit
/// is wrapped in a synthetic one whose `memory.id` is its index in `results`
/// (the reranker re-sorts its slice, so position cannot be used to map back) and
/// whose content is the tool's name plus summary -- the same text the embedder
/// sees. `rerank_score` is the blended score the backend produced, which keeps
/// the reranked and non-reranked rows on one scale.
pub async fn rerank_find_results(
    reranker: &dyn Reranker,
    query: &str,
    results: &mut [FindResult],
) -> Result<()> {
    if results.is_empty() {
        return Ok(());
    }
    let mut candidates: Vec<crate::memory::types::SearchResult> = results
        .iter()
        .enumerate()
        .map(|(idx, r)| synthetic_result(idx as i64, r))
        .collect();

    reranker.rerank_results(query, &mut candidates).await?;

    for c in &candidates {
        let idx = c.memory.id as usize;
        if let Some(slot) = results.get_mut(idx) {
            slot.rerank_score = Some(c.score);
        }
    }
    results.sort_by(|a, b| {
        let sa = a.rerank_score.unwrap_or(a.score);
        let sb = b.rerank_score.unwrap_or(b.score);
        sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(())
}

/// Wrap one hit as the `SearchResult` the reranker trait expects. `Memory` has
/// no `Default`, so every field is spelled out; only `id` and `content` carry
/// meaning here.
fn synthetic_result(idx: i64, hit: &FindResult) -> crate::memory::types::SearchResult {
    use crate::memory::types::{Memory, SearchResult};
    let content = format!("{}\n{}", hit.tool.name, hit.tool.summary);
    let memory = Memory {
        id: idx,
        content,
        category: "toolbox".to_string(),
        source: "toolbox".to_string(),
        session_id: None,
        importance: 5,
        embedding: None,
        version: 1,
        is_latest: true,
        parent_memory_id: None,
        root_memory_id: None,
        source_count: 1,
        is_static: false,
        is_forgotten: false,
        is_archived: false,
        is_fact: false,
        is_decomposed: false,
        forget_after: None,
        forget_reason: None,
        model: None,
        recall_hits: 0,
        recall_misses: 0,
        adaptive_score: None,
        pagerank_score: None,
        last_accessed_at: None,
        access_count: 0,
        tags: None,
        episode_id: None,
        decay_score: None,
        confidence: 1.0,
        sync_id: None,
        status: "active".to_string(),
        user_id: 0,
        space_id: None,
        fsrs_stability: None,
        fsrs_difficulty: None,
        fsrs_storage_strength: None,
        fsrs_retrieval_strength: None,
        fsrs_learning_state: None,
        fsrs_reps: None,
        fsrs_lapses: None,
        fsrs_last_review_at: None,
        valence: None,
        arousal: None,
        dominant_emotion: None,
        created_at: hit.tool.created_at.clone(),
        updated_at: hit.tool.updated_at.clone(),
        is_superseded: false,
        is_consolidated: false,
        lang: None,
    };
    SearchResult {
        memory,
        score: hit.score,
        search_type: "toolbox".to_string(),
        decay_score: None,
        combined_score: None,
        semantic_score: None,
        fts_score: None,
        graph_score: None,
        personality_signal_score: None,
        temporal_boost: None,
        channels: None,
        question_type: None,
        reranked: None,
        reranker_ms: None,
        candidate_count: None,
        rrf_pre_boost: None,
        decay_factor: None,
        pr_boost: None,
        src_boost: None,
        stat_boost: None,
        contradiction: None,
        matching_chunk: None,
        linked: None,
        version_chain: None,
        ce_confidence: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fts_query_ors_tokens_and_drops_punctuation() {
        assert_eq!(
            fts_query("a-t-on un outil pour \"parser\" du JSON ?").as_deref(),
            Some("on OR un OR outil OR pour OR parser OR du OR json")
        );
    }

    #[test]
    fn fts_query_is_none_when_nothing_usable_remains() {
        assert!(fts_query("  ?! * \" ").is_none());
        assert!(fts_query("a b").is_none(), "single-char tokens are dropped");
    }
}
