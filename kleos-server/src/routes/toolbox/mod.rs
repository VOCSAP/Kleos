//! HTTP surface for the toolbox catalog (Patch 53).
//!
//! Six routes over two tables that never meet in SQL: the shared sheets live in
//! the main database (`state.db`, where the monolith overlays created
//! `toolbox_tools`), the caller's locations live in whatever database
//! [`ResolvedDb`] resolves to (their shard, or the same main database in
//! monolith mode). Every handler therefore carries both.
//!
//! Three invariants hold across all of them:
//!
//! * **No shared read without the caller's key filter.** A handler first asks
//!   the caller's database which tool keys they own a location for; the shared
//!   table is only ever queried through that list. A tool nobody on this shard
//!   has indexed is invisible, whoever else stored it.
//! * **`space` / `space_id` are ignored.** The MCP bridge injects them into
//!   every call it dispatches; the request types are plain serde structs with no
//!   `deny_unknown_fields`, so they fall on the floor. The toolbox sits outside
//!   the spaces partitioning on purpose.
//! * **The server stores and retrieves, nothing else.** No clone, no fetch, no
//!   directory walk: the client digests the tool and sends the sheet.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::AppError;
use crate::extractors::{Auth, ResolvedDb};
use crate::state::AppState;
use kleos_lib::toolbox::{self, FindOptions, ToolEntry, ToolLocation, UpsertToolRequest};
use kleos_lib::{kleos_env, EngError};

// --- Limits ---------------------------------------------------------------

/// Default ceiling on a sheet's markdown body, overridable with
/// `KLEOS_TOOLBOX_MAX_BODY_BYTES`. Well under the 2 MiB global body limit.
const DEFAULT_MAX_BODY_BYTES: usize = 262_144;
/// A summary is what gets embedded; a long one is a bug in the client prompt.
const MAX_SUMMARY_BYTES: usize = 4096;
const MAX_NAME_BYTES: usize = 512;
const MAX_KIND_BYTES: usize = 64;
const MAX_KEYWORDS: usize = 64;
const MAX_TAGS: usize = 64;
const MAX_NOTES_BYTES: usize = 4096;

/// Listing page size. Entries carry their full body, so the page stays small.
const DEFAULT_LIST_LIMIT: usize = 25;
const MAX_LIST_LIMIT: usize = 100;

/// How many sheets one reindex call will embed before returning. Each one is a
/// round-trip to the embedder, so the work is bounded and the route is meant to
/// be called again until `candidates` comes back zero.
const DEFAULT_REINDEX_LIMIT: usize = 200;
const MAX_REINDEX_LIMIT: usize = 1000;

/// Recorded as `embedding_model` when nothing names the loaded embedder.
const UNKNOWN_EMBEDDING_MODEL: &str = "unknown";

// --- Environment ----------------------------------------------------------

/// Parse `KLEOS_TOOLBOX_MAX_BODY_BYTES`. A missing, unparsable or zero value
/// falls back to the default rather than disabling the limit.
fn parse_max_body_bytes(raw: Option<&str>) -> usize {
    raw.and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_MAX_BODY_BYTES)
}

fn max_body_bytes() -> usize {
    parse_max_body_bytes(kleos_env("TOOLBOX_MAX_BODY_BYTES").ok().as_deref())
}

/// Parse a boolean-ish env value (`1`, `true`, `yes`, `on`, and their
/// negatives, case-insensitive). Anything else keeps `default`.
fn parse_flag(raw: Option<&str>, default: bool) -> bool {
    match raw.map(|v| v.trim().to_ascii_lowercase()) {
        Some(v) if matches!(v.as_str(), "1" | "true" | "yes" | "on") => true,
        Some(v) if matches!(v.as_str(), "0" | "false" | "no" | "off") => false,
        _ => default,
    }
}

/// Default for a find's `rerank` flag: `KLEOS_TOOLBOX_RERANK`, off unless set.
fn rerank_default() -> bool {
    parse_flag(kleos_env("TOOLBOX_RERANK").ok().as_deref(), false)
}

/// Name recorded alongside a stored vector. `EmbeddingProvider` is object-safe
/// and exposes no name, so this comes from configuration:
/// `KLEOS_TOOLBOX_EMBEDDING_MODEL` first, then the `KLEOS_EMBEDDING_MODEL` the
/// OpenAI-compatible provider already reads, then a placeholder. The name only
/// has to be stable: a reindex compares it to decide what to recompute.
fn embedding_model_name() -> String {
    kleos_env("TOOLBOX_EMBEDDING_MODEL")
        .ok()
        .or_else(|| kleos_env("EMBEDDING_MODEL").ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| UNKNOWN_EMBEDDING_MODEL.to_string())
}

