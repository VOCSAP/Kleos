use std::collections::HashMap;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::error::AppError;
use crate::extractors::{Auth, ResolvedDb};
use crate::state::AppState;
use kleos_lib::auth::{AuthContext, Scope};
use kleos_lib::db::Database;
use kleos_lib::services::brain::{
    get_memory_for_absorb, verify_memory_ownership, AbsorbRequest, DecayRequest, FeedbackRequest,
};
use kleos_lib::EngError;

mod types;
use types::BrainQueryRequest;

// H-R3-001: dream / decay / evolution_train mutate the global brain. Any
// auth+write user could pin CPU or corrupt the shared model. Gating these
// behind admin scope keeps the surface available to operators while denying
// it to ordinary tenants.
fn require_admin(auth: &AuthContext) -> Result<(), AppError> {
    if !auth.has_scope(&Scope::Admin) {
        return Err(AppError(kleos_lib::EngError::Auth(
            "admin scope required for global brain mutations".into(),
        )));
    }
    Ok(())
}

/// Upper bound on /brain/decay ticks per call. Exists so a caller cannot
/// pass body.ticks = u32::MAX and pin the decay loop. The chosen value is
/// large enough for any realistic decay sweep without being weaponizable.
const MAX_DECAY_TICKS: u32 = 10_000;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/brain/stats", get(stats_handler))
        .route("/brain/query", post(query_handler))
        .route("/brain/absorb", post(absorb_handler))
        .route("/brain/dream", post(dream_handler))
        .route("/brain/feedback", post(feedback_handler))
        .route("/brain/decay", post(decay_handler))
        .route(
            "/brain/evolution/feedback",
            post(evolution_feedback_handler),
        )
        .route("/brain/evolution/train", post(evolution_train_handler))
        .route("/brain/evolution/stats", get(evolution_stats_handler))
}

async fn require_brain(state: &AppState) -> Result<(), AppError> {
    if let Some(ref brain) = state.brain {
        if brain.is_ready() {
            return Ok(());
        }
    }
    Err(AppError(kleos_lib::EngError::Internal(
        "brain not ready".into(),
    )))
}

// Stats are per-tenant: counts the caller's own patterns.
async fn stats_handler(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<Value>, AppError> {
    require_brain(&state).await?;
    let brain = state
        .brain
        .as_ref()
        .ok_or_else(|| AppError(kleos_lib::EngError::Internal("brain not configured".into())))?;
    let stats = brain.stats(auth.effective_user_id()).await?;
    Ok(Json(json!({ "ok": true, "stats": stats })))
}

// Query scopes the recall to the caller's pattern space via auth.effective_user_id().
// Patch 36 -- accepts an optional `space` / `space_id` / `include_unscoped`
// trio that post-filters the activated patterns returned by `brain.query`.
// The Hopfield substrate stays global (cf. plan Patch 33 section 4
// paragraphe Brain); filtering happens here, after ranking, so the brain
// engine is not modified.
async fn query_handler(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Json(body): Json<BrainQueryRequest>,
) -> Result<Json<Value>, AppError> {
    require_brain(&state).await?;
    let brain = state
        .brain
        .as_ref()
        .ok_or_else(|| AppError(kleos_lib::EngError::Internal("brain not configured".into())))?;
    let embedder = state.current_embedder().await.ok_or_else(|| {
        AppError(kleos_lib::EngError::Internal(
            "embedder not ready (still loading)".into(),
        ))
    })?;

    // Resolve target space before doing the Hopfield work so an invalid
    // payload short-circuits with 400 instead of consuming brain CPU.
    let target_space = kleos_lib::space::resolve_space_filter(
        &db,
        auth.effective_user_id(),
        body.space_id,
        body.space.as_deref(),
    )
    .await?;

    let mut result = brain
        .query(
            embedder.as_ref(),
            &body.inner.query,
            auth.effective_user_id(),
            &body.inner,
        )
        .await?;

    if let Some(target_id) = target_space {
        let ids: Vec<i64> = result.activated.iter().map(|m| m.id).collect();
        if !ids.is_empty() {
            let space_map = load_memory_space_ids(&db, &ids).await?;
            let include_unscoped = body.include_unscoped;
            result.activated.retain(|m| match space_map.get(&m.id) {
                Some(Some(sid)) => *sid == target_id,
                Some(None) => include_unscoped,
                None => false,
            });
        }
    }

    Ok(Json(json!({ "ok": true, "result": result })))
}

/// Patch 36 helper -- batch lookup of `memories.space_id` for the subset
/// of ids returned by `brain.query`. Returns a `HashMap` so the handler
/// can resolve each pattern in O(1) during the retain pass. The query
/// is a single SELECT with inlined `?` placeholders bounded by the
/// activated set size (typically <= top_k, small).
async fn load_memory_space_ids(
    db: &Database,
    ids: &[i64],
) -> Result<HashMap<i64, Option<i64>>, AppError> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = std::iter::repeat("?")
        .take(ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT id, space_id FROM memories WHERE id IN ({})",
        placeholders
    );
    let ids_owned: Vec<i64> = ids.to_vec();
    let rows = db
        .read(move |conn| -> Result<Vec<(i64, Option<i64>)>, EngError> {
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| EngError::DatabaseMessage(e.to_string()))?;
            let params = rusqlite::params_from_iter(ids_owned.iter());
            let mapped = stmt
                .query_map(params, |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?))
                })
                .map_err(|e| EngError::DatabaseMessage(e.to_string()))?;
            mapped
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| EngError::DatabaseMessage(e.to_string()))
        })
        .await
        .map_err(AppError)?;
    Ok(rows.into_iter().collect())
}

