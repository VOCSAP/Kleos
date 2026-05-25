//! Integration tests for Phylax agent-native credential features.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware;
use axum::Router;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use kleos_cred::crypto::derive_key;
use kleos_credd::auth::{auth_middleware, preauth_rate_limit};
use kleos_credd::state::AppState;
use kleos_lib::db::Database;
use kleos_phylax::router::phylax_routes;
use kleos_phylax::state::PhylaxState;

/// Test harness wrapping a Phylax-enabled router.
struct TestApp {
    /// Combined credd + phylax router with all middleware applied.
    router: Router,
    /// Hex-encoded master token derived from the test password.
    master_token: String,
}

impl TestApp {
    /// Create a test app with in-memory DB, migrations applied.
    async fn new() -> Self {
        let db = Database::connect_memory().await.expect("in-memory db");
        let master_password = "test-master-password";
        let master_key = derive_key(1, master_password.as_bytes(), None);
        let master_token = hex::encode(*master_key);

        let app_state = AppState::new(db, *master_key);
        let phylax_state = PhylaxState::from_app_state(app_state);

        // Build combined router: credd base (with auth middleware) merged with
        // phylax extension routes. Phylax routes need the same auth middleware
        // applied explicitly because Axum's merge does not propagate layers
        // from one router to the other.
        let credd_router = kleos_credd::build_router(phylax_state.inner.clone());
        let phylax_router = phylax_routes()
            .layer(middleware::from_fn_with_state(
                phylax_state.inner.clone(),
                auth_middleware,
            ))
            .layer(middleware::from_fn_with_state(
                phylax_state.inner.clone(),
                preauth_rate_limit,
            ))
            .with_state(phylax_state.clone());

        // Phylax routes take precedence for /phylax/* paths.
        let router = credd_router.merge(phylax_router);

        Self {
            router,
            master_token,
        }
    }

    /// Send an authenticated request with the master token.
    async fn request_master(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        self.request_auth(method, path, body, &self.master_token.clone())
            .await
    }

    /// Send an authenticated request with a specific token.
    async fn request_auth(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        token: &str,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header("authorization", format!("Bearer {}", token));

        let body = if let Some(json) = body {
            builder = builder.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&json).unwrap())
        } else {
            Body::empty()
        };

        let req = builder.body(body).unwrap();
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
        let json: Value = if body_bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&body_bytes).unwrap_or(Value::Null)
        };
        (status, json)
    }
}

// ---- Policy CRUD tests ----

