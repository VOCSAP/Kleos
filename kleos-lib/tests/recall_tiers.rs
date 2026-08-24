//! Regression tests for the recall "static" and "important" tiers (Phase 1.1 / 1.2).
//!
//! The recall endpoint must always surface pinned/static memories and high-importance
//! memories regardless of how recently they were written. The previous implementation
//! listed the newest N rows and filtered afterwards, so any pinned or important memory
//! older than that window silently disappeared. These tests seed old pinned/important
//! rows behind a wall of newer, low-importance rows and assert they still surface via
//! `memory::list_static` / `memory::list_important`.

use kleos_lib::db::Database;
use kleos_lib::memory;
use kleos_lib::memory::types::StoreRequest;

/// Owner id for the tests.
const UID: i64 = 1;

/// Build a store request with explicit static flag, importance, and space.
fn req(content: &str, importance: i32, is_static: bool, space_id: Option<i64>) -> StoreRequest {
    StoreRequest {
        content: content.to_string(),
        category: "general".to_string(),
        source: "test".to_string(),
        importance,
        tags: None,
        embedding: None,
        chunk_embeddings: None,
        session_id: None,
        is_static: Some(is_static),
        user_id: Some(UID),
        space_id,
        // Patch 33: wire-only `space` name (resolved server-side); tests pass space_id directly.
        space: None,
        parent_memory_id: None,
        sync_id: None,
        artifacts: None,
        created_at: None,
    }
}

/// Store one memory and return its row id.
async fn store(
    db: &Database,
    content: &str,
    importance: i32,
    is_static: bool,
    space: Option<i64>,
) -> i64 {
    memory::store(db, req(content, importance, is_static, space), None, false)
        .await
        .expect("store")
        .id
}

/// Store one memory for an explicit `user_id` (unlike `store`/`req`, which hardcode `UID`).
async fn store_for_user(
    db: &Database,
    owner: i64,
    content: &str,
    importance: i32,
    is_static: bool,
    space: Option<i64>,
) -> i64 {
    let mut request = req(content, importance, is_static, space);
    request.user_id = Some(owner);
    memory::store(db, request, None, false)
        .await
        .expect("store")
        .id
}

/// Insert a minimal `users` row so `spaces.user_id REFERENCES users(id)` does not fail.
/// `connect_memory()` seeds no users; `memory::store`/`list_static` never need this row
/// (memories.user_id carries no FK), but `space::resolve_or_create_space` /
/// `space::default_space_id` insert into `spaces` and do.
async fn seed_user(db: &Database, uid: i64) {
    db.write(move |conn| {
        conn.execute(
            "INSERT OR IGNORE INTO users (id, username) VALUES (?1, ?2)",
            rusqlite::params![uid, format!("patch49-tiers-user-{uid}")],
        )?;
        Ok(())
    })
    .await
    .expect("seed synthetic user");
}

/// Seed an old pinned and an old high-importance memory, then bury them under many newer
/// low-importance rows. Both must still surface through the dedicated tier queries.
#[tokio::test]
async fn static_and_important_survive_the_recency_window() {
    let db = Database::connect_memory().await.expect("db");

    // Oldest rows: one pinned, one high-importance. They will fall outside any
    // newest-10 or newest-20 window once the filler below is stored.
    let old_static = store(
        &db,
        "permanent pinned identity fact for the owner",
        8,
        true,
        None,
    )
    .await;
    let old_important = store(
        &db,
        "critical high importance architecture decision",
        10,
        false,
        None,
    )
    .await;

    // 25 newer, distinct, low-importance rows so the two old rows are well outside the
    // old recency windows (10 and 20).
    for i in 0..25 {
        let content =
            format!("routine log entry {i} concerning widget {i} and gadget {i} status report");
        store(&db, &content, 4, false, None).await;
    }

    // Static tier: the pinned fact must be present despite being the oldest row.
    let statics = memory::list_static(&db, UID, None, None, RECALL_STATIC_LIMIT)
        .await
        .expect("list_static");
    let static_ids: Vec<i64> = statics.iter().map(|m| m.id).collect();
    assert!(
        static_ids.contains(&old_static),
        "old pinned/static memory must surface in the static tier; got {static_ids:?}"
    );
    assert!(
        statics.iter().all(|m| m.is_static),
        "list_static must only return static memories"
    );

    // Important tier: the high-importance decision must be present and ranked first
    // (importance 10 ahead of the pinned importance-8 row), and the importance-4 filler
    // must be excluded.
    let important = memory::list_important(&db, UID, None, None, 7, 10)
        .await
        .expect("list_important");
    let important_ids: Vec<i64> = important.iter().map(|m| m.id).collect();
    assert!(
        important_ids.contains(&old_important),
        "old high-importance memory must surface in the important tier; got {important_ids:?}"
    );
    assert_eq!(
        important.first().map(|m| m.id),
        Some(old_important),
        "important tier must be ordered by importance (10 before 8)"
    );
    assert!(
        important.iter().all(|m| m.importance >= 7),
        "list_important must respect the importance floor"
    );
}