// C-R3-001: absorb fetches the memory from the caller's tenant DB and pipes
// auth.effective_user_id() into get_memory_for_absorb so monolith fetches still enforce
// ownership.
async fn absorb_handler(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Json(body): Json<AbsorbRequest>,
) -> Result<Json<Value>, AppError> {
    require_brain(&state).await?;
    let brain = state
        .brain
        .as_ref()
        .ok_or_else(|| AppError(kleos_lib::EngError::Internal("brain not configured".into())))?;
    let embedder = state.current_embedder().await.ok_or_else(|| {
        AppError(kleos_lib::EngError::Internal(
            "embedder not ready (still loading)".into(),
        ))
    })?;
    let memory = get_memory_for_absorb(&db, body.id, auth.effective_user_id()).await?;
    brain
        .absorb(embedder.as_ref(), auth.effective_user_id(), memory)
        .await?;
    Ok(Json(json!({ "ok": true, "id": body.id })))
}

// H-R3-001: dream_cycle is a global mutation; admin only.
async fn dream_handler(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<Value>, AppError> {
    require_admin(&auth)?;
    require_brain(&state).await?;
    let brain = state
        .brain
        .as_ref()
        .ok_or_else(|| AppError(kleos_lib::EngError::Internal("brain not configured".into())))?;
    let result = brain.dream_cycle().await?;
    Ok(Json(json!({ "ok": true, "result": result })))
}

// C-R3-001: feedback verifies that every memory_id in the body is owned by
// the calling user before it influences the brain. Previously the helper
// only checked existence -- the name lied.
async fn feedback_handler(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Json(body): Json<FeedbackRequest>,
) -> Result<Json<Value>, AppError> {
    require_brain(&state).await?;

    let owned = verify_memory_ownership(&db, &body.memory_ids, auth.effective_user_id()).await?;
    if !owned {
        return Err(AppError(kleos_lib::EngError::Auth(
            "One or more memory_ids not found or not owned by you".into(),
        )));
    }

    let brain = state
        .brain
        .as_ref()
        .ok_or_else(|| AppError(kleos_lib::EngError::Internal("brain not configured".into())))?;
    let result = brain
        .feedback_signal(
            auth.effective_user_id(),
            body.memory_ids,
            body.edge_pairs,
            body.useful,
        )
        .await?;
    Ok(Json(json!({ "ok": true, "result": result })))
}

// H-R3-001: decay tick was unbounded i64; any auth+write user could pass
// i64::MAX and saturate the decay loop. Now admin-only and clamped to
// MAX_DECAY_TICKS.
async fn decay_handler(
    State(state): State<AppState>,
    Auth(auth): Auth,
    Json(body): Json<DecayRequest>,
) -> Result<Json<Value>, AppError> {
    require_admin(&auth)?;
    require_brain(&state).await?;
    let brain = state
        .brain
        .as_ref()
        .ok_or_else(|| AppError(kleos_lib::EngError::Internal("brain not configured".into())))?;
    let ticks = body.ticks.clamp(0, MAX_DECAY_TICKS);
    brain.decay_tick(auth.effective_user_id(), ticks).await?;
    Ok(Json(json!({ "ok": true, "ticks_applied": ticks })))
}

// C-R3-001: same ownership gate as feedback_handler.
async fn evolution_feedback_handler(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Json(body): Json<FeedbackRequest>,
) -> Result<Json<Value>, AppError> {
    require_brain(&state).await?;

    let owned = verify_memory_ownership(&db, &body.memory_ids, auth.effective_user_id()).await?;
    if !owned {
        return Err(AppError(kleos_lib::EngError::Auth(
            "One or more memory_ids not found or not owned by you".into(),
        )));
    }

    let brain = state
        .brain
        .as_ref()
        .ok_or_else(|| AppError(kleos_lib::EngError::Internal("brain not configured".into())))?;
    let result = brain
        .feedback_signal(
            auth.effective_user_id(),
            body.memory_ids,
            body.edge_pairs,
            body.useful,
        )
        .await?;
    Ok(Json(json!({ "ok": true, "result": result })))
}

// H-R3-001: evolution training touches the global model; admin only.
async fn evolution_train_handler(
    State(state): State<AppState>,
    Auth(auth): Auth,
) -> Result<Json<Value>, AppError> {
    require_admin(&auth)?;
    require_brain(&state).await?;
    let brain = state
        .brain
        .as_ref()
        .ok_or_else(|| AppError(kleos_lib::EngError::Internal("brain not configured".into())))?;
    let result = brain.evolution_train().await?;
    Ok(Json(json!({ "ok": true, "result": result })))
}

async fn evolution_stats_handler(
    State(state): State<AppState>,
    Auth(_auth): Auth,
) -> Result<Json<Value>, AppError> {
    require_brain(&state).await?;
    let brain = state
        .brain
        .as_ref()
        .ok_or_else(|| AppError(kleos_lib::EngError::Internal("brain not configured".into())))?;
    let result = brain.evolution_stats().await?;
    Ok(Json(json!({ "ok": true, "result": result })))
}
