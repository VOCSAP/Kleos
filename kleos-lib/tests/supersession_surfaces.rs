//! A memory marked `is_superseded = 1` must never reach an agent, whatever the
//! read surface: hybrid search (and the linked/version payload it carries), the
//! facts channel, assembled context, `/pack`, prompt generation, graph search and
//! tag search.
//!
//! Every test follows the review-gate shape: seed one live memory and one
//! superseded memory with distinct tokens, assert the live one is served and the
//! superseded one is withheld, then clear the mark and assert it is served again
//! (the predicate is a gate, not a blanket exclusion). The mark is written the way
//! `intelligence::duplicates::deduplicate` writes it: the flag plus a `supersedes`
//! link from the keeper to the retired row.

use kleos_lib::context::{assemble_context, ContextOptions};
use kleos_lib::db::Database;
use kleos_lib::graph;
use kleos_lib::memory;
use kleos_lib::memory::facts_channel::search_facts_fts;
use kleos_lib::memory::fts::fts_or_match_query;
use kleos_lib::memory::search::hybrid_search;
use kleos_lib::memory::types::{SearchRequest, StoreRequest};
use kleos_lib::pack::{self, PackFormat};
use kleos_lib::prompts;
use rusqlite::params;

const USER: i64 = 1;

fn store_req(
    content: &str,
    is_static: bool,
    importance: i32,
    tags: Option<Vec<String>>,
    parent: Option<i64>,
) -> StoreRequest {
    StoreRequest {
        content: content.to_string(),
        category: "decision".to_string(),
        source: "test".to_string(),
        importance,
        tags,
        embedding: None,
        chunk_embeddings: None,
        session_id: None,
        is_static: Some(is_static),
        user_id: Some(USER),
        space_id: None,
        space: None,
        parent_memory_id: parent,
        sync_id: None,
        artifacts: None,
        created_at: None,
    }
}

async fn store(db: &Database, content: &str, is_static: bool, importance: i32) -> i64 {
    store_tagged(db, content, is_static, importance, None).await
}

async fn store_tagged(
    db: &Database,
    content: &str,
    is_static: bool,
    importance: i32,
    tags: Option<Vec<String>>,
) -> i64 {
    memory::store(db, store_req(content, is_static, importance, tags, None), None, false)
        .await
        .expect("store memory")
        .id
}

async fn mark_superseded(db: &Database, keeper: i64, retired: i64) {
    db.write(move |conn| {
        conn.execute(
            "UPDATE memories SET is_superseded = 1 WHERE id = ?1",
            params![retired],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO memory_links (source_id, target_id, similarity, type) \
             VALUES (?1, ?2, 0.95, 'supersedes')",
            params![keeper, retired],
        )?;
        Ok(())
    })
    .await
    .expect("mark superseded");
}

async fn clear_superseded(db: &Database, retired: i64) {
    db.write(move |conn| {
        conn.execute(
            "UPDATE memories SET is_superseded = 0 WHERE id = ?1",
            params![retired],
        )?;
        Ok(())
    })
    .await
    .expect("clear superseded");
}

async fn insert_fact(db: &Database, memory_id: i64, subject: &str, object: &str) {
    let (subject, object) = (subject.to_string(), object.to_string());
    db.write(move |conn| {
        conn.execute(
            "INSERT INTO structured_facts (memory_id, subject, predicate, object, verb, user_id) \
             VALUES (?1, ?2, 'compressedWith', ?3, 'is', ?4)",
            params![memory_id, subject, object, USER],
        )?;
        Ok(())
    })
    .await
    .expect("insert structured_fact");
}

/// The whole serialized search output, so text carried by `linked` or
/// `version_chain` counts as served, not only the top-level memories.
async fn search_dump(db: &Database, query: &str) -> (Vec<i64>, String) {
    let results = hybrid_search(
        db,
        SearchRequest {
            query: query.to_string(),
            user_id: Some(USER),
            limit: Some(20),
            include_links: true,
            ..Default::default()
        },
    )
    .await
    .expect("search");
    let ids = results.iter().map(|r| r.memory.id).collect();
    (ids, serde_json::to_string(&results).expect("serialize results"))
}

async fn assemble(db: &Database, query: &str) -> String {
    assemble_context(
        db,
        ContextOptions {
            query: query.to_string(),
            include_static: Some(true),
            include_recent: Some(true),
            min_relevance: Some(0.0),
            token_budget: Some(8000),
            ..Default::default()
        },
        USER,
        None,
        None,
        None,
    )
    .await
    .expect("assemble context")
    .context
}

