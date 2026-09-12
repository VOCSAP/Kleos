//! Storage for the toolbox: the shared sheets and the per-user locations.
//!
//! Two tables, two databases in sharded mode (`toolbox_tools` in the main DB,
//! `toolbox_locations` in the caller's shard), so every function here takes the
//! database it works on explicitly and the two are **never** joined in SQL --
//! they may not live in the same file. The join happens in Rust, in
//! [`super::search`] and in the handlers.

use super::embedding;
use super::types::{
    CommitInfo, KeyKind, ToolEntry, ToolLocation, UpsertOutcome, UpsertToolRequest,
};
use crate::db::Database;
use crate::{EngError, Result};
use rusqlite::{params, OptionalExtension};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Columns of `toolbox_tools` in the order [`row_to_tool`] expects. The raw
/// embedding is deliberately absent: only its presence is exposed.
const TOOL_COLUMNS: &str = "id, tool_key, key_kind, kind, name, summary, body, keywords, \
     canonical_url, indexed_commit, indexed_commit_time, content_hash, \
     embedding IS NOT NULL, embedding_model, indexed_by_user_id, created_at, updated_at";

const LOCATION_COLUMNS: &str =
    "id, user_id, tool_key, host, local_path, tags, notes, last_seen_at, created_at";

/// SQLite's default parameter ceiling is 999; 500 keys per `IN (...)` batch
/// leaves room for the other bound values in the same statement.
pub(crate) const MAX_IN_PARAMS: usize = 500;

/// Field separator for the content hash. `\x1f` (unit separator) cannot appear
/// in the human text being hashed, so no field can spoof another's boundary.
const HASH_SEP: &str = "\x1f";

/// Timestamp format shared with the rest of the schema (`datetime('now')`).
fn now_ts() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Fold a keyword list into the stored form: lowercase, trimmed, deduplicated,
/// space-separated. Whitespace inside a keyword becomes a separator, so the
/// stored string is always a clean FTS token sequence.
pub fn normalize_keywords(keywords: &[String]) -> String {
    let mut out: Vec<String> = Vec::new();
    for raw in keywords {
        for token in raw.split_whitespace() {
            let token = token.trim().to_lowercase();
            if !token.is_empty() && !out.contains(&token) {
                out.push(token);
            }
        }
    }
    out.join(" ")
}

/// Fold a tag list: lowercase, trimmed, deduplicated, order preserved. Tag
/// filtering in [`super::search::find_tools`] compares against this form.
pub fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in tags {
        let tag = raw.trim().to_lowercase();
        if !tag.is_empty() && !out.contains(&tag) {
            out.push(tag);
        }
    }
    out
}

/// SHA-256 (hex) over the four sheet fields. Two indexings of the same tool that
/// produce byte-identical text produce the same hash, which is what lets an
/// upsert report `Unchanged` instead of rewriting the row.
///
/// `keywords` is the normalized space-separated form (see [`normalize_keywords`]).
pub fn content_hash(name: &str, summary: &str, body: &str, keywords: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(name.as_bytes());
    hasher.update(HASH_SEP.as_bytes());
    hasher.update(summary.as_bytes());
    hasher.update(HASH_SEP.as_bytes());
    hasher.update(body.as_bytes());
    hasher.update(HASH_SEP.as_bytes());
    hasher.update(keywords.as_bytes());
    hex::encode(hasher.finalize())
}

/// The text handed to the embedder. The body is left out on purpose: a sheet's
/// discriminating signal is its name, its summary and its keywords, and a long
/// body drowns them in a single pooled vector.
///
/// `keywords` is the normalized space-separated form.
pub fn embedding_text(name: &str, summary: &str, keywords: &str) -> String {
    format!("{name}\n{summary}\n{keywords}")
}

