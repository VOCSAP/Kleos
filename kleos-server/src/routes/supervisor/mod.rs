use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use rusqlite::params;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::sync::watch;

use crate::error::AppError;
use crate::extractors::{Auth, ResolvedDb};
use crate::state::AppState;

mod types;
use types::{InjectBody, InjectionRow, PendingQuery};

/// Patch 20 (2026-05-22): default upper bound on how long
/// `/supervisor/pending` keeps the long-poll connection open before
/// returning an empty list. Override via `KLEOS_SUPERVISOR_LONGPOLL_TIMEOUT_SECS`.
/// The client HTTP timeout should be set higher (typically
/// `KLEOS_HTTP_LONGPOLL_TIMEOUT_SECS = supervisor + ~30s`) so the server has
/// time to return its empty-list response gracefully before the client
/// aborts the connection.
const DEFAULT_SUPERVISOR_LONGPOLL_TIMEOUT_SECS: u64 = 30;

fn supervisor_longpoll_timeout() -> Duration {
    let secs = std::env::var("KLEOS_SUPERVISOR_LONGPOLL_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_SUPERVISOR_LONGPOLL_TIMEOUT_SECS);
    Duration::from_secs(secs)
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/supervisor/inject", post(inject_handler))
        .route("/supervisor/pending", get(pending_handler))
}

/// Return the `watch::Sender` for `user_id`, lazily creating one if missing.
/// Acquires the write lock only on the first long-poll or inject for a
/// given tenant; subsequent calls take the read lock and clone.
async fn notifier_for(state: &AppState, user_id: i64) -> watch::Sender<()> {
    {
        let map = state.supervisor_notifiers.read().await;
        if let Some(tx) = map.get(&user_id) {
            return tx.clone();
        }
    }
    let mut map = state.supervisor_notifiers.write().await;
    map.entry(user_id)
        .or_insert_with(|| {
            let (tx, _rx) = watch::channel(());
            tx
        })
        .clone()
}

/// POST /supervisor/inject
/// Persists a violation reported by eidolon-supervisor. The row is keyed by
/// the calling user_id and the supplied session_id and stays unclaimed until
/// the agent drains it via GET /supervisor/pending.
///
/// Patch 20: after a successful INSERT we signal the tenant's
/// `supervisor_notifiers` channel so any blocked long-poll for the same
/// `user_id` returns immediately with the newly available row. The signal
/// is best-effort: if no long-poll is currently parked, the channel send is
/// a no-op and the next `/supervisor/pending` will pick the row up via its
/// initial DB query before parking.
async fn inject_handler(
    State(state): State<AppState>,
    ResolvedDb(db): ResolvedDb,
    Auth(auth): Auth,
    Json(body): Json<InjectBody>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    if body.session_id.trim().is_empty() {
        return Err(AppError::from(kleos_lib::EngError::InvalidInput(
            "session_id is required".into(),
        )));
    }
    if body.rule_id.trim().is_empty() {
        return Err(AppError::from(kleos_lib::EngError::InvalidInput(
            "rule_id is required".into(),
        )));
    }

    let user_id = auth.effective_user_id();
    let id = db
        .write(move |conn| {
            conn.execute(
                "INSERT INTO supervisor_injections (user_id, session_id, rule_id, severity, message)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    user_id,
                    body.session_id,
                    body.rule_id,
                    body.severity,
                    body.message,
                ],
            )
            ?;
            Ok(conn.last_insert_rowid())
        })
        .await?;

    // Wake any parked long-poll for this tenant. send() returns Err if no
    // receivers are subscribed -- that means nobody is waiting, which is
    // fine; the next /supervisor/pending will see the freshly inserted row
    // in its initial DB query before entering the await.
    let tx = notifier_for(&state, user_id).await;
    let _ = tx.send(());

    Ok((StatusCode::CREATED, Json(json!({ "ok": true, "id": id }))))
}

