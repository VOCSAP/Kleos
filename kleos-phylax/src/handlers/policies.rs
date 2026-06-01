//! Access policy CRUD handlers. Master-only.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use kleos_cred::CredError;
use kleos_credd::auth::Auth;
use kleos_credd::handlers::AppError;

use crate::models::policy;
use crate::state::PhylaxState;

/// Request body for creating a policy.
#[derive(Deserialize)]
pub struct CreatePolicyRequest {
    /// Namespace this policy applies to.
    pub namespace: String,
    /// Category filter (optional).
    pub category: Option<String>,
    /// Secret name filter (optional).
    pub secret_name: Option<String>,
    /// Whether approval is required.
    pub require_approval: bool,
    /// Allowed resolve modes.
    pub allowed_modes: Option<Vec<String>>,
}

/// Request body for updating a policy.
#[derive(Deserialize)]
pub struct UpdatePolicyRequest {
    /// Whether approval is required.
    pub require_approval: bool,
    /// Allowed resolve modes.
    pub allowed_modes: Vec<String>,
}

/// List all access policies. Master-only.
pub async fn list_policies(
    Auth(auth): Auth,
    State(state): State<PhylaxState>,
) -> Result<impl IntoResponse, AppError> {
    if !auth.is_master() {
        return Err(CredError::PermissionDenied("master key required".into()).into());
    }

    let policies = policy::list_policies(&state.inner.db, auth.user_id()).await?;
    let items: Vec<_> = policies.iter().map(|p| p.to_json()).collect();
    Ok(Json(json!({ "policies": items })))
}

/// Create a new access policy. Master-only.
pub async fn create_policy(
    Auth(auth): Auth,
    State(state): State<PhylaxState>,
    Json(body): Json<CreatePolicyRequest>,
) -> Result<impl IntoResponse, AppError> {
    if !auth.is_master() {
        return Err(CredError::PermissionDenied("master key required".into()).into());
    }

    let modes = body
        .allowed_modes
        .unwrap_or_else(|| vec!["text".into(), "proxy".into(), "raw".into()]);

    let p = policy::create_policy(
        &state.inner.db,
        auth.user_id(),
        &body.namespace,
        body.category.as_deref(),
        body.secret_name.as_deref(),
        body.require_approval,
        &modes,
    )
    .await?;

    Ok(Json(p.to_json()))
}

/// Update an existing policy. Master-only.
pub async fn update_policy(
    Auth(auth): Auth,
    State(state): State<PhylaxState>,
    Path(id): Path<i64>,
    Json(body): Json<UpdatePolicyRequest>,
) -> Result<impl IntoResponse, AppError> {
    if !auth.is_master() {
        return Err(CredError::PermissionDenied("master key required".into()).into());
    }

    policy::update_policy(
        &state.inner.db,
        id,
        body.require_approval,
        &body.allowed_modes,
    )
    .await?;

    Ok(Json(json!({ "ok": true })))
}

/// Delete a policy. Master-only.
pub async fn delete_policy(
    Auth(auth): Auth,
    State(state): State<PhylaxState>,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    if !auth.is_master() {
        return Err(CredError::PermissionDenied("master key required".into()).into());
    }

    policy::delete_policy(&state.inner.db, id).await?;
    Ok(Json(json!({ "ok": true })))
}
