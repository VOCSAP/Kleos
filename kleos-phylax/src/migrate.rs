//! Phylax table migration -- ensures phylax_* tables exist.
//!
//! This is a standalone idempotent migration for use by phylaxd at startup.
//! The same tables are also created by kleos-lib migration 82, but this
//! function provides a fallback for databases that don't use the full
//! kleos-lib migration chain.

use kleos_cred::CredError;

/// Ensure all Phylax tables exist. Idempotent (IF NOT EXISTS on all statements).
pub fn ensure_tables(conn: &rusqlite::Connection) -> Result<(), CredError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS phylax_approvals (
            id INTEGER PRIMARY KEY,
            user_id INTEGER NOT NULL,
            agent_name TEXT NOT NULL,
            category TEXT NOT NULL,
            secret_name TEXT NOT NULL,
            resolve_mode TEXT NOT NULL,
            status INTEGER NOT NULL DEFAULT 0,
            decided_by TEXT,
            reason TEXT,
            lease_id INTEGER,
            correlation_id TEXT,
            created_at TEXT NOT NULL,
            decided_at TEXT,
            expires_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_phylax_approvals_pending
            ON phylax_approvals(status) WHERE status = 0;
        CREATE INDEX IF NOT EXISTS idx_phylax_approvals_agent
            ON phylax_approvals(agent_name, status);

        CREATE TABLE IF NOT EXISTS phylax_leases (
            id INTEGER PRIMARY KEY,
            user_id INTEGER NOT NULL,
            approval_id INTEGER NOT NULL,
            agent_name TEXT NOT NULL,
            category TEXT NOT NULL,
            secret_name TEXT NOT NULL,
            jti TEXT NOT NULL UNIQUE,
            correlation_id TEXT,
            created_at TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            used_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_phylax_leases_active
            ON phylax_leases(agent_name) WHERE used_at IS NULL;

        CREATE TABLE IF NOT EXISTS phylax_access_policies (
            id INTEGER PRIMARY KEY,
            user_id INTEGER NOT NULL,
            namespace TEXT NOT NULL,
            category TEXT,
            secret_name TEXT,
            require_approval INTEGER NOT NULL DEFAULT 1,
            allowed_modes TEXT NOT NULL DEFAULT '[\"text\",\"proxy\",\"raw\"]',
            created_at TEXT NOT NULL,
            UNIQUE(user_id, namespace, category, secret_name)
        );

        CREATE TABLE IF NOT EXISTS phylax_piv_pubkeys (
            id INTEGER PRIMARY KEY,
            user_id INTEGER NOT NULL,
            agent_name TEXT NOT NULL,
            public_key_pem TEXT NOT NULL,
            created_at TEXT NOT NULL,
            revoked_at TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_phylax_piv_active
            ON phylax_piv_pubkeys(agent_name) WHERE revoked_at IS NULL;

        CREATE TABLE IF NOT EXISTS phylax_ssh_settings (
            id INTEGER PRIMARY KEY,
            user_id INTEGER NOT NULL,
            category TEXT NOT NULL,
            secret_name TEXT NOT NULL,
            auto_sign INTEGER NOT NULL DEFAULT 0,
            auto_load INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(user_id, category, secret_name)
        );",
    )
    .map_err(|e| CredError::Database(e.to_string()))?;
    Ok(())
}