/// GET /supervisor/pending?session_id=<id>
/// Atomically claims and returns all unclaimed injections for the calling
/// user_id and the supplied session_id.
///
/// Patch 20: now a proper long-poll. The handler:
/// 1. Attempts an immediate UPDATE-RETURNING. If at least one row is
///    claimed, returns immediately.
/// 2. Otherwise subscribes to the tenant's supervisor_notifiers channel and
///    awaits a signal (or the configured timeout, whichever comes first).
/// 3. On wake-up, retries the claim. If still empty and time remains, loops.
/// 4. On timeout (no signal during the configured window), returns an empty
///    list so the client can re-poll.
///
/// Exact-once semantics preserved by the SQL itself (`WHERE claimed_at IS
/// NULL RETURNING`): if multiple long-pollers for the same tenant wake on
/// the same signal, the first to execute the UPDATE claims the rows and
/// the others see zero rows on retry.
async fn pending_handler(
    State(state): State<AppState>,
    ResolvedDb(db): ResolvedDb,
    Auth(auth): Auth,
    Query(q): Query<PendingQuery>,
) -> Result<Json<Value>, AppError> {
    if q.session_id.trim().is_empty() {
        return Err(AppError::from(kleos_lib::EngError::InvalidInput(
            "session_id is required".into(),
        )));
    }

    let user_id = auth.effective_user_id();
    let session_id = q.session_id.clone();

    // Subscribe BEFORE the first claim attempt so an inject racing in
    // between the claim and the await still wakes us up.
    let tx = notifier_for(&state, user_id).await;
    let mut rx = tx.subscribe();
    // The watch channel marks the initial value as "seen" on subscribe by
    // default; mark it again explicitly so the first changed().await waits
    // for a future send(), not the initial state.
    rx.mark_unchanged();

    let deadline = tokio::time::Instant::now() + supervisor_longpoll_timeout();

    loop {
        let session_id_for_claim = session_id.clone();
        let user_id_for_claim = user_id;
        let claimed: Vec<InjectionRow> = db
            .transaction(move |tx| {
                let mut stmt = tx
                    .prepare(
                        "UPDATE supervisor_injections
                         SET claimed_at = datetime('now')
                         WHERE user_id = ?1 AND session_id = ?2 AND claimed_at IS NULL
                         RETURNING id, session_id, rule_id, severity, message, created_at",
                    )
                    .map_err(|e| kleos_lib::EngError::DatabaseMessage(e.to_string()))?;

                let rows = stmt
                    .query_map(params![user_id_for_claim, session_id_for_claim], |row| {
                        Ok(InjectionRow {
                            id: row.get(0)?,
                            session_id: row.get(1)?,
                            rule_id: row.get(2)?,
                            severity: row.get(3)?,
                            message: row.get(4)?,
                            created_at: row.get(5)?,
                        })
                    })
                    .map_err(|e| kleos_lib::EngError::DatabaseMessage(e.to_string()))?;

                let mut out = Vec::new();
                for r in rows {
                    out.push(r.map_err(|e| kleos_lib::EngError::DatabaseMessage(e.to_string()))?);
                }
                Ok(out)
            })
            .await?;

        if !claimed.is_empty() {
            let count = claimed.len();
            return Ok(Json(json!({
                "injections": claimed,
                "claimed": count,
                "session_id": q.session_id,
            })));
        }

        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Ok(Json(json!({
                "injections": Vec::<InjectionRow>::new(),
                "claimed": 0,
                "session_id": q.session_id,
            })));
        }

        let remaining = deadline - now;
        match tokio::time::timeout(remaining, rx.changed()).await {
            Ok(Ok(_)) => continue, // signal received, retry claim
            Ok(Err(_)) => {
                // Sender dropped (state torn down). Treat as timeout: return
                // empty list rather than 500. Rare in practice.
                return Ok(Json(json!({
                    "injections": Vec::<InjectionRow>::new(),
                    "claimed": 0,
                    "session_id": q.session_id,
                })));
            }
            Err(_) => {
                // tokio::time::timeout elapsed
                return Ok(Json(json!({
                    "injections": Vec::<InjectionRow>::new(),
                    "claimed": 0,
                    "session_id": q.session_id,
                })));
            }
        }
    }
}
