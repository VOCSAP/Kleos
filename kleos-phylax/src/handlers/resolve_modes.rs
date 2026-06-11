//! Non-plaintext resolve modes: verify, sign, derive (exec lives here too
//! once it lands).
//!
//! These endpoints let an agent USE a secret without ever holding it: the
//! secret is loaded server-side, the cryptographic operation happens here,
//! and only the operation's result (a boolean, a signature, derived key
//! material) crosses the agent boundary. The policy middleware has already
//! ruled on /resolve/* requests before these handlers run; handlers still
//! enforce category access themselves (defense in depth).

use axum::extract::State;
use axum::Json;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use ed25519_dalek::{Signer as DalekSigner, Verifier};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::json;
use sha2::Sha256;
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use kleos_cred::storage::get_secret;
use kleos_cred::types::SecretData;
use kleos_cred::CredError;
use kleos_credd::auth::Auth;
use kleos_credd::handlers::AppError;

use crate::audit::{actions, log_phylax_audit};
use crate::state::PhylaxState;

/// Maximum derivable key length in bytes.
const MAX_DERIVE_LEN: usize = 64;

/// HMAC algorithm tag accepted by sign/verify.
const ALGO_HMAC: &str = "hmac-sha256";
/// Ed25519 algorithm tag accepted by sign/verify (requires an SshKey secret).
const ALGO_ED25519: &str = "ed25519";

/// Body for POST /resolve/sign.
#[derive(Deserialize)]
pub struct SignModeRequest {
    /// Secret category.
    pub category: String,
    /// Secret name.
    pub name: String,
    /// Base64-encoded payload to sign.
    pub payload_b64: String,
    /// Signature algorithm: "hmac-sha256" or "ed25519".
    pub algo: String,
}

/// Body for POST /resolve/verify.
#[derive(Deserialize)]
pub struct VerifyModeRequest {
    /// Secret category.
    pub category: String,
    /// Secret name.
    pub name: String,
    /// Base64-encoded payload the signature claims to cover.
    pub payload_b64: String,
    /// Base64-encoded signature to check.
    pub signature_b64: String,
    /// Signature algorithm: "hmac-sha256" or "ed25519".
    pub algo: String,
}

/// Body for POST /resolve/derive.
#[derive(Deserialize)]
pub struct DeriveModeRequest {
    /// Secret category.
    pub category: String,
    /// Secret name.
    pub name: String,
    /// Domain-separation string; REQUIRED non-empty. Different purposes
    /// yield unrelated keys from the same root secret.
    pub purpose: String,
    /// Output length in bytes (1..=64).
    pub length: usize,
}

/// Load a secret's data after enforcing category access for the caller.
async fn load_secret(
    state: &PhylaxState,
    auth: &kleos_credd::auth::AuthInfo,
    category: &str,
    name: &str,
) -> Result<SecretData, AppError> {
    if !auth.is_master() && !auth.can_access_category(category) {
        return Err(CredError::PermissionDenied("category not permitted".into()).into());
    }
    let (_row, data) = get_secret(
        &state.inner.db,
        auth.user_id(),
        category,
        name,
        state.inner.master_key.as_ref(),
    )
    .await?;
    Ok(data)
}

/// The canonical secret bytes of each storable type, for keying HMAC/HKDF.
/// Environment secrets hold many values and are rejected: there is no single
/// "the secret" to key with.
fn secret_key_bytes(data: SecretData) -> Result<Zeroizing<Vec<u8>>, AppError> {
    let bytes = match data {
        SecretData::Note { content } => content.into_bytes(),
        SecretData::ApiKey { key, .. } => key.into_bytes(),
        SecretData::Login { password, .. } => password.into_bytes(),
        SecretData::OAuthApp { client_secret, .. } => client_secret.into_bytes(),
        SecretData::SshKey { private_key, .. } => private_key.into_bytes(),
        SecretData::Environment { .. } => {
            return Err(CredError::InvalidInput(
                "environment secrets have no single key value".into(),
            )
            .into())
        }
    };
    Ok(Zeroizing::new(bytes))
}

/// Parse a stored ssh_key secret into an ed25519 signing key. The stored
/// PEM never appears in errors; parse failures report only the shape problem.
fn ed25519_signing_key(data: SecretData) -> Result<ed25519_dalek::SigningKey, AppError> {
    let SecretData::SshKey { private_key, .. } = data else {
        return Err(
            CredError::InvalidInput("ed25519 requires an ssh_key-type secret".into()).into(),
        );
    };
    let key = ssh_key::PrivateKey::from_openssh(private_key.as_bytes())
        .map_err(|_| CredError::InvalidInput("stored key is not OpenSSH-format".into()))?;
    let pair = key
        .key_data()
        .ed25519()
        .ok_or_else(|| CredError::InvalidInput("stored key is not ed25519".into()))?;
    Ok(ed25519_dalek::SigningKey::from_bytes(
        &pair.private.to_bytes(),
    ))
}

