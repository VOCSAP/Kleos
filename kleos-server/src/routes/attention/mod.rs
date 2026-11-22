use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::AppError;
use crate::extractors::{Auth, ResolvedDb};
use crate::state::AppState;
use kleos_lib::attention::{self, CreateNoteRequest, UpdateNoteRequest};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/attention", get(list_notes))
        .route("/attention", post(create_note))
        .route("/attention/{id}", patch(update_note))
        .route("/attention/{id}", delete(delete_note))
}

#[derive(Deserialize)]
struct ListParams {
    limit: Option<i64>,
}

// POST /attention
async fn create_note(
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Json(body): Json<CreateNoteRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let note = attention::create_note(&db, body, auth.effective_user_id()).await?;
    Ok((StatusCode::CREATED, Json(json!(note))))
}

// GET /attention
async fn list_notes(
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Query(params): Query<ListParams>,
) -> Result<Json<Value>, AppError> {
    let limit = params.limit.unwrap_or(50).clamp(1, 200);
    let notes = attention::list_notes(&db, auth.effective_user_id(), limit).await?;
    Ok(Json(json!({ "notes": notes, "count": notes.len() })))
}

// PATCH /attention/{id}
async fn update_note(
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Path(id): Path<i64>,
    Json(body): Json<UpdateNoteRequest>,
) -> Result<Json<Value>, AppError> {
    let note = attention::update_note(&db, id, body, auth.effective_user_id()).await?;
    Ok(Json(json!(note)))
}

// DELETE /attention/{id}
async fn delete_note(
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    attention::delete_note(&db, id, auth.effective_user_id()).await?;
    Ok(StatusCode::NO_CONTENT)
}
