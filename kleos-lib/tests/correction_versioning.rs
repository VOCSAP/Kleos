//! A correction is a new version of the memory it corrects: the original stops
//! being the latest version and the corrected text is chained to it, the same
//! way `memory::update` chains an edit.

use kleos_lib::db::Database;
use kleos_lib::embeddings::EmbeddingProvider;
use kleos_lib::intelligence::correction::{correct_memory, correct_memory_with_embedder};
use kleos_lib::memory;
use kleos_lib::memory::types::StoreRequest;
use rusqlite::params;

const USER: i64 = 1;
const DIM: usize = 1024;

fn unit(axis: usize) -> Vec<f32> {
    let mut vector = vec![0.0; DIM];
    vector[axis] = 1.0;
    vector
}

struct AxisEmbedder(usize);

impl EmbeddingProvider for AxisEmbedder {
    fn embed<'a>(
        &'a self,
        _text: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = kleos_lib::Result<Vec<f32>>> + Send + 'a>>
    {
        Box::pin(async move { Ok(unit(self.0)) })
    }
}

async fn store_original(db: &Database, content: &str, embedding: Option<Vec<f32>>) -> i64 {
    memory::store(
        db,
        StoreRequest {
            content: content.to_string(),
            category: "decision".to_string(),
            source: "test".to_string(),
            importance: 8,
            tags: Some(vec!["ports".to_string()]),
            embedding,
            is_static: Some(true),
            user_id: Some(USER),
            ..Default::default()
        },
        None,
        false,
    )
    .await
    .expect("store original")
    .id
}

#[derive(Debug)]
struct Row {
    version: i64,
    is_latest: bool,
    is_superseded: bool,
    parent: Option<i64>,
    root: Option<i64>,
    category: String,
    importance: i64,
    tags: Option<String>,
    is_static: bool,
    space_id: Option<i64>,
    first_float: Option<f32>,
}

async fn row(db: &Database, id: i64) -> Row {
    db.read(move |conn| {
        Ok(conn.query_row(
            "SELECT version, is_latest, is_superseded, parent_memory_id, root_memory_id, \
             category, importance, tags, is_static, space_id, embedding_vec_1024 \
             FROM memories WHERE id = ?1",
            params![id],
            |r| {
                let blob: Option<Vec<u8>> = r.get(10)?;
                Ok(Row {
                    version: r.get(0)?,
                    is_latest: r.get(1)?,
                    is_superseded: r.get(2)?,
                    parent: r.get(3)?,
                    root: r.get(4)?,
                    category: r.get(5)?,
                    importance: r.get(6)?,
                    tags: r.get(7)?,
                    is_static: r.get(8)?,
                    space_id: r.get(9)?,
                    first_float: blob
                        .filter(|b| b.len() >= 4)
                        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
                })
            },
        )?)
    })
    .await
    .expect("read row")
}

async fn count(db: &Database, sql: &'static str, id: i64) -> i64 {
    db.read(move |conn| Ok(conn.query_row(sql, params![id], |r| r.get(0))?))
        .await
        .expect("count")
}

#[tokio::test]
async fn a_correction_is_the_next_version_of_the_original() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let original = store_original(&db, "the deploy port of halvard is 4100", None).await;

    let corrected = correct_memory(
        &db,
        USER,
        original,
        "the deploy port of halvard is 4200",
        None,
    )
    .await
    .expect("correct");

    let old = row(&db, original).await;
    let new = row(&db, corrected.id).await;
    assert!(!old.is_latest, "{old:?}");
    assert!(old.is_superseded, "{old:?}");
    assert_eq!(new.version, old.version + 1, "{new:?}");
    assert_eq!(new.parent, Some(original));
    assert_eq!(new.root, Some(original));
    assert!(new.is_latest && !new.is_superseded, "{new:?}");
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM memory_links WHERE target_id = ?1 AND type = 'supersedes'",
            original
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM reconsolidations WHERE memory_id = ?1",
            original
        )
        .await,
        1
    );
}

#[tokio::test]
async fn a_correction_keeps_the_original_metadata() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let original = store_original(&db, "the backup window of halvard is 02:00", None).await;

    let corrected = correct_memory(
        &db,
        USER,
        original,
        "the backup window of halvard is 03:00",
        None,
    )
    .await
    .expect("correct");

    let old = row(&db, original).await;
    let new = row(&db, corrected.id).await;
    assert_eq!(new.category, old.category);
    assert_eq!(new.importance, old.importance);
    assert_eq!(new.tags, old.tags);
    assert_eq!(new.is_static, old.is_static);
    assert_eq!(new.space_id, old.space_id);
}

#[tokio::test]
async fn a_second_correction_chains_on_the_latest_version() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let original = store_original(&db, "the queue of halvard is rabbit", None).await;
    let first = correct_memory(&db, USER, original, "the queue of halvard is nats", None)
        .await
        .expect("first correction");

    let second = correct_memory(
        &db,
        USER,
        first.id,
        "the queue of halvard is redpanda",
        None,
    )
    .await
    .expect("second correction");

    let middle = row(&db, first.id).await;
    let last = row(&db, second.id).await;
    assert!(!middle.is_latest && middle.is_superseded, "{middle:?}");
    assert_eq!(last.version, middle.version + 1);
    assert_eq!(last.parent, Some(first.id));
    assert_eq!(last.root, Some(original));
}

#[tokio::test]
async fn correcting_a_version_that_is_no_longer_latest_writes_nothing() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let original = store_original(&db, "the cache of halvard is memcached", None).await;
    correct_memory(&db, USER, original, "the cache of halvard is valkey", None)
        .await
        .expect("first correction");
    let rows_before = count(
        &db,
        "SELECT COUNT(*) FROM memories WHERE user_id = ?1",
        USER,
    )
    .await;

    let refused = correct_memory(
        &db,
        USER,
        original,
        "the cache of halvard is dragonfly",
        None,
    )
    .await;

    assert!(
        refused.is_err(),
        "correcting a superseded version must fail"
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM memories WHERE user_id = ?1",
            USER
        )
        .await,
        rows_before
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM memory_links WHERE target_id = ?1 AND type = 'supersedes'",
            original
        )
        .await,
        1
    );
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM reconsolidations WHERE memory_id = ?1",
            original
        )
        .await,
        1
    );
}

#[tokio::test]
async fn with_an_embedder_the_corrected_text_gets_its_own_vector() {
    let db = Database::connect_memory().await.expect("in-mem db");
    let original = store_original(&db, "the region of halvard is eu-west", Some(unit(1))).await;
    assert_eq!(row(&db, original).await.first_float, Some(0.0));

    let embedder = AxisEmbedder(0);
    let corrected = correct_memory_with_embedder(
        &db,
        Some(&embedder),
        USER,
        original,
        "the region of halvard is eu-central",
        None,
    )
    .await
    .expect("correct with embedder");

    assert_eq!(row(&db, corrected.id).await.first_float, Some(1.0));
}
