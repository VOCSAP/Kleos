//! Integration test for GET /stats under tenant sharding.

mod common;

use axum::http::StatusCode;
use common::{bootstrap_admin_key, get, test_app_with_sharding};
use kleos_lib::db::Database;

async fn insert_memories(db: &Database, n: usize) {
    db.write(move |conn| {
        for i in 0..n {
            conn.execute(
                "INSERT INTO memories (content) VALUES (?1)",
                rusqlite::params![format!("memory {i}")],
            )?;
        }
        Ok(())
    })
    .await
    .expect("insert memories");
}

#[tokio::test]
async fn stats_counts_memories_of_the_caller_tenant_shard() {
    let (app, state, _tmp) = test_app_with_sharding().await;
    let admin_key = bootstrap_admin_key(&app).await;

    let tenant = state
        .tenant_registry
        .as_ref()
        .expect("sharding enabled")
        .get_or_create("1")
        .await
        .expect("tenant shard");
    insert_memories(&tenant.database(), 3).await;
    insert_memories(&state.db, 5).await;

    let (status, body) = get(&app, "/stats", &admin_key).await;
    assert_eq!(status, StatusCode::OK, "stats failed: {body}");
    assert_eq!(
        body["memories"], 3,
        "stats must count the tenant shard: {body}"
    );
}
