//! Contradiction detection -- find memories that contradict each other
//! using SVO (subject-verb-object) triple matching from structured_facts.

use super::types::Contradiction;
use crate::db::Database;
use crate::memory::types::Memory;
use crate::Result;
use rusqlite::params;
use tracing::{info, warn};

/// Detect contradictions between a new memory and existing facts.
///
/// Extracts SVO triples from the memory's structured_facts and compares
/// against existing facts with the same subject+predicate. If the object
/// differs, flags as a contradiction.
#[tracing::instrument(skip(db, memory), fields(memory_id = memory.id, user_id = memory.user_id))]
pub async fn detect_contradictions(db: &Database, memory: &Memory) -> Result<Vec<Contradiction>> {
    let memory_id = memory.id;
    let user_id = memory.user_id;

    // Get structured facts for this memory (tenant-scoped)
    let new_facts: Vec<(i64, String, String, String, f64)> = db
        .read(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, subject, predicate, object, confidence \
                     FROM structured_facts \
                     WHERE memory_id = ?1 AND user_id = ?2",
            )?;
            let rows = stmt
                .query_map(params![memory_id, user_id], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, f64>(4)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?;

    let mut contradictions = Vec::new();

    // For each new fact, check against existing facts with same subject+predicate
    // SECURITY: scoped to memory.user_id to prevent cross-tenant fact leakage.
    for (new_fact_id, subject, predicate, new_object, _confidence) in &new_facts {
        let subject_c = subject.clone();
        let predicate_c = predicate.clone();
        let nfid = *new_fact_id;

        let existing: Vec<(i64, String, i64, f64)> = db
            .read(move |conn| {
                let mut stmt = conn.prepare(
                    // Patch 37.1 -- partition single-path detection by the
                    // new memory's space_id so online contradiction checks
                    // never cross spaces. Sibling of Patch 37 which covered
                    // scan_all_contradictions + detect_fact_contradictions
                    // but missed this fn. Same JOIN shape as temporal.rs:
                    // m_cand for the candidate fact's owner, m_new for the
                    // new memory bound via the existing ?3 placeholder.
                    // NULL legacy memories isolate naturally (NULL = NULL
                    // is false in SQL) -- safe-by-default during the
                    // Patch 33 transition window.
                    // Patch 38.3 -- also exclude soft-deleted (is_forgotten)
                    // memories, aligning on the discipline of every other
                    // intelligence pass (duplicates.rs, temporal.rs,
                    // consolidation.rs); this fn was the lone exception.
                    "SELECT sf.id, sf.object, sf.memory_id, sf.confidence \
                         FROM structured_facts sf \
                         JOIN memories m_cand ON m_cand.id = sf.memory_id \
                         JOIN memories m_new ON m_new.id = ?3 \
                         WHERE sf.subject = ?1 AND sf.predicate = ?2 \
                           AND sf.memory_id != ?3 \
                           AND sf.id != ?4 \
                           AND (m_cand.space_id = m_new.space_id \
                                OR (m_cand.space_id IS NULL AND m_new.space_id IS NULL)) \
                           AND m_cand.is_forgotten = 0 \
                           AND m_new.is_forgotten = 0 \
                           AND sf.user_id = ?5 \
                         ORDER BY sf.confidence DESC",
                )?;
                let rows = stmt
                    .query_map(
                        params![subject_c, predicate_c, memory_id, nfid, user_id],
                        |row| {
                            Ok((
                                row.get::<_, i64>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, i64>(2)?,
                                row.get::<_, f64>(3)?,
                            ))
                        },
                    )?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await?;

        for (_old_fact_id, old_object, old_memory_id, old_confidence) in existing {
            // Guard the RAW stored confidence before any arithmetic:
            // structured_facts.confidence has no write-side validation, and
            // f32::min laundering (NAN.min(1.0) == 1.0, INFINITY.min(1.0)
            // == 1.0) would otherwise turn a degenerate stored value into a
            // full-confidence contradiction downstream.
            if !old_confidence.is_finite() || old_confidence <= 0.0 {
                continue;
            }
            // Compare objects -- if they differ, it's a contradiction
            if !objects_match(new_object, &old_object) {
                let conf = old_confidence as f32 * 0.8; // Scale by old fact confidence

                contradictions.push(Contradiction {
                    memory_a: memory_id.to_string(),
                    memory_b: old_memory_id.to_string(),
                    confidence: conf.min(1.0),
                    description: format!(
                        "Conflicting {}: '{}' vs '{}' (subject: {}, predicate: {})",
                        predicate, new_object, old_object, subject, predicate
                    ),
                });
            }
        }
    }

    if !contradictions.is_empty() {
        info!(
            memory_id = memory_id,
            user_id = user_id,
            contradictions = contradictions.len(),
            "contradictions_detected"
        );

        // Record contradiction links in memory_links. Skip degenerate
        // confidence (<= 0 or NaN, both fail the `> 0.0` test): this write
        // bypasses insert_link, and a non-positive similarity row would feed
        // the PageRank edge weights that insert_link now guards against.
        for c in &contradictions {
            let mem_b_id: i64 = c.memory_b.parse().unwrap_or(0);
            let conf_f64 = c.confidence as f64;
            if mem_b_id > 0 && conf_f64 > 0.0 {
                if let Err(e) = db
                    .write(move |conn| {
                        conn.execute(
                            "INSERT OR IGNORE INTO memory_links \
                             (source_id, target_id, similarity, type) \
                             VALUES (?1, ?2, ?3, 'contradicts')",
                            params![memory_id, mem_b_id, conf_f64],
                        )?;
                        Ok(())
                    })
                    .await
                {
                    warn!(memory_id, mem_b_id, error = %e, "contradiction: failed to insert memory_links row");
                }
            }
        }
    }

    Ok(contradictions)
}

/// Scan all memories for internal contradictions.
///
/// Compares all structured_facts with the same subject+predicate to find
/// conflicting objects. Returns all detected contradictions.
#[allow(clippy::type_complexity)]
#[tracing::instrument(skip(db))]
pub async fn scan_all_contradictions(db: &Database, user_id: i64) -> Result<Vec<Contradiction>> {
    let rows: Vec<(i64, i64, String, String, String, String, f64, f64)> = db
        .read(move |conn| {
            let mut stmt = conn.prepare(
                // Patch 37 -- partition pair detection by space_id so
                // contradictions never cross spaces (aligns with the
                // discipline applied to duplicates/consolidation in
                // Patch 33 part 1). NULL legacy memories isolate
                // naturally because NULL = NULL evaluates to false in
                // SQL, so legacy rows neither merge with each other
                // nor with spaced rows -- safe-by-default in transition.
                // Patch 38.3 -- also exclude soft-deleted (is_forgotten)
                // memories on both sides, aligning on duplicates.rs:30.
                "SELECT sf1.memory_id, sf2.memory_id, \
                            sf1.subject, sf1.predicate, sf1.object, sf2.object, \
                            sf1.confidence, sf2.confidence \
                     FROM structured_facts sf1 \
                     JOIN structured_facts sf2 ON sf1.subject = sf2.subject \
                       AND sf1.predicate = sf2.predicate \
                       AND sf1.id < sf2.id \
                       AND sf1.memory_id != sf2.memory_id \
                     JOIN memories m1 ON m1.id = sf1.memory_id \
                     JOIN memories m2 ON m2.id = sf2.memory_id \
                       AND (m1.space_id = m2.space_id \
                            OR (m1.space_id IS NULL AND m2.space_id IS NULL)) \
                       AND m1.is_forgotten = 0 \
                       AND m2.is_forgotten = 0 \
                     WHERE sf1.user_id = ?1 AND sf2.user_id = ?1 \
                     LIMIT 500",
            )?;
            let rows = stmt
                .query_map(params![user_id], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, f64>(6)?,
                        row.get::<_, f64>(7)?,
                    ))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .await?;

    let mut contradictions = Vec::new();

    for (mem_a_id, mem_b_id, subject, predicate, object_a, object_b, conf_a, conf_b) in rows {
        if !objects_match(&object_a, &object_b) {
            let conf = (conf_a.min(conf_b) * 0.8) as f32;

            contradictions.push(Contradiction {
                memory_a: mem_a_id.to_string(),
                memory_b: mem_b_id.to_string(),
                confidence: conf.min(1.0),
                description: format!(
                    "Conflicting {}: '{}' vs '{}' (subject: {}, predicate: {})",
                    predicate, object_a, object_b, subject, predicate
                ),
            });
        }
    }

    info!(
        contradictions = contradictions.len(),
        "scan_all_contradictions_complete"
    );

    Ok(contradictions)
}

/// Compare two object strings for equivalence.
/// Handles case-insensitive comparison and minor whitespace differences.
fn objects_match(a: &str, b: &str) -> bool {
    let a_norm = a.trim().to_lowercase();
    let b_norm = b.trim().to_lowercase();
    a_norm == b_norm
}

/// Unit tests for object-comparison helpers and contradiction formatting.
#[cfg(test)]
mod tests {
    use super::*;

    /// objects_match treats two identical strings as a match.
    #[test]
    fn test_objects_match_identical() {
        assert!(objects_match("hello", "hello"));
    }

    /// objects_match is case-insensitive.
    #[test]
    fn test_objects_match_case_insensitive() {
        assert!(objects_match("Hello World", "hello world"));
    }

    /// objects_match trims surrounding whitespace before comparing.
    #[test]
    fn test_objects_match_whitespace() {
        assert!(objects_match("  hello  ", "hello"));
    }

    /// objects_match returns false for clearly different strings.
    #[test]
    fn test_objects_mismatch() {
        assert!(!objects_match("blue", "red"));
    }

    /// Verify the contradiction description string format matches the
    /// "Conflicting {predicate}: '{a}' vs '{b}' (subject: {s}, predicate: {p})"
    /// shape that downstream consumers parse.
    #[test]
    fn test_contradiction_description_format() {
        let c = Contradiction {
            memory_a: "1".to_string(),
            memory_b: "2".to_string(),
            confidence: 0.8,
            description: format!(
                "Conflicting {}: '{}' vs '{}' (subject: {}, predicate: {})",
                "prefers", "coffee", "tea", "user", "prefers"
            ),
        };
        assert!(c.description.contains("coffee"));
        assert!(c.description.contains("tea"));
    }

    // -------------------------------------------------------------------
    // Patch 37 / 37.1 regression: space_id partitioning must survive the
    // 156-commit 2026-08 upstream merge. read_isolation_a1.rs already
    // proves cross-USER isolation (two different user_id values); these
    // tests prove the orthogonal cross-SPACE isolation for a SINGLE user
    // (two different space_id values), which is the actual VOCSAP
    // behaviour Patch 37/37.1 added and the merge could silently drop.
    // -------------------------------------------------------------------

    /// Store a memory for `user_id` in `space_id` and return its id.
    async fn store_in_space(
        db: &crate::db::Database,
        user_id: i64,
        space_id: Option<i64>,
        content: &str,
    ) -> i64 {
        use crate::memory::types::StoreRequest;
        let req = StoreRequest {
            content: content.to_string(),
            category: "general".to_string(),
            source: "test".to_string(),
            importance: 5,
            tags: None,
            embedding: None,
            chunk_embeddings: None,
            session_id: None,
            is_static: Some(false),
            user_id: Some(user_id),
            space_id,
            space: None,
            parent_memory_id: None,
            sync_id: None,
            artifacts: None,
            created_at: None,
        };
        crate::memory::store(db, req, None, false)
            .await
            .expect("store memory")
            .id
    }

    /// Attach a subject/predicate/object fact to `memory_id`.
    async fn fact(
        db: &crate::db::Database,
        memory_id: i64,
        user_id: i64,
        subject: &str,
        predicate: &str,
        object: &str,
    ) {
        use crate::facts::{create_fact, CreateFactRequest};
        create_fact(
            db,
            CreateFactRequest {
                memory_id: Some(memory_id),
                subject: subject.to_string(),
                predicate: predicate.to_string(),
                object: object.to_string(),
                confidence: Some(0.9),
            },
            user_id,
        )
        .await
        .expect("create fact");
    }

    /// Same user, same space, contradicting facts -> detect_contradictions
    /// must fire. Positive control: proves the mechanism (and this test's
    /// own wiring) actually works, so the cross-space negative test below
    /// cannot be a false negative from unrelated breakage.
    ///
    /// Content for the two memories must be simhash-distinct: `memory::store`
    /// dedups near-identical content scoped to (user_id, space_id) -- see
    /// `same_space_near_duplicate_within_band_collapses` in
    /// kleos-lib/tests/store_dedup.rs. Two near-identical strings here would
    /// collapse to the SAME memory id, and detect_contradictions excludes a
    /// candidate fact when its memory_id equals the new memory's id -- which
    /// would silently zero out this positive control regardless of whether
    /// the space_id predicate under test is even reached.
    #[tokio::test]
    async fn detect_contradictions_fires_within_same_space() {
        let db = crate::db::Database::open_tenant_memory().await.unwrap();
        let a = store_in_space(
            &db,
            1,
            Some(100),
            "wireguard builds an encrypted tunnel using public key cryptography",
        )
        .await;
        let b = store_in_space(
            &db,
            1,
            Some(100),
            "postgres uses a write ahead log for durability of committed transactions",
        )
        .await;
        assert_ne!(a, b, "test fixture bug: the two memories deduped into one");
        fact(&db, a, 1, "sky", "color", "blue").await;
        fact(&db, b, 1, "sky", "color", "green").await;

        let memory_a = crate::memory::get(&db, a, 1).await.expect("load memory a");
        let contradictions = detect_contradictions(&db, &memory_a).await.unwrap();
        assert_eq!(
            contradictions.len(),
            1,
            "same-space contradicting facts must be detected"
        );
    }

    /// Same user, DIFFERENT space, contradicting facts -> detect_contradictions
    /// must NOT fire. Guards the Patch 37.1 `m_cand.space_id = m_new.space_id`
    /// predicate (contradiction.rs online path).
    #[tokio::test]
    async fn detect_contradictions_does_not_cross_space() {
        let db = crate::db::Database::open_tenant_memory().await.unwrap();
        let a = store_in_space(
            &db,
            1,
            Some(100),
            "wireguard builds an encrypted tunnel using public key cryptography",
        )
        .await;
        let b = store_in_space(
            &db,
            1,
            Some(200),
            "postgres uses a write ahead log for durability of committed transactions",
        )
        .await;
        assert_ne!(a, b, "test fixture bug: the two memories deduped into one");
        fact(&db, a, 1, "sky", "color", "blue").await;
        fact(&db, b, 1, "sky", "color", "green").await;

        let memory_a = crate::memory::get(&db, a, 1).await.expect("load memory a");
        let contradictions = detect_contradictions(&db, &memory_a).await.unwrap();
        assert!(
            contradictions.is_empty(),
            "same-user cross-space facts must NOT be flagged as a contradiction, got {contradictions:?}"
        );
    }

    /// Same user, same space -> scan_all_contradictions must fire.
    /// Positive control for the batch path (its own separate SQL query).
    #[tokio::test]
    async fn scan_all_contradictions_fires_within_same_space() {
        let db = crate::db::Database::open_tenant_memory().await.unwrap();
        let a = store_in_space(
            &db,
            1,
            Some(100),
            "wireguard builds an encrypted tunnel using public key cryptography",
        )
        .await;
        let b = store_in_space(
            &db,
            1,
            Some(100),
            "postgres uses a write ahead log for durability of committed transactions",
        )
        .await;
        assert_ne!(a, b, "test fixture bug: the two memories deduped into one");
        fact(&db, a, 1, "sky", "color", "blue").await;
        fact(&db, b, 1, "sky", "color", "green").await;

        let contradictions = scan_all_contradictions(&db, 1).await.unwrap();
        assert_eq!(
            contradictions.len(),
            1,
            "same-space contradicting facts must be detected by the batch scan"
        );
    }

    /// Same user, DIFFERENT space -> scan_all_contradictions must NOT fire.
    /// Guards the Patch 37 `m1.space_id = m2.space_id` predicate (batch path).
    #[tokio::test]
    async fn scan_all_contradictions_does_not_cross_space() {
        let db = crate::db::Database::open_tenant_memory().await.unwrap();
        let a = store_in_space(
            &db,
            1,
            Some(100),
            "wireguard builds an encrypted tunnel using public key cryptography",
        )
        .await;
        let b = store_in_space(
            &db,
            1,
            Some(200),
            "postgres uses a write ahead log for durability of committed transactions",
        )
        .await;
        assert_ne!(a, b, "test fixture bug: the two memories deduped into one");
        fact(&db, a, 1, "sky", "color", "blue").await;
        fact(&db, b, 1, "sky", "color", "green").await;

        let contradictions = scan_all_contradictions(&db, 1).await.unwrap();
        assert!(
            contradictions.is_empty(),
            "same-user cross-space facts must NOT be flagged by the batch scan, got {contradictions:?}"
        );
    }
}
