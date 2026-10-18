//! The dedup pass must never retire a memory that is served in favour of a copy
//! that is not. An archived memory (every growth observation is born archived),
//! an old version or a memory awaiting review cannot be a keeper, and on equal
//! importance the newer of two reformulations is the one kept. Archiving a
//! keeper later serves its retired memory again; forgetting it does not.

use kleos_lib::db::Database;
use kleos_lib::intelligence::duplicates::{deduplicate, find_duplicates};
use kleos_lib::memory;
use kleos_lib::memory::types::StoreRequest;
use rusqlite::params;

const USER: i64 = 1;
const THRESHOLD: f64 = 0.9;

async fn store(db: &Database, content: &str, importance: i32) -> i64 {
    let req = StoreRequest {
        content: content.to_string(),
        category: "discovery".to_string(),
        source: "test".to_string(),
        importance,
        tags: None,
        embedding: None,
        chunk_embeddings: None,
        session_id: None,
        is_static: Some(false),
        user_id: Some(USER),
        space_id: None,
        space: None,
        parent_memory_id: None,
        sync_id: None,
        artifacts: None,
        created_at: None,
    };
    memory::store(db, req, None, false)
        .await
        .expect("store memory")
        .id
}

async fn archive(db: &Database, id: i64) {
    db.write(move |conn| {
        conn.execute(
            "UPDATE memories SET is_archived = 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    })
    .await
    .expect("archive");
}

async fn link_similar(db: &Database, a: i64, b: i64) {
    db.write(move |conn| {
        conn.execute(
            "INSERT OR IGNORE INTO memory_links (source_id, target_id, similarity, type) \
             VALUES (?1, ?2, 0.97, 'similarity')",
            params![a, b],
        )?;
        Ok(())
    })
    .await
    .expect("link similar");
}

async fn is_superseded(db: &Database, id: i64) -> bool {
    db.read(move |conn| {
        Ok(conn.query_row(
            "SELECT is_superseded FROM memories WHERE id = ?1",
            params![id],
            |r| r.get::<_, i64>(0),
        )? == 1)
    })
    .await
    .expect("read is_superseded")
}

async fn supersedes_links(db: &Database) -> Vec<(i64, i64)> {
    db.read(|conn| {
        let mut stmt = conn.prepare(
            "SELECT source_id, target_id FROM memory_links WHERE type = 'supersedes' ORDER BY source_id",
        )?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .await
    .expect("read supersedes links")
}

#[tokio::test]
async fn archived_copy_never_retires_a_served_memory() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let served = store(&db, "lantern quartz harbour served original", 7).await;
    let draft = store(&db, "lantern quartz harbour archived draft", 7).await;
    archive(&db, draft).await;
    link_similar(&db, draft, served).await;

    let result = deduplicate(&db, USER, THRESHOLD, false).await.expect("dedup");

    assert_eq!(result.merged, 0);
    assert!(!is_superseded(&db, served).await, "served memory was retired by an archived copy");
    assert!(supersedes_links(&db).await.is_empty());
}

#[tokio::test]
async fn archived_copy_with_higher_importance_is_still_ignored() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let served = store(&db, "copper meadow falcon served original", 5).await;
    let draft = store(&db, "copper meadow falcon archived draft", 9).await;
    archive(&db, draft).await;
    link_similar(&db, served, draft).await;

    deduplicate(&db, USER, THRESHOLD, false).await.expect("dedup");

    assert!(!is_superseded(&db, served).await);
    assert!(!is_superseded(&db, draft).await);
}

#[tokio::test]
async fn find_duplicates_skips_pairs_with_an_archived_member() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let a = store(&db, "violet canyon ember first", 7).await;
    let b = store(&db, "violet canyon ember second", 7).await;
    let c = store(&db, "violet canyon ember third", 7).await;
    let d = store(&db, "violet canyon ember fourth", 7).await;
    archive(&db, b).await;
    archive(&db, c).await;
    archive(&db, d).await;
    link_similar(&db, a, b).await;
    link_similar(&db, c, d).await;

    let pairs = find_duplicates(&db, USER, THRESHOLD, 50).await.expect("find");
    assert!(pairs.is_empty(), "archived members leaked into pairs: {pairs:?}");

    let dry = deduplicate(&db, USER, THRESHOLD, true).await.expect("dry run");
    assert_eq!(dry.pairs_found, 0);
}

#[tokio::test]
async fn equal_importance_keeps_the_newer_memory() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let older = store(&db, "saffron glacier orbit first wording", 7).await;
    let newer = store(&db, "saffron glacier orbit refined wording", 7).await;
    assert!(newer > older);
    link_similar(&db, older, newer).await;

    let result = deduplicate(&db, USER, THRESHOLD, false).await.expect("dedup");

    assert_eq!(result.merged, 1);
    assert!(is_superseded(&db, older).await, "older reformulation should be retired");
    assert!(!is_superseded(&db, newer).await, "newer reformulation should be kept");
    assert_eq!(supersedes_links(&db).await, vec![(newer, older)]);
}

#[tokio::test]
async fn higher_importance_wins_over_recency() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let older = store(&db, "indigo turbine willow important", 9).await;
    let newer = store(&db, "indigo turbine willow minor", 5).await;
    link_similar(&db, newer, older).await;

    deduplicate(&db, USER, THRESHOLD, false).await.expect("dedup");

    assert!(!is_superseded(&db, older).await);
    assert!(is_superseded(&db, newer).await);
    assert_eq!(supersedes_links(&db).await, vec![(older, newer)]);
}