// --- Validation -----------------------------------------------------------

fn too_long(field: &str, len: usize, max: usize) -> EngError {
    EngError::InvalidInput(format!("{field} is {len} bytes, max {max}"))
}

/// Reject a sheet the catalog should not store. Everything here is a client
/// bug, not a server condition, so all of it is a 400.
fn validate_upsert(req: &UpsertToolRequest, max_body: usize) -> Result<(), EngError> {
    for (field, value, max) in [
        ("name", req.name.as_str(), MAX_NAME_BYTES),
        ("summary", req.summary.as_str(), MAX_SUMMARY_BYTES),
        ("kind", req.kind.as_str(), MAX_KIND_BYTES),
    ] {
        if value.trim().is_empty() {
            return Err(EngError::InvalidInput(format!("{field} is required")));
        }
        if value.len() > max {
            return Err(too_long(field, value.len(), max));
        }
    }
    if req.body.len() > max_body {
        return Err(too_long("body", req.body.len(), max_body));
    }
    if req.notes.len() > MAX_NOTES_BYTES {
        return Err(too_long("notes", req.notes.len(), MAX_NOTES_BYTES));
    }
    if req.keywords.len() > MAX_KEYWORDS {
        return Err(EngError::InvalidInput(format!(
            "keywords holds {} entries, max {MAX_KEYWORDS}",
            req.keywords.len()
        )));
    }
    if req.tags.len() > MAX_TAGS {
        return Err(EngError::InvalidInput(format!(
            "tags holds {} entries, max {MAX_TAGS}",
            req.tags.len()
        )));
    }
    Ok(())
}

/// Clamp a caller-supplied count into `1..=max`, with `default` when absent.
fn clamp(raw: Option<usize>, default: usize, max: usize) -> usize {
    match raw {
        None => default,
        Some(0) => 1,
        Some(n) => n.min(max),
    }
}

// --- Router ---------------------------------------------------------------

/// Register the `/toolbox` routes.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/toolbox/entries", post(index_entry).get(list_entries))
        .route("/toolbox/entries/{id}", get(get_entry).delete(forget_entry))
        .route("/toolbox/find", post(find_entries))
        .route("/toolbox/reindex", post(reindex_entries))
}

// --- Handlers -------------------------------------------------------------

/// `POST /toolbox/entries` -- store (or refresh) a sheet and the caller's
/// location for it.
///
/// The canonical key is always recomputed from `key`; a key sent by the client
/// would be trusted input on a table shared by every user. The shared sheet
/// follows the newer-commit-wins policy in `toolbox::store::upsert_tool`, and
/// the caller's location row is written whatever that policy decided.
#[tracing::instrument(skip_all)]
async fn index_entry(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Json(body): Json<UpsertToolRequest>,
) -> Result<Json<Value>, AppError> {
    let user_id = auth.effective_user_id();
    validate_upsert(&body, max_body_bytes())?;
    let (tool_key, key_kind) = toolbox::normalize_tool_key(&body.key)?;

    // Embedding is best effort: with no embedder loaded, or one that fails, the
    // sheet is stored without a vector and stays reachable through FTS. A
    // later /toolbox/reindex fills the gap.
    let model = embedding_model_name();
    let keywords = toolbox::normalize_keywords(&body.keywords);
    let text = toolbox::embedding_text(body.name.trim(), body.summary.trim(), &keywords);
    let vector = match state.current_embedder().await {
        Some(embedder) => match embedder.embed(&text).await {
            Ok(v) if !v.is_empty() => Some(v),
            Ok(_) => None,
            Err(e) => {
                tracing::warn!(tool_key = %tool_key, "toolbox: embedding failed: {e}");
                None
            }
        },
        None => {
            tracing::warn!(tool_key = %tool_key, "toolbox: no embedder loaded, storing sheet without a vector");
            None
        }
    };

    let (entry, outcome) = toolbox::upsert_tool(
        &state.db,
        &body,
        &tool_key,
        key_kind,
        vector.as_deref().map(|v| (v, model.as_str())),
        user_id,
    )
    .await?;

    let host = body.key.host.clone().unwrap_or_default();
    let local_path = body.key.local_path.clone().unwrap_or_default();
    let location = toolbox::upsert_location(
        &db,
        user_id,
        &tool_key,
        &host,
        &local_path,
        &body.tags,
        &body.notes,
    )
    .await?;

    Ok(Json(json!({
        "id": entry.id,
        "tool_key": entry.tool_key,
        "key_kind": entry.key_kind,
        "outcome": outcome.as_str(),
        "embedded": vector.is_some(),
        "has_embedding": entry.has_embedding,
        "location_id": location.id,
    })))
}

