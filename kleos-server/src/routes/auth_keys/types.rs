use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(super) struct CreateKeyBody {
    pub name: Option<String>,
    pub scopes: Option<String>,
    pub user_id: Option<i64>,
    pub rate_limit: Option<i64>,
    pub expires_at: Option<String>,
    /// Alternative to absolute `expires_at`: relative TTL in seconds.
    pub ttl_secs: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RotateKeyBody {
    pub key_id: i64,
    /// Optional per-request override for the grace period applied to the
    /// old key. When absent, falls back to `config.auth_key_rotation_grace_hours`.
    pub grace_hours: Option<i64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub(super) struct CreateUserBody {
    pub username: String,
    pub email: Option<String>,
    pub role: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct CreateSpaceBody {
    pub name: String,
    pub description: Option<String>,
}

/// Body for creating or updating an instance-level access grant. The grant
/// lets `grantee_user_id` reach `owner_user_id`'s entire shard at `access`.
#[derive(Debug, Deserialize)]
pub(super) struct CreateGrantBody {
    /// The shard owner whose data the grant exposes.
    pub owner_user_id: i64,
    /// The user who receives delegated access.
    pub grantee_user_id: i64,
    /// Access level: `read` or `write`.
    pub access: String,
}

/// Query parameters for listing instance grants. When `owner` is omitted the
/// caller's own shard is used.
#[derive(Debug, Deserialize)]
pub(super) struct ListGrantsQuery {
    /// The shard owner whose issued grants to enumerate.
    pub owner: Option<i64>,
}
