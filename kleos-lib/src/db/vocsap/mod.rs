//! VOCSAP schema overlay channel (Patch 41).
//!
//! Kleos is an active fork of Ghost-Frame. Historically, VOCSAP schema additions
//! lived inside the numbered migration arrays shared with upstream
//! ([`super::migrations::MIGRATIONS`] and [`super::tenant_migrations::TENANT_MIGRATIONS`]).
//! Every upstream merge that reclaimed a slot VOCSAP had taken forced a renumber,
//! which on live deployments made the version counter pass over the upstream body
//! occupying that slot -- the runner skipped it silently and a later migration
//! crashed with `no such column`. The aa6a0bec merge hit exactly this (repaired
//! one-shot by the now-removed Patch 40 self-heal).
//!
//! This channel removes VOCSAP additions from the numbered sequence entirely.
//! Overlays run *after* the upstream dispatch loop, carry no version number in the
//! shared sequence, and are idempotent via an explicit `needs` guard (SQLite has
//! no `ALTER TABLE ADD COLUMN IF NOT EXISTS`). VOCSAP therefore never occupies a
//! slot upstream might later want, and the whole renumber/skip bug class is gone.
//!
//! Post-only by design: the only pre-dispatch use was the Patch 40 self-heal, a
//! one-shot repair of the aa6a0bec renumber that is no longer needed once VOCSAP
//! stops taking slots. A pre-dispatch phase can be added additively later if a
//! genuine "prerequisite before an upstream migration" case ever appears.
//!
//! The DDL is embedded at compile time (inline `execute_batch` or, for larger
//! additions, `include_str!("<name>.sql")`), never hot-reloaded: a malformed DDL
//! applied every boot could corrupt or lock the production DB, an asymmetry of
//! risk that does not apply to the prompts/lexicon/gate overlays.

use crate::{EngError, Result};
use rusqlite::Connection;
use tracing::info;

/// A VOCSAP schema overlay for the monolith DB.
///
/// `needs` returns `true` when the addition is not yet present (apply required),
/// `false` when it is already there (skip). `apply` performs the DDL; it may
/// assume `needs` was just `true` but should still be written defensively
/// (the ported bodies keep their own `IF NOT EXISTS` / column guards).
struct VocsapOverlay {
    /// Stable name for logs. Never a number from the upstream version sequence.
    name: &'static str,
    needs: fn(&Connection) -> Result<bool>,
    apply: fn(&Connection) -> Result<()>,
}

/// A VOCSAP schema overlay for a tenant shard.
///
/// Same contract as [`VocsapOverlay`], plus `apply` receives the shard owner so a
/// `user_id`-style backfill can target it. None of the current overlays need the
/// backfill (their columns are nullable or carry a literal `DEFAULT`), but the
/// parameter is kept so a future owner-scoped tenant addition does not require a
/// signature change. `owner_user_id` is `None` for the reserved non-numeric
/// handoffs shard and for in-memory test shards.
struct VocsapTenantOverlay {
    name: &'static str,
    needs: fn(&Connection) -> Result<bool>,
    apply: fn(&Connection, Option<i64>) -> Result<()>,
}

// --- schema introspection helpers (the idempotence guards) ---

/// True if `table` has a column named `column`. Returns `false` for a missing
/// table (no rows in `pragma_table_info`). A missing table and a present-but-
/// column-less table are therefore indistinguishable here, so every overlay
/// `needs` guard ANDs this with [`table_exists`]: overlays also run after a
/// PARTIAL migration (e.g. tests that stop before the version that creates the
/// table), where the table may legitimately not exist yet.
fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
            rusqlite::params![table, column],
            |row| row.get(0),
        )
        .map_err(|e| EngError::DatabaseMessage(format!("pragma_table_info({table}) failed: {e}")))?;
    Ok(count > 0)
}

/// True if an index named `index` exists.
fn index_exists(conn: &Connection, index: &str) -> Result<bool> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
            rusqlite::params![index],
            |row| row.get(0),
        )
        .map_err(|e| EngError::DatabaseMessage(format!("sqlite_master index lookup failed: {e}")))?;
    Ok(count > 0)
}

/// True if a table named `table` exists. Every overlay `needs` guard ANDs this
/// in front of its column/index probe so an overlay cleanly skips when its target
/// table has not been created yet -- the case under a PARTIAL migration (upstream
/// tests that stop before the version that creates the table run the overlays too).
fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            rusqlite::params![table],
            |row| row.get(0),
        )
        .map_err(|e| EngError::DatabaseMessage(format!("sqlite_master table lookup failed: {e}")))?;
    Ok(count > 0)
}