/// Query string of `GET /toolbox/entries`. Unknown parameters (`space`, which
/// the MCP bridge appends to every GET) are ignored.
#[derive(Debug, Deserialize)]
struct ListParams {
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
}

/// `GET /toolbox/entries` -- the caller's catalog, name-ordered.
#[tracing::instrument(skip_all)]
async fn list_entries(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Query(params): Query<ListParams>,
) -> Result<Json<Value>, AppError> {
    let user_id = auth.effective_user_id();
    let limit = clamp(params.limit, DEFAULT_LIST_LIMIT, MAX_LIST_LIMIT);
    let offset = params.offset.unwrap_or(0);

    let keys = toolbox::list_user_keys(&db, user_id).await?;
    if keys.is_empty() {
        return Ok(Json(json!({ "items": [], "count": 0, "total": 0 })));
    }
    let locations = toolbox::locations_for_keys(&db, user_id, &keys).await?;
    let tools = toolbox::tools_by_keys(&state.db, &keys).await?;

    let kind = params
        .kind
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty());
    let matching: Vec<ToolEntry> = tools
        .into_iter()
        .filter(|t| kind.is_none_or(|k| t.kind.eq_ignore_ascii_case(k)))
        .collect();
    let total = matching.len();

    let items = matching
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|t| {
            let locs = locations.get(&t.tool_key).cloned().unwrap_or_default();
            entry_json(&t, &locs)
        })
        .collect::<Result<Vec<Value>, EngError>>()?;

    Ok(Json(json!({
        "items": items,
        "count": items.len(),
        "total": total,
    })))
}

/// `GET /toolbox/entries/{id}` -- one sheet plus the caller's locations for it.
///
/// 404 when the caller has no location for that sheet's key, with the same body
/// as a genuinely missing id: whether someone else indexed a tool is not the
/// caller's business.
#[tracing::instrument(skip_all)]
async fn get_entry(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Path(id): Path<i64>,
) -> Result<Json<Value>, AppError> {
    let user_id = auth.effective_user_id();
    let tool = owned_tool(&state, &db, user_id, id).await?;
    let locations =
        toolbox::locations_for_keys(&db, user_id, std::slice::from_ref(&tool.tool_key)).await?;
    let locs = locations.get(&tool.tool_key).cloned().unwrap_or_default();
    Ok(Json(entry_json(&tool, &locs)?))
}

/// `DELETE /toolbox/entries/{id}` -- forget a tool *for this caller*.
///
/// Only the caller's location rows go. The shared sheet stays: other shards are
/// invisible from here, so nothing can tell whether the last owner just left,
/// and the sheet plus its embedding remain useful to whoever else has it.
#[tracing::instrument(skip_all)]
async fn forget_entry(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Path(id): Path<i64>,
) -> Result<Json<Value>, AppError> {
    let user_id = auth.effective_user_id();
    let tool = owned_tool(&state, &db, user_id, id).await?;
    let deleted = toolbox::delete_locations(&db, user_id, &tool.tool_key).await?;
    Ok(Json(json!({
        "id": tool.id,
        "tool_key": tool.tool_key,
        "deleted": deleted,
    })))
}

/// Body of `POST /toolbox/find`. Unknown fields (`space`, `space_id`) ignored.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct FindBody {
    query: String,
    kind: Option<String>,
    tags: Vec<String>,
    limit: Option<usize>,
    rerank: Option<bool>,
}

/// `POST /toolbox/find` -- hybrid search over the caller's catalog.
#[tracing::instrument(skip_all)]
async fn find_entries(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Json(body): Json<FindBody>,
) -> Result<Json<Value>, AppError> {
    let user_id = auth.effective_user_id();
    let query = body.query.trim().to_string();
    if query.is_empty() {
        return Err(EngError::InvalidInput("query is required".into()).into());
    }

    let embedding = match state.current_embedder().await {
        Some(embedder) => match embedder.embed(&query).await {
            Ok(v) if !v.is_empty() => Some(v),
            Ok(_) => None,
            Err(e) => {
                tracing::warn!("toolbox: query embedding failed, falling back to FTS: {e}");
                None
            }
        },
        None => None,
    };

    let opts = FindOptions {
        kind: body.kind.clone(),
        tags: body.tags.clone(),
        limit: body.limit.unwrap_or(0),
        rerank: false,
    };
    let mut results =
        toolbox::find_tools(&state.db, &db, user_id, &query, embedding.as_deref(), &opts).await?;

    // Reranking is a second pass over the fused hits, not part of the fusion:
    // kleos-lib cannot reach the server's reranker, so the handler drives it.
    let mut reranked = false;
    if body.rerank.unwrap_or_else(rerank_default) {
        if let Some(reranker) = state.current_reranker().await {
            match toolbox::rerank_find_results(reranker.as_ref(), &query, &mut results).await {
                Ok(()) => reranked = true,
                Err(e) => tracing::warn!("toolbox: rerank failed, keeping fused order: {e}"),
            }
        }
    }

    let payload = serde_json::to_value(&results).map_err(EngError::Serialization)?;
    Ok(Json(json!({
        "results": payload,
        "count": results.len(),
        "embedded_query": embedding.is_some(),
        "reranked": reranked,
    })))
}

