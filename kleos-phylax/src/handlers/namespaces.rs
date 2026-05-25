//! Namespace enumeration handler.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use kleos_cred::CredError;
use kleos_credd::auth::Auth;
use kleos_credd::handlers::AppError;

use crate::state::PhylaxState;

/// List distinct namespaces from access policies, scoped to the caller's permissions.
///
/// Master sees all namespaces. Agents see only namespaces they're allowed to access
/// (based on agent key namespace permissions, once implemented in Task 9).
pub async fn list_namespaces(
    Auth(auth): Auth,
    State(state): State<PhylaxState>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = auth.user_id();

    let namespaces: Vec<String> = state
        .inner
        .db
        .read(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT namespace FROM phylax_access_policies
                 WHERE user_id = ?1
                 ORDER BY namespace",
            )?;
            let rows = stmt.query_map(rusqlite::params![user_id], |row| row.get(0))?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
        .await
        .map_err(|e| CredError::Database(e.to_string()))?;

    Ok(Json(json!({ "namespaces": namespaces })))
}