/// Record a mode operation in the phylax audit log (never key material).
async fn audit_mode(
    state: &PhylaxState,
    auth: &kleos_credd::auth::AuthInfo,
    action: &str,
    category: &str,
    name: &str,
    success: bool,
) {
    let agent_name = auth.agent_name().unwrap_or("master").to_string();
    let _ = log_phylax_audit(
        &state.inner.db,
        auth.user_id(),
        Some(&agent_name),
        None,
        None,
        None,
        None,
        action,
        category,
        name,
        success,
        None,
    )
    .await;
}

/// POST /resolve/sign -- sign a payload with a stored secret. The key never
/// leaves the process; only the signature is returned.
pub async fn sign_payload(
    Auth(auth): Auth,
    State(state): State<PhylaxState>,
    Json(body): Json<SignModeRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Validate client-shaped inputs before touching the vault so input
    // errors surface as 400 rather than as access-control responses.
    if body.algo != ALGO_HMAC && body.algo != ALGO_ED25519 {
        return Err(CredError::InvalidInput(format!("unknown algo '{}'", body.algo)).into());
    }
    let payload = B64
        .decode(&body.payload_b64)
        .map_err(|_| CredError::InvalidInput("bad payload_b64".into()))?;

    let data = load_secret(&state, &auth, &body.category, &body.name).await?;
    let signature = match body.algo.as_str() {
        ALGO_HMAC => {
            let key = secret_key_bytes(data)?;
            let mut mac =
                Hmac::<Sha256>::new_from_slice(&key).expect("hmac accepts any key length");
            mac.update(&payload);
            mac.finalize().into_bytes().to_vec()
        }
        ALGO_ED25519 => {
            let signing_key = ed25519_signing_key(data)?;
            signing_key.sign(&payload).to_bytes().to_vec()
        }
        other => {
            return Err(CredError::InvalidInput(format!("unknown algo '{other}'")).into());
        }
    };

    audit_mode(
        &state,
        &auth,
        actions::SIGN_RESOLVED,
        &body.category,
        &body.name,
        true,
    )
    .await;
    Ok(Json(json!({ "signature_b64": B64.encode(signature) })))
}

/// POST /resolve/verify -- check a signature against a stored secret.
/// Returns only {"valid": bool}; never key material.
pub async fn verify_payload(
    Auth(auth): Auth,
    State(state): State<PhylaxState>,
    Json(body): Json<VerifyModeRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    if body.algo != ALGO_HMAC && body.algo != ALGO_ED25519 {
        return Err(CredError::InvalidInput(format!("unknown algo '{}'", body.algo)).into());
    }
    let payload = B64
        .decode(&body.payload_b64)
        .map_err(|_| CredError::InvalidInput("bad payload_b64".into()))?;
    let signature = B64
        .decode(&body.signature_b64)
        .map_err(|_| CredError::InvalidInput("bad signature_b64".into()))?;

    let data = load_secret(&state, &auth, &body.category, &body.name).await?;
    let valid = match body.algo.as_str() {
        ALGO_HMAC => {
            let key = secret_key_bytes(data)?;
            let mut mac =
                Hmac::<Sha256>::new_from_slice(&key).expect("hmac accepts any key length");
            mac.update(&payload);
            let expected = mac.finalize().into_bytes();
            // Constant-time compare; a wrong-length signature is simply
            // invalid, not an error worth distinguishing.
            expected.len() == signature.len()
                && expected.as_slice().ct_eq(&signature).unwrap_u8() == 1
        }
        ALGO_ED25519 => {
            let verifying_key = ed25519_signing_key(data)?.verifying_key();
            match ed25519_dalek::Signature::from_slice(&signature) {
                Ok(sig) => verifying_key.verify(&payload, &sig).is_ok(),
                Err(_) => false,
            }
        }
        other => {
            return Err(CredError::InvalidInput(format!("unknown algo '{other}'")).into());
        }
    };

    audit_mode(
        &state,
        &auth,
        actions::VERIFY_RESOLVED,
        &body.category,
        &body.name,
        true,
    )
    .await;
    Ok(Json(json!({ "valid": valid })))
}

/// POST /resolve/derive -- HKDF-SHA256 key derivation from a stored secret.
/// The purpose string domain-separates outputs; the root secret is
/// unrecoverable from any derived key.
pub async fn derive_key_material(
    Auth(auth): Auth,
    State(state): State<PhylaxState>,
    Json(body): Json<DeriveModeRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    if body.purpose.is_empty() {
        return Err(CredError::InvalidInput("purpose must be non-empty".into()).into());
    }
    if body.length == 0 || body.length > MAX_DERIVE_LEN {
        return Err(CredError::InvalidInput(format!("length must be 1..={MAX_DERIVE_LEN}")).into());
    }

    let data = load_secret(&state, &auth, &body.category, &body.name).await?;
    let key = secret_key_bytes(data)?;
    let hk = Hkdf::<Sha256>::new(None, &key);
    let mut okm = Zeroizing::new(vec![0u8; body.length]);
    hk.expand(
        format!("phylax-derive:{}", body.purpose).as_bytes(),
        &mut okm,
    )
    .map_err(|_| CredError::InvalidInput("derive length invalid".into()))?;

    audit_mode(
        &state,
        &auth,
        actions::DERIVE_RESOLVED,
        &body.category,
        &body.name,
        true,
    )
    .await;
    Ok(Json(json!({ "derived_b64": B64.encode(&*okm) })))
}