#[tokio::test]
async fn hybrid_search_withholds_superseded_memory() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let live = store(&db, "archive compression for orvane uses zstd level nineteen", false, 5).await;
    let retired = store(&db, "archive compression for orvane uses gzip with quillfeather", false, 5).await;
    mark_superseded(&db, live, retired).await;

    let (ids, dump) = search_dump(&db, "archive compression orvane").await;
    assert!(ids.contains(&live), "live memory must surface (ids: {ids:?})");
    assert!(!ids.contains(&retired), "superseded memory must not surface (ids: {ids:?})");
    assert!(
        !dump.contains("quillfeather"),
        "superseded text must not ride along in linked or version payloads"
    );

    clear_superseded(&db, retired).await;
    let (ids, _) = search_dump(&db, "archive compression orvane").await;
    assert!(ids.contains(&retired), "memory must surface once the mark is cleared (ids: {ids:?})");
}

#[tokio::test]
async fn graph_neighbour_of_a_live_hit_withholds_superseded_text() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let live = store(&db, "release train for brackmoor ships every tuesday", false, 5).await;
    let retired = store(&db, "cadence decided long ago was marrowgate fortnightly", false, 5).await;
    mark_superseded(&db, live, retired).await;

    let (ids, dump) = search_dump(&db, "brackmoor release train").await;
    assert!(ids.contains(&live), "live memory must surface (ids: {ids:?})");
    assert!(
        !dump.contains("marrowgate"),
        "superseded memory must not be served through graph expansion or links: {dump}"
    );

    clear_superseded(&db, retired).await;
    let (_, dump) = search_dump(&db, "brackmoor release train").await;
    assert!(
        dump.contains("marrowgate"),
        "the linked memory must be served once the mark is cleared"
    );
}

#[tokio::test]
async fn facts_channel_withholds_superseded_parent() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let live = store(&db, "storage notes for the tessaly backups", false, 5).await;
    let retired = store(&db, "older storage notes for the tessaly archive", false, 5).await;
    insert_fact(&db, live, "Tessaly", "Zstd").await;
    insert_fact(&db, retired, "Tessalyold", "Gzip").await;
    mark_superseded(&db, live, retired).await;

    let hits = search_facts_fts(&db, &fts_or_match_query("Tessalyold compressed"), USER, 10)
        .await
        .expect("facts search");
    assert!(
        !hits.iter().any(|h| h.memory_id == retired),
        "a fact of a superseded memory must not surface it: {hits:?}"
    );

    clear_superseded(&db, retired).await;
    let hits = search_facts_fts(&db, &fts_or_match_query("Tessalyold compressed"), USER, 10)
        .await
        .expect("facts search");
    assert!(
        hits.iter().any(|h| h.memory_id == retired),
        "the fact must surface its memory once the mark is cleared: {hits:?}"
    );
}

#[tokio::test]
async fn assembled_context_withholds_superseded_static_memory() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let live = store(&db, "the production database engine is codenamed brightwater", true, 9).await;
    let retired = store(&db, "the production database engine is codenamed thornfield", true, 9).await;
    mark_superseded(&db, live, retired).await;

    let content = assemble(&db, "production database engine").await;
    assert!(content.contains("brightwater"), "live static must be injected (context: {content})");
    assert!(
        !content.contains("thornfield"),
        "superseded static must not be injected (context: {content})"
    );

    clear_superseded(&db, retired).await;
    let content = assemble(&db, "production database engine").await;
    assert!(content.contains("thornfield"), "memory must be injected once the mark is cleared");
}

#[tokio::test]
async fn assembled_context_withholds_superseded_recent_memory() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let live = store(&db, "the nightly export now lands in quarterdeck", false, 5).await;
    let retired = store(&db, "the nightly export now lands in millbrook", false, 5).await;
    mark_superseded(&db, live, retired).await;

    let content = assemble(&db, "nightly export").await;
    assert!(content.contains("quarterdeck"), "live recent memory must be injected (context: {content})");
    assert!(
        !content.contains("millbrook"),
        "superseded recent memory must not be injected (context: {content})"
    );

    clear_superseded(&db, retired).await;
    let content = assemble(&db, "nightly export").await;
    assert!(content.contains("millbrook"), "memory must be injected once the mark is cleared");
}