// --- monolith overlay bodies (ported from the former numbered migrations) ---

/// Add nullable `gate_id INTEGER` to `approvals` so the gate `pending_approval`
/// workflow can correlate a `gate_requests` row with the `approvals` row consumed
/// by the TUI (Patch 21, was monolith v84 / originally v64).
fn monolith_approvals_gate_id_apply(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE approvals ADD COLUMN gate_id INTEGER;
         CREATE INDEX IF NOT EXISTS idx_approvals_gate_id
            ON approvals(gate_id) WHERE gate_id IS NOT NULL;",
    )
    .map_err(|e| EngError::DatabaseMessage(format!("vocsap overlay approvals_gate_id failed: {e}")))?;
    Ok(())
}

/// Add nullable `space_id INTEGER` to the monolith `conversations` table so the
/// spaces partitioning convention extends to multi-turn agent threads in single-DB
/// mode (Patch 33). The tenant shards get this via [`tenant_conversations_space_id_apply`];
/// the monolith path was missing its mirror overlay, so a fresh single-DB built by
/// [`super::Database::connect_memory`] had no `conversations.space_id` even though
/// `create_conversation` INSERTs it unconditionally. Identical DDL to the tenant body.
fn monolith_conversations_space_id_apply(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE conversations ADD COLUMN space_id INTEGER;
         CREATE INDEX IF NOT EXISTS idx_conv_space ON conversations(space_id);",
    )
    .map_err(|e| {
        EngError::DatabaseMessage(format!("vocsap overlay conversations_space_id failed: {e}"))
    })?;
    Ok(())
}

/// Create the shared toolbox catalog: `toolbox_tools`, its FTS5 shadow and the
/// three sync triggers (Patch 53). User-agnostic by design -- one row per
/// canonical tool key, shared across users; who owns what is recorded by
/// `toolbox_locations` instead. Monolith-only: the sheet and its embedding
/// always live in the main DB, never in a shard.
fn monolith_toolbox_tools_apply(conn: &Connection) -> Result<()> {
    conn.execute_batch(include_str!("toolbox_tools.sql"))
        .map_err(|e| EngError::DatabaseMessage(format!("vocsap overlay toolbox_tools failed: {e}")))?;
    Ok(())
}

/// Create `toolbox_locations` (Patch 53). Registered on BOTH registries: in
/// sharded mode the locations live in the caller's shard, in single-DB mode
/// `ResolvedDb` is the main DB and the table must exist there too.
fn toolbox_locations_ddl(conn: &Connection) -> Result<()> {
    conn.execute_batch(include_str!("toolbox_locations.sql"))
        .map_err(|e| {
            EngError::DatabaseMessage(format!("vocsap overlay toolbox_locations failed: {e}"))
        })?;
    Ok(())
}

fn monolith_toolbox_locations_apply(conn: &Connection) -> Result<()> {
    toolbox_locations_ddl(conn)
}

// --- tenant overlay bodies (ported from the former numbered migrations) ---

/// Repair tenants stuck on the pre-merge supervisor_injections layout: re-add
/// `rule_id` / `claimed_at` and rebuild the pending index with the post-v48
/// predicate (Patch 20, was tenant v72 / originally v55). Idempotent: guarded on
/// `claimed_at`, and the index rebuild uses DROP IF EXISTS / CREATE IF NOT EXISTS.
fn tenant_supervisor_injections_repair_apply(conn: &Connection, _owner: Option<i64>) -> Result<()> {
    if !table_has_column(conn, "supervisor_injections", "rule_id")? {
        conn.execute_batch(
            "ALTER TABLE supervisor_injections ADD COLUMN rule_id TEXT NOT NULL DEFAULT '';",
        )
        .map_err(|e| {
            EngError::DatabaseMessage(format!("vocsap overlay supervisor repair (rule_id) failed: {e}"))
        })?;
    }
    if !table_has_column(conn, "supervisor_injections", "claimed_at")? {
        conn.execute_batch("ALTER TABLE supervisor_injections ADD COLUMN claimed_at TEXT;")
            .map_err(|e| {
                EngError::DatabaseMessage(format!(
                    "vocsap overlay supervisor repair (claimed_at) failed: {e}"
                ))
            })?;
    }
    conn.execute_batch(
        "DROP INDEX IF EXISTS idx_supervisor_injections_pending;
         CREATE INDEX IF NOT EXISTS idx_supervisor_injections_pending
            ON supervisor_injections(user_id, session_id)
            WHERE claimed_at IS NULL;",
    )
    .map_err(|e| {
        EngError::DatabaseMessage(format!("vocsap overlay supervisor repair (index) failed: {e}"))
    })?;
    Ok(())
}