#[tokio::test]
async fn rerun_is_a_no_op() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let older = store(&db, "amber lighthouse sparrow first", 7).await;
    let newer = store(&db, "amber lighthouse sparrow second", 7).await;
    link_similar(&db, older, newer).await;

    deduplicate(&db, USER, THRESHOLD, false).await.expect("first run");
    let second = deduplicate(&db, USER, THRESHOLD, false).await.expect("second run");

    assert_eq!(second.merged, 0);
    assert_eq!(supersedes_links(&db).await.len(), 1);
}

async fn set_column(db: &Database, id: i64, assignment: &'static str) {
    db.write(move |conn| {
        conn.execute(&format!("UPDATE memories SET {assignment} WHERE id = ?1"), params![id])?;
        Ok(())
    })
    .await
    .expect("set column");
}

#[tokio::test]
async fn old_version_never_becomes_a_keeper() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let served = store(&db, "copper meadow falcon served", 5).await;
    let old_version = store(&db, "copper meadow falcon old version", 9).await;
    set_column(&db, old_version, "is_latest = 0").await;
    link_similar(&db, old_version, served).await;

    let result = deduplicate(&db, USER, THRESHOLD, false).await.expect("dedup");

    assert_eq!(result.merged, 0);
    assert!(!is_superseded(&db, served).await);
}

#[tokio::test]
async fn pending_memory_never_becomes_a_keeper() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let served = store(&db, "velvet canyon otter served", 5).await;
    let pending = store(&db, "velvet canyon otter awaiting review", 9).await;
    set_column(&db, pending, "status = 'pending'").await;
    link_similar(&db, pending, served).await;

    let result = deduplicate(&db, USER, THRESHOLD, false).await.expect("dedup");

    assert_eq!(result.merged, 0);
    assert!(!is_superseded(&db, served).await);
}

#[tokio::test]
async fn archiving_the_keeper_serves_the_retired_memory_again() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let retired = store(&db, "granite orchard heron first", 5).await;
    let keeper = store(&db, "granite orchard heron second", 5).await;
    link_similar(&db, retired, keeper).await;
    deduplicate(&db, USER, THRESHOLD, false).await.expect("dedup");
    assert!(is_superseded(&db, retired).await);

    memory::mark_archived(&db, keeper, USER).await.expect("archive keeper");

    assert!(!is_superseded(&db, retired).await, "archiving the keeper must serve its target again");
    assert_eq!(supersedes_links(&db).await, vec![(keeper, retired)], "the link is kept");
}

#[tokio::test]
async fn forgetting_the_keeper_keeps_the_retired_memory_hidden() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let retired = store(&db, "saffron glacier lynx first", 5).await;
    let keeper = store(&db, "saffron glacier lynx second", 5).await;
    link_similar(&db, retired, keeper).await;
    deduplicate(&db, USER, THRESHOLD, false).await.expect("dedup");

    memory::mark_forgotten(&db, keeper, USER).await.expect("forget keeper");
    memory::mark_archived(&db, keeper, USER).await.expect("archive forgotten keeper");

    assert!(is_superseded(&db, retired).await);
}