/// Test full create/read/update/delete lifecycle for access policies.
#[tokio::test]
async fn test_policy_crud() {
    let app = TestApp::new().await;

    // List policies -- should be empty.
    let (status, body) = app.request_master("GET", "/phylax/policies", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["policies"].as_array().unwrap().len(), 0);

    // Create a policy.
    let (status, body) = app
        .request_master(
            "POST",
            "/phylax/policies",
            Some(json!({
                "namespace": "default",
                "category": "prod",
                "require_approval": true,
                "allowed_modes": ["text", "proxy"]
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let policy_id = body["id"].as_i64().unwrap();
    assert!(policy_id > 0);
    assert_eq!(body["namespace"], "default");
    assert_eq!(body["require_approval"], true);

    // List policies -- should have one.
    let (status, body) = app.request_master("GET", "/phylax/policies", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["policies"].as_array().unwrap().len(), 1);

    // Update the policy.
    let (status, _) = app
        .request_master(
            "PUT",
            &format!("/phylax/policies/{}", policy_id),
            Some(json!({
                "require_approval": false,
                "allowed_modes": ["text"]
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Delete the policy.
    let (status, _) = app
        .request_master(
            "DELETE",
            &format!("/phylax/policies/{}", policy_id),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // List policies -- should be empty again.
    let (status, body) = app.request_master("GET", "/phylax/policies", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["policies"].as_array().unwrap().len(), 0);
}

// ---- Approval flow tests ----

/// Test full approval flow: agent requests, master approves, lease is minted.
#[tokio::test]
async fn test_approval_flow() {
    let app = TestApp::new().await;

    // Create a policy requiring approval for prod secrets.
    let (status, _) = app
        .request_master(
            "POST",
            "/phylax/policies",
            Some(json!({
                "namespace": "default",
                "category": "prod",
                "require_approval": true
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Create an agent key for testing.
    let (status, body) = app
        .request_master(
            "POST",
            "/agents",
            Some(json!({
                "name": "test-agent",
                "categories": ["prod/*"],
                "allow_raw": false
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let agent_key = body["key"].as_str().unwrap().to_string();

    // Agent requests approval.
    let (status, body) = app
        .request_auth(
            "POST",
            "/phylax/approvals",
            Some(json!({
                "category": "prod",
                "secret_name": "deploy-key",
                "resolve_mode": "text"
            })),
            &agent_key,
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let approval_id = body["approval_id"].as_i64().unwrap();
    assert!(approval_id > 0);
    assert!(body["poll_url"]
        .as_str()
        .unwrap()
        .contains(&approval_id.to_string()));

    // Get approval status -- should be pending (status=0).
    let (status, body) = app
        .request_master(
            "GET",
            &format!("/phylax/approvals/{}", approval_id),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], 0);

    // Approve it.
    let (status, body) = app
        .request_master(
            "PUT",
            &format!("/phylax/approvals/{}", approval_id),
            Some(json!({
                "decision": "approved",
                "reason": "test approval"
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "approved");
    assert!(body["lease"]["jti"].is_string());

    // List leases -- should have at least one active lease.
    let (status, body) = app.request_master("GET", "/phylax/leases", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["leases"].as_array().unwrap().len() >= 1);
}

// ---- Approval denial test ----

/// Test that an approval can be denied and returns denied status.
#[tokio::test]
async fn test_approval_denial() {
    let app = TestApp::new().await;

    // Create an agent key.
    let (status, body) = app
        .request_master(
            "POST",
            "/agents",
            Some(json!({
                "name": "deny-agent",
                "categories": ["prod/*"],
                "allow_raw": false
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let agent_key = body["key"].as_str().unwrap().to_string();

    // Agent requests approval.
    let (status, body) = app
        .request_auth(
            "POST",
            "/phylax/approvals",
            Some(json!({
                "category": "prod",
                "secret_name": "secret-key",
                "resolve_mode": "text"
            })),
            &agent_key,
        )
        .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let approval_id = body["approval_id"].as_i64().unwrap();

    // Deny it.
    let (status, body) = app
        .request_master(
            "PUT",
            &format!("/phylax/approvals/{}", approval_id),
            Some(json!({
                "decision": "denied",
                "reason": "test denial"
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "denied");
}

// ---- Namespace enumeration test ----

/// Test that policies created in different namespaces are all visible via
/// the namespace enumeration endpoint.
#[tokio::test]
async fn test_namespace_list() {
    let app = TestApp::new().await;

    // Create policies in different namespaces.
    for ns in &["alpha", "beta", "gamma"] {
        let (status, _) = app
            .request_master(
                "POST",
                "/phylax/policies",
                Some(json!({
                    "namespace": ns,
                    "require_approval": true
                })),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
    }

    // List namespaces.
    let (status, body) = app
        .request_master("GET", "/phylax/namespaces", None)
        .await;
    assert_eq!(status, StatusCode::OK);
    let namespaces = body["namespaces"].as_array().unwrap();
    assert_eq!(namespaces.len(), 3);
    assert!(namespaces.iter().any(|n| n == "alpha"));
    assert!(namespaces.iter().any(|n| n == "beta"));
    assert!(namespaces.iter().any(|n| n == "gamma"));
}

// ---- ECDH challenge test ----

/// Test that an ECDH challenge can be issued and returns a valid 32-byte nonce.
#[tokio::test]
async fn test_ecdh_challenge() {
    let app = TestApp::new().await;

    // Issue a challenge.
    let (status, body) = app
        .request_master("POST", "/phylax/ecdh/challenge", None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["challenge_id"].is_string());
    assert!(body["nonce"].is_string());
    // Nonce should be 64 hex chars (32 bytes).
    assert_eq!(body["nonce"].as_str().unwrap().len(), 64);
}

// ---- SSH settings test ----

/// Test get/update lifecycle for SSH key settings on a specific secret.
#[tokio::test]
async fn test_ssh_settings() {
    let app = TestApp::new().await;

    // Get default settings (no settings configured yet).
    let (status, body) = app
        .request_master("GET", "/phylax/ssh/ssh-keys/deploy", None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["auto_sign"], false);
    assert_eq!(body["auto_load"], false);

    // Update settings.
    let (status, body) = app
        .request_master(
            "PUT",
            "/phylax/ssh/ssh-keys/deploy",
            Some(json!({
                "auto_sign": true,
                "auto_load": true
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["auto_sign"], true);
    assert_eq!(body["auto_load"], true);

    // Get updated settings -- should reflect the change.
    let (status, body) = app
        .request_master("GET", "/phylax/ssh/ssh-keys/deploy", None)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["auto_sign"], true);
    assert_eq!(body["auto_load"], true);
}

// ---- PIV enrollment test ----

/// Test PIV public key enrollment and revocation, including idempotency
/// check (double-revoke must fail).
#[tokio::test]
async fn test_piv_enroll_revoke() {
    let app = TestApp::new().await;

    // Enroll a pubkey.
    let (status, body) = app
        .request_master(
            "POST",
            "/phylax/ecdh/enroll",
            Some(json!({
                "agent_name": "test-agent",
                "public_key_pem": "-----BEGIN PUBLIC KEY-----\ntest\n-----END PUBLIC KEY-----"
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let pubkey_id = body["id"].as_i64().unwrap();
    assert!(pubkey_id > 0);

    // Revoke it.
    let (status, _) = app
        .request_master(
            "POST",
            "/phylax/ecdh/revoke",
            Some(json!({
                "pubkey_id": pubkey_id
            })),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    // Revoking again should fail (already revoked).
    let (status, _) = app
        .request_master(
            "POST",
            "/phylax/ecdh/revoke",
            Some(json!({
                "pubkey_id": pubkey_id
            })),
        )
        .await;
    assert_ne!(status, StatusCode::OK);
}
