//! Single-DB (shared) mode isolation.
//!
//! In single-DB mode (`ENGRAM_TENANT_SHARDING=0`) one monolith SQLite file
//! serves every user. Isolation does not come from separate shard files (as it
//! does in `tenant_isolation.rs`); it comes entirely from the always-applied
//! `WHERE user_id = ?` predicate restored across the data layer. These tests
//! seed two users on ONE `Database` and assert that user B can never observe
//! user A's data. They are the only harness that can catch a single-DB leak --
//! the cross-shard suite uses distinct files and would pass even if the
//! predicate were missing.

use kleos_lib::approvals::{self, CreateApprovalRequest};
use kleos_lib::db::Database;
use kleos_lib::memory::types::{ListOptions, StoreRequest};
use kleos_lib::memory::{self};
use kleos_lib::webhooks;

/// Build a monolith (non-tenant) in-memory database with the full monolith
/// migration chain applied (includes v64, so every data table carries
/// `user_id`). This is the single-DB deployment shape.
async fn monolith() -> Database {
    Database::connect_memory().await.expect("monolith db")
}

/// Construct a minimal `StoreRequest` owned by `user_id`. Only the fields the
/// isolation tests care about are meaningful; the rest take inert defaults.
fn store_req(content: &str, user_id: i64) -> StoreRequest {
    StoreRequest {
        content: content.to_string(),
        category: "general".to_string(),
        source: "test".to_string(),
        importance: 5,
        tags: None,
        embedding: None,
        chunk_embeddings: None,
        session_id: None,
        is_static: None,
        user_id: Some(user_id),
        space_id: None,
        parent_memory_id: None,
        sync_id: None,
    }
}

/// A memory stored by one user must be invisible to another user sharing the
/// same monolith database: not readable by id, absent from the other user's
/// list, present in the owner's list.
#[tokio::test]
async fn memories_isolated_between_users_single_db() {
    let db = monolith().await;

    let alice = memory::store(&db, store_req("alice secret", 10))
        .await
        .expect("store alice")
        .id;
    memory::store(&db, store_req("bob secret", 20))
        .await
        .expect("store bob");

    // Bob (user 20) must not read Alice's memory by id.
    assert!(
        memory::get(&db, alice, 20).await.is_err(),
        "bob must not read alice's memory by id"
    );

    // Bob's list must exclude Alice's content.
    let list_bob = memory::list(
        &db,
        ListOptions {
            user_id: Some(20),
            ..Default::default()
        },
    )
    .await
    .expect("list bob");
    assert!(
        list_bob.iter().all(|m| m.content != "alice secret"),
        "bob's list must exclude alice's memory"
    );

    // Alice still sees her own memory.
    let list_alice = memory::list(
        &db,
        ListOptions {
            user_id: Some(10),
            ..Default::default()
        },
    )
    .await
    .expect("list alice");
    assert!(
        list_alice.iter().any(|m| m.content == "alice secret"),
        "alice must see her own memory"
    );
}

/// A webhook created by one user must be invisible to and undeletable by
/// another user on the same monolith database.
#[tokio::test]
async fn webhooks_isolated_between_users_single_db() {
    let db = monolith().await;

    let (hook_id, _) = webhooks::create_webhook(
        &db,
        "https://example.com/hook",
        &["*".to_string()],
        None,
        10,
    )
    .await
    .expect("user 10 creates webhook");

    // User 20 must not list user 10's webhook.
    let list_bob = webhooks::list_webhooks(&db, 20).await.expect("list bob");
    assert!(
        list_bob.is_empty(),
        "user 20 must not see user 10's webhook"
    );

    // User 10 sees their own.
    let list_alice = webhooks::list_webhooks(&db, 10).await.expect("list alice");
    assert_eq!(list_alice.len(), 1, "user 10 must see their own webhook");

    // User 20 attempting to delete user 10's webhook must not remove it.
    webhooks::delete_webhook(&db, hook_id, 20)
        .await
        .expect("delete call succeeds (no-op)");
    assert_eq!(
        webhooks::list_webhooks(&db, 10)
            .await
            .expect("relist alice")
            .len(),
        1,
        "user 20's delete must not remove user 10's webhook"
    );

    // The owner can delete it.
    webhooks::delete_webhook(&db, hook_id, 10)
        .await
        .expect("owner deletes own webhook");
    assert!(
        webhooks::list_webhooks(&db, 10)
            .await
            .expect("relist alice after delete")
            .is_empty(),
        "owner delete removes the webhook"
    );
}

/// An approval created by one user must be invisible to another user on the
/// same monolith database, by id and in the pending list.
#[tokio::test]
async fn approvals_isolated_between_users_single_db() {
    let db = monolith().await;

    let req = CreateApprovalRequest {
        action: "alice action".to_string(),
        context: None,
        requester: "agent".to_string(),
        window_secs: None,
    };
    let approval = approvals::create_approval(&db, &req, 10)
        .await
        .expect("user 10 creates approval");

    // User 20 must not fetch user 10's approval by id.
    assert!(
        approvals::get_approval(&db, &approval.id, 20)
            .await
            .expect("get bob")
            .is_none(),
        "user 20 must not read user 10's approval"
    );

    // User 20's pending list must be empty.
    assert!(
        approvals::list_pending(&db, 20)
            .await
            .expect("list bob")
            .is_empty(),
        "user 20's pending list must exclude user 10's approval"
    );

    // User 10 sees their own pending approval.
    assert_eq!(
        approvals::list_pending(&db, 10)
            .await
            .expect("list alice")
            .len(),
        1,
        "user 10 must see their own pending approval"
    );
}
