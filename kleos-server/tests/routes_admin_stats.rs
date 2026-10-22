//! Integration tests for GET /stats, sharded and monolith.

mod common;

use axum::http::StatusCode;
use axum::Router;
use common::{bootstrap_admin_key, get, post, test_app, test_app_with_sharding};
use kleos_lib::db::Database;
use serde_json::json;

async fn insert_rows(db: &Database, n: usize) {
    db.write(move |conn| {
        for i in 0..n {
            conn.execute(
                "INSERT INTO memories (content) VALUES (?1)",
                rusqlite::params![format!("memory {i}")],
            )?;
            conn.execute(
                "INSERT INTO tasks (title, user_id) VALUES (?1, 1)",
                rusqlite::params![format!("task {i}")],
            )?;
            conn.execute(
                "INSERT INTO agents (name, user_id) VALUES (?1, 1)",
                rusqlite::params![format!("agent {i}")],
            )?;
        }
        Ok(())
    })
    .await
    .expect("insert rows");
}

/// Stores five memories through POST /store, then archives one and forgets
/// another so the /list cross-check has to opt into both flags.
async fn store_memories(app: &Router, key: &str, db: &Database) {
    let contents = [
        "The backup job runs nightly on the storage host",
        "Rust edition 2021 is pinned in the workspace manifest",
        "Grafana alerts page the oncall rotation by SMS",
        "The coffee machine on floor three needs descaling monthly",
        "Invoices are exported to the accounting tool every Friday",
    ];
    let mut ids = Vec::new();
    for content in contents {
        let (status, body) = post(
            app,
            "/store",
            key,
            json!({ "content": content, "category": "test" }),
        )
        .await;
        assert!(status.is_success(), "store failed: {status} {body}");
        ids.push(
            body["id"]
                .as_i64()
                .unwrap_or_else(|| panic!("stored id: {body}")),
        );
    }
    let (archived, forgotten) = (ids[0], ids[1]);
    db.write(move |conn| {
        conn.execute(
            "UPDATE memories SET is_archived = 1 WHERE id = ?1",
            [archived],
        )?;
        conn.execute(
            "UPDATE memories SET is_forgotten = 1 WHERE id = ?1",
            [forgotten],
        )?;
        Ok(())
    })
    .await
    .expect("archive and forget");
}

async fn paginated_list_total(app: &Router, key: &str) -> usize {
    let mut total = 0;
    loop {
        let path = format!(
            "/list?limit=2&offset={total}&include_archived=true&include_forgotten=true"
        );
        let (status, body) = get(app, &path, key).await;
        assert_eq!(status, StatusCode::OK, "list failed: {body}");
        let page = body["results"].as_array().expect("results array").len();
        if page == 0 {
            return total;
        }
        total += page;
    }
}

async fn assert_stats_matches_list(app: &Router, key: &str, expected: usize) {
    let (status, body) = get(app, "/stats", key).await;
    assert_eq!(status, StatusCode::OK, "stats failed: {body}");
    let listed = paginated_list_total(app, key).await;
    assert_eq!(listed, expected, "list total");
    assert_eq!(
        body["memories"], listed,
        "stats must match the paginated /list total: {body}"
    );
}

#[tokio::test]
async fn stats_counts_rows_of_the_caller_tenant_shard() {
    let (app, state, _tmp) = test_app_with_sharding().await;
    let admin_key = bootstrap_admin_key(&app).await;

    let tenant = state
        .tenant_registry
        .as_ref()
        .expect("sharding enabled")
        .get_or_create("1")
        .await
        .expect("tenant shard");
    insert_rows(&tenant.database(), 3).await;
    insert_rows(&state.db, 5).await;

    let (status, body) = get(&app, "/stats", &admin_key).await;
    assert_eq!(status, StatusCode::OK, "stats failed: {body}");
    for field in ["memories", "tasks", "agents"] {
        assert_eq!(
            body[field], 3,
            "stats must count {field} in the tenant shard: {body}"
        );
    }
    assert_eq!(
        body["api_keys"], 1,
        "api_keys stays counted on the monolith: {body}"
    );
}

#[tokio::test]
async fn stats_memories_match_paginated_list_in_sharded_mode() {
    let (app, state, _tmp) = test_app_with_sharding().await;
    let admin_key = bootstrap_admin_key(&app).await;
    let tenant = state
        .tenant_registry
        .as_ref()
        .expect("sharding enabled")
        .get_or_create("1")
        .await
        .expect("tenant shard");

    store_memories(&app, &admin_key, &tenant.database()).await;

    assert_stats_matches_list(&app, &admin_key, 5).await;
}

#[tokio::test]
async fn stats_memories_match_paginated_list_in_monolith_mode() {
    let (app, state) = test_app().await;
    let admin_key = bootstrap_admin_key(&app).await;

    store_memories(&app, &admin_key, &state.db).await;

    assert_stats_matches_list(&app, &admin_key, 5).await;
}
