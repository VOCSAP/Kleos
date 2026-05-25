//! PIV 9A public key enrollment and revocation (DB-backed).

use kleos_cred::{CredError, Result};
use kleos_lib::db::Database;
use rusqlite::params;

/// An enrolled PIV 9A public key for ECDH authentication.
#[derive(Debug, Clone)]
pub struct PivPubkey {
    /// Row ID.
    pub id: i64,
    /// Owner user ID.
    pub user_id: i64,
    /// Agent this key belongs to.
    pub agent_name: String,
    /// PEM-encoded P256 public key.
    pub public_key_pem: String,
    /// When the key was enrolled.
    pub created_at: String,
    /// When the key was revoked (None if active).
    pub revoked_at: Option<String>,
}

/// Serializable pubkey for JSON responses.
impl PivPubkey {
    /// Convert to a serde_json::Value for API responses.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "agent_name": self.agent_name,
            "public_key_pem": self.public_key_pem,
            "created_at": self.created_at,
            "revoked_at": self.revoked_at,
        })
    }
}

/// Enroll a new PIV 9A public key for an agent.
pub async fn enroll_pubkey(
    db: &Database,
    user_id: i64,
    agent_name: &str,
    public_key_pem: &str,
) -> Result<PivPubkey> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let agent = agent_name.to_string();
    let pem = public_key_pem.to_string();
    let now2 = now.clone();

    let id = db
        .write(move |conn| {
            conn.execute(
                "INSERT INTO phylax_piv_pubkeys
                 (user_id, agent_name, public_key_pem, created_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![user_id, agent, pem, now2],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .await
        .map_err(|e| CredError::Database(e.to_string()))?;

    Ok(PivPubkey {
        id,
        user_id,
        agent_name: agent_name.to_string(),
        public_key_pem: public_key_pem.to_string(),
        created_at: now,
        revoked_at: None,
    })
}

/// List active (non-revoked) pubkeys for an agent.
pub async fn list_active_pubkeys(db: &Database, agent_name: &str) -> Result<Vec<PivPubkey>> {
    let agent = agent_name.to_string();
    db.read(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, user_id, agent_name, public_key_pem, created_at, revoked_at
             FROM phylax_piv_pubkeys
             WHERE agent_name = ?1 AND revoked_at IS NULL",
        )?;
        let rows = stmt.query_map(params![agent], row_to_pubkey)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    })
    .await
    .map_err(|e| CredError::Database(e.to_string()))
}

/// Revoke a pubkey by ID. Only affects non-revoked keys.
pub async fn revoke_pubkey(db: &Database, id: i64) -> Result<()> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let affected = db
        .write(move |conn| {
            Ok(conn.execute(
                "UPDATE phylax_piv_pubkeys SET revoked_at = ?1
                 WHERE id = ?2 AND revoked_at IS NULL",
                params![now, id],
            )?)
        })
        .await
        .map_err(|e| CredError::Database(e.to_string()))?;

    if affected == 0 {
        return Err(CredError::NotFound(
            "pubkey not found or already revoked".into(),
        ));
    }
    Ok(())
}

/// Parse a database row into a PivPubkey struct.
fn row_to_pubkey(row: &rusqlite::Row<'_>) -> rusqlite::Result<PivPubkey> {
    Ok(PivPubkey {
        id: row.get(0)?,
        user_id: row.get(1)?,
        agent_name: row.get(2)?,
        public_key_pem: row.get(3)?,
        created_at: row.get(4)?,
        revoked_at: row.get(5)?,
    })
}
