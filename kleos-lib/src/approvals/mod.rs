use crate::db::Database;
use crate::Result;
use chrono::{DateTime, Duration, Utc};

pub mod types;
pub use types::*;

/// Default approval window in seconds.
pub const DEFAULT_APPROVAL_WINDOW_SECS: i64 = 120;

fn rusqlite_to_eng_error(err: rusqlite::Error) -> crate::EngError {
    crate::EngError::DatabaseMessage(err.to_string())
}

/// Create a new approval request with a 120s (or custom) window.
#[tracing::instrument(skip(db, req), fields(action = %req.action))]
pub async fn create_approval(
    db: &Database,
    req: &CreateApprovalRequest,
    user_id: i64,
) -> Result<Approval> {
    create_approval_inner(db, req, user_id, None).await
}

/// Patch 21 (2026-05-22): variant of `create_approval` that correlates the
/// new approval with a `gate_requests.id`. Used by the gate Patch 19b
/// long-poll path so the TUI consumer of `/approvals/pending` can decide
/// the gate via `POST /approvals/{id}/decide`. `gate_id` is stored
/// verbatim in the column added by tenant migration v56 / main migration
/// v64; if the column is missing (older deployment that has not yet run
/// the migration), the INSERT falls back to the legacy column set and the
/// caller logs a warning.
#[tracing::instrument(skip(db, req), fields(action = %req.action, gate_id))]
pub async fn create_approval_with_gate(
    db: &Database,
    req: &CreateApprovalRequest,
    user_id: i64,
    gate_id: i64,
) -> Result<Approval> {
    create_approval_inner(db, req, user_id, Some(gate_id)).await
}

async fn create_approval_inner(
    db: &Database,
    req: &CreateApprovalRequest,
    user_id: i64,
    gate_id: Option<i64>,
) -> Result<Approval> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now();
    let window = req.window_secs.unwrap_or(DEFAULT_APPROVAL_WINDOW_SECS);
    let expires_at = now + Duration::seconds(window);

    let created_str = now.to_rfc3339();
    let expires_str = expires_at.to_rfc3339();

    let action = req.action.clone();
    let context = req.context.clone();
    let requester = req.requester.clone();
    let id_clone = id.clone();
    let gate_id_param = gate_id;

    db.write(move |conn| {
        // Patch 21 (renumbered at the aa6a0bec merge): the optional gate_id
        // column was added by migration v72 (tenant) / v84 (main). We always
        // attempt the INSERT including both gate_id (Patch 21 correlation) and
        // the upstream user_id column. On legacy deployments where the gate_id
        // migration has not yet run, rusqlite returns a "no such column" error
        // which we fall back from by issuing the INSERT without gate_id
        // (user_id is migrated earlier so it is retained in the fallback).
        let res = conn.execute(
            "INSERT INTO approvals (id, action, context, requester, status, created_at, expires_at, gate_id, user_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                id_clone,
                action,
                context,
                requester,
                "pending",
                created_str,
                expires_str,
                gate_id_param,
                user_id,
            ],
        );
        if let Err(err) = res {
            // Fallback: schema without gate_id (gate_id migration not yet run).
            // We still create the row so a `POST /approvals` workflow keeps
            // working, but the gate correlation is lost (the caller will time
            // out via the existing path). user_id is retained.
            let msg = err.to_string();
            if msg.contains("no such column") || msg.contains("has no column named") {
                conn.execute(
                    "INSERT INTO approvals (id, action, context, requester, status, created_at, expires_at, user_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![
                        id_clone,
                        action,
                        context,
                        requester,
                        "pending",
                        created_str,
                        expires_str,
                        user_id,
                    ],
                )
                .map_err(rusqlite_to_eng_error)?;
            } else {
                return Err(rusqlite_to_eng_error(err));
            }
        }
        Ok(())
    })
    .await?;

    Ok(Approval {
        id,
        action: req.action.clone(),
        context: req.context.clone(),
        requester: req.requester.clone(),
        status: ApprovalStatus::Pending,
        decision_by: None,
        decision_reason: None,
        created_at: now,
        expires_at,
        decided_at: None,
        user_id,
        gate_id,
    })
}

