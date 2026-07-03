// Portability routes: export, import (auto-detect), state, preferences

use axum::{
    body::Body,
    extract::{Path, Query},
    http::{header, StatusCode},
    response::Response,
    routing::get,
    Json, Router,
};
use rusqlite::params;
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

use crate::{
    error::AppError,
    extractors::{Auth, ResolvedDb},
    state::AppState,
};
use kleos_lib::db::Database;

/// Routes for export/import portability plus current-state and preference CRUD.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/export", get(export_handler))
        .route("/import", axum::routing::post(import_handler))
        // NOTE: /import/mem0 is in ingestion.rs to avoid duplicate routes
        .route(
            "/state",
            get(get_state_handler).delete(delete_state_handler),
        )
        .route(
            "/preferences",
            get(list_preferences_handler)
                .put(put_preferences_handler)
                .delete(delete_all_preferences_handler),
        )
        .route(
            "/preferences/{key}",
            get(get_preference_handler).delete(delete_preference_handler),
        )
}

// --- Export ---

// DOS-L2: stream export as NDJSON so large user datasets don't require
// buffering the entire response as a single JSON blob. One JSON object per
// line; clients can parse records as they arrive.
async fn export_handler(
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
) -> Result<Response, AppError> {
    let data = kleos_lib::admin::export_user_data(&db, auth.effective_user_id()).await?;

    let mut lines: Vec<Result<axum::body::Bytes, std::convert::Infallible>> = Vec::new();

    lines.push(Ok(axum::body::Bytes::from(
        json!({
            "type": "header",
            "version": data.version,
            "exported_at": data.exported_at,
            "user_id": data.user_id,
        })
        .to_string()
            + "\n",
    )));

    for (type_name, records) in [
        ("memory", &data.memories),
        ("conversation", &data.conversations),
        ("episode", &data.episodes),
        ("entity", &data.entities),
        ("fact", &data.facts),
        ("preference", &data.preferences),
        ("skill", &data.skills),
    ] {
        for record in records {
            let mut v = record.clone();
            if let Value::Object(ref mut map) = v {
                map.insert("type".into(), Value::String(type_name.to_string()));
            }
            lines.push(Ok(axum::body::Bytes::from(v.to_string() + "\n")));
        }
    }

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .body(Body::from_stream(futures::stream::iter(lines)))
        .unwrap())
}

// --- Import (auto-detect format) ---

/// Cap on per-request import error messages echoed back to the caller.
const MAX_REPORTED_ERRORS: usize = 5;

/// Build the import response. Any failed write makes the status 207
/// Multi-Status so callers can detect partial or total data loss instead of
/// reading an unconditional 200; `skipped` counts only rows intentionally
/// ignored (empty content), never failures.
fn import_response(
    format: &str,
    imported: i64,
    skipped: i64,
    failed: i64,
    errors: Vec<String>,
) -> (StatusCode, Json<Value>) {
    let status = if failed > 0 {
        StatusCode::MULTI_STATUS
    } else {
        StatusCode::OK
    };
    (
        status,
        Json(json!({
            "imported": imported,
            "skipped": skipped,
            "failed": failed,
            "errors": errors,
            "format": format,
        })),
    )
}

/// POST /import: auto-detects the payload format (kleos export, mem0, plain
/// array) and inserts the rows for the caller. Returns 207 when any write
/// failed (see `import_response`).
async fn import_handler(
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Json(body): Json<Value>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    // Auto-detect format based on shape
    if body.is_array() {
        let arr = body.as_array().ok_or_else(|| {
            AppError(kleos_lib::EngError::InvalidInput(
                "expected JSON array".into(),
            ))
        })?;
        return import_array(&db, auth.effective_user_id(), arr).await;
    }
    if let Some(obj) = body.as_object() {
        if obj.contains_key("memories") {
            // Kleos JSON export or generic format with memories key
            let version = obj.get("version").and_then(|v| v.as_str());
            if version.is_some() {
                return import_kleos_export(&db, auth.effective_user_id(), obj).await;
            }
            // mem0-style: has "memories" but no version
            if let Some(arr) = obj.get("memories").and_then(|v| v.as_array()) {
                return import_mem0_array(&db, auth.effective_user_id(), arr).await;
            }
        }
        if obj.contains_key("results") {
            if let Some(arr) = obj.get("results").and_then(|v| v.as_array()) {
                return import_mem0_array(&db, auth.effective_user_id(), arr).await;
            }
        }
        if obj.contains_key("documents") || obj.contains_key("data") {
            let items = obj
                .get("documents")
                .or_else(|| obj.get("data"))
                .and_then(|v| v.as_array());
            if let Some(arr) = items {
                return import_array(&db, auth.effective_user_id(), arr).await;
            }
        }
    }
    Err(AppError(kleos_lib::EngError::InvalidInput(
        "unrecognized import format".into(),
    )))
}

