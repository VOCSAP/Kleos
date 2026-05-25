//! Patch 33 (2026-05-25): centralized helpers for the spaces partitioning
//! convention used across memories, conversations, entities and the
//! intelligence pipeline. The post-Patch 33 convention is:
//!
//! * Every newly-written row carries a NOT NULL `space_id` resolved through
//!   [`normalize_space_input`]. The space named `default` (auto-created by
//!   `auth_keys::create_user`) is the canonical representation of the
//!   "cross-project" bucket.
//! * Legacy rows written before Patch 33 may still have `space_id = NULL`;
//!   the inclusive read filter `WHERE space_id IN (?cur, ?def) OR space_id
//!   IS NULL` keeps them visible until the migration chantier (cf. plan
//!   section 11) reassigns them.
//! * Pair sweeps in the intelligence pipeline rely on `a.space_id =
//!   b.space_id`, which isolates legacy NULL rows naturally (NULL!=NULL in
//!   SQL semantics).
//!
//! The helpers exposed here are deliberately stateless: callers pass the
//! shared [`Database`] handle and the resolved `user_id`. A tiny in-process
//! cache exists for [`default_space_id`] because it is hit on every
//! `store`-style request.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use rusqlite::{params, OptionalExtension};

use crate::db::Database;
use crate::{EngError, Result};

/// In-process cache of `(user_id -> default_space_id)`. The mapping is
/// stable for the lifetime of a user (the row in `spaces` is created
/// at user provisioning and never auto-deleted). Cache invalidation is
/// only required if an operator manually drops the `default` row.
static DEFAULT_SPACE_CACHE: LazyLock<Mutex<HashMap<i64, i64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Strip the in-process cache entry for a user. Call this after admin
/// operations that may delete or recreate the default space (rare).
pub fn invalidate_default_space_cache(user_id: i64) {
    if let Ok(mut cache) = DEFAULT_SPACE_CACHE.lock() {
        cache.remove(&user_id);
    }
}

/// Aliases that the input normalization layer must collapse to the
/// `default` space. Comparison is case-insensitive after trimming.
const DEFAULT_ALIASES: &[&str] = &["default", "null", "none", "cross", ""];

/// Maximum length of a space name accepted at the API surface. Names
/// longer than this are rejected to keep the `spaces.name` column and
/// indices reasonable. The DB has no hard cap but applications should.
pub const MAX_SPACE_NAME_LEN: usize = 64;

/// Normalize a free-form space name to the canonical wire/storage form
/// (lowercase, trimmed, only `[a-z0-9_-]` retained). This is the form
/// shared by the CLI, the bash hook, and the server helpers, so the
/// resolution stays stable across the three implementations.
pub fn normalize_space_name(input: &str) -> String {
    input
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect()
}

/// Return `true` if `name` (already lowercased + trimmed) is one of the
/// well-known aliases that collapse to the default space.
fn is_default_alias(name: &str) -> bool {
    DEFAULT_ALIASES.iter().any(|a| a.eq_ignore_ascii_case(name))
}