/// Get a single approval by ID.
#[tracing::instrument(skip(db))]
pub async fn get_approval(db: &Database, id: &str, user_id: i64) -> Result<Option<Approval>> {
    let id = id.to_string();
    db.read(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, action, context, requester, status, decision_by, decision_reason,
                        created_at, expires_at, decided_at
                 FROM approvals WHERE id = ?1 AND user_id = ?2",
        )?;

        let mut rows = stmt.query(rusqlite::params![id, user_id])?;

        match rows.next()? {
            Some(row) => Ok(Some(row_to_approval(row, user_id)?)),
            None => Ok(None),
        }
    })
    .await
}

/// List all pending approvals for a user, ordered by expiry (soonest first).
#[tracing::instrument(skip(db))]
pub async fn list_pending(db: &Database, user_id: i64) -> Result<Vec<Approval>> {
    db.read(move |conn| {
        let mut stmt = conn.prepare(
            "SELECT id, action, context, requester, status, decision_by, decision_reason,
                        created_at, expires_at, decided_at
                 FROM approvals
                 WHERE status = 'pending' AND user_id = ?1
                 ORDER BY expires_at ASC",
        )?;

        let rows = stmt.query_map(rusqlite::params![user_id], |row| {
            // query_map requires a rusqlite::Result return; we map inside
            Ok(row_to_approval(row, user_id))
        })?;

        let mut approvals = Vec::new();
        for item in rows {
            let approval = item??;
            approvals.push(approval);
        }
        Ok(approvals)
    })
    .await
}

/// Decide on an approval (approve or deny).
#[tracing::instrument(skip(db, req), fields(decision = ?req.decision))]
pub async fn decide(
    db: &Database,
    id: &str,
    req: &DecideRequest,
    user_id: i64,
) -> Result<Approval> {
    // First check it exists and is pending
    let approval = get_approval(db, id, user_id)
        .await?
        .ok_or_else(|| crate::EngError::NotFound(format!("approval {} not found", id)))?;

    if approval.status != ApprovalStatus::Pending {
        return Err(crate::EngError::InvalidInput(format!(
            "approval {} is not pending (status: {:?})",
            id, approval.status
        )));
    }

    // Check if expired
    if approval.is_expired() {
        let id_str = id.to_string();
        db.write(move |conn| {
            conn.execute(
                "UPDATE approvals SET status = 'expired' WHERE id = ?1 AND user_id = ?2",
                rusqlite::params![id_str, user_id],
            )?;
            Ok(())
        })
        .await?;
        return Err(crate::EngError::InvalidInput(format!(
            "approval {} has expired",
            id
        )));
    }

    let now = Utc::now();
    let decided_str = now.to_rfc3339();
    let new_status = match req.decision {
        ApprovalDecision::Approved => "approved",
        ApprovalDecision::Denied => "denied",
    };

    let id_str = id.to_string();
    let decided_by = req.decided_by.clone();
    let reason = req.reason.clone();

    db.write(move |conn| {
        conn.execute(
            "UPDATE approvals
             SET status = ?1, decision_by = ?2, decision_reason = ?3, decided_at = ?4
             WHERE id = ?5 AND user_id = ?6",
            rusqlite::params![new_status, decided_by, reason, decided_str, id_str, user_id],
        )?;
        Ok(())
    })
    .await?;

    Ok(Approval {
        status: match req.decision {
            ApprovalDecision::Approved => ApprovalStatus::Approved,
            ApprovalDecision::Denied => ApprovalStatus::Denied,
        },
        decision_by: req.decided_by.clone(),
        decision_reason: req.reason.clone(),
        decided_at: Some(now),
        ..approval
    })
}