/// Add nullable `gate_id INTEGER` to tenant `approvals` (Patch 21, was tenant
/// v73 / originally v56).
fn tenant_approvals_gate_id_apply(conn: &Connection, _owner: Option<i64>) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE approvals ADD COLUMN gate_id INTEGER;
         CREATE INDEX IF NOT EXISTS idx_approvals_gate_id
            ON approvals(gate_id) WHERE gate_id IS NOT NULL;",
    )
    .map_err(|e| {
        EngError::DatabaseMessage(format!("vocsap overlay tenant approvals_gate_id failed: {e}"))
    })?;
    Ok(())
}

/// Add nullable `space_id INTEGER` to `conversations` so the spaces partitioning
/// convention extends to multi-turn agent threads (Patch 33, was tenant v74 /
/// originally v57).
fn tenant_conversations_space_id_apply(conn: &Connection, _owner: Option<i64>) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE conversations ADD COLUMN space_id INTEGER;
         CREATE INDEX IF NOT EXISTS idx_conv_space ON conversations(space_id);",
    )
    .map_err(|e| {
        EngError::DatabaseMessage(format!("vocsap overlay conversations_space_id failed: {e}"))
    })?;
    Ok(())
}

/// De-duplicate `structured_facts` then enforce uniqueness on
/// `(memory_id, subject, predicate, object)` (Patch 38, was tenant v75 /
/// originally v58). The DELETE is idempotent (no-op on a deduplicated table) and
/// MUST run before the CREATE UNIQUE INDEX, which would otherwise fail on
/// duplicates.
fn tenant_structured_facts_unique_apply(conn: &Connection, _owner: Option<i64>) -> Result<()> {
    conn.execute_batch(
        "DELETE FROM structured_facts \
           WHERE id NOT IN ( \
             SELECT MIN(id) FROM structured_facts \
               GROUP BY memory_id, subject, predicate, object \
           ); \
         CREATE UNIQUE INDEX IF NOT EXISTS idx_structured_facts_subj_pred_obj \
           ON structured_facts(memory_id, subject, predicate, object);",
    )
    .map_err(|e| {
        EngError::DatabaseMessage(format!(
            "vocsap overlay structured_facts unique index failed: {e}"
        ))
    })?;
    Ok(())
}

/// Add `extraction_source TEXT NOT NULL DEFAULT 'embedded'` to `structured_facts`
/// (Patch 38, was tenant v76 / originally v59).
fn tenant_structured_facts_extraction_source_apply(
    conn: &Connection,
    _owner: Option<i64>,
) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE structured_facts \
           ADD COLUMN extraction_source TEXT NOT NULL DEFAULT 'embedded';",
    )
    .map_err(|e| {
        EngError::DatabaseMessage(format!(
            "vocsap overlay structured_facts extraction_source failed: {e}"
        ))
    })?;
    Ok(())
}

/// Create `toolbox_locations` in a tenant shard (Patch 53). Same DDL as the
/// monolith body; no owner-scoped backfill (the table is created empty).
fn tenant_toolbox_locations_apply(conn: &Connection, _owner: Option<i64>) -> Result<()> {
    toolbox_locations_ddl(conn)
}

// --- overlay registries ---

