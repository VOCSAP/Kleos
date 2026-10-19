//! The memory JSON shape carries `is_superseded`, so a replica that reads
//! GET /list can drop a retired memory it projected earlier.

mod common;

use axum::Router;
use common::{bootstrap_admin_key, get, post, test_app_with_sharding};
use serde_json::{json, Value};

fn listed(body: &Value, id: i64) -> Option<&Value> {
    body["results"]
        .as_array()
        .expect("results array")
        .iter()
        .find(|row| row["id"].as_i64() == Some(id))
}

async fn store(app: &Router, key: &str, content: &str) -> i64 {
    let (status, stored) = post(
        app,
        "/store",
        key,
        json!({ "content": content, "category": "test" }),
    )
    .await;
    assert!(status.is_success(), "store failed: {status} {stored}");
    stored["id"].as_i64().expect("numeric id")
}

#[tokio::test]
async fn list_reports_is_superseded_as_a_boolean() {
    let (app, state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;
    let id = store(&app, &key, "replica flag probe").await;

    let (status, body) = get(&app, "/list", &key).await;
    assert!(status.is_success(), "list failed: {status} {body}");
    let row =
        listed(&body, id).unwrap_or_else(|| panic!("stored memory missing from /list: {body}"));
    assert_eq!(row["is_superseded"], json!(false), "{row}");

    // Retire the row the way deduplicate does: still the latest version, so
    // GET /list keeps returning it and the replica must read the flag.
    let owner = row["user_id"].as_i64().expect("user_id");
    let shard = kleos_server::extractors::resolve_db_for_user(&state, owner)
        .await
        .expect("resolve shard");
    shard
        .write(move |conn| {
            conn.execute(
                "UPDATE memories SET is_superseded = 1 WHERE id = ?1",
                rusqlite::params![id],
            )?;
            Ok(())
        })
        .await
        .expect("mark retired");

    let (status, body) = get(&app, "/list", &key).await;
    assert!(status.is_success(), "list failed: {status} {body}");
    let row = listed(&body, id)
        .unwrap_or_else(|| panic!("retired latest row missing from /list: {body}"));
    assert_eq!(row["is_superseded"], json!(true), "{row}");
}

#[tokio::test]
async fn a_corrected_memory_is_retired_and_replaced_in_the_listing() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;
    let id = store(&app, &key, "the deploy port is 4100").await;

    let (status, corrected) = post(
        &app,
        "/correct",
        &key,
        json!({ "memory_id": id, "content": "the deploy port is 4200" }),
    )
    .await;
    assert!(status.is_success(), "correct failed: {status} {corrected}");
    let copy = corrected["id"].as_i64().expect("corrected id");

    let (status, original) = get(&app, &format!("/memory/{id}"), &key).await;
    assert!(status.is_success(), "get failed: {status} {original}");
    assert_eq!(original["is_superseded"], json!(true), "{original}");
    assert_eq!(original["is_latest"], json!(false), "{original}");

    let (status, body) = get(&app, "/list", &key).await;
    assert!(status.is_success(), "list failed: {status} {body}");
    assert!(
        listed(&body, id).is_none(),
        "corrected original still listed: {body}"
    );
    let row =
        listed(&body, copy).unwrap_or_else(|| panic!("correction missing from /list: {body}"));
    assert_eq!(row["parent_memory_id"], json!(id), "{row}");
    assert_eq!(row["is_superseded"], json!(false), "{row}");
}