#[tokio::test]
async fn pack_withholds_superseded_static() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let live = store(&db, "static decision brightwater", true, 9).await;
    let retired = store(&db, "static decision thornfield", true, 9).await;
    mark_superseded(&db, live, retired).await;

    let packed = pack::pack_memories(&db, "", 8000, PackFormat::Text, USER)
        .await
        .expect("pack")
        .packed;
    assert!(packed.contains("brightwater"), "live static is packed");
    assert!(!packed.contains("thornfield"), "superseded static must not be packed");

    clear_superseded(&db, retired).await;
    let packed = pack::pack_memories(&db, "", 8000, PackFormat::Text, USER)
        .await
        .expect("pack")
        .packed;
    assert!(packed.contains("thornfield"), "memory must be packed once the mark is cleared");
}

#[tokio::test]
async fn generate_prompt_withholds_superseded_static() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let live = store(&db, "prompt decision brightwater", true, 9).await;
    let retired = store(&db, "prompt decision thornfield", true, 9).await;
    mark_superseded(&db, live, retired).await;

    let prompt = prompts::generate_prompt(&db, "text", 8000, "", USER)
        .await
        .expect("generate prompt")
        .prompt;
    assert!(prompt.contains("brightwater"), "live static in prompt");
    assert!(!prompt.contains("thornfield"), "superseded static must not appear in the prompt");

    clear_superseded(&db, retired).await;
    let prompt = prompts::generate_prompt(&db, "text", 8000, "", USER)
        .await
        .expect("generate prompt")
        .prompt;
    assert!(prompt.contains("thornfield"), "memory must appear once the mark is cleared");
}

#[tokio::test]
async fn graph_search_withholds_superseded() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let live = store(&db, "graph decision brightwater", false, 5).await;
    let retired = store(&db, "graph decision thornfield", false, 5).await;
    mark_superseded(&db, live, retired).await;

    let nodes = graph::search::graph_search(&db, "graph decision", 50, USER)
        .await
        .expect("graph search");
    let contents: Vec<&str> = nodes.iter().map(|n| n.content.as_str()).collect();
    assert!(contents.iter().any(|c| c.contains("brightwater")), "live node present");
    assert!(
        !contents.iter().any(|c| c.contains("thornfield")),
        "superseded node must be withheld from graph search"
    );

    clear_superseded(&db, retired).await;
    let nodes = graph::search::graph_search(&db, "graph decision", 50, USER)
        .await
        .expect("graph search");
    assert!(
        nodes.iter().any(|n| n.content.contains("thornfield")),
        "node must be present once the mark is cleared"
    );
}

#[tokio::test]
async fn tag_search_withholds_superseded() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let tag = vec!["compression".to_string()];
    let live = store_tagged(&db, "tagged brightwater", false, 5, Some(tag.clone())).await;
    let retired = store_tagged(&db, "tagged thornfield", false, 5, Some(tag.clone())).await;
    mark_superseded(&db, live, retired).await;

    for match_all in [true, false] {
        let hits = memory::search_by_tags(&db, USER, &tag, match_all, 50)
            .await
            .expect("tag search");
        assert!(hits.iter().any(|m| m.content.contains("brightwater")), "live memory found");
        assert!(
            !hits.iter().any(|m| m.content.contains("thornfield")),
            "superseded memory must be withheld from tag search (match_all={match_all})"
        );
    }

    clear_superseded(&db, retired).await;
    let hits = memory::search_by_tags(&db, USER, &tag, true, 50)
        .await
        .expect("tag search");
    assert!(hits.iter().any(|m| m.content.contains("thornfield")), "found once the mark is cleared");
}

/// The Evolution phase of assembled context renders the version chain of each
/// semantic hit (non-static, depth >= 2). An older version may be named, but its
/// text must not be injected next to the current one.
#[tokio::test]
async fn assembled_context_does_not_inject_text_of_older_versions() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let first = memory::store(
        &db,
        store_req("the cache layer for veloria runs on hollowmere", false, 5, None, None),
        None,
        false,
    )
    .await
    .expect("store v1")
    .id;
    memory::store(
        &db,
        store_req("the cache layer for veloria runs on silverkeep", false, 5, None, Some(first)),
        None,
        false,
    )
    .await
    .expect("store v2");

    let content = assemble(&db, "veloria cache layer").await;
    assert!(content.contains("silverkeep"), "current version must be injected (context: {content})");
    assert!(
        !content.contains("hollowmere"),
        "text of an older version must not be injected (context: {content})"
    );
}