/// Patch 21 (2026-05-22): read the optional `gate_id` correlation for an
/// approval. Returns `Ok(None)` for legacy approvals created via
/// `POST /approvals` and `Ok(Some(_))` for approvals created by the gate
/// `require_approval_patterns` pipeline. Returns `Ok(None)` as a safe
/// fallback if the column does not exist (deployment pre-migration v56),
/// so this helper is callable from any code path without crashing.
#[tracing::instrument(skip(db))]
pub async fn read_gate_id_for_approval(db: &Database, id: &str) -> Result<Option<i64>> {
    let id = id.to_string();
    db.read(move |conn| {
        let res = conn.query_row(
            "SELECT gate_id FROM approvals WHERE id = ?1",
            rusqlite::params![id],
            |row| row.get::<_, Option<i64>>(0),
        );
        match res {
            Ok(v) => Ok(v),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => {
                let msg = err.to_string();
                if msg.contains("no such column") {
                    // Pre-migration deployment: treat as a legacy approval.
                    Ok(None)
                } else {
                    Err(rusqlite_to_eng_error(err))
                }
            }
        }
    })
    .await
}

/// Expire all stale pending approvals. Returns the number of rows updated.
#[tracing::instrument(skip(db))]
pub async fn expire_stale(db: &Database) -> Result<u64> {
    let now = Utc::now().to_rfc3339();
    db.write(move |conn| {
        let rows = conn.execute(
            "UPDATE approvals SET status = 'expired'
                 WHERE status = 'pending' AND expires_at < ?1",
            rusqlite::params![now],
        )?;
        Ok(rows as u64)
    })
    .await
}

