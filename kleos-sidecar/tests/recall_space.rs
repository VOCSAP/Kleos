// Patch 33.2 -- /recall forwards the caller's space scope to Kleos /search.
//
// Kept in its own file (additive) rather than extending integration.rs, so the
// upstream test harness stays untouched.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{
    body::Body, extract::State as AxumState, http::Request, response::IntoResponse, routing::any,
    Json, Router,
};
use kleos_sidecar::{build_test_state, routes};
use tokio::net::TcpListener;

type Captured = Arc<Mutex<Vec<serde_json::Value>>>;

async fn mock_search(
    AxumState(seen): AxumState<Captured>,
    req: Request<Body>,
) -> impl IntoResponse {
    let bytes = axum::body::to_bytes(req.into_body(), 1024 * 1024)
        .await
        .unwrap_or_default();
    seen.lock()
        .unwrap()
        .push(serde_json::from_slice(&bytes).unwrap_or_default());
    Json(serde_json::json!({ "results": [] }))
}

async fn spawn() -> (String, Captured) {
    let seen: Captured = Arc::default();
    let upstream = Router::new()
        .route("/search", any(mock_search))
        .route("/memory/search", any(mock_search))
        .fallback(any(|| async { Json(serde_json::json!({})) }))
        .with_state(seen.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, upstream).await.unwrap() });

    std::env::set_var("KLEOS_NET_ALLOW_PRIVATE", "1");
    let state = build_test_state(upstream_url, None);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let sidecar_url = format!("http://{}", listener.local_addr().unwrap());
    let app = routes::router(state);
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (sidecar_url, seen)
}

async fn recall(sidecar_url: &str, body: serde_json::Value) {
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
        .post(format!("{sidecar_url}/recall"))
        .json(&body)
        .send()
        .await
        .unwrap();
    assert!(
        response.status().is_success(),
        "recall status {}",
        response.status()
    );
}

#[tokio::test]
async fn recall_forwards_space_scope_to_search() {
    let (sidecar_url, seen) = spawn().await;
    recall(
        &sidecar_url,
        serde_json::json!({
            "query": "where is the deploy target",
            "space": "kleos",
            "include_unscoped": false,
        }),
    )
    .await;
    let calls = seen.lock().unwrap().clone();
    assert_eq!(calls.len(), 1, "one upstream search expected: {calls:?}");
    assert_eq!(calls[0]["space"], "kleos");
    assert_eq!(calls[0]["include_unscoped"], false);
    assert!(calls[0].get("space_id").is_none());
}

#[tokio::test]
async fn recall_without_space_sends_no_scope() {
    let (sidecar_url, seen) = spawn().await;
    recall(
        &sidecar_url,
        serde_json::json!({ "query": "where is the deploy target", "space": "  " }),
    )
    .await;
    let calls = seen.lock().unwrap().clone();
    assert_eq!(calls.len(), 1, "one upstream search expected: {calls:?}");
    assert!(calls[0].get("space").is_none());
    assert!(calls[0].get("space_id").is_none());
    assert!(calls[0].get("include_unscoped").is_none());
}