// Each `needs` guard ANDs `table_exists` in front of its column/index probe.
// `table_has_column` / `index_exists` return false on a MISSING table, which on
// its own would make `needs` true and fire `apply` against a non-existent table
// under a PARTIAL migration (e.g. upstream tests that stop before the version
// that creates the table, which still run the overlays). The `table_exists`
// guard makes such an overlay a clean no-op until its target table exists.
static VOCSAP_MONOLITH_OVERLAYS: &[VocsapOverlay] = &[
    VocsapOverlay {
        name: "approvals_gate_id",
        needs: |conn| {
            Ok(table_exists(conn, "approvals")? && !table_has_column(conn, "approvals", "gate_id")?)
        },
        apply: monolith_approvals_gate_id_apply,
    },
    VocsapOverlay {
        name: "conversations_space_id",
        needs: |conn| {
            Ok(table_exists(conn, "conversations")?
                && !table_has_column(conn, "conversations", "space_id")?)
        },
        apply: monolith_conversations_space_id_apply,
    },
    // Creator overlays: they own the table they guard on, so their `needs` is a
    // plain "not there yet" and they legitimately fire on a connection where the
    // table is absent (see CREATOR_OVERLAYS in the tests).
    VocsapOverlay {
        name: "toolbox_tools",
        needs: |conn| Ok(!table_exists(conn, "toolbox_tools")?),
        apply: monolith_toolbox_tools_apply,
    },
    VocsapOverlay {
        name: "toolbox_locations",
        needs: |conn| Ok(!table_exists(conn, "toolbox_locations")?),
        apply: monolith_toolbox_locations_apply,
    },
];

static VOCSAP_TENANT_OVERLAYS: &[VocsapTenantOverlay] = &[
    VocsapTenantOverlay {
        name: "supervisor_injections_repair",
        needs: |conn| {
            Ok(table_exists(conn, "supervisor_injections")?
                && !table_has_column(conn, "supervisor_injections", "claimed_at")?)
        },
        apply: tenant_supervisor_injections_repair_apply,
    },
    VocsapTenantOverlay {
        name: "approvals_gate_id",
        needs: |conn| {
            Ok(table_exists(conn, "approvals")?
                && !table_has_column(conn, "approvals", "gate_id")?)
        },
        apply: tenant_approvals_gate_id_apply,
    },
    VocsapTenantOverlay {
        name: "conversations_space_id",
        needs: |conn| {
            Ok(table_exists(conn, "conversations")?
                && !table_has_column(conn, "conversations", "space_id")?)
        },
        apply: tenant_conversations_space_id_apply,
    },
    VocsapTenantOverlay {
        name: "structured_facts_unique_index",
        needs: |conn| {
            Ok(table_exists(conn, "structured_facts")?
                && !index_exists(conn, "idx_structured_facts_subj_pred_obj")?)
        },
        apply: tenant_structured_facts_unique_apply,
    },
    VocsapTenantOverlay {
        name: "structured_facts_extraction_source",
        needs: |conn| {
            Ok(table_exists(conn, "structured_facts")?
                && !table_has_column(conn, "structured_facts", "extraction_source")?)
        },
        apply: tenant_structured_facts_extraction_source_apply,
    },
    // Creator overlay (see the monolith registry): the shard holds the caller's
    // tool locations; the shared sheets stay in the main DB.
    VocsapTenantOverlay {
        name: "toolbox_locations",
        needs: |conn| Ok(!table_exists(conn, "toolbox_locations")?),
        apply: tenant_toolbox_locations_apply,
    },
];

// --- entry points (called at the end of each runner) ---

/// Apply every monolith overlay whose `needs` is `true`. Called at the end of
/// [`super::migrations::run_migrations`], after the upstream dispatch loop.
/// Idempotent: a no-op on an already-complete schema.
pub(crate) fn apply_monolith_overlays(conn: &Connection) -> Result<()> {
    for o in VOCSAP_MONOLITH_OVERLAYS {
        if (o.needs)(conn)? {
            info!("vocsap monolith overlay: applying {}", o.name);
            (o.apply)(conn)?;
        }
    }
    Ok(())
}