fn row_to_approval(row: &rusqlite::Row<'_>, owner_user_id: i64) -> Result<Approval> {
    let id: String = row.get(0)?;
    let action: String = row.get(1)?;
    let context: Option<String> = row.get(2)?;
    let requester: String = row.get(3)?;
    let status_str: String = row.get(4)?;
    let decision_by: Option<String> = row.get(5)?;
    let decision_reason: Option<String> = row.get(6)?;
    let created_at_str: String = row.get(7)?;
    let expires_at_str: String = row.get(8)?;
    let decided_at_str: Option<String> = row.get(9)?;

    let status = ApprovalStatus::parse(&status_str)
        .ok_or_else(|| crate::EngError::Internal(format!("invalid status: {}", status_str)))?;

    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map_err(|e| crate::EngError::Internal(format!("invalid created_at: {}", e)))?
        .with_timezone(&Utc);

    let expires_at = DateTime::parse_from_rfc3339(&expires_at_str)
        .map_err(|e| crate::EngError::Internal(format!("invalid expires_at: {}", e)))?
        .with_timezone(&Utc);

    let decided_at = decided_at_str
        .map(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| crate::EngError::Internal(format!("invalid decided_at: {}", e)))
        })
        .transpose()?;

    Ok(Approval {
        id,
        action,
        context,
        requester,
        status,
        decision_by,
        decision_reason,
        created_at,
        expires_at,
        decided_at,
        user_id: owner_user_id,
        // Patch 21: `gate_id` is not read by the legacy SELECT projections
        // used by `list_pending` / `get_approval`. The decide_handler reads
        // it separately via `read_gate_id_for_approval` to relay the
        // decision to the gate caller. Keeping the projection unchanged
        // here preserves wire-format compatibility with upstream and the
        // TUI consumer.
        gate_id: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_get_approval() {
        let db = Database::connect_memory().await.expect("in-memory db");

        let req = CreateApprovalRequest {
            action: "DELETE /memories/123".to_string(),
            context: Some(r#"{"memory_id": 123}"#.to_string()),
            requester: "test-agent".to_string(),
            window_secs: None,
        };

        let approval = create_approval(&db, &req, 1).await.expect("create");
        assert_eq!(approval.status, ApprovalStatus::Pending);
        assert_eq!(approval.action, "DELETE /memories/123");
        assert!(approval.seconds_remaining() > 0);
        assert!(approval.seconds_remaining() <= 120);

        let fetched = get_approval(&db, &approval.id, 1)
            .await
            .expect("get")
            .expect("should exist");
        assert_eq!(fetched.id, approval.id);
        assert_eq!(fetched.status, ApprovalStatus::Pending);
    }

    #[tokio::test]
    async fn test_decide_approval() {
        let db = Database::connect_memory().await.expect("in-memory db");

        let req = CreateApprovalRequest {
            action: "DELETE /memories/456".to_string(),
            context: None,
            requester: "test-agent".to_string(),
            window_secs: Some(300),
        };

        let approval = create_approval(&db, &req, 1).await.expect("create");

        let decide_req = DecideRequest {
            decision: ApprovalDecision::Approved,
            decided_by: Some("admin".to_string()),
            reason: Some("Looks good".to_string()),
        };

        let decided = decide(&db, &approval.id, &decide_req, 1)
            .await
            .expect("decide");
        assert_eq!(decided.status, ApprovalStatus::Approved);
        assert_eq!(decided.decision_by, Some("admin".to_string()));
        assert!(decided.decided_at.is_some());
    }

    #[tokio::test]
    async fn test_list_pending() {
        let db = Database::connect_memory().await.expect("in-memory db");

        // Create two approvals
        let req1 = CreateApprovalRequest {
            action: "action1".to_string(),
            context: None,
            requester: "agent1".to_string(),
            window_secs: Some(60),
        };
        let req2 = CreateApprovalRequest {
            action: "action2".to_string(),
            context: None,
            requester: "agent2".to_string(),
            window_secs: Some(120),
        };

        create_approval(&db, &req1, 1).await.expect("create1");
        create_approval(&db, &req2, 1).await.expect("create2");

        let pending = list_pending(&db, 1).await.expect("list");
        assert_eq!(pending.len(), 2);
        // Should be ordered by expires_at (soonest first)
        assert!(pending[0].expires_at <= pending[1].expires_at);
    }

    #[tokio::test]
    async fn test_cannot_decide_twice() {
        let db = Database::connect_memory().await.expect("in-memory db");

        let req = CreateApprovalRequest {
            action: "test action".to_string(),
            context: None,
            requester: "agent".to_string(),
            window_secs: None,
        };

        let approval = create_approval(&db, &req, 1).await.expect("create");

        let decide_req = DecideRequest {
            decision: ApprovalDecision::Denied,
            decided_by: None,
            reason: None,
        };

        decide(&db, &approval.id, &decide_req, 1)
            .await
            .expect("first decide");

        // Second decide should fail
        let result = decide(&db, &approval.id, &decide_req, 1).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_expire_stale() {
        let db = Database::connect_memory().await.expect("in-memory db");

        // Create an approval with 1 second window
        let req = CreateApprovalRequest {
            action: "quick action".to_string(),
            context: None,
            requester: "agent".to_string(),
            window_secs: Some(1),
        };

        let approval = create_approval(&db, &req, 1).await.expect("create");

        // Wait for expiry
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;

        // Expire stale
        let expired_count = expire_stale(&db).await.expect("expire");
        assert_eq!(expired_count, 1);

        // Verify status changed
        let fetched = get_approval(&db, &approval.id, 1)
            .await
            .expect("get")
            .expect("should exist");
        assert_eq!(fetched.status, ApprovalStatus::Expired);
    }

    /// Single-DB isolation: with user_id restored (monolith migration 66 /
    /// tenant v57), one shared in-memory DB again separates user 1 and user 2.
    /// The cross-shard invariant is also covered by
    /// kleos-lib/tests/tenant_isolation.rs::approvals_isolated_across_tenants.
    #[tokio::test]
    async fn test_tenant_isolation() {
        let db = Database::connect_memory().await.expect("in-memory db");

        let req = CreateApprovalRequest {
            action: "user1 action".to_string(),
            context: None,
            requester: "agent".to_string(),
            window_secs: None,
        };

        let approval = create_approval(&db, &req, 1).await.expect("create");

        // User 2 should not see user 1's approval
        let fetched = get_approval(&db, &approval.id, 2).await.expect("get");
        assert!(fetched.is_none());

        // User 2's pending list should be empty
        let pending = list_pending(&db, 2).await.expect("list");
        assert!(pending.is_empty());
    }
}
