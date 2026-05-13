# Schema Migration Analysis -- v0.3.2 to v1.0.0

**Date:** 2026-05-10
**Purpose:** Assess risk of deploying v1.0.0 binaries over an existing v0.3.2 database.

---

## Current State (v0.3.2 server, 192.168.10.21)

- Server version: 0.3.2
- Migration version: **51**
- Table count: **127**
- Database path: `/var/lib/kleos/kleos.db` (SQLCipher, KLEOS_ENCRYPTION_MODE=env)
- Memories at time of analysis: 81

Queried via:
```bash
curl -s -H "Authorization: Bearer $KLEOS_API_KEY" \
  "http://192.168.10.21:4200/admin/migrations"
# => {"current_version":51, "pending_up":[], ...}
```

---

## Source State (v1.0.0)

- Max migration defined in `kleos-lib/src/db/migrations.rs`: **57**
- Migrations that will auto-run on first startup: **52 through 57 (6 migrations)**

---

## Migrations to Execute (52-57)

All are additive. Zero destructive operations.

| # | Name | Operations | Risk |
|---|------|-----------|------|
| 52 | `activity_log_table` | CREATE TABLE activity_log + 4 indexes | Safe |
| 53 | `identity_keys_scopes` | ALTER TABLE identity_keys ADD COLUMN scopes TEXT | Safe |
| 54 | `tool_manifests` | CREATE TABLE tool_manifests + 1 index | Safe |
| 55 | `handoffs_global` | CREATE TABLE handoffs + FTS5 + 3 triggers + 9 indexes | Safe |
| 56 | `user_active_and_enrollment_invites` | ALTER TABLE users ADD COLUMN is_active INT DEFAULT 1, CREATE TABLE enrollment_invites | Safe |
| 57 | `skill_dispatch_configs` | CREATE TABLE skill_dispatch_configs + 2 indexes + 1 seed row | Safe |

No DROP TABLE, no DROP COLUMN, no column renames on existing data.

---

## Tables in DB but not in source schema (46 "orphan" tables)

These tables were created by earlier migrations (version <= 51) but are no longer
referenced in the v1.0.0 schema source. The server does NOT drop them on startup.
They persist silently.

**Financial / billing (6):**
account_balances, daily_spend, payment_quotes, payment_settlements, service_pricing, volume_discounts

**Monitoring / audit (4):**
error_events, pagerank_dirty, service_dead_letters, supervisor_injections

**Credentials -- migrated to kleos-credd (4):**
cred_agent_keys, cred_audit, cred_recovery, cred_secrets
(Note: these credential tables were likely moved to kleos-credd's own DB)

**Brain / ML backend (5):**
brain_dream_runs, brain_edges, brain_meta, brain_pca_models, brain_patterns
(Note: brain backend is feature-gated, may re-appear if feature enabled)

**Vector sync (1):**
vector_sync_pending

**FTS5 auxiliary (22):**
artifacts_fts_config/data/docsize/idx, episodes_fts_*, memories_fts_*, messages_fts_*, skills_fts_*
(Note: these are SQLite-internal shadow tables auto-created by FTS5 virtual tables.
They are listed in the schema dump but are not manually defined.)

**Other (4):**
account_balances (dup), schema_version, current_state, app_state

---

## Data safety verdict

**SAFE to deploy.** The migration is entirely additive. No data loss risk.

The 72-memory export at `kleos_memory_export_2026-05-10.json` (root of repo) was
made as a precaution and is not needed for this migration.

---

## Post-deployment verification

After starting the new server, confirm:

```bash
# Check migration version advanced to 57
curl -s -H "Authorization: Bearer <key>" http://192.168.10.21:4200/admin/migrations
# Expected: {"current_version":57, "pending_up":[], ...}

# Check health
curl -s http://192.168.10.21:4200/health
# Expected: version "1.0.0", status "ok"

# Quick memory sanity check
curl -s -H "Authorization: Bearer <key>" "http://192.168.10.21:4200/list?limit=5"
# Expected: same memories as before (no data loss)
```
