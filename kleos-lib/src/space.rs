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

/// Resolve a `(space_id?, space?)` pair for **read** paths (search /
/// list / recall). Unlike [`normalize_space_input`], this returns `None`
/// when both inputs are absent, preserving upstream "no filter" semantics
/// instead of forcing the default space.
///
/// Returns:
/// * `Ok(Some(id))` when `space_id` or `space` resolves to a concrete id.
/// * `Ok(None)`     when both are absent.
/// * `Err(InvalidInput)` when `space_id` is set but does not belong to user.
pub async fn resolve_space_filter(
    db: &Database,
    user_id: i64,
    space_id: Option<i64>,
    space: Option<&str>,
) -> Result<Option<i64>> {
    if let Some(id) = space_id {
        if id > 0 {
            if space_belongs_to_user(db, user_id, id).await? {
                return Ok(Some(id));
            }
            return Err(EngError::InvalidInput(format!(
                "space_id {id} does not belong to user {user_id}"
            )));
        }
    }
    if let Some(name) = space {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        let id = resolve_or_create_space(db, user_id, trimmed).await?;
        return Ok(Some(id));
    }
    Ok(None)
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

/// Patch 33.2 -- a read scope resolved once per request, for the read paths
/// that do not go through `hybrid_search`'s own post-filter (context assembly
/// layers, prompt generation). Same semantics as the `/search` filter:
///
/// * strict (`include_unscoped = false`): `space_id = current` only;
/// * inclusive (`include_unscoped = true`, the HTTP default): `current`, the
///   user's `default` space, and legacy `space_id IS NULL` rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpaceScope {
    /// The space the caller is scoped to.
    pub space_id: i64,
    /// Whether the default space and legacy NULL rows are also visible.
    pub include_unscoped: bool,
    /// The user's default space id, resolved only in inclusive mode.
    default_space_id: Option<i64>,
}

impl SpaceScope {
    /// Build a scope for `space_id`. In inclusive mode the default space id is
    /// resolved now; a lookup failure degrades to "current + NULL" rather than
    /// failing the read.
    pub async fn new(db: &Database, user_id: i64, space_id: i64, include_unscoped: bool) -> Self {
        let default_space_id = if include_unscoped {
            default_space_id(db, user_id).await.ok()
        } else {
            None
        };
        Self {
            space_id,
            include_unscoped,
            default_space_id,
        }
    }

    /// Whether a row stored with `row_space_id` is visible in this scope.
    pub fn allows(&self, row_space_id: Option<i64>) -> bool {
        match row_space_id {
            Some(sid) if sid == self.space_id => true,
            Some(sid) => self.include_unscoped && self.default_space_id == Some(sid),
            None => self.include_unscoped,
        }
    }

    /// The `(space_id, include_unscoped)` pair to hand to a `SearchRequest`,
    /// whose post-filter applies the same semantics.
    pub fn search_filter(scope: Option<&Self>) -> (Option<i64>, Option<bool>) {
        match scope {
            Some(s) => (Some(s.space_id), Some(s.include_unscoped)),
            None => (None, None),
        }
    }
}

/// Patch 33.2 -- batch lookup of `memories.space_id` for `ids`, scoped to the
/// owner. Ids absent from the result do not exist for this user.
pub async fn memory_space_ids(
    db: &Database,
    user_id: i64,
    ids: &[i64],
) -> Result<HashMap<i64, Option<i64>>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = vec!["?"; ids.len()].join(",");
    let sql =
        format!("SELECT id, space_id FROM memories WHERE user_id = ? AND id IN ({placeholders})");
    let mut bind: Vec<i64> = Vec::with_capacity(ids.len() + 1);
    bind.push(user_id);
    bind.extend_from_slice(ids);
    db.read(move |conn| {
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| EngError::DatabaseMessage(e.to_string()))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(bind.iter()), |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?))
            })
            .map_err(|e| EngError::DatabaseMessage(e.to_string()))?;
        rows.collect::<std::result::Result<HashMap<_, _>, _>>()
            .map_err(|e| EngError::DatabaseMessage(e.to_string()))
    })
    .await
}

