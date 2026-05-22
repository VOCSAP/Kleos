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
use kleos_lib::conversations::{self, CreateConversationRequest};
use kleos_lib::db::Database;
use kleos_lib::intelligence::{causal, consolidation, reflections};
use kleos_lib::memory::types::{ListOptions, StoreRequest};
use kleos_lib::memory::{self};
use kleos_lib::services::axon::{self, PublishEventRequest};
use kleos_lib::services::chiasm::{self, CreateTaskRequest};
use kleos_lib::services::soma::{self, RegisterAgentRequest};
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

/// Build a minimal agent registration owned by `user_id`.
fn agent_req(name: &str, user_id: i64) -> RegisterAgentRequest {
    RegisterAgentRequest {
        user_id: Some(user_id),
        name: name.to_string(),
        type_: "cli".to_string(),
        description: None,
        capabilities: None,
        config: None,
    }
}

/// soma_agents must isolate per user on one monolith: two users can both own an
/// agent with the same name (UNIQUE(name, user_id)), neither sees nor can delete
/// the other's, and the register upsert never clobbers across users.
#[tokio::test]
async fn soma_agents_isolated_between_users_single_db() {
    let db = monolith().await;

    let a10 = soma::register_agent(&db, agent_req("claude-code", 10))
        .await
        .expect("user 10 registers claude-code");
    // The same agent name under a different user must succeed, not collide.
    let a20 = soma::register_agent(&db, agent_req("claude-code", 20))
        .await
        .expect("user 20 registers claude-code (distinct owner)");
    assert_ne!(
        a10.id, a20.id,
        "same-named agents owned by different users must be distinct rows"
    );

    // User 20 cannot read user 10's agent by id.
    assert!(
        soma::get_agent(&db, a10.id, 20).await.is_err(),
        "user 20 must not read user 10's agent by id"
    );

    // Each user's listing shows only their own agent.
    let list_10 = soma::list_agents(&db, 10, None, None, 100)
        .await
        .expect("list user 10");
    assert_eq!(list_10.len(), 1, "user 10 sees exactly their own agent");
    assert_eq!(list_10[0].id, a10.id);

    // User 20 deleting user 10's agent is a no-op.
    soma::delete_agent(&db, a10.id, 20)
        .await
        .expect("cross-user delete call ok");
    assert!(
        soma::get_agent(&db, a10.id, 10).await.is_ok(),
        "user 20's delete must not remove user 10's agent"
    );
}

/// An axon event published by one user must be invisible to another user on the
/// same monolith: not fetchable by id, absent from the other user's query.
#[tokio::test]
async fn axon_events_isolated_between_users_single_db() {
    let db = monolith().await;

    let ev = axon::publish_event(
        &db,
        PublishEventRequest {
            channel: "shared-channel".to_string(),
            action: "secret.event".to_string(),
            payload: None,
            source: Some("alice-agent".to_string()),
            agent: None,
            user_id: Some(10),
        },
    )
    .await
    .expect("user 10 publishes event");

    // User 20 cannot fetch user 10's event by id.
    assert!(
        axon::get_event(&db, ev.id, 20).await.is_err(),
        "user 20 must not read user 10's event by id"
    );

    // User 20's query on the same channel name must not see it.
    let q20 = axon::query_events(&db, Some("shared-channel"), None, None, 100, 0, 20)
        .await
        .expect("query user 20");
    assert!(
        q20.iter().all(|e| e.id != ev.id),
        "user 20's query must exclude user 10's event"
    );

    // User 10 sees their own event.
    let q10 = axon::query_events(&db, Some("shared-channel"), None, None, 100, 0, 10)
        .await
        .expect("query user 10");
    assert!(
        q10.iter().any(|e| e.id == ev.id),
        "user 10 must see their own event"
    );
}

/// Build a minimal chiasm task request owned by `user_id`.
fn task_req(title: &str, user_id: i64) -> CreateTaskRequest {
    CreateTaskRequest {
        agent: "agent".to_string(),
        project: "proj".to_string(),
        title: title.to_string(),
        status: None,
        summary: None,
        user_id: Some(user_id),
        expected_output: None,
        output_format: None,
        condition: None,
        guardrail_url: None,
        heartbeat_interval: None,
    }
}

/// A chiasm task created by one user must be invisible to and unmodifiable by
/// another user on the same monolith.
#[tokio::test]
async fn chiasm_tasks_isolated_between_users_single_db() {
    let db = monolith().await;

    let t10 = chiasm::create_task(&db, task_req("alice task", 10))
        .await
        .expect("user 10 creates task");
    chiasm::create_task(&db, task_req("bob task", 20))
        .await
        .expect("user 20 creates task");

    // User 20 cannot fetch user 10's task by id.
    assert!(
        chiasm::get_task(&db, t10.id, 20).await.is_err(),
        "user 20 must not read user 10's task by id"
    );

    // Each user's listing shows only their own task.
    let list_10 = chiasm::list_tasks(&db, 10, None, None, None, 100, 0)
        .await
        .expect("list user 10");
    assert!(
        list_10.iter().all(|t| t.title == "alice task"),
        "user 10's list must contain only their task"
    );
    let list_20 = chiasm::list_tasks(&db, 20, None, None, None, 100, 0)
        .await
        .expect("list user 20");
    assert!(
        list_20.iter().all(|t| t.title == "bob task"),
        "user 20's list must contain only their task"
    );

    // User 20 deleting user 10's task is a no-op.
    chiasm::delete_task(&db, t10.id, 20)
        .await
        .expect("cross-user delete ok");
    assert!(
        chiasm::get_task(&db, t10.id, 10).await.is_ok(),
        "user 20's delete must not remove user 10's task"
    );
}