/// Body of `POST /toolbox/reindex`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ReindexBody {
    /// Only embed sheets that have no vector at all. The default also picks up
    /// sheets embedded by a different model than the one configured now.
    missing_only: bool,
    limit: Option<usize>,
}

/// `POST /toolbox/reindex` -- compute the embeddings missing from the caller's
/// tools, bounded by `limit`. Idempotent: call it until `candidates` is 0.
#[tracing::instrument(skip_all)]
async fn reindex_entries(
    State(state): State<AppState>,
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Json(body): Json<ReindexBody>,
) -> Result<Json<Value>, AppError> {
    let user_id = auth.effective_user_id();
    let limit = clamp(body.limit, DEFAULT_REINDEX_LIMIT, MAX_REINDEX_LIMIT);
    let model = embedding_model_name();

    let Some(embedder) = state.current_embedder().await else {
        // A no-op is the honest answer for a maintenance route on a server with
        // no embedder: nothing is wrong with the request.
        tracing::warn!("toolbox: reindex requested with no embedder loaded");
        return Ok(Json(json!({
            "embedder": false, "candidates": 0, "embedded": 0, "failed": 0, "model": model,
        })));
    };

    let keys = toolbox::list_user_keys(&db, user_id).await?;
    let mut pending = toolbox::tools_needing_embedding(
        &state.db,
        &keys,
        if body.missing_only {
            None
        } else {
            Some(model.as_str())
        },
    )
    .await?;
    let candidates = pending.len();
    pending.truncate(limit);

    let (mut embedded, mut failed) = (0usize, 0usize);
    for (id, text) in pending {
        match embedder.embed(&text).await {
            Ok(v) if !v.is_empty() => {
                toolbox::set_embedding(&state.db, id, &v, &model).await?;
                embedded += 1;
            }
            Ok(_) => failed += 1,
            Err(e) => {
                tracing::warn!(tool_id = id, "toolbox: reindex embedding failed: {e}");
                failed += 1;
            }
        }
    }

    Ok(Json(json!({
        "embedder": true,
        "candidates": candidates,
        "embedded": embedded,
        "failed": failed,
        "model": model,
    })))
}

// --- Helpers --------------------------------------------------------------

/// Fetch a shared sheet by id, but only for a caller who owns a location for
/// it. The caller's key list is read first, so the shared table is never
/// queried on behalf of a user who owns nothing, and a sheet whose key is not
/// theirs is reported exactly like a nonexistent one.
async fn owned_tool(
    state: &AppState,
    user_db: &kleos_lib::db::Database,
    user_id: i64,
    id: i64,
) -> Result<ToolEntry, AppError> {
    let missing = || EngError::NotFound(format!("toolbox entry {id}"));
    let keys = toolbox::list_user_keys(user_db, user_id).await?;
    if keys.is_empty() {
        return Err(missing().into());
    }
    let tool = toolbox::get_tool_by_id(&state.db, id)
        .await?
        .ok_or_else(missing)?;
    if !keys.iter().any(|k| k == &tool.tool_key) {
        return Err(missing().into());
    }
    Ok(tool)
}

