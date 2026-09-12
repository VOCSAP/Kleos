-- VOCSAP Patch 53 -- toolbox: per-user tool locations.
--
-- Where a given canonical tool key lives for one user: host, local path, tags,
-- notes. This table is the ONLY thing that says "this user knows this tool", so
-- every read of the shared toolbox_tools table is filtered by the keys found
-- here (anti-leak between users). It is registered as BOTH a monolith and a
-- tenant overlay: in sharded mode it lives in the user's shard, in single-DB
-- mode it sits next to toolbox_tools in the main DB. The join is always done in
-- Rust, never in SQL, because the two tables may live in different files.

CREATE TABLE IF NOT EXISTS toolbox_locations (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id      INTEGER NOT NULL,
    tool_key     TEXT    NOT NULL,
    host         TEXT    NOT NULL DEFAULT '',
    local_path   TEXT    NOT NULL DEFAULT '',
    tags         TEXT    NOT NULL DEFAULT '[]',
    notes        TEXT    NOT NULL DEFAULT '',
    last_seen_at TEXT    NOT NULL,
    created_at   TEXT    NOT NULL,
    UNIQUE(user_id, tool_key, host, local_path)
);

CREATE INDEX IF NOT EXISTS idx_toolbox_locations_user_key
    ON toolbox_locations(user_id, tool_key);