/// A conversation created by one user must be invisible to and undeletable by
/// another user on the same monolith.
#[tokio::test]
async fn conversations_isolated_between_users_single_db() {
    let db = monolith().await;

    let conv = conversations::create_conversation(
        &db,
        CreateConversationRequest {
            agent: "alice-agent".to_string(),
            session_id: Some("s-alice".to_string()),
            title: Some("alice convo".to_string()),
            metadata: None,
        },
        10,
    )
    .await
    .expect("user 10 creates conversation");

    // User 20 cannot fetch user 10's conversation by id.
    assert!(
        conversations::get_conversation_for_user(&db, conv.id, 20)
            .await
            .is_err(),
        "user 20 must not read user 10's conversation"
    );

    // User 20's list must be empty; user 10's must contain it.
    assert!(
        conversations::list_conversations(&db, 20, 100)
            .await
            .expect("list user 20")
            .is_empty(),
        "user 20 must not see user 10's conversation"
    );
    assert_eq!(
        conversations::list_conversations(&db, 10, 100)
            .await
            .expect("list user 10")
            .len(),
        1,
        "user 10 must see their own conversation"
    );

    // User 20 deleting user 10's conversation must fail / not remove it.
    let _ = conversations::delete_conversation(&db, conv.id, 20).await;
    assert!(
        conversations::get_conversation_for_user(&db, conv.id, 10)
            .await
            .is_ok(),
        "user 20's delete must not remove user 10's conversation"
    );
}

/// A reflection created by one user must be invisible to another user listing
/// reflections on the same monolith.
#[tokio::test]
async fn reflections_isolated_between_users_single_db() {
    let db = monolith().await;

    reflections::create_reflection(&db, "alice insight", "insight", &[], 0.9, 10)
        .await
        .expect("user 10 creates reflection");

    // User 20's list must be empty; user 10's must contain it.
    assert!(
        reflections::list_reflections(&db, 20, 100)
            .await
            .expect("list user 20")
            .is_empty(),
        "user 20 must not see user 10's reflection"
    );
    let list_10 = reflections::list_reflections(&db, 10, 100)
        .await
        .expect("list user 10");
    assert_eq!(list_10.len(), 1, "user 10 must see their own reflection");
    assert_eq!(list_10[0].content, "alice insight");
}

/// A consolidation record created by one user must be invisible to another
/// user, and a user must not be able to consolidate another user's memories.
#[tokio::test]
async fn consolidations_isolated_between_users_single_db() {
    let db = monolith().await;

    // User 10 owns two memories and consolidates them.
    let m1 = memory::store(&db, store_req("alice fact one", 10))
        .await
        .expect("store m1")
        .id;
    let m2 = memory::store(&db, store_req("alice fact two", 10))
        .await
        .expect("store m2")
        .id;

    // User 20 must not consolidate user 10's memories.
    assert!(
        consolidation::consolidate(&db, &[m1.to_string(), m2.to_string()], 20)
            .await
            .is_err(),
        "user 20 must not consolidate user 10's memories"
    );

    // User 10 consolidates their own memories.
    consolidation::consolidate(&db, &[m1.to_string(), m2.to_string()], 10)
        .await
        .expect("user 10 consolidates own memories");

    // User 20's consolidation list must be empty; user 10's must contain one.
    assert!(
        consolidation::list_consolidations(&db, 20, 100)
            .await
            .expect("list user 20")
            .is_empty(),
        "user 20 must not see user 10's consolidation"
    );
    assert_eq!(
        consolidation::list_consolidations(&db, 10, 100)
            .await
            .expect("list user 10")
            .len(),
        1,
        "user 10 must see their own consolidation"
    );
}

/// A causal chain created by one user must be invisible to another user: not
/// fetchable by id, absent from the other user's list, and its links must not
/// surface through the other user's backward traversal.
#[tokio::test]
async fn causal_chains_isolated_between_users_single_db() {
    let db = monolith().await;

    let cause = memory::store(&db, store_req("alice cause", 10))
        .await
        .expect("store cause")
        .id;
    let effect = memory::store(&db, store_req("alice effect", 10))
        .await
        .expect("store effect")
        .id;
    let chain = causal::create_chain(&db, Some(cause), Some("alice chain"), 10)
        .await
        .expect("user 10 creates chain");
    causal::add_link(&db, chain.id, cause, effect, 1.0, 0, 10)
        .await
        .expect("user 10 adds link");

    // User 20 cannot fetch user 10's chain by id.
    assert!(
        causal::get_chain(&db, chain.id, 20).await.is_err(),
        "user 20 must not read user 10's causal chain"
    );

    // User 20's chain list must be empty; user 10's must contain it.
    assert!(
        causal::list_chains(&db, 20, 100)
            .await
            .expect("list user 20")
            .is_empty(),
        "user 20 must not see user 10's chain"
    );
    assert_eq!(
        causal::list_chains(&db, 10, 100)
            .await
            .expect("list user 10")
            .len(),
        1,
        "user 10 must see their own chain"
    );

    // User 20's backward traversal must not surface user 10's causal links.
    assert!(
        causal::backward_chain(&db, effect, 20, 5)
            .await
            .expect("backward user 20")
            .is_empty(),
        "user 20 must not traverse user 10's causal links"
    );
    assert_eq!(
        causal::backward_chain(&db, effect, 10, 5)
            .await
            .expect("backward user 10")
            .len(),
        1,
        "user 10 must traverse their own causal links"
    );
}
