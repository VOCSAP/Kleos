//! SSH key settings handlers.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use kleos_cred::CredError;
use kleos_credd::auth::Auth;
use kleos_credd::handlers::AppError;

use crate::audit::{actions, log_phylax_audit};
use crate::models::ssh_settings;
use crate::state::PhylaxState;

/// Request body for updating SSH settings.
#[derive(Deserialize)]
pub struct UpdateSshRequest {
    /// Whether to auto-sign without approval.
    pub auto_sign: bool,
    /// Whether to auto-load into SSH agent on startup.
    pub auto_load: bool,
}

/// Get SSH settings for a specific secret.
pub async fn get_settings(
    Auth(auth): Auth,
    State(state): State<PhylaxState>,
    Path((category, name)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let settings =
        ssh_settings::get_ssh_settings(&state.inner.db, auth.user_id(), &category, &name).await?;

    match settings {
        Some(s) => Ok(Json(s.to_json())),
        None => Ok(Json(json!({
            "category": category,
            "secret_name": name,
            "auto_sign": false,
            "auto_load": false,
        }))),
    }
}

/// Update SSH settings for a specific secret. Master-only.
pub async fn update_settings(
    Auth(auth): Auth,
    State(state): State<PhylaxState>,
    Path((category, name)): Path<(String, String)>,
    Json(body): Json<UpdateSshRequest>,
) -> Result<impl IntoResponse, AppError> {
    if !auth.is_master() {
        return Err(CredError::PermissionDenied("master key required".into()).into());
    }

    let s = ssh_settings::upsert_ssh_settings(
        &state.inner.db,
        auth.user_id(),
        &category,
        &name,
        body.auto_sign,
        body.auto_load,
    )
    .await?;

    let _ = log_phylax_audit(
        &state.inner.db,
        auth.user_id(),
        None,
        actions::SSH_SETTINGS,
        &category,
        &name,
        true,
        None,
    )
    .await;

    Ok(Json(s.to_json()))
}