/// Patch 33.2 -- drop the items of `items` whose memory is outside `scope`
/// (no-op when `scope` is `None`). Fails closed: if the space lookup errors,
/// every item is dropped rather than leaking out-of-scope memories.
pub async fn retain_in_scope<T>(
    db: &Database,
    user_id: i64,
    scope: Option<&SpaceScope>,
    items: &mut Vec<T>,
    id_of: impl Fn(&T) -> i64,
) {
    let Some(scope) = scope else {
        return;
    };
    if items.is_empty() {
        return;
    }
    let ids: Vec<i64> = items.iter().map(&id_of).collect();
    match memory_space_ids(db, user_id, &ids).await {
        Ok(map) => items.retain(|it| map.get(&id_of(it)).is_some_and(|sid| scope.allows(*sid))),
        Err(e) => {
            tracing::warn!(
                "space scope lookup failed, dropping {} item(s): {e}",
                items.len()
            );
            items.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_allows_strict_and_inclusive() {
        let strict = SpaceScope {
            space_id: 7,
            include_unscoped: false,
            default_space_id: Some(2),
        };
        assert!(strict.allows(Some(7)));
        assert!(!strict.allows(Some(2)));
        assert!(!strict.allows(Some(9)));
        assert!(!strict.allows(None));

        let inclusive = SpaceScope {
            include_unscoped: true,
            ..strict
        };
        assert!(inclusive.allows(Some(7)));
        assert!(inclusive.allows(Some(2)));
        assert!(!inclusive.allows(Some(9)));
        assert!(inclusive.allows(None));

        // Default space unresolved: inclusive still admits current + NULL only.
        let degraded = SpaceScope {
            default_space_id: None,
            ..inclusive
        };
        assert!(degraded.allows(Some(7)));
        assert!(!degraded.allows(Some(2)));
        assert!(degraded.allows(None));
    }

    #[test]
    fn search_filter_maps_scope() {
        let s = SpaceScope {
            space_id: 3,
            include_unscoped: false,
            default_space_id: None,
        };
        assert_eq!(SpaceScope::search_filter(Some(&s)), (Some(3), Some(false)));
        assert_eq!(SpaceScope::search_filter(None), (None, None));
    }

    #[tokio::test]
    async fn retain_in_scope_filters_by_memory_space() {
        let db = Database::connect_memory().await.expect("in-mem db");
        let uid: i64 = 3_302_002;
        db.write(move |conn| {
            conn.execute(
                "INSERT OR IGNORE INTO users (id, username) VALUES (?1, ?2)",
                params![uid, format!("patch33-2-user-{uid}")],
            )?;
            Ok(())
        })
        .await
        .expect("seed user");
        let named = resolve_or_create_space(&db, uid, "patch33-2-named")
            .await
            .unwrap();
        let other = resolve_or_create_space(&db, uid, "patch33-2-other")
            .await
            .unwrap();
        let def = default_space_id(&db, uid).await.unwrap();

        async fn seed(db: &Database, uid: i64, content: &str, space_id: Option<i64>) -> i64 {
            crate::memory::store(
                db,
                crate::memory::types::StoreRequest {
                    content: content.to_string(),
                    user_id: Some(uid),
                    space_id,
                    ..Default::default()
                },
                None,
                false,
            )
            .await
            .expect("seed store")
            .id
        }
        let in_named = seed(&db, uid, "patch33-2 row in named space", Some(named)).await;
        let in_other = seed(&db, uid, "patch33-2 row in other space", Some(other)).await;
        let in_default = seed(&db, uid, "patch33-2 row in default space", Some(def)).await;
        let legacy = seed(&db, uid, "patch33-2 legacy row without space", None).await;
        // store() may normalise a missing space; force a true legacy NULL row.
        db.write(move |conn| {
            conn.execute(
                "UPDATE memories SET space_id = NULL WHERE id = ?1",
                params![legacy],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let all = vec![in_named, in_other, in_default, legacy, 987_654_321];

        let strict = SpaceScope::new(&db, uid, named, false).await;
        let mut items = all.clone();
        retain_in_scope(&db, uid, Some(&strict), &mut items, |id| *id).await;
        assert_eq!(items, vec![in_named]);

        let inclusive = SpaceScope::new(&db, uid, named, true).await;
        let mut items = all.clone();
        retain_in_scope(&db, uid, Some(&inclusive), &mut items, |id| *id).await;
        assert_eq!(items, vec![in_named, in_default, legacy]);

        let mut items = all.clone();
        retain_in_scope(&db, uid, None, &mut items, |id| *id).await;
        assert_eq!(items, all);
    }

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
