//! Patch 33.2 -- space scoping on the write path of `/ingest` and on the read
//! paths of `/context` and `/prompt/generate`.
//!
//! Before this patch `/ingest` wrote `space_id = NULL` (visible in every
//! scoped recall through the inclusive filter) and `/context` /
//! `/prompt/generate` accepted no space at all.

mod common;

use axum::http::StatusCode;
use serde_json::{json, Value};

use common::{bootstrap_admin_key, get, post, test_app_with_sharding};

/// Contents of every memory listed in `space` (strict, no default / NULL rows).
async fn contents_in_space(app: &axum::Router, key: &str, space: &str) -> Vec<String> {
    let (status, body) = get(
        app,
        &format!("/list?space={space}&include_unscoped=false&limit=200"),
        key,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "list failed: {body}");
    body["results"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|m| m["content"].as_str().map(str::to_string))
        .collect()
}

async fn store(app: &axum::Router, key: &str, content: &str, space: &str) {
    let (status, body) = post(
        app,
        "/store",
        key,
        json!({
            "content": content,
            "space": space,
            "is_static": true,
            "importance": 9,
            // Not "general": /prompt/generate excludes it by default
            // (KLEOS_RECALL_EXCLUDE_CATEGORIES).
            "category": "decision",
        }),
    )
    .await;
    assert!(status.is_success(), "store failed: {status} {body}");
}

#[tokio::test]
async fn ingest_writes_into_requested_space() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;

    let (status, body) = post(
        &app,
        "/ingest",
        &key,
        json!({ "text": "quillfeather ingest marker for space alpha", "space": "alpha" }),
    )
    .await;
    assert!(status.is_success(), "ingest failed: {status} {body}");

    let alpha = contents_in_space(&app, &key, "alpha").await;
    assert!(
        alpha.iter().any(|c| c.contains("quillfeather")),
        "ingested memory must land in space alpha: {alpha:?}"
    );
}

#[tokio::test]
async fn ingest_without_space_lands_in_default_not_null() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;

    let (status, body) = post(
        &app,
        "/ingest",
        &key,
        json!({ "text": "lanternwick ingest marker without a space" }),
    )
    .await;
    assert!(status.is_success(), "ingest failed: {status} {body}");

    let default = contents_in_space(&app, &key, "default").await;
    assert!(
        default.iter().any(|c| c.contains("lanternwick")),
        "unscoped ingest must land in the default space, not NULL: {default:?}"
    );
}

#[tokio::test]
async fn ingest_rejects_foreign_space_id() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;

    let (status, body) = post(
        &app,
        "/ingest",
        &key,
        json!({ "text": "should not be stored", "space_id": 987_654_321 }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "foreign space_id: {body}");
}

fn context_text(body: &Value) -> String {
    body["context"].as_str().unwrap_or_default().to_string()
}

#[tokio::test]
async fn context_is_scoped_to_requested_space() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;

    store(
        &app,
        &key,
        "marigoldvault fact belongs to project alpha",
        "alpha",
    )
    .await;
    store(
        &app,
        &key,
        "marigoldvault fact belongs to project beta",
        "beta",
    )
    .await;

    let request = |extra: Value| {
        let mut body = json!({
            "query": "marigoldvault fact",
            "include_static": true,
            "include_recent": true,
            "min_relevance": 0.0,
            "token_budget": 8000,
        });
        for (k, v) in extra.as_object().unwrap() {
            body[k] = v.clone();
        }
        body
    };

    let (status, body) = post(
        &app,
        "/context",
        &key,
        request(json!({ "space": "alpha", "include_unscoped": false })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "context failed: {body}");
    let scoped = context_text(&body);
    assert!(scoped.contains("project alpha"), "alpha missing: {scoped}");
    assert!(
        !scoped.contains("project beta"),
        "beta leaked into alpha: {scoped}"
    );

    // No space: upstream behaviour, both projects visible.
    let (status, body) = post(&app, "/context", &key, request(json!({}))).await;
    assert_eq!(status, StatusCode::OK, "context failed: {body}");
    let unscoped = context_text(&body);
    assert!(
        unscoped.contains("project alpha"),
        "alpha missing: {unscoped}"
    );
    assert!(
        unscoped.contains("project beta"),
        "beta missing: {unscoped}"
    );
}

#[tokio::test]
async fn prompt_generate_is_scoped_to_requested_space() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;

    // Capitalised: /prompt/generate drops chunks that start mid-word.
    store(
        &app,
        &key,
        "Heronstone deploy note for project alpha",
        "alpha",
    )
    .await;
    store(
        &app,
        &key,
        "Heronstone deploy note for project beta",
        "beta",
    )
    .await;

    let request = |extra: Value| {
        let mut body = json!({
            "agent": "claude-code",
            "task": "heronstone deploy note",
            "include_memories": true,
            "include_personality": false,
        });
        for (k, v) in extra.as_object().unwrap() {
            body[k] = v.clone();
        }
        body
    };

    let (status, body) = post(
        &app,
        "/prompt/generate",
        &key,
        request(json!({ "space": "alpha", "include_unscoped": false })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "prompt/generate failed: {body}");
    let scoped = body["prompt"].as_str().unwrap_or_default();
    assert!(scoped.contains("project alpha"), "alpha missing: {scoped}");
    assert!(
        !scoped.contains("project beta"),
        "beta leaked into alpha: {scoped}"
    );

    let (status, body) = post(&app, "/prompt/generate", &key, request(json!({}))).await;
    assert_eq!(status, StatusCode::OK, "prompt/generate failed: {body}");
    let unscoped = body["prompt"].as_str().unwrap_or_default();
    assert!(
        unscoped.contains("project alpha"),
        "alpha missing: {unscoped}"
    );
    assert!(
        unscoped.contains("project beta"),
        "beta missing: {unscoped}"
    );
}