/// One `{ tool, locations }` envelope, the shape both the listing and the
/// single-entry route return.
fn entry_json(tool: &ToolEntry, locations: &[ToolLocation]) -> Result<Value, EngError> {
    Ok(json!({
        "tool": serde_json::to_value(tool).map_err(EngError::Serialization)?,
        "locations": serde_json::to_value(locations).map_err(EngError::Serialization)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kleos_lib::toolbox::KeyInput;

    fn valid_request() -> UpsertToolRequest {
        UpsertToolRequest {
            key: KeyInput {
                git_remote: Some("git@github.com:vocsap/kleos.git".into()),
                host: Some("workstation".into()),
                ..Default::default()
            },
            kind: "repo".into(),
            name: "Kleos".into(),
            summary: "Memory server.".into(),
            ..Default::default()
        }
    }

    #[test]
    fn max_body_bytes_falls_back_on_junk() {
        assert_eq!(parse_max_body_bytes(None), DEFAULT_MAX_BODY_BYTES);
        assert_eq!(parse_max_body_bytes(Some("  4096 ")), 4096);
        assert_eq!(parse_max_body_bytes(Some("nope")), DEFAULT_MAX_BODY_BYTES);
        // Zero would disable the limit entirely; treat it as unset.
        assert_eq!(parse_max_body_bytes(Some("0")), DEFAULT_MAX_BODY_BYTES);
    }

    #[test]
    fn flag_parses_the_usual_spellings() {
        for on in ["1", "true", "TRUE", "yes", "on"] {
            assert!(parse_flag(Some(on), false), "{on} should enable");
        }
        for off in ["0", "false", "no", "OFF"] {
            assert!(!parse_flag(Some(off), true), "{off} should disable");
        }
        // Unset or unrecognized keeps the default, both ways.
        assert!(!parse_flag(None, false));
        assert!(parse_flag(None, true));
        assert!(parse_flag(Some("maybe"), true));
    }

    #[test]
    fn validation_requires_the_identifying_fields() {
        let max = DEFAULT_MAX_BODY_BYTES;
        assert!(validate_upsert(&valid_request(), max).is_ok());

        for blank in ["", "   "] {
            let mut req = valid_request();
            req.name = blank.into();
            assert!(validate_upsert(&req, max).is_err(), "blank name must fail");
            let mut req = valid_request();
            req.summary = blank.into();
            assert!(
                validate_upsert(&req, max).is_err(),
                "blank summary must fail"
            );
            let mut req = valid_request();
            req.kind = blank.into();
            assert!(validate_upsert(&req, max).is_err(), "blank kind must fail");
        }
    }

    #[test]
    fn validation_enforces_the_size_caps() {
        let mut req = valid_request();
        req.body = "x".repeat(64);
        assert!(
            validate_upsert(&req, 64).is_ok(),
            "exactly at the cap passes"
        );
        assert!(
            validate_upsert(&req, 63).is_err(),
            "body over the configured cap must fail"
        );

        let mut req = valid_request();
        req.summary = "x".repeat(MAX_SUMMARY_BYTES + 1);
        assert!(validate_upsert(&req, DEFAULT_MAX_BODY_BYTES).is_err());

        let mut req = valid_request();
        req.keywords = (0..MAX_KEYWORDS + 1).map(|i| format!("k{i}")).collect();
        assert!(validate_upsert(&req, DEFAULT_MAX_BODY_BYTES).is_err());

        let mut req = valid_request();
        req.tags = (0..MAX_TAGS + 1).map(|i| format!("t{i}")).collect();
        assert!(validate_upsert(&req, DEFAULT_MAX_BODY_BYTES).is_err());
    }

    #[test]
    fn space_fields_are_ignored_rather_than_rejected() {
        // The MCP bridge injects `space` into every dispatched call; the toolbox
        // has no spaces, so the field must fall on the floor, not 400.
        let raw = serde_json::json!({
            "key": { "git_remote": "https://github.com/vocsap/kleos.git" },
            "kind": "repo", "name": "Kleos", "summary": "Memory server.",
            "space": "kleos", "space_id": 3
        });
        let req: UpsertToolRequest = serde_json::from_value(raw).expect("unknown fields ignored");
        assert_eq!(req.name, "Kleos");

        let raw = serde_json::json!({ "query": "json parser", "space": "kleos" });
        let body: FindBody = serde_json::from_value(raw).expect("unknown fields ignored");
        assert_eq!(body.query, "json parser");
    }

    #[test]
    fn limits_are_clamped_into_range() {
        assert_eq!(
            clamp(None, DEFAULT_LIST_LIMIT, MAX_LIST_LIMIT),
            DEFAULT_LIST_LIMIT
        );
        assert_eq!(clamp(Some(0), DEFAULT_LIST_LIMIT, MAX_LIST_LIMIT), 1);
        assert_eq!(
            clamp(Some(10_000), DEFAULT_LIST_LIMIT, MAX_LIST_LIMIT),
            MAX_LIST_LIMIT
        );
        assert_eq!(clamp(Some(7), DEFAULT_LIST_LIMIT, MAX_LIST_LIMIT), 7);
    }
}
