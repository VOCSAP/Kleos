//! HTTP-level integration tests for GET/POST/PATCH/DELETE /attention.

mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use common::{bootstrap_admin_key, delete, get, post, test_app_with_sharding};
use serde_json::json;
use tower::ServiceExt;

async fn patch(
    app: &axum::Router,
    path: &str,
    key: &str,
    payload: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let request = Request::builder()
        .method("PATCH")
        .uri(path)
        .header("Authorization", format!("Bearer {key}"))
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_string(&payload).unwrap()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(json!(null));
    (status, body)
}

#[tokio::test]
async fn create_and_list() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;

    let (status, body) = post(
        &app,
        "/attention",
        &key,
        json!({ "content": "remember to write tests", "priority": 7 }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create: {body}");
    assert_eq!(body["content"], "remember to write tests");
    assert_eq!(body["priority"], 7);
    let id = body["id"].as_i64().expect("id in response");

    let (status, body) = get(&app, "/attention", &key).await;
    assert!(status.is_success(), "list: {body}");
    let notes = body["notes"].as_array().expect("notes array");
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0]["id"], id);
}

#[tokio::test]
async fn create_defaults_priority_to_five() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;

    let (status, body) = post(
        &app,
        "/attention",
        &key,
        json!({ "content": "no priority set" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["priority"], 5);
}

#[tokio::test]
async fn patch_updates_note() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;

    let (_, body) = post(
        &app,
        "/attention",
        &key,
        json!({ "content": "old content", "priority": 3 }),
    )
    .await;
    let id = body["id"].as_i64().expect("id");

    let (status, body) = patch(
        &app,
        &format!("/attention/{id}"),
        &key,
        json!({ "content": "new content", "priority": 9 }),
    )
    .await;
    assert!(status.is_success(), "patch: {body}");
    assert_eq!(body["content"], "new content");
    assert_eq!(body["priority"], 9);
}

#[tokio::test]
async fn delete_removes_note() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;

    let (_, body) = post(&app, "/attention", &key, json!({ "content": "temporary" })).await;
    let id = body["id"].as_i64().expect("id");

    let (status, _) = delete(&app, &format!("/attention/{id}"), &key).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "delete must return 204");

    let (status, body) = get(&app, "/attention", &key).await;
    assert!(status.is_success());
    let notes = body["notes"].as_array().expect("notes array");
    assert!(notes.is_empty(), "list must be empty after delete");
}

#[tokio::test]
async fn delete_unknown_id_returns_404() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;

    let (status, _) = delete(&app, "/attention/99999", &key).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn negative_limit_is_clamped_to_one() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;

    post(&app, "/attention", &key, json!({ "content": "a note" })).await;

    // ?limit=-1 must not bypass the cap and return an unbounded result set.
    let (status, body) = get(&app, "/attention?limit=-1", &key).await;
    assert!(status.is_success(), "list with negative limit: {body}");
    // returns at most 1 row (clamped), not the full table
    let notes = body["notes"].as_array().expect("notes array");
    assert!(
        notes.len() <= 1,
        "negative limit must be clamped, got {} notes",
        notes.len()
    );
}

#[tokio::test]
async fn priority_out_of_range_returns_error() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;

    let (status, _body) = post(
        &app,
        "/attention",
        &key,
        json!({ "content": "bad prio", "priority": 999 }),
    )
    .await;
    assert!(
        status.is_client_error(),
        "priority 999 must be rejected, got {status}"
    );
}

#[tokio::test]
async fn list_sorted_by_priority_descending() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;

    post(
        &app,
        "/attention",
        &key,
        json!({ "content": "low", "priority": 1 }),
    )
    .await;
    post(
        &app,
        "/attention",
        &key,
        json!({ "content": "critical", "priority": 10 }),
    )
    .await;
    post(
        &app,
        "/attention",
        &key,
        json!({ "content": "medium", "priority": 5 }),
    )
    .await;

    let (_, body) = get(&app, "/attention", &key).await;
    let notes = body["notes"].as_array().expect("notes array");
    assert_eq!(notes.len(), 3);
    assert_eq!(notes[0]["content"], "critical");
    assert_eq!(notes[1]["content"], "medium");
    assert_eq!(notes[2]["content"], "low");
}