/// Resolve the id of the `default` space for `user_id`, creating it on
/// the fly if missing (covers the exotic case where a user was migrated
/// without going through `create_user`). The first lookup hits the DB;
/// subsequent calls in the same process serve from cache.
pub async fn default_space_id(db: &Database, user_id: i64) -> Result<i64> {
    if let Ok(cache) = DEFAULT_SPACE_CACHE.lock() {
        if let Some(&id) = cache.get(&user_id) {
            return Ok(id);
        }
    }

    let id = db
        .read(move |conn| {
            conn.query_row(
                "SELECT id FROM spaces WHERE user_id = ?1 AND name = 'default' LIMIT 1",
                params![user_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|e| EngError::DatabaseMessage(e.to_string()))
        })
        .await?;

    let id = match id {
        Some(id) => id,
        None => {
            // Exotic path: user has no `default` row. Create it idempotently.
            db.write(move |conn| {
                conn.execute(
                    "INSERT OR IGNORE INTO spaces (user_id, name) VALUES (?1, 'default')",
                    params![user_id],
                )
                .map_err(|e| EngError::DatabaseMessage(e.to_string()))?;
                conn.query_row(
                    "SELECT id FROM spaces WHERE user_id = ?1 AND name = 'default' LIMIT 1",
                    params![user_id],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|e| EngError::DatabaseMessage(e.to_string()))
            })
            .await?
        }
    };

    if let Ok(mut cache) = DEFAULT_SPACE_CACHE.lock() {
        cache.insert(user_id, id);
    }
    Ok(id)
}

/// Resolve a space by name, creating it if it does not exist. The name
/// is normalized through [`normalize_space_name`] first; the canonical
/// `default` alias short-circuits to [`default_space_id`].
///
/// Idempotent: concurrent callers race the `INSERT OR IGNORE` and read
/// back the row that won the race.
pub async fn resolve_or_create_space(
    db: &Database,
    user_id: i64,
    name: &str,
) -> Result<i64> {
    let normalized = normalize_space_name(name);
    if normalized.is_empty() || is_default_alias(&normalized) {
        return default_space_id(db, user_id).await;
    }
    if normalized.len() > MAX_SPACE_NAME_LEN {
        return Err(EngError::InvalidInput(format!(
            "space name too long (max {MAX_SPACE_NAME_LEN} chars)"
        )));
    }

    let lookup_name = normalized.clone();
    let existing = db
        .read(move |conn| {
            conn.query_row(
                "SELECT id FROM spaces WHERE user_id = ?1 AND name = ?2 LIMIT 1",
                params![user_id, lookup_name],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|e| EngError::DatabaseMessage(e.to_string()))
        })
        .await?;
    if let Some(id) = existing {
        return Ok(id);
    }

    let insert_name = normalized.clone();
    db.write(move |conn| {
        conn.execute(
            "INSERT OR IGNORE INTO spaces (user_id, name) VALUES (?1, ?2)",
            params![user_id, insert_name],
        )
        .map_err(|e| EngError::DatabaseMessage(e.to_string()))?;
        conn.query_row(
            "SELECT id FROM spaces WHERE user_id = ?1 AND name = ?2 LIMIT 1",
            params![user_id, normalized],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| EngError::DatabaseMessage(e.to_string()))
    })
    .await
}

/// Validate that `space_id` belongs to `user_id`. Used by the
/// normalization layer to reject cross-user `space_id` payloads.
pub async fn space_belongs_to_user(
    db: &Database,
    user_id: i64,
    space_id: i64,
) -> Result<bool> {
    db.read(move |conn| {
        conn.query_row(
            "SELECT 1 FROM spaces WHERE id = ?1 AND user_id = ?2 LIMIT 1",
            params![space_id, user_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map(|row| row.is_some())
        .map_err(|e| EngError::DatabaseMessage(e.to_string()))
    })
    .await
}

/// Apply the Patch 33 input routing table on a `(space_id?, space?)`
/// payload pair and return the final `space_id` to persist. Returns
/// [`EngError::InvalidInput`] if `space_id` is set but does not belong
/// to the user.
///
/// Routing table:
///
/// | Payload                                | Stored `space_id`              |
/// |----------------------------------------|--------------------------------|
/// | both absent / `space_id = 0` / `space` empty | `default_space_id(user)` |
/// | `space` matches a default alias        | `default_space_id(user)`       |
/// | `space` is a non-alias name            | `resolve_or_create_space`      |
/// | `space_id = N` valid for user          | `N`                            |
/// | `space_id = N` not owned by user       | 400 `InvalidInput`             |
pub async fn normalize_space_input(
    db: &Database,
    user_id: i64,
    space_id: Option<i64>,
    space: Option<&str>,
) -> Result<i64> {
    // 1. Explicit numeric id takes precedence when > 0.
    if let Some(id) = space_id {
        if id > 0 {
            if space_belongs_to_user(db, user_id, id).await? {
                return Ok(id);
            }
            return Err(EngError::InvalidInput(format!(
                "space_id {id} does not belong to user {user_id}"
            )));
        }
    }

    // 2. Sentinel 0 / explicit alias / empty string -> default.
    if let Some(name) = space {
        let trimmed = name.trim();
        if trimmed.is_empty() || is_default_alias(trimmed) {
            return default_space_id(db, user_id).await;
        }
        return resolve_or_create_space(db, user_id, trimmed).await;
    }

    // 3. Nothing supplied -> default.
    default_space_id(db, user_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_accent_and_spaces() {
        assert_eq!(normalize_space_name("Kleos VOCSAP"), "kleosvocsap");
        assert_eq!(normalize_space_name(" Foo-Bar_42 "), "foo-bar_42");
        assert_eq!(
            normalize_space_name("local-firewall  "),
            "local-firewall"
        );
    }

    #[test]
    fn normalize_drops_special_chars() {
        assert_eq!(normalize_space_name("foo!@#bar"), "foobar");
        assert_eq!(normalize_space_name("a/b\\c"), "abc");
    }

    #[test]
    fn aliases_recognized() {
        for alias in ["default", "Default", "NULL", "none", "Cross", ""] {
            assert!(is_default_alias(alias), "{alias} should be alias");
        }
        for non in ["kleos", "mnemo", "test-project"] {
            assert!(!is_default_alias(non), "{non} should NOT be alias");
        }
    }

    #[test]
    fn cache_invalidation_drops_entry() {
        // Seed
        if let Ok(mut cache) = DEFAULT_SPACE_CACHE.lock() {
            cache.insert(9999, 42);
        }
        invalidate_default_space_cache(9999);
        let cached = DEFAULT_SPACE_CACHE.lock().unwrap().get(&9999).copied();
        assert!(cached.is_none());
    }
}
