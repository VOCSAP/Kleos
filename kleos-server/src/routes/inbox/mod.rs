use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

use crate::error::AppError;
use crate::extractors::{Auth, ResolvedDb};
use crate::state::AppState;

mod types;
use types::{BulkBody, EditBody, PagingQuery, RejectBody};

/// Run the post-store derivation for a memory that has just cleared review, so an
/// approved memory finally seeds facts, entity links, and brain associations --
/// the derivation the store route deliberately defers while the memory is pending.
/// Best-effort: a fetch failure is logged, not propagated, because the approval it
/// follows has already committed.
async fn derive_after_approve(
    state: &AppState,
    db: &std::sync::Arc<kleos_lib::db::Database>,
    id: i64,
    user_id: i64,
) {
    match kleos_lib::memory::get(db, id, user_id).await {
        Ok(m) => {
            crate::routes::memory::spawn_post_store_derivation(
                state,
                db,
                id,
                user_id,
                m.content,
                m.category,
                m.source,
                m.importance as f64,
            )
            .await;
        }
        Err(e) => tracing::warn!(
            memory_id = id,
            error = %e,
            "post-approve derivation skipped: memory fetch failed"
        ),
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/inbox", get(list_inbox))
        .route("/inbox/{id}/approve", post(approve))
        .route("/inbox/{id}/reject", post(reject))
        .route("/inbox/{id}/edit", post(edit))
        .route("/inbox/bulk", post(bulk_action))
        .route("/pending", get(list_pending_legacy))
}

async fn list_inbox(
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Query(q): Query<PagingQuery>,
) -> Result<Json<Value>, AppError> {
    // Clamp to at least 1 so a client-supplied limit of 0 does not silently
    // return an empty page, and at most 200 to bound the response size.
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0);
    let user_id = auth.effective_user_id();
    let pending = kleos_lib::inbox::list_pending(&db, user_id, limit, offset).await?;
    let total = kleos_lib::inbox::count_pending(&db, user_id).await?;
    Ok(Json(
        json!({ "pending": pending, "count": pending.len(), "total": total, "offset": offset, "limit": limit }),
    ))
}

async fn approve(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Path(id): Path<i64>,
) -> Result<Json<Value>, AppError> {
    let user_id = auth.effective_user_id();
    kleos_lib::inbox::approve_memory(&db, id, user_id).await?;
    // The memory has cleared review: run the fact/entity/brain derivation the
    // store route defers for gated memories.
    derive_after_approve(&state, &db, id, user_id).await;
    Ok(Json(json!({ "approved": true, "id": id })))
}

async fn reject(
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Path(id): Path<i64>,
    Json(body): Json<RejectBody>,
) -> Result<Json<Value>, AppError> {
    let user_id = auth.effective_user_id();
    kleos_lib::inbox::reject_memory(&db, id, user_id).await?;
    if let Some(reason) = &body.reason {
        if let Err(e) = kleos_lib::inbox::set_forget_reason(&db, id, user_id, reason).await {
            tracing::warn!(
                memory_id = id,
                user_id = auth.effective_user_id(),
                error = %e,
                "failed to record forget reason after inbox reject",
            );
        }
    }
    Ok(Json(json!({ "rejected": true, "id": id })))
}

// SECURITY: scopes to the caller's user_id so monolith (shared-DB) mode cannot
// edit another tenant's pending memory by id. The predicate is a no-op in a
// single-owner shard.
async fn edit(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Path(id): Path<i64>,
    Json(body): Json<EditBody>,
) -> Result<Json<Value>, AppError> {
    let user_id = auth.effective_user_id();
    kleos_lib::inbox::edit_and_approve(
        &db,
        id,
        user_id,
        body.content.as_deref(),
        body.category.as_deref(),
        body.importance,
        body.tags.as_deref(),
    )
    .await?;
    // edit_and_approve also approves, so derive from the edited (approved) content.
    derive_after_approve(&state, &db, id, user_id).await;
    Ok(Json(json!({ "approved": true, "edited": true, "id": id })))
}

async fn bulk_action(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Json(body): Json<BulkBody>,
) -> Result<Json<Value>, AppError> {
    let user_id = auth.effective_user_id();
    let mut count = 0;
    for id in &body.ids {
        match body.action.as_str() {
            "approve" => {
                kleos_lib::inbox::approve_memory(&db, *id, user_id).await?;
                // Same deferred derivation the single-approve path runs.
                derive_after_approve(&state, &db, *id, user_id).await;
                count += 1;
            }
            "reject" => {
                kleos_lib::inbox::reject_memory(&db, *id, user_id).await?;
                count += 1;
            }
            _ => {
                return Err(AppError(kleos_lib::EngError::InvalidInput(
                    "action must be approve or reject".into(),
                )))
            }
        }
    }
    Ok(Json(
        json!({ "action": body.action, "count": count, "ids": body.ids }),
    ))
}

async fn list_pending_legacy(
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Query(q): Query<PagingQuery>,
) -> Result<Json<Value>, AppError> {
    // Clamp to at least 1 so a client-supplied limit of 0 does not silently
    // return an empty page, and at most 200 to bound the response size.
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let offset = q.offset.unwrap_or(0);
    let user_id = auth.effective_user_id();
    let pending = kleos_lib::inbox::list_pending(&db, user_id, limit, offset).await?;
    let total = kleos_lib::inbox::count_pending(&db, user_id).await?;
    Ok(Json(
        json!({ "pending": pending, "count": pending.len(), "total": total, "offset": offset, "limit": limit }),
    ))
}