/// Apply every tenant overlay whose `needs` is `true`, passing `owner_user_id`
/// through for any owner-scoped backfill. Called at the end of
/// [`super::tenant_migrations::run_tenant_migrations`], after the upstream
/// dispatch loop. Idempotent: a no-op on an already-complete shard.
pub(crate) fn apply_tenant_overlays(conn: &Connection, owner_user_id: Option<i64>) -> Result<()> {
    for o in VOCSAP_TENANT_OVERLAYS {
        if (o.needs)(conn)? {
            info!("vocsap tenant overlay: applying {}", o.name);
            (o.apply)(conn, owner_user_id)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Overlays that CREATE their own table instead of altering an upstream one.
    /// The "no-op when the target table is absent" contract only binds the
    /// alter-style overlays: a creator overlay firing on a connection where its
    /// table does not exist yet is exactly its job, not the partial-migration
    /// hazard that guard was written for.
    const CREATOR_OVERLAYS: &[&str] = &["toolbox_tools", "toolbox_locations"];

    /// Minimal upstream-shaped fixtures: just enough of each target table for the
    /// overlays to act on. Deliberately omits the VOCSAP columns/index so the
    /// overlays have work to do on the first run.
    fn make_monolith_base() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE approvals (id INTEGER PRIMARY KEY, status TEXT);
             CREATE TABLE conversations (id INTEGER PRIMARY KEY, title TEXT);",
        )
        .unwrap();
        conn
    }

    fn make_tenant_base() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE supervisor_injections (
                id INTEGER PRIMARY KEY,
                user_id INTEGER NOT NULL DEFAULT 1,
                session_id TEXT
             );
             CREATE TABLE approvals (id INTEGER PRIMARY KEY, status TEXT);
             CREATE TABLE conversations (id INTEGER PRIMARY KEY, title TEXT);
             CREATE TABLE structured_facts (
                id INTEGER PRIMARY KEY,
                memory_id INTEGER,
                subject TEXT,
                predicate TEXT,
                object TEXT
             );",
        )
        .unwrap();
        conn
    }

    #[test]
    fn monolith_overlays_apply_then_noop() {
        let conn = make_monolith_base();
        assert!(!table_has_column(&conn, "approvals", "gate_id").unwrap());
        assert!(!table_has_column(&conn, "conversations", "space_id").unwrap());

        apply_monolith_overlays(&conn).unwrap();
        assert!(table_has_column(&conn, "approvals", "gate_id").unwrap());
        assert!(index_exists(&conn, "idx_approvals_gate_id").unwrap());
        assert!(table_has_column(&conn, "conversations", "space_id").unwrap());
        assert!(index_exists(&conn, "idx_conv_space").unwrap());

        // Second run: every needs() is now false -> no apply, no error.
        for o in VOCSAP_MONOLITH_OVERLAYS {
            assert!(!(o.needs)(&conn).unwrap(), "overlay {} should be no-op", o.name);
        }
        apply_monolith_overlays(&conn).unwrap();
    }

    #[test]
    fn tenant_overlays_apply_then_noop() {
        let conn = make_tenant_base();
        apply_tenant_overlays(&conn, Some(1)).unwrap();

        assert!(table_has_column(&conn, "supervisor_injections", "claimed_at").unwrap());
        assert!(table_has_column(&conn, "supervisor_injections", "rule_id").unwrap());
        assert!(table_has_column(&conn, "approvals", "gate_id").unwrap());
        assert!(table_has_column(&conn, "conversations", "space_id").unwrap());
        assert!(index_exists(&conn, "idx_structured_facts_subj_pred_obj").unwrap());
        assert!(table_has_column(&conn, "structured_facts", "extraction_source").unwrap());

        // Second run: all needs() false, no apply, no error.
        for o in VOCSAP_TENANT_OVERLAYS {
            assert!(!(o.needs)(&conn).unwrap(), "overlay {} should be no-op", o.name);
        }
        apply_tenant_overlays(&conn, Some(1)).unwrap();
    }

    #[test]
    fn tenant_overlays_noop_on_complete_schema() {
        // A shard that already has every VOCSAP addition: zero apply on the first
        // call (mirrors the downgraded-but-already-migrated LXC 121 case).
        let conn = make_tenant_base();
        apply_tenant_overlays(&conn, Some(1)).unwrap();
        for o in VOCSAP_TENANT_OVERLAYS {
            assert!(!(o.needs)(&conn).unwrap());
        }
    }

    #[test]
    fn structured_facts_unique_dedups_before_index() {
        let conn = make_tenant_base();
        // Two duplicate (memory_id, subject, predicate, object) rows: the unique
        // index can only be created after the DELETE removes the duplicate.
        conn.execute_batch(
            "INSERT INTO structured_facts (memory_id, subject, predicate, object)
                VALUES (1, 's', 'p', 'o'), (1, 's', 'p', 'o'), (2, 's2', 'p2', 'o2');",
        )
        .unwrap();
        apply_tenant_overlays(&conn, Some(1)).unwrap();

        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM structured_facts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 2, "the duplicate row must have been removed");
        assert!(index_exists(&conn, "idx_structured_facts_subj_pred_obj").unwrap());
    }

    #[test]
    fn overlays_are_noop_when_target_tables_absent() {
        // Partial-migration scenario: a connection where NONE of the target tables
        // exist yet (e.g. run_tenant_migrations_to(conn, _, 23), which stops before
        // the versions that create supervisor_injections/approvals/etc. but still
        // calls apply_*_overlays). Without the table_exists guard, table_has_column
        // returns false on a missing table -> needs() = true -> apply() = ERROR
        // "no such table". With the guard every overlay must cleanly no-op.
        // Creator overlays are exempt: they own their table (see CREATOR_OVERLAYS).
        let conn = Connection::open_in_memory().unwrap();
        for o in VOCSAP_MONOLITH_OVERLAYS {
            if CREATOR_OVERLAYS.contains(&o.name) {
                continue;
            }
            assert!(!(o.needs)(&conn).unwrap(), "monolith overlay {} must skip when its table is absent", o.name);
        }
        for o in VOCSAP_TENANT_OVERLAYS {
            if CREATOR_OVERLAYS.contains(&o.name) {
                continue;
            }
            assert!(!(o.needs)(&conn).unwrap(), "tenant overlay {} must skip when its table is absent", o.name);
        }
        // The entry points must not error against the table-less connection.
        apply_monolith_overlays(&conn).unwrap();
        apply_tenant_overlays(&conn, Some(1)).unwrap();
        apply_tenant_overlays(&conn, None).unwrap();
    }

    #[test]
    fn toolbox_overlays_create_then_noop_on_monolith() {
        // Bare connection: the toolbox overlays create their own tables, so they
        // must fire here and be a clean no-op on the second run.
        let conn = make_monolith_base();
        apply_monolith_overlays(&conn).unwrap();

        assert!(table_exists(&conn, "toolbox_tools").unwrap());
        assert!(table_exists(&conn, "toolbox_tools_fts").unwrap());
        assert!(table_exists(&conn, "toolbox_locations").unwrap());
        assert!(index_exists(&conn, "idx_toolbox_tools_kind").unwrap());
        assert!(index_exists(&conn, "idx_toolbox_locations_user_key").unwrap());

        for o in VOCSAP_MONOLITH_OVERLAYS {
            assert!(!(o.needs)(&conn).unwrap(), "overlay {} should be no-op", o.name);
        }
        apply_monolith_overlays(&conn).unwrap();

        // The FTS triggers keep the shadow table in sync with the content table.
        conn.execute_batch(
            "INSERT INTO toolbox_tools
                (tool_key, key_kind, kind, name, summary, content_hash, created_at, updated_at)
             VALUES ('github.com/vocsap/kleos', 'git', 'repo', 'Kleos',
                     'persistent semantic memory server', 'h', 'now', 'now');",
        )
        .unwrap();
        let hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM toolbox_tools_fts WHERE toolbox_tools_fts MATCH 'semantic'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1, "the fts insert trigger must index the new row");
    }

    #[test]
    fn toolbox_locations_overlay_creates_then_noop_on_tenant() {
        let conn = make_tenant_base();
        apply_tenant_overlays(&conn, Some(1)).unwrap();
        assert!(table_exists(&conn, "toolbox_locations").unwrap());
        // The shared sheet table is monolith-only: a shard must NOT get it.
        assert!(!table_exists(&conn, "toolbox_tools").unwrap());

        for o in VOCSAP_TENANT_OVERLAYS {
            assert!(!(o.needs)(&conn).unwrap(), "overlay {} should be no-op", o.name);
        }
        apply_tenant_overlays(&conn, Some(1)).unwrap();

        // The uniqueness contract the location upsert relies on.
        conn.execute_batch(
            "INSERT INTO toolbox_locations (user_id, tool_key, host, local_path, last_seen_at, created_at)
             VALUES (1, 'k', 'h', '/p', 'now', 'now');",
        )
        .unwrap();
        let dup = conn.execute_batch(
            "INSERT INTO toolbox_locations (user_id, tool_key, host, local_path, last_seen_at, created_at)
             VALUES (1, 'k', 'h', '/p', 'now', 'now');",
        );
        assert!(dup.is_err(), "(user_id, tool_key, host, local_path) must be unique");
    }

    #[test]
    fn tenant_overlays_skip_backfill_when_owner_none() {
        // owner == None (reserved handoffs shard / in-memory test): the DDL still
        // applies, no backfill is attempted, no error.
        let conn = make_tenant_base();
        apply_tenant_overlays(&conn, None).unwrap();
        assert!(table_has_column(&conn, "conversations", "space_id").unwrap());
    }
}