/// Import a versioned Kleos JSON export's memories array.
async fn import_kleos_export(
    db: &Arc<Database>,
    user_id: i64,
    obj: &serde_json::Map<String, Value>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let mut imported = 0i64;
    let mut skipped = 0i64;
    let mut failed = 0i64;
    let mut errors: Vec<String> = Vec::new();
    if let Some(memories) = obj.get("memories").and_then(|v| v.as_array()) {
        for mem in memories {
            let content = mem
                .get("content")
                .or_else(|| mem.get("col_1"))
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string());
            let content = match content.filter(|c| !c.is_empty()) {
                Some(c) => c,
                None => {
                    skipped += 1;
                    continue;
                }
            };
            let category = mem
                .get("category")
                .or_else(|| mem.get("col_2"))
                .and_then(|v| v.as_str())
                .unwrap_or("general")
                .to_string();
            let source = mem
                .get("source")
                .or_else(|| mem.get("col_3"))
                .and_then(|v| v.as_str())
                .unwrap_or("import")
                .to_string();
            let importance = mem
                .get("importance")
                .or_else(|| mem.get("col_4"))
                .and_then(|v| v.as_i64())
                .unwrap_or(5) as i32;
            let sync_id = Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            let created_at = mem
                .get("created_at")
                .and_then(|v| v.as_str())
                .unwrap_or(&now)
                .to_string();
            let updated_at = mem
                .get("updated_at")
                .and_then(|v| v.as_str())
                .unwrap_or(&now)
                .to_string();
            match db.write(move |conn| {
                conn.execute(
                    "INSERT INTO memories (user_id, content, category, source, importance, sync_id, created_at, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![user_id, content, category, source, importance, sync_id, created_at, updated_at],
                ).map_err(|e| kleos_lib::EngError::Internal(e.to_string()))
            }).await {
                Ok(_) => imported += 1,
                Err(e) => {
                    tracing::warn!("import_kleos_memory_failed: {}", e);
                    if errors.len() < MAX_REPORTED_ERRORS {
                        errors.push(e.to_string());
                    }
                    failed += 1;
                }
            }
        }
    }
    Ok(import_response("kleos", imported, skipped, failed, errors))
}

