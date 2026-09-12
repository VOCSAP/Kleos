-- VOCSAP Patch 53 -- toolbox: shared tool catalog (user-agnostic).
--
-- One row per canonical tool key (see kleos_lib::toolbox::key). The sheet
-- (name/summary/body/keywords) and its embedding are shared across users; only
-- toolbox_locations (the other overlay) is user-scoped. Nothing here carries a
-- space_id: the toolbox is deliberately outside the spaces partitioning.
--
-- Every statement is IF NOT EXISTS so the overlay body stays idempotent even if
-- it is entered with the table half-created.

CREATE TABLE IF NOT EXISTS toolbox_tools (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    tool_key            TEXT    NOT NULL UNIQUE,
    key_kind            TEXT    NOT NULL,
    kind                TEXT    NOT NULL,
    name                TEXT    NOT NULL,
    summary             TEXT    NOT NULL,
    body                TEXT    NOT NULL DEFAULT '',
    keywords            TEXT    NOT NULL DEFAULT '',
    canonical_url       TEXT,
    indexed_commit      TEXT,
    indexed_commit_time INTEGER,
    content_hash        TEXT    NOT NULL,
    embedding           BLOB,
    embedding_model     TEXT,
    indexed_by_user_id  INTEGER,
    created_at          TEXT    NOT NULL,
    updated_at          TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_toolbox_tools_kind ON toolbox_tools(kind);

CREATE VIRTUAL TABLE IF NOT EXISTS toolbox_tools_fts USING fts5(
    name,
    summary,
    body,
    keywords,
    content='toolbox_tools',
    content_rowid='id'
);

CREATE TRIGGER IF NOT EXISTS toolbox_tools_fts_insert AFTER INSERT ON toolbox_tools BEGIN
    INSERT INTO toolbox_tools_fts(rowid, name, summary, body, keywords)
    VALUES (new.id, new.name, new.summary, new.body, new.keywords);
END;

CREATE TRIGGER IF NOT EXISTS toolbox_tools_fts_delete AFTER DELETE ON toolbox_tools BEGIN
    INSERT INTO toolbox_tools_fts(toolbox_tools_fts, rowid, name, summary, body, keywords)
    VALUES ('delete', old.id, old.name, old.summary, old.body, old.keywords);
END;

CREATE TRIGGER IF NOT EXISTS toolbox_tools_fts_update AFTER UPDATE ON toolbox_tools BEGIN
    INSERT INTO toolbox_tools_fts(toolbox_tools_fts, rowid, name, summary, body, keywords)
    VALUES ('delete', old.id, old.name, old.summary, old.body, old.keywords);
    INSERT INTO toolbox_tools_fts(rowid, name, summary, body, keywords)
    VALUES (new.id, new.name, new.summary, new.body, new.keywords);
END;
