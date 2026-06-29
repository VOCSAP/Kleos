#![cfg(feature = "http")]

//! HTTP transport for kleos-mcp.
//!
//! Listens on the configured address and proxies JSON-RPC requests onto
//! kleos-server, signing each onward call with the MCP host's own PIV
//! identity. There is no front-door client-to-MCP auth; reachability is the
//! boundary. Bind to a host or interface that only authorized clients can
//! reach (loopback, private LAN, mesh VPN), not 0.0.0.0 on a public
//! interface.

use crate::{handle_jsonrpc, App};
use axum::{
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde_json::Value;
use std::net::SocketAddr;
use std::time::Duration;
use tower_http::timeout::TimeoutLayer;

/// Maximum request body size (2 MiB).
const BODY_LIMIT: usize = 2 * 1024 * 1024;
/// Per-request timeout in seconds before the server returns 408.
const REQUEST_TIMEOUT_SECS: u64 = 30;

/// Handles one `/mcp` JSON-RPC request.
async fn mcp(State(app): State<App>, Json(body): Json<Value>) -> Json<Value> {
    let response = handle_jsonrpc(&app, body)
        .await
        .unwrap_or_else(|| serde_json::json!({}));
    Json(response)
}

/// Assembles the Axum router with size + timeout layers.
pub fn router(app: App) -> Router {
    Router::new()
        .route("/mcp", post(mcp))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(REQUEST_TIMEOUT_SECS),
        ))
        .with_state(app)
}

/// Binds the listener and serves until shutdown.
pub async fn serve(app: App, listen: &str) -> Result<(), String> {
    let addr: SocketAddr = listen
        .parse()
        .map_err(|e| format!("invalid listen address: {e}"))?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| e.to_string())?;
    tracing::info!(
        addr = %addr,
        "MCP HTTP transport listening (network boundary is the auth boundary; bind accordingly)"
    );
    axum::serve(listener, router(app))
        .await
        .map_err(|e| e.to_string())
}