/// Import a plain JSON array of objects carrying content/text/memory fields.
async fn import_array(
    db: &Arc<Database>,
    user_id: i64,
    arr: &[Value],
) -> Result<(StatusCode, Json<Value>), AppError> {
    let mut imported = 0i64;
    let mut skipped = 0i64;
    let mut failed = 0i64;
    let mut errors: Vec<String> = Vec::new();
    for item in arr {
        let content = item
            .get("content")
            .or_else(|| item.get("text"))
            .or_else(|| item.get("memory"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string());
        let content = match content.filter(|c| !c.is_empty()) {
            Some(c) => c,
            None => {
                skipped += 1;
                continue;
            }
        };
        let category = item
            .get("category")
            .and_then(|v| v.as_str())
            .unwrap_or("general")
            .to_string();
        let source = item
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("import")
            .to_string();
        let importance = item.get("importance").and_then(|v| v.as_i64()).unwrap_or(5) as i32;
        let sync_id = Uuid::new_v4().to_string();
        match db.write(move |conn| {
            conn.execute(
                "INSERT INTO memories (user_id, content, category, source, importance, sync_id, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'), datetime('now'))",
                params![user_id, content, category, source, importance, sync_id],
            ).map_err(|e| kleos_lib::EngError::Internal(e.to_string()))
        }).await {
            Ok(_) => imported += 1,
            Err(e) => {
                tracing::warn!("import_array_write_failed: {}", e);
                if errors.len() < MAX_REPORTED_ERRORS {
                    errors.push(e.to_string());
                }
                failed += 1;
            }
        }
    }
    Ok(import_response("array", imported, skipped, failed, errors))
}

/// Import a mem0-style array (memory/text/content + optional metadata).
async fn import_mem0_array(
    db: &Arc<Database>,
    user_id: i64,
    arr: &[Value],
) -> Result<(StatusCode, Json<Value>), AppError> {
    let mut imported = 0i64;
    let mut skipped = 0i64;
    let mut failed = 0i64;
    let mut errors: Vec<String> = Vec::new();
    for mem in arr {
        let content = mem
            .get("memory")
            .or_else(|| mem.get("text"))
            .or_else(|| mem.get("content"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string());
        let content = match content.filter(|c| !c.is_empty()) {
            Some(c) => c,
            None => {
                skipped += 1;
                continue;
            }
        };
        let meta = mem.get("metadata").and_then(|m| m.as_object());
        let category = meta
            .and_then(|m| m.get("category"))
            .and_then(|v| v.as_str())
            .or_else(|| mem.get("category").and_then(|v| v.as_str()))
            .unwrap_or("general")
            .to_string();
        let source = meta
            .and_then(|m| m.get("source"))
            .and_then(|v| v.as_str())
            .or_else(|| mem.get("source").and_then(|v| v.as_str()))
            .unwrap_or("mem0-import")
            .to_string();
        let importance = meta
            .and_then(|m| m.get("importance"))
            .and_then(|v| v.as_i64())
            .unwrap_or(5) as i32;
        let sync_id = Uuid::new_v4().to_string();
        match db.write(move |conn| {
            conn.execute(
                "INSERT INTO memories (user_id, content, category, source, importance, sync_id, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'), datetime('now'))",
                params![user_id, content, category, source, importance, sync_id],
            ).map_err(|e| kleos_lib::EngError::Internal(e.to_string()))
        }).await {
            Ok(_) => imported += 1,
            Err(e) => {
                tracing::warn!("import_mem0_write_failed: {}", e);
                if errors.len() < MAX_REPORTED_ERRORS {
                    errors.push(e.to_string());
                }
                failed += 1;
            }
        }
    }
    Ok(import_response("mem0", imported, skipped, failed, errors))
}

// --- State ---

async fn get_state_handler(
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Query(params): Query<GetStateQuery>,
) -> Result<Json<Value>, AppError> {
    let prefix = format!("user:{}:", auth.effective_user_id());
    let prefix_len = prefix.len();
    let filter_key = params.key.clone();

    let user_state: serde_json::Map<String, Value> = db
        .read(move |conn| {
            if let Some(key) = &filter_key {
                let full_key = format!("{}{}", prefix, key);
                let mut stmt = conn
                    .prepare("SELECT key, value FROM app_state WHERE key = ?1")
                    .map_err(|e| kleos_lib::EngError::Internal(e.to_string()))?;
                let rows = stmt
                    .query_map(params![full_key], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })
                    .map_err(|e| kleos_lib::EngError::Internal(e.to_string()))?;
                let mut result = serde_json::Map::new();
                for row in rows {
                    let (k, v) = row.map_err(|e| kleos_lib::EngError::Internal(e.to_string()))?;
                    let short_key = k[prefix_len..].to_string();
                    result.insert(short_key, Value::String(v));
                }
                Ok(result)
            } else {
                let prefix_like = format!("{}%", prefix);
                let mut stmt = conn
                    .prepare("SELECT key, value FROM app_state WHERE key LIKE ?1 ORDER BY key")
                    .map_err(|e| kleos_lib::EngError::Internal(e.to_string()))?;
                let rows = stmt
                    .query_map(params![prefix_like], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                    })
                    .map_err(|e| kleos_lib::EngError::Internal(e.to_string()))?;
                let mut result = serde_json::Map::new();
                for row in rows {
                    let (k, v) = row.map_err(|e| kleos_lib::EngError::Internal(e.to_string()))?;
                    let short_key = k[prefix_len..].to_string();
                    result.insert(short_key, Value::String(v));
                }
                Ok(result)
            }
        })
        .await
        .map_err(AppError)?;
    Ok(Json(json!({ "state": user_state })))
}

#[derive(Debug, serde::Deserialize)]
/// Query params for GET /state (optional key filter).
struct GetStateQuery {
    key: Option<String>,
}

/// DELETE /state: remove the caller's current-state rows (optionally by key).
async fn delete_state_handler(
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
) -> Result<Json<Value>, AppError> {
    let prefix = format!("user:{}:%", auth.effective_user_id());
    let affected = db
        .write(move |conn| {
            conn.execute("DELETE FROM app_state WHERE key LIKE ?1", params![prefix])
                .map_err(|e| kleos_lib::EngError::Internal(e.to_string()))
        })
        .await
        .map_err(AppError)? as i64;
    Ok(Json(json!({ "deleted": affected })))
}

