//! ECDH challenge and PIV key management handlers.
//!
//! Implements P256 ECDH challenge-response auth flow:
//! 1. Agent requests challenge (32-byte nonce)
//! 2. Agent signs challenge with PIV 9A key
//! 3. Server verifies signature and derives bearer token

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use kleos_cred::CredError;
use kleos_credd::auth::Auth;
use kleos_credd::handlers::AppError;
use rand::rngs::OsRng;
use rand::TryRngCore;

use crate::audit::{actions, log_phylax_audit};
use crate::models::piv_pubkey;
use crate::state::PhylaxState;

/// Request body for PIV key enrollment.
#[derive(Deserialize)]
pub struct EnrollRequest {
    /// Agent name to enroll the key for.
    pub agent_name: String,
    /// PEM-encoded P256 public key.
    pub public_key_pem: String,
}

/// Request body for PIV key revocation.
#[derive(Deserialize)]
pub struct RevokeRequest {
    /// ID of the pubkey to revoke.
    pub pubkey_id: i64,
}

/// Issue a 32-byte ECDH challenge nonce. Stored in-memory with 60s TTL.
pub async fn issue_challenge(
    Auth(auth): Auth,
    State(state): State<PhylaxState>,
) -> Result<impl IntoResponse, AppError> {
    // GC expired challenges periodically.
    state.gc_challenges();

    let mut nonce = [0u8; 32];
    OsRng
        .try_fill_bytes(&mut nonce)
        .expect("OS CSPRNG must be available");

    let challenge_id = uuid::Uuid::new_v4().to_string();
    state.challenges.insert(
        challenge_id.clone(),
        (nonce.to_vec(), std::time::Instant::now()),
    );

    let _ = log_phylax_audit(
        &state.inner.db,
        auth.user_id(),
        auth.agent_name(),
        None,
        None,
        None,
        None,
        actions::ECDH_CHALLENGE,
        "",
        "",
        true,
        None,
    )
    .await;

    Ok(Json(json!({
        "challenge_id": challenge_id,
        "nonce": hex::encode(nonce),
    })))
}

/// Enroll a PIV 9A public key for an agent. Master-only.
pub async fn enroll_pubkey(
    Auth(auth): Auth,
    State(state): State<PhylaxState>,
    Json(body): Json<EnrollRequest>,
) -> Result<impl IntoResponse, AppError> {
    if !auth.is_master() {
        return Err(CredError::PermissionDenied("master key required".into()).into());
    }

    let pk = piv_pubkey::enroll_pubkey(
        &state.inner.db,
        auth.user_id(),
        &body.agent_name,
        &body.public_key_pem,
    )
    .await?;

    let _ = log_phylax_audit(
        &state.inner.db,
        auth.user_id(),
        Some(&body.agent_name),
        None,
        None,
        None,
        None,
        actions::PIV_ENROLLED,
        "",
        "",
        true,
        None,
    )
    .await;

    Ok(Json(pk.to_json()))
}

/// Revoke an enrolled PIV 9A public key. Master-only.
pub async fn revoke_pubkey(
    Auth(auth): Auth,
    State(state): State<PhylaxState>,
    Json(body): Json<RevokeRequest>,
) -> Result<impl IntoResponse, AppError> {
    if !auth.is_master() {
        return Err(CredError::PermissionDenied("master key required".into()).into());
    }

    piv_pubkey::revoke_pubkey(&state.inner.db, body.pubkey_id).await?;

    let _ = log_phylax_audit(
        &state.inner.db,
        auth.user_id(),
        None,
        None,
        None,
        None,
        None,
        actions::PIV_REVOKED,
        "",
        "",
        true,
        None,
    )
    .await;

    Ok(Json(json!({ "ok": true })))
}
