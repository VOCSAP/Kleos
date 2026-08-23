//! kleos-mcp -- MCP transport adapter that forwards JSON-RPC requests to
//! the server-side POST /mcp endpoint, signing each request with the
//! local PIV / Ed25519 identity.
//!
//! The server handles all dispatch, scope enforcement, rate limiting, and
//! tool registry. This binary is a thin bridge between the MCP stdio/HTTP
//! transport and the authenticated server endpoint.

/// MCP tool registry, dispatcher, and curated tool list.
pub mod tools;
/// `tools/list` response filtering: global exclusion overridable per project.
mod tool_filter;
/// Transport layer (stdio and optional HTTP).
pub mod transport;

use kleos_client::Client;
use serde_json::{json, Value};
use std::sync::Arc;

/// Application state -- a single shared HTTP client.
#[derive(Clone)]
pub struct App {
    pub client: Arc<Client>,
}

/// App lifecycle and bootstrap helpers.
impl App {
    /// Bootstrap an App from environment variables.
    ///
    /// Reads `KLEOS_URL` for the server endpoint (default
    /// `http://127.0.0.1:4200`, matching the rest of the workspace) and
    /// loads a PIV / Ed25519 signer via the standard
    /// `RequestSigner::from_env_or_file` path.
    pub fn from_env() -> Result<Self, String> {
        let base_url =
            std::env::var("KLEOS_URL").unwrap_or_else(|_| "http://127.0.0.1:4200".to_string());
        let host_label = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".into());
        let agent_label = std::env::var("KLEOS_AGENT_LABEL").unwrap_or_else(|_| "kleos-mcp".into());
        let model_label = std::env::var("KLEOS_MODEL_LABEL").unwrap_or_else(|_| "none".into());

        let signer = kleos_lib::auth_piv::RequestSigner::from_env_or_file(
            &host_label,
            &agent_label,
            &model_label,
        )
        .map_err(|e| format!("PIV identity load failed: {e}"))?;

        let api_key = std::env::var("KLEOS_API_KEY")
            .ok()
            .filter(|k| !k.trim().is_empty());

        if signer.is_none() && api_key.is_none() {
            return Err(
                "no auth configured: set KLEOS_IDENTITY_PATH (or run `kleos-cli identity init`) \
                 for PIV signing, or set KLEOS_API_KEY as a bearer fallback. Refusing to start \
                 unauthenticated."
                    .to_string(),
            );
        }

        let client = Client::new(base_url, api_key, signer);
        Ok(Self {
            client: Arc::new(client),
        })
    }
}

/// Builds a JSON-RPC error response with the given code and message.
fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message }})
}

/// Extracts the request id from a JSON-RPC envelope.
fn request_id(req: &Value) -> Option<Value> {
    req.get("id").cloned()
}

/// Forwards one JSON-RPC request to the server-side POST /mcp endpoint.
/// Returns the server's response, or None for notifications.
/// On transport errors, wraps the error as a JSON-RPC error envelope.
#[tracing::instrument(skip(app, req), fields(method = req.get("method").and_then(|v| v.as_str()).unwrap_or("")))]
pub async fn handle_jsonrpc(app: &App, req: Value) -> Option<Value> {
    let id = request_id(&req);
    let req = inject_space_for_tool_calls(req);
    match app.client.post_mcp(&req).await {
        // Patch 47: post-process the tools/list response, the response-side
        // counterpart to inject_space_for_tool_calls's request-side mutation.
        // No-op for every other response shape (see filter_tools_list_response).
        Ok(resp) => resp.map(tool_filter::filter_tools_list_response),
        Err(e) => id.map(|id| error_response(id, -32603, &e)),
    }
}

/// Scopes headless `tools/call` writes to the project space named by the
/// `KLEOS_SPACE` env var.
///
/// The server resolves the space from the `space` / `space_id` field carried
/// in a tool's `arguments`; otherwise it falls back to `default`. A headless
/// agent (e.g. OpenClaw) has no Claude Code session file to source a space
/// from, so we mirror what `kleos-cli` already does (read `KLEOS_SPACE`) and
/// inject `space` into the call arguments before forwarding.
///
/// No-op when: `KLEOS_SPACE` is unset/blank, the request is not a `tools/call`,
/// `params.arguments` is not an object, or the caller already specified a
/// `space` / `space_id` (explicit args win).
fn inject_space_for_tool_calls(req: Value) -> Value {
    match std::env::var("KLEOS_SPACE") {
        Ok(space) if !space.trim().is_empty() => inject_space(req, &space),
        _ => req,
    }
}

/// Pure core of [`inject_space_for_tool_calls`]: injects `space` into a
/// `tools/call`'s arguments. Env-free for testability.
fn inject_space(mut req: Value, space: &str) -> Value {
    if req.get("method").and_then(Value::as_str) != Some("tools/call") {
        return req;
    }
    if let Some(args) = req
        .pointer_mut("/params/arguments")
        .and_then(Value::as_object_mut)
    {
        if !args.contains_key("space") && !args.contains_key("space_id") {
            args.insert("space".to_string(), Value::String(space.to_string()));
        }
    }
    req
}

#[cfg(test)]
mod tests {
    use super::inject_space;
    use serde_json::json;

    #[test]
    fn injects_space_into_tools_call_without_space() {
        let req = json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "memory_store", "arguments": { "content": "x" } }
        });
        let out = inject_space(req, "openclaw");
        assert_eq!(out["params"]["arguments"]["space"], json!("openclaw"));
    }

    #[test]
    fn preserves_explicit_space() {
        let req = json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "memory_store", "arguments": { "content": "x", "space": "other" } }
        });
        let out = inject_space(req, "openclaw");
        assert_eq!(out["params"]["arguments"]["space"], json!("other"));
    }

    #[test]
    fn preserves_explicit_space_id() {
        let req = json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "memory_store", "arguments": { "content": "x", "space_id": 7 } }
        });
        let out = inject_space(req, "openclaw");
        assert!(out["params"]["arguments"].get("space").is_none());
        assert_eq!(out["params"]["arguments"]["space_id"], json!(7));
    }

    #[test]
    fn ignores_non_tools_call() {
        let req = json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}});
        let out = inject_space(req.clone(), "openclaw");
        assert_eq!(out, req);
    }

    #[test]
    fn ignores_non_object_arguments() {
        let req = json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": "x", "arguments": "not-an-object" }
        });
        let out = inject_space(req.clone(), "openclaw");
        assert_eq!(out, req);
    }
}