// --- Preferences ---

async fn list_preferences_handler(
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
) -> Result<Json<Value>, AppError> {
    let prefs = kleos_lib::preferences::list_preferences(&db, auth.effective_user_id()).await?;
    let count = prefs.len();
    let items = serde_json::to_value(prefs)
        .map_err(|e| AppError(kleos_lib::EngError::Internal(e.to_string())))?;
    Ok(Json(json!({ "items": items, "count": count })))
}

/// GET /preferences/{key}: fetch one preference for the caller.
async fn get_preference_handler(
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Path(key): Path<String>,
) -> Result<Json<Value>, AppError> {
    let pref = kleos_lib::preferences::get_preference(&db, auth.effective_user_id(), &key).await?;
    Ok(Json(serde_json::to_value(pref).map_err(|e| {
        AppError(kleos_lib::EngError::Internal(e.to_string()))
    })?))
}

/// PUT /preferences: upsert the caller's preferences from a JSON object.
async fn put_preferences_handler(
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Json(body): Json<serde_json::Map<String, Value>>,
) -> Result<Json<Value>, AppError> {
    let mut updated = 0i64;
    for (key, val) in &body {
        let v = val
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_else(|| val.to_string());
        kleos_lib::preferences::set_preference(&db, auth.effective_user_id(), key, &v).await?;
        updated += 1;
    }
    Ok(Json(json!({ "updated": updated })))
}

/// DELETE /preferences: remove every preference for the caller.
async fn delete_all_preferences_handler(
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
) -> Result<Json<Value>, AppError> {
    let deleted =
        kleos_lib::preferences::delete_all_preferences(&db, auth.effective_user_id()).await?;
    Ok(Json(json!({ "deleted": deleted })))
}

/// DELETE /preferences/{key}: remove one preference for the caller.
async fn delete_preference_handler(
    Auth(auth): Auth,
    ResolvedDb(db): ResolvedDb,
    Path(key): Path<String>,
) -> Result<Json<Value>, AppError> {
    kleos_lib::preferences::delete_preference(&db, auth.effective_user_id(), &key).await?;
    Ok(Json(json!({ "deleted": true, "key": key })))
}

#[cfg(test)]
/// Tests for the import helpers' loss accounting: skipped counts only
/// intentionally ignored rows, failed writes surface as 207 Multi-Status.
mod tests {
    use super::*;

    /// Empty-content rows are skipped, valid rows import, status stays 200.
    #[tokio::test]
    async fn import_array_counts_skips_separately_from_failures() {
        let db = Arc::new(
            kleos_lib::db::Database::connect_memory()
                .await
                .expect("in-mem db"),
        );
        let rows = vec![
            json!({ "content": "a valid imported memory" }),
            json!({ "content": "   " }),
            json!({ "note": "no content field at all" }),
        ];
        let (status, Json(body)) = import_array(&db, 1, &rows).await.expect("import ok");
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["imported"], 1);
        assert_eq!(body["skipped"], 2);
        assert_eq!(body["failed"], 0);
        assert!(body["errors"].as_array().unwrap().is_empty());
    }

    /// Write failures are reported as failed + 207, never silently folded
    /// into skipped with a 200 (the silent-data-loss regression this module
    /// shipped with).
    #[tokio::test]
    async fn import_array_write_failures_return_multi_status() {
        let db = Arc::new(
            kleos_lib::db::Database::connect_memory()
                .await
                .expect("in-mem db"),
        );
        // Make every INSERT fail deterministically: move the table away.
        db.write(|conn| {
            conn.execute_batch("ALTER TABLE memories RENAME TO memories_gone;")
                .map_err(|e| kleos_lib::EngError::Internal(e.to_string()))
        })
        .await
        .expect("rename table");

        let rows = vec![
            json!({ "content": "first row that will fail to write" }),
            json!({ "content": "second row that will fail to write" }),
        ];
        let (status, Json(body)) = import_array(&db, 1, &rows).await.expect("handler returns");
        assert_eq!(status, StatusCode::MULTI_STATUS);
        assert_eq!(body["imported"], 0);
        assert_eq!(body["skipped"], 0);
        assert_eq!(body["failed"], 2);
        assert!(
            !body["errors"].as_array().unwrap().is_empty(),
            "failure messages must be surfaced to the caller"
        );
    }
}