fn row_to_tool(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolEntry> {
    let keywords: String = row.get(7)?;
    Ok(ToolEntry {
        id: row.get(0)?,
        tool_key: row.get(1)?,
        key_kind: row.get(2)?,
        kind: row.get(3)?,
        name: row.get(4)?,
        summary: row.get(5)?,
        body: row.get(6)?,
        keywords: keywords.split_whitespace().map(str::to_string).collect(),
        canonical_url: row.get(8)?,
        indexed_commit: row.get(9)?,
        indexed_commit_time: row.get(10)?,
        content_hash: row.get(11)?,
        has_embedding: row.get(12)?,
        embedding_model: row.get(13)?,
        indexed_by_user_id: row.get(14)?,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
    })
}

fn row_to_location(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolLocation> {
    let tags: String = row.get(5)?;
    Ok(ToolLocation {
        id: row.get(0)?,
        user_id: row.get(1)?,
        tool_key: row.get(2)?,
        host: row.get(3)?,
        local_path: row.get(4)?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        notes: row.get(6)?,
        last_seen_at: row.get(7)?,
        created_at: row.get(8)?,
    })
}

/// `?1, ?2, ...` placeholders for an `IN` list of `n` values.
pub(crate) fn placeholders(n: usize) -> String {
    (1..=n)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Write the shared sheet for `tool_key`, applying the overwrite policy.
///
/// Policy, when a row already exists (stored commit time `s`, incoming `i`):
///
/// | stored | incoming | action |
/// |---|---|---|
/// | `Some(s)` | `Some(i)`, `i > s` | overwrite |
/// | `Some(s)` | `Some(i)`, `i == s` | overwrite only with `force` |
/// | `Some(s)` | `Some(i)`, `i < s` | kept -- `force` does NOT override a newer stored commit |
/// | `Some(s)` | `None` | kept, unless `force` |
/// | `None` | any | overwrite (last writer wins) |
///
/// An overwrite replaces the sheet *and* its embedding: passing `embedding:
/// None` clears the stored vector rather than leaving one that describes the
/// previous text. A kept sheet whose stored embedding is NULL is backfilled
/// opportunistically when the caller has one.
///
/// The outcome is `Unchanged` whenever the stored content hash already matches
/// the incoming one and the commit metadata would not change either -- nothing
/// is written in that case (beyond a possible embedding backfill).
///
/// `embedding` is `(vector, model_name)`. Caller-scoped location rows are NOT
/// touched here; see [`upsert_location`].
pub async fn upsert_tool(
    shared: &Database,
    req: &UpsertToolRequest,
    tool_key: &str,
    key_kind: KeyKind,
    embedding: Option<(&[f32], &str)>,
    user_id: i64,
) -> Result<(ToolEntry, UpsertOutcome)> {
    let keywords = normalize_keywords(&req.keywords);
    let hash = content_hash(&req.name, &req.summary, &req.body, &keywords);
    let key = tool_key.to_string();
    let key_kind_s = key_kind.to_string();
    let kind = req.kind.trim().to_string();
    let name = req.name.trim().to_string();
    let summary = req.summary.trim().to_string();
    let body = req.body.clone();
    let canonical_url = req.canonical_url.clone();
    let (commit_sha, commit_time) = match &req.commit {
        Some(CommitInfo { sha, time }) => (Some(sha.trim().to_string()), *time),
        None => (None, None),
    };
    let force = req.force;
    let emb: Option<(Vec<u8>, String)> =
        embedding.map(|(v, m)| (embedding::to_blob(v), m.to_string()));

    shared
        .transaction(move |tx| {
            let existing: Option<(i64, String, Option<String>, Option<i64>, bool)> = tx
                .query_row(
                    "SELECT id, content_hash, indexed_commit, indexed_commit_time, \
                     embedding IS NOT NULL FROM toolbox_tools WHERE tool_key = ?1",
                    params![key],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()?;

            let now = now_ts();
            let (id, outcome) = match existing {
                None => {
                    tx.execute(
                        "INSERT INTO toolbox_tools (tool_key, key_kind, kind, name, summary, body, \
                            keywords, canonical_url, indexed_commit, indexed_commit_time, \
                            content_hash, embedding, embedding_model, indexed_by_user_id, \
                            created_at, updated_at) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15)",
                        params![
                            key,
                            key_kind_s,
                            kind,
                            name,
                            summary,
                            body,
                            keywords,
                            canonical_url,
                            commit_sha,
                            commit_time,
                            hash,
                            emb.as_ref().map(|(b, _)| b.as_slice()),
                            emb.as_ref().map(|(_, m)| m.as_str()),
                            user_id,
                            now,
                        ],
                    )?;
                    (tx.last_insert_rowid(), UpsertOutcome::Inserted)
                }
                Some((id, stored_hash, stored_commit, stored_time, has_embedding)) => {
                    let overwrite = match (stored_time, commit_time) {
                        (None, _) => true,
                        (Some(_), None) => force,
                        (Some(s), Some(i)) => i > s || (i == s && force),
                    };
                    let same_sheet = stored_hash == hash
                        && stored_commit == commit_sha
                        && stored_time == commit_time;

                    let outcome = if same_sheet {
                        UpsertOutcome::Unchanged
                    } else if overwrite {
                        UpsertOutcome::Updated
                    } else if stored_hash == hash {
                        UpsertOutcome::Unchanged
                    } else {
                        UpsertOutcome::Kept
                    };

                    if outcome == UpsertOutcome::Updated {
                        tx.execute(
                            "UPDATE toolbox_tools SET key_kind = ?2, kind = ?3, name = ?4, \
                                summary = ?5, body = ?6, keywords = ?7, canonical_url = ?8, \
                                indexed_commit = ?9, indexed_commit_time = ?10, content_hash = ?11, \
                                embedding = ?12, embedding_model = ?13, indexed_by_user_id = ?14, \
                                updated_at = ?15 \
                             WHERE id = ?1",
                            params![
                                id,
                                key_kind_s,
                                kind,
                                name,
                                summary,
                                body,
                                keywords,
                                canonical_url,
                                commit_sha,
                                commit_time,
                                hash,
                                emb.as_ref().map(|(b, _)| b.as_slice()),
                                emb.as_ref().map(|(_, m)| m.as_str()),
                                user_id,
                                now,
                            ],
                        )?;
                    } else if !has_embedding {
                        // Opportunistic backfill: the sheet stays as stored, but a
                        // row with no vector is invisible to the vector channel, so
                        // take the embedding we happen to have.
                        if let Some((blob, model)) = emb.as_ref() {
                            tx.execute(
                                "UPDATE toolbox_tools SET embedding = ?2, embedding_model = ?3 \
                                 WHERE id = ?1 AND embedding IS NULL",
                                params![id, blob.as_slice(), model],
                            )?;
                        }
                    }
                    (id, outcome)
                }
            };

            let sql = format!("SELECT {TOOL_COLUMNS} FROM toolbox_tools WHERE id = ?1");
            let entry = tx.query_row(&sql, params![id], row_to_tool)?;
            Ok((entry, outcome))
        })
        .await
}

/// Record (or refresh) where this user keeps this tool. Idempotent on
/// `(user_id, tool_key, host, local_path)`: a re-index refreshes `last_seen_at`,
/// the tags and the notes rather than adding a row.
pub async fn upsert_location(
    user_db: &Database,
    user_id: i64,
    tool_key: &str,
    host: &str,
    local_path: &str,
    tags: &[String],
    notes: &str,
) -> Result<ToolLocation> {
    let key = tool_key.to_string();
    let host = host.trim().to_lowercase();
    let local_path = local_path.trim().to_string();
    let tags_json = serde_json::to_string(&normalize_tags(tags))?;
    let notes = notes.to_string();

    user_db
        .transaction(move |tx| {
            let now = now_ts();
            tx.execute(
                "INSERT INTO toolbox_locations \
                    (user_id, tool_key, host, local_path, tags, notes, last_seen_at, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7) \
                 ON CONFLICT(user_id, tool_key, host, local_path) DO UPDATE SET \
                    tags = excluded.tags, notes = excluded.notes, \
                    last_seen_at = excluded.last_seen_at",
                params![user_id, key, host, local_path, tags_json, notes, now],
            )?;
            let sql = format!(
                "SELECT {LOCATION_COLUMNS} FROM toolbox_locations \
                 WHERE user_id = ?1 AND tool_key = ?2 AND host = ?3 AND local_path = ?4"
            );
            let loc = tx.query_row(
                &sql,
                params![user_id, key, host, local_path],
                row_to_location,
            )?;
            Ok(loc)
        })
        .await
}

/// Every tool key this user has a location for. This list is the anti-leak
/// filter: no read of the shared table happens without it.
pub async fn list_user_keys(user_db: &Database, user_id: i64) -> Result<Vec<String>> {
    user_db
        .read(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT tool_key FROM toolbox_locations WHERE user_id = ?1 \
                 ORDER BY tool_key",
            )?;
            let keys = stmt
                .query_map(params![user_id], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(keys)
        })
        .await
}

/// This user's locations for the given keys, grouped by key. Keys with no
/// location are simply absent from the map.
pub async fn locations_for_keys(
    user_db: &Database,
    user_id: i64,
    keys: &[String],
) -> Result<HashMap<String, Vec<ToolLocation>>> {
    if keys.is_empty() {
        return Ok(HashMap::new());
    }
    let mut out: HashMap<String, Vec<ToolLocation>> = HashMap::new();
    for chunk in keys.chunks(MAX_IN_PARAMS) {
        let chunk: Vec<String> = chunk.to_vec();
        let rows = user_db
            .read(move |conn| {
                let sql = format!(
                    "SELECT {LOCATION_COLUMNS} FROM toolbox_locations \
                     WHERE user_id = ?{} AND tool_key IN ({}) ORDER BY id",
                    chunk.len() + 1,
                    placeholders(chunk.len())
                );
                let mut stmt = conn.prepare(&sql)?;
                let mut binds: Vec<Box<dyn rusqlite::ToSql>> = chunk
                    .iter()
                    .map(|k| Box::new(k.clone()) as Box<dyn rusqlite::ToSql>)
                    .collect();
                binds.push(Box::new(user_id));
                let refs: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();
                let locs = stmt
                    .query_map(refs.as_slice(), row_to_location)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(locs)
            })
            .await?;
        for loc in rows {
            out.entry(loc.tool_key.clone()).or_default().push(loc);
        }
    }
    Ok(out)
}

/// Fetch one shared sheet by row id.
pub async fn get_tool_by_id(shared: &Database, id: i64) -> Result<Option<ToolEntry>> {
    shared
        .read(move |conn| {
            let sql = format!("SELECT {TOOL_COLUMNS} FROM toolbox_tools WHERE id = ?1");
            Ok(conn.query_row(&sql, params![id], row_to_tool).optional()?)
        })
        .await
}

/// Fetch one shared sheet by canonical key.
pub async fn get_tool_by_key(shared: &Database, tool_key: &str) -> Result<Option<ToolEntry>> {
    let key = tool_key.to_string();
    shared
        .read(move |conn| {
            let sql = format!("SELECT {TOOL_COLUMNS} FROM toolbox_tools WHERE tool_key = ?1");
            Ok(conn.query_row(&sql, params![key], row_to_tool).optional()?)
        })
        .await
}

/// Fetch the shared sheets for a set of keys, in name order. Keys with no sheet
/// (a location pointing at a tool nobody has indexed yet) are simply missing.
pub async fn tools_by_keys(shared: &Database, keys: &[String]) -> Result<Vec<ToolEntry>> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for chunk in keys.chunks(MAX_IN_PARAMS) {
        let chunk: Vec<String> = chunk.to_vec();
        let mut rows = shared
            .read(move |conn| {
                let sql = format!(
                    "SELECT {TOOL_COLUMNS} FROM toolbox_tools WHERE tool_key IN ({}) \
                     ORDER BY name COLLATE NOCASE",
                    placeholders(chunk.len())
                );
                let mut stmt = conn.prepare(&sql)?;
                let refs: Vec<&dyn rusqlite::ToSql> =
                    chunk.iter().map(|k| k as &dyn rusqlite::ToSql).collect();
                let tools = stmt
                    .query_map(refs.as_slice(), row_to_tool)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(tools)
            })
            .await?;
        out.append(&mut rows);
    }
    Ok(out)
}

/// Fetch the shared sheets for a set of row ids, unordered.
pub(crate) async fn tools_by_ids(shared: &Database, ids: &[i64]) -> Result<Vec<ToolEntry>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for chunk in ids.chunks(MAX_IN_PARAMS) {
        let chunk: Vec<i64> = chunk.to_vec();
        let mut rows = shared
            .read(move |conn| {
                let sql = format!(
                    "SELECT {TOOL_COLUMNS} FROM toolbox_tools WHERE id IN ({})",
                    placeholders(chunk.len())
                );
                let mut stmt = conn.prepare(&sql)?;
                let refs: Vec<&dyn rusqlite::ToSql> =
                    chunk.iter().map(|i| i as &dyn rusqlite::ToSql).collect();
                let tools = stmt
                    .query_map(refs.as_slice(), row_to_tool)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(tools)
            })
            .await?;
        out.append(&mut rows);
    }
    Ok(out)
}

/// Forget a tool for this user: delete their locations for `tool_key`. The
/// shared sheet is NEVER deleted -- other users' shards are invisible from here,
/// so nobody can tell whether the last owner just left.
pub async fn delete_locations(user_db: &Database, user_id: i64, tool_key: &str) -> Result<usize> {
    let key = tool_key.to_string();
    user_db
        .write(move |conn| {
            let n = conn.execute(
                "DELETE FROM toolbox_locations WHERE user_id = ?1 AND tool_key = ?2",
                params![user_id, key],
            )?;
            Ok(n)
        })
        .await
}

/// Rows among `keys` that need an embedding computed: no vector at all, or one
/// produced by a different model than `model`.
///
/// Returns `(id, embedding_text)` pairs ready for the embedder. `model` is
/// `None` when the caller only wants the missing ones.
pub async fn tools_needing_embedding(
    shared: &Database,
    keys: &[String],
    model: Option<&str>,
) -> Result<Vec<(i64, String)>> {
    if keys.is_empty() {
        return Ok(Vec::new());
    }
    let model = model.map(str::to_string);
    let mut out = Vec::new();
    for chunk in keys.chunks(MAX_IN_PARAMS) {
        let chunk: Vec<String> = chunk.to_vec();
        let model = model.clone();
        let mut rows = shared
            .read(move |conn| {
                let n = chunk.len();
                let mut sql = format!(
                    "SELECT id, name, summary, keywords FROM toolbox_tools \
                     WHERE tool_key IN ({}) AND (embedding IS NULL",
                    placeholders(n)
                );
                if model.is_some() {
                    sql.push_str(&format!(
                        " OR embedding_model IS NULL OR embedding_model <> ?{}",
                        n + 1
                    ));
                }
                sql.push(')');
                let mut stmt = conn.prepare(&sql)?;
                let mut binds: Vec<Box<dyn rusqlite::ToSql>> = chunk
                    .iter()
                    .map(|k| Box::new(k.clone()) as Box<dyn rusqlite::ToSql>)
                    .collect();
                if let Some(m) = model {
                    binds.push(Box::new(m));
                }
                let refs: Vec<&dyn rusqlite::ToSql> = binds.iter().map(|b| b.as_ref()).collect();
                let rows = stmt
                    .query_map(refs.as_slice(), |r| {
                        let id: i64 = r.get(0)?;
                        let name: String = r.get(1)?;
                        let summary: String = r.get(2)?;
                        let keywords: String = r.get(3)?;
                        Ok((id, embedding_text(&name, &summary, &keywords)))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?;
        out.append(&mut rows);
    }
    Ok(out)
}

/// Store a freshly computed embedding on a shared sheet. Does not touch
/// `updated_at`: the sheet's text did not change.
pub async fn set_embedding(shared: &Database, id: i64, vector: &[f32], model: &str) -> Result<()> {
    let blob = embedding::to_blob(vector);
    let model = model.to_string();
    shared
        .write(move |conn| {
            let n = conn.execute(
                "UPDATE toolbox_tools SET embedding = ?2, embedding_model = ?3 WHERE id = ?1",
                params![id, blob, model],
            )?;
            if n == 0 {
                return Err(EngError::NotFound(format!("toolbox tool {id}")));
            }
            Ok(())
        })
        .await
}
