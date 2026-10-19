//! The memory JSON shape carries `is_superseded`, so a replica that reads
//! GET /list can drop a retired memory it projected earlier.

mod common;

use common::{bootstrap_admin_key, get, post, test_app_with_sharding};
use serde_json::json;

#[tokio::test]
async fn list_reports_is_superseded_as_a_boolean() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;
    let (status, stored) = post(
        &app,
        "/store",
        &key,
        json!({ "content": "replica flag probe", "category": "test" }),
    )
    .await;
    assert!(status.is_success(), "store failed: {status} {stored}");
    let id = stored["id"].as_i64().expect("numeric id");

    let (status, body) = get(&app, "/list", &key).await;
    assert!(status.is_success(), "list failed: {status} {body}");
    let row = body["results"]
        .as_array()
        .expect("results array")
        .iter()
        .find(|row| row["id"].as_i64() == Some(id))
        .unwrap_or_else(|| panic!("stored memory missing from /list: {body}"));
    assert_eq!(row["is_superseded"], json!(false), "{row}");
}

#[tokio::test]
async fn a_corrected_memory_is_reported_as_superseded() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;
    let (_, stored) = post(
        &app,
        "/store",
        &key,
        json!({ "content": "the deploy port is 4100", "category": "test" }),
    )
    .await;
    let id = stored["id"].as_i64().expect("numeric id");

    let (status, corrected) = post(
        &app,
        "/correct",
        &key,
        json!({ "memory_id": id, "content": "the deploy port is 4200" }),
    )
    .await;
    assert!(status.is_success(), "correct failed: {status} {corrected}");

    let (status, original) = get(&app, &format!("/memory/{id}"), &key).await;
    assert!(status.is_success(), "get failed: {status} {original}");
    assert_eq!(original["is_superseded"], json!(true), "{original}");

    let (status, body) = get(&app, "/list", &key).await;
    assert!(status.is_success(), "list failed: {status} {body}");
    let row = body["results"]
        .as_array()
        .expect("results array")
        .iter()
        .find(|row| row["id"].as_i64() == Some(id))
        .unwrap_or_else(|| panic!("corrected memory missing from /list: {body}"));
    assert_eq!(row["is_superseded"], json!(true), "{row}");
}
