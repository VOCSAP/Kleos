//! Regression: the monolith drain must preserve user_id so drained rows stay
//! visible to their owner after they land in the tenant shard.

mod common;

use common::{bootstrap_admin_key, post, seed_user, test_app_with_sharding};
use kleos_lib::memory::types::StoreRequest;
use serde_json::json;

/// A memory drained from the monolith into a user's shard must carry the real
/// owner user_id. Before the fix `MONOLITH_DRAIN_COLUMNS` omitted user_id, so
/// shard rows defaulted to user_id = 0 and became invisible to the user_id
/// scoped reads that every read path applies.
#[tokio::test]
async fn drain_preserves_user_id_in_shard() {
    let (app, state, _tmp) = test_app_with_sharding().await;
    let admin_key = bootstrap_admin_key(&app).await;
    let (uid, _key) = seed_user(&app, &admin_key, "drainee").await;

    // Seed a memory directly into the monolith (state.db) as if it predates
    // sharding; a normal /store in sharded mode targets the shard instead.
    let content = "monolith-only memory to be drained zzq";
    let req = StoreRequest {
        content: content.to_string(),
        category: "general".to_string(),
        source: "test".to_string(),
        importance: 5,
        user_id: Some(uid),
        ..Default::default()
    };
    kleos_lib::memory::store(&state.db, req, None, false)
        .await
        .expect("seed monolith memory");

    // Drain the monolith into tenant shards.
    let (status, body) = post(&app, "/admin/monolith/drain", &admin_key, json!({})).await;
    assert!(status.is_success(), "drain failed: {status}: {body}");

    // The drained row must be in uid's shard WITH user_id = uid.
    let shard = kleos_server::extractors::resolve_db_for_user(&state, uid)
        .await
        .expect("resolve shard");
    let content_owned = content.to_string();
    let found: i64 = shard
        .read(move |conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM memories WHERE content = ?1 AND user_id = ?2",
                rusqlite::params![content_owned, uid],
                |r| r.get(0),
            )?)
        })
        .await
        .expect("read shard");
    assert_eq!(
        found, 1,
        "drained memory must be present in the owner's shard with the correct user_id"
    );
}