/// Static tier must respect space scoping.
#[tokio::test]
async fn static_tier_respects_space_scope() {
    let db = Database::connect_memory().await.expect("db");

    let in_space = store(&db, "pinned fact scoped to space five", 6, true, Some(5)).await;
    let other_space = store(&db, "pinned fact scoped to space nine", 6, true, Some(9)).await;
    let no_space = store(&db, "pinned fact with no space", 6, true, None).await;

    let scoped = memory::list_static(&db, UID, Some(5), None, 25)
        .await
        .expect("list_static scoped");
    let ids: Vec<i64> = scoped.iter().map(|m| m.id).collect();
    assert!(ids.contains(&in_space), "space-5 pinned fact must surface");
    assert!(
        !ids.contains(&other_space),
        "space-9 pinned fact must not leak into space-5"
    );
    assert!(
        !ids.contains(&no_space),
        "unscoped pinned fact must not match a space filter"
    );
}

/// Patch 49 Lot D: `list_static` and `list_important` gained an `include_unscoped`
/// parameter so the recall "static" and "important" tiers honor the same inclusive-space
/// contract as `list`/`hybrid_search` (Patch 33): a request scoped to a named space also
/// sees the caller's `default`-space rows and legacy pre-Patch-33 NULL-space rows when
/// `include_unscoped` is `Some(true)`; `None` and `Some(false)` both keep the strict,
/// named-space-only behaviour (documented library-boundary default, mod.rs comment on
/// `list_static`/`list_important`).
///
/// The default-space and legacy-NULL rows are given importance strictly ABOVE every
/// named-space row so their presence/absence and sort position are unambiguous under the
/// tiers' `LIMIT 10` (`important` tier: `ORDER BY importance DESC, id DESC`, mod.rs:1315):
/// if they ranked below the named-space filler, a passing assertion could mean "correctly
/// excluded by the space filter" or just "truncated by LIMIT" -- indistinguishable without
/// this ordering.
#[tokio::test]
async fn tiers_honor_include_unscoped_across_named_default_and_legacy_null_space() {
    let db = Database::connect_memory().await.expect("db");
    // spaces.user_id REFERENCES users(id) -- the row must exist before
    // resolve_or_create_space/default_space_id can insert against it. connect_memory()
    // seeds no users, and store()/list_static do not enforce this FK (memories.user_id is
    // a plain column), so the earlier space-agnostic tests in this file never needed this.
    seed_user(&db, UID).await;
    let named = kleos_lib::space::resolve_or_create_space(&db, UID, "patch49-tiers-probe")
        .await
        .expect("named space");
    let default_space = kleos_lib::space::default_space_id(&db, UID)
        .await
        .expect("default space");

    // Named-space rows: importance 5/6, must surface in every mode (strict AND inclusive).
    let named_static = store(&db, "patch49 named space pinned control", 5, true, Some(named)).await;
    let named_important =
        store(&db, "patch49 named space important control", 6, false, Some(named)).await;

    // Default-space-bucket rows: importance 9/10, strictly above every named-space row
    // above, so LIMIT 10 never masks their presence and they sort first when included.
    let default_static = store(
        &db,
        "patch49 default bucket pinned probe",
        9,
        true,
        Some(default_space),
    )
    .await;
    let default_important = store(
        &db,
        "patch49 default bucket important probe",
        10,
        false,
        Some(default_space),
    )
    .await;

    // Legacy pre-Patch-33 rows: space_id NULL, importance 7/8 -- between the named-space
    // control and the default-bucket probe, and reached through a DIFFERENT branch of the
    // inclusive OR clause (`space_id IS NULL`) than the default-space subquery branch, so
    // asserting them separately actually exercises both branches.
    let legacy_static = store(&db, "patch49 legacy null pinned probe", 7, true, None).await;
    let legacy_important = store(&db, "patch49 legacy null important probe", 8, false, None).await;

    // --- None and Some(false) both keep strict, named-space-only behaviour. ---
    for include_unscoped in [None, Some(false)] {
        let statics = memory::list_static(&db, UID, Some(named), include_unscoped, 25)
            .await
            .expect("list_static strict");
        let ids: Vec<i64> = statics.iter().map(|m| m.id).collect();
        assert!(
            ids.contains(&named_static),
            "{include_unscoped:?}: named-space static row must always surface; got {ids:?}"
        );
        assert!(
            !ids.contains(&default_static),
            "{include_unscoped:?}: strict mode must not leak the default-bucket static row; got {ids:?}"
        );
        assert!(
            !ids.contains(&legacy_static),
            "{include_unscoped:?}: strict mode must not leak the legacy NULL static row; got {ids:?}"
        );

        let important = memory::list_important(&db, UID, Some(named), include_unscoped, 1, 10)
            .await
            .expect("list_important strict");
        let ids: Vec<i64> = important.iter().map(|m| m.id).collect();
        assert!(
            ids.contains(&named_important),
            "{include_unscoped:?}: named-space important row must always surface; got {ids:?}"
        );
        assert!(
            !ids.contains(&default_important),
            "{include_unscoped:?}: strict mode must not leak the default-bucket important row; got {ids:?}"
        );
        assert!(
            !ids.contains(&legacy_important),
            "{include_unscoped:?}: strict mode must not leak the legacy NULL important row; got {ids:?}"
        );
    }

    // --- Some(true) widens to named + default bucket + legacy NULL. ---
    let statics = memory::list_static(&db, UID, Some(named), Some(true), 25)
        .await
        .expect("list_static inclusive");
    let ids: Vec<i64> = statics.iter().map(|m| m.id).collect();
    assert!(
        ids.contains(&named_static) && ids.contains(&default_static) && ids.contains(&legacy_static),
        "inclusive mode must surface named + default-bucket + legacy NULL static rows; got {ids:?}"
    );
    assert_eq!(
        statics.first().map(|m| m.id),
        Some(default_static),
        "inclusive static tier must rank the highest-importance (default-bucket) row first; got {ids:?}"
    );

    let important = memory::list_important(&db, UID, Some(named), Some(true), 1, 10)
        .await
        .expect("list_important inclusive");
    let ids: Vec<i64> = important.iter().map(|m| m.id).collect();
    assert!(
        ids.contains(&named_important)
            && ids.contains(&default_important)
            && ids.contains(&legacy_important),
        "inclusive mode must surface named + default-bucket + legacy NULL important rows; got {ids:?}"
    );
    assert_eq!(
        important.first().map(|m| m.id),
        Some(default_important),
        "inclusive important tier must rank the highest-importance (default-bucket) row first; got {ids:?}"
    );

    // --- Bonus (review request): the default-bucket subquery must stay scoped to the
    // calling user_id. A second user's default-space row, seeded with importance above
    // everything else so it would win LIMIT/ORDER if it leaked, must never appear for UID.
    const OTHER_UID: i64 = 4_949_003;
    seed_user(&db, OTHER_UID).await;
    let other_default_space = kleos_lib::space::default_space_id(&db, OTHER_UID)
        .await
        .expect("other user default space");
    let other_user_static = store_for_user(
        &db,
        OTHER_UID,
        "patch49 other user default bucket pinned probe",
        99,
        true,
        Some(other_default_space),
    )
    .await;
    let other_user_important = store_for_user(
        &db,
        OTHER_UID,
        "patch49 other user default bucket important probe",
        99,
        false,
        Some(other_default_space),
    )
    .await;

    let statics = memory::list_static(&db, UID, Some(named), Some(true), 25)
        .await
        .expect("list_static inclusive, cross-user check");
    let ids: Vec<i64> = statics.iter().map(|m| m.id).collect();
    assert!(
        !ids.contains(&other_user_static),
        "inclusive static tier must never surface another user's default-bucket row; got {ids:?}"
    );

    let important = memory::list_important(&db, UID, Some(named), Some(true), 1, 10)
        .await
        .expect("list_important inclusive, cross-user check");
    let ids: Vec<i64> = important.iter().map(|m| m.id).collect();
    assert!(
        !ids.contains(&other_user_important),
        "inclusive important tier must never surface another user's default-bucket row; got {ids:?}"
    );
}

/// Cap for the static tier mirrored from the server route default.
const RECALL_STATIC_LIMIT: usize = 25;
