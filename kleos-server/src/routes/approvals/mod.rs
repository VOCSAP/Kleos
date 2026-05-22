use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::time::Duration;

use crate::error::AppError;
use crate::extractors::{Auth, ResolvedDb};
use crate::state::AppState;
use kleos_lib::approvals::{
    create_approval, decide, expire_stale, get_approval, list_pending, CreateApprovalRequest,
    DecideRequest,
};

mod types;
use types::{ApprovalResponse, DecideBody};

/// Patch 20c (2026-05-22): default upper bound on how long
/// `/approvals/pending` keeps the long-poll connection open before
/// returning the (possibly empty) list. Override via
/// `KLEOS_APPROVALS_LONGPOLL_TIMEOUT_SECS`. Mirrors the supervisor
/// counterpart so a single client-side `KLEOS_HTTP_LONGPOLL_TIMEOUT_SECS`
/// covers both endpoints with the same `client > server` margin.
const DEFAULT_APPROVALS_LONGPOLL_TIMEOUT_SECS: u64 = 30;

fn approvals_longpoll_timeout() -> Duration {
    let secs = std::env::var("KLEOS_APPROVALS_LONGPOLL_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_APPROVALS_LONGPOLL_TIMEOUT_SECS);
    Duration::from_secs(secs)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/approvals", post(create_handler))
        .route("/approvals/pending", get(list_pending_handler))
        .route("/approvals/{id}", get(get_handler))
        .route("/approvals/{id}/decide", post(decide_handler))
}

async fn create_handler(
    State(state): State<AppState>,
    ResolvedDb(db): ResolvedDb,
    Auth(auth): Auth,
    Json(body): Json<CreateApprovalRequest>,
) -> Result<(StatusCode, Json<ApprovalResponse>), AppError> {
    let approval = create_approval(&db, &body, auth.user_id).await?;

    // Notify any waiting watchers that a new approval is pending
    if let Some(ref tx) = state.approval_notify {
        let _ = tx.send(());
    }

    Ok((StatusCode::CREATED, Json(approval.into())))
}

async fn get_handler(
    ResolvedDb(db): ResolvedDb,
    Auth(auth): Auth,
    Path(id): Path<String>,
) -> Result<Json<ApprovalResponse>, AppError> {
    let approval = get_approval(&db, &id, auth.user_id)
        .await?
        .ok_or_else(|| kleos_lib::EngError::NotFound(format!("approval {} not found", id)))?;

    Ok(Json(approval.into()))
}

/// GET /approvals/pending
/// Returns approvals waiting for a human decision for the authenticated
/// user. Patch 20c turns this into a long-poll proper: when the user's
/// queue is empty, the handler subscribes to `state.approval_notify`
/// (signalled by `create_handler` and `decide_handler`) and awaits either
/// a notification or the configured deadline before returning. This
/// removes the previous hot-loop pattern where the TUI re-polled the
/// endpoint every few hundred milliseconds, blowing past the per-IP
/// pre-auth rate limit (20/min).
///
/// The notify channel is global (shared with `/gate/check` and other
/// approval flows), so a wake-up may fire for an event belonging to
/// another tenant. That is harmless: the handler re-queries the DB
/// filtered by `auth.user_id` and returns empty if nothing matches,
/// then re-enters the await. Cost is one cheap DB query per spurious
/// wake-up, well below the cost of busy-polling.
async fn list_pending_handler(
    State(state): State<AppState>,
    ResolvedDb(db): ResolvedDb,
    Auth(auth): Auth,
) -> Result<Json<Value>, AppError> {
    // Subscribe before the first claim attempt so a create racing in
    // between the claim and the await still wakes us up.
    let mut rx = state.approval_notify.as_ref().map(|tx| {
        let mut rx = tx.subscribe();
        rx.mark_unchanged();
        rx
    });

    let deadline = tokio::time::Instant::now() + approvals_longpoll_timeout();

    loop {
        // Expire stale approvals on every iteration; the work is cheap
        // and keeps the table from accumulating zombies across long-polls.
        let expired_count = expire_stale(&db).await?;
        let approvals = list_pending(&db, auth.user_id).await?;
        let responses: Vec<ApprovalResponse> = approvals.into_iter().map(Into::into).collect();
        let count = responses.len();

        if count > 0 {
            return Ok(Json(json!({
                "approvals": responses,
                "count": count,
                "expired_count": expired_count,
            })));
        }

        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Ok(Json(json!({
                "approvals": Vec::<ApprovalResponse>::new(),
                "count": 0,
                "expired_count": expired_count,
            })));
        }

        let remaining = deadline - now;
        match rx.as_mut() {
            Some(rx_inner) => {
                // Either a signal arrives or the per-iter timeout elapses;
                // either way we loop and re-check the DB. A dropped
                // sender (`Err`) is treated like a timeout: we let the
                // outer deadline check terminate us on the next iter.
                let _ = tokio::time::timeout(remaining, rx_inner.changed()).await;
            }
            None => {
                // No notify channel configured (degenerate test setup);
                // fall back to a short sleep so we do not spin.
                tokio::time::sleep(remaining.min(Duration::from_secs(1))).await;
            }
        }
    }
}

async fn decide_handler(
    State(state): State<AppState>,
    ResolvedDb(db): ResolvedDb,
    Auth(auth): Auth,
    Path(id): Path<String>,
    Json(body): Json<DecideBody>,
) -> Result<Json<ApprovalResponse>, AppError> {
    let req = DecideRequest {
        decision: body.decision,
        decided_by: body.decided_by,
        reason: body.reason,
    };

    let approval = decide(&db, &id, &req, auth.user_id).await?;

    // Notify any waiting watchers that a decision was made
    if let Some(ref tx) = state.approval_notify {
        let _ = tx.send(());
    }

    Ok(Json(approval.into()))
}
