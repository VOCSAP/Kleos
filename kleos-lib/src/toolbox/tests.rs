//! Store and search tests. The key and embedding helpers keep their own inline
//! tests; these need a database, so they live together with the fixtures.
//!
//! `Database::connect_memory()` runs the monolith migrations, which end with the
//! VOCSAP overlay pass -- so both `toolbox_tools` and `toolbox_locations` exist
//! on a fresh in-memory DB. Sharded mode is simulated by using two distinct
//! in-memory databases (shared sheets vs. user shard), which is also the only
//! way to prove the two tables are never joined in SQL.

use super::store;
use super::types::*;
use crate::db::Database;

async fn db() -> Database {
    Database::connect_memory().await.unwrap()
}

/// A request for a git-hosted tool. `commit` is `(sha, time)`.
fn request(
    remote: &str,
    name: &str,
    summary: &str,
    kind: &str,
    keywords: &[&str],
    commit: Option<(&str, i64)>,
) -> UpsertToolRequest {
    UpsertToolRequest {
        key: KeyInput {
            git_remote: Some(remote.to_string()),
            host: Some("workstation".to_string()),
            ..Default::default()
        },
        kind: kind.to_string(),
        name: name.to_string(),
        summary: summary.to_string(),
        body: String::new(),
        keywords: keywords.iter().map(|k| k.to_string()).collect(),
        canonical_url: None,
        commit: commit.map(|(sha, time)| CommitInfo {
            sha: sha.to_string(),
            time: Some(time),
        }),
        tags: Vec::new(),
        notes: String::new(),
        force: false,
    }
}

/// Upsert a sheet the way a handler would: key recomputed from the raw fields.
async fn upsert(
    shared: &Database,
    req: &UpsertToolRequest,
    embedding: Option<(&[f32], &str)>,
    user_id: i64,
) -> (ToolEntry, UpsertOutcome) {
    let (key, kind) = super::normalize_tool_key(&req.key).unwrap();
    store::upsert_tool(shared, req, &key, kind, embedding, user_id)
        .await
        .unwrap()
}

/// Give `user_id` a location for `req`'s tool, so searches can see it.
async fn own(user_db: &Database, user_id: i64, req: &UpsertToolRequest, tags: &[&str]) {
    let (key, _) = super::normalize_tool_key(&req.key).unwrap();
    let tags: Vec<String> = tags.iter().map(|t| t.to_string()).collect();
    store::upsert_location(
        user_db,
        user_id,
        &key,
        "workstation",
        "/srv/tools/x",
        &tags,
        "",
    )
    .await
    .unwrap();
}

// --- store ---

#[tokio::test]
async fn insert_then_same_content_is_unchanged() {
    let shared = db().await;
    let req = request(
        "https://github.com/Owner/Repo.git",
        "Repo",
        "does things",
        "repo",
        &["cli", "Rust"],
        Some(("abc", 100)),
    );

    let (entry, outcome) = upsert(&shared, &req, None, 1).await;
    assert_eq!(outcome, UpsertOutcome::Inserted);
    assert_eq!(entry.tool_key, "github.com/owner/repo");
    assert_eq!(entry.key_kind, "git");
    assert_eq!(entry.keywords, vec!["cli".to_string(), "rust".to_string()]);
    assert!(!entry.has_embedding);

    let (again, outcome) = upsert(&shared, &req, None, 2).await;
    assert_eq!(outcome, UpsertOutcome::Unchanged);
    assert_eq!(again.id, entry.id);
    assert_eq!(
        again.indexed_by_user_id,
        Some(1),
        "no rewrite, no new owner"
    );
}

#[tokio::test]
async fn newer_commit_overwrites_the_shared_sheet() {
    let shared = db().await;
    let old = request(
        "https://github.com/o/r",
        "R",
        "old summary",
        "repo",
        &[],
        Some(("aaa", 100)),
    );
    upsert(&shared, &old, None, 1).await;

    let new = request(
        "https://github.com/o/r",
        "R",
        "new summary",
        "repo",
        &[],
        Some(("bbb", 200)),
    );
    let (entry, outcome) = upsert(&shared, &new, None, 2).await;
    assert_eq!(outcome, UpsertOutcome::Updated);
    assert_eq!(entry.summary, "new summary");
    assert_eq!(entry.indexed_commit.as_deref(), Some("bbb"));
    assert_eq!(entry.indexed_commit_time, Some(200));
    assert_eq!(entry.indexed_by_user_id, Some(2));
}

#[tokio::test]
async fn older_commit_is_kept_even_with_force() {
    let shared = db().await;
    let new = request(
        "https://github.com/o/r",
        "R",
        "new summary",
        "repo",
        &[],
        Some(("bbb", 200)),
    );
    upsert(&shared, &new, None, 1).await;

    let mut stale = request(
        "https://github.com/o/r",
        "R",
        "stale summary",
        "repo",
        &[],
        Some(("aaa", 100)),
    );
    let (entry, outcome) = upsert(&shared, &stale, None, 2).await;
    assert_eq!(outcome, UpsertOutcome::Kept);
    assert_eq!(entry.summary, "new summary");

    // force does NOT let an older commit win: the policy table has no exception
    // for that row.
    stale.force = true;
    let (entry, outcome) = upsert(&shared, &stale, None, 2).await;
    assert_eq!(outcome, UpsertOutcome::Kept);
    assert_eq!(entry.summary, "new summary");
}

#[tokio::test]
async fn same_commit_time_needs_force_to_overwrite() {
    let shared = db().await;
    let first = request(
        "https://github.com/o/r",
        "R",
        "first",
        "repo",
        &[],
        Some(("aaa", 100)),
    );
    upsert(&shared, &first, None, 1).await;

    let mut second = request(
        "https://github.com/o/r",
        "R",
        "second",
        "repo",
        &[],
        Some(("aaa", 100)),
    );
    let (entry, outcome) = upsert(&shared, &second, None, 2).await;
    assert_eq!(outcome, UpsertOutcome::Kept);
    assert_eq!(entry.summary, "first");

    second.force = true;
    let (entry, outcome) = upsert(&shared, &second, None, 2).await;
    assert_eq!(outcome, UpsertOutcome::Updated);
    assert_eq!(entry.summary, "second");
}

#[tokio::test]
async fn incoming_without_commit_is_kept_unless_forced() {
    let shared = db().await;
    let committed = request(
        "https://github.com/o/r",
        "R",
        "committed",
        "repo",
        &[],
        Some(("aaa", 100)),
    );
    upsert(&shared, &committed, None, 1).await;

    let mut loose = request("https://github.com/o/r", "R", "loose", "repo", &[], None);
    let (entry, outcome) = upsert(&shared, &loose, None, 2).await;
    assert_eq!(outcome, UpsertOutcome::Kept);
    assert_eq!(entry.summary, "committed");

    loose.force = true;
    let (entry, outcome) = upsert(&shared, &loose, None, 2).await;
    assert_eq!(outcome, UpsertOutcome::Updated);
    assert_eq!(entry.summary, "loose");
    assert_eq!(entry.indexed_commit, None);
}

#[tokio::test]
async fn stored_without_commit_is_last_writer_wins() {
    let shared = db().await;
    let first = request("https://github.com/o/r", "R", "first", "repo", &[], None);
    upsert(&shared, &first, None, 1).await;

    let second = request("https://github.com/o/r", "R", "second", "repo", &[], None);
    let (entry, outcome) = upsert(&shared, &second, None, 2).await;
    assert_eq!(outcome, UpsertOutcome::Updated);
    assert_eq!(entry.summary, "second");
}

#[tokio::test]
async fn embedding_is_backfilled_on_a_kept_sheet() {
    let shared = db().await;
    let committed = request(
        "https://github.com/o/r",
        "R",
        "committed",
        "repo",
        &[],
        Some(("aaa", 200)),
    );
    let (entry, _) = upsert(&shared, &committed, None, 1).await;
    assert!(!entry.has_embedding);

    // An older indexing loses the sheet but still donates its vector.
    let stale = request(
        "https://github.com/o/r",
        "R",
        "stale",
        "repo",
        &[],
        Some(("bbb", 100)),
    );
    let vector = [0.1f32, 0.2, 0.3];
    let (entry, outcome) = upsert(&shared, &stale, Some((&vector, "test-model")), 2).await;
    assert_eq!(outcome, UpsertOutcome::Kept);
    assert_eq!(entry.summary, "committed");
    assert!(entry.has_embedding, "the empty vector slot must be filled");
    assert_eq!(entry.embedding_model.as_deref(), Some("test-model"));
}

#[tokio::test]
async fn overwrite_without_an_embedder_keeps_the_vector_and_marks_the_model_stale() {
    let shared = db().await;
    let first = request(
        "https://github.com/o/r",
        "R",
        "first",
        "repo",
        &[],
        Some(("aaa", 100)),
    );
    let vector = [1.0f32, 0.0, 0.0];
    let (entry, _) = upsert(&shared, &first, Some((&vector, "m")), 1).await;
    assert!(entry.has_embedding);

    // A client indexing against a server with no embedder rewrites the sheet.
    // Dropping the vector here would take the row out of the search's vector
    // channel with nothing able to put it back until someone reindexes; keeping
    // it with a NULL model says "stale, recompute me" instead.
    let second = request(
        "https://github.com/o/r",
        "R",
        "rewritten",
        "repo",
        &[],
        Some(("bbb", 200)),
    );
    let (entry, outcome) = upsert(&shared, &second, None, 1).await;
    assert_eq!(outcome, UpsertOutcome::Updated);
    assert_eq!(entry.summary, "rewritten");
    assert!(
        entry.has_embedding,
        "the stored vector must survive an overwrite that carries none"
    );
    assert_eq!(
        entry.embedding_model, None,
        "a vector describing the previous text must be flagged stale"
    );

    // And the reindex route finds it: it targets rows whose model differs from
    // the configured one, which NULL always does.
    let keys = vec!["github.com/o/r".to_string()];
    let stale = store::tools_needing_embedding(&shared, &keys, Some("m"))
        .await
        .unwrap();
    assert_eq!(
        stale.len(),
        1,
        "the stale-model row must be a reindex target"
    );
    assert_eq!(stale[0].0, entry.id);

    // `missing_only` looks at the vector, not the model: this row has one.
    let missing = store::tools_needing_embedding(&shared, &keys, None)
        .await
        .unwrap();
    assert!(missing.is_empty());
}

#[tokio::test]
async fn trailing_whitespace_is_not_new_content() {
    let shared = db().await;
    let req = request(
        "https://github.com/o/r",
        "R",
        "does things",
        "repo",
        &["cli"],
        Some(("aaa", 100)),
    );
    let (entry, outcome) = upsert(&shared, &req, None, 1).await;
    assert_eq!(outcome, UpsertOutcome::Inserted);

    // The row stores the trimmed text, so the hash has to be taken on the same
    // values: otherwise a re-send that only gained a trailing newline would
    // rewrite the sheet and steal `indexed_by_user_id`.
    let mut padded = req.clone();
    padded.name = "R  ".to_string();
    padded.summary = "does things\n".to_string();
    let (again, outcome) = upsert(&shared, &padded, None, 2).await;
    assert_eq!(outcome, UpsertOutcome::Unchanged);
    assert_eq!(again.content_hash, entry.content_hash);
    assert_eq!(again.indexed_by_user_id, Some(1));
}

#[tokio::test]
async fn tools_by_keys_orders_by_name_across_batches() {
    let shared = db().await;
    // One key more than a single `IN (...)` batch holds: the row that lands
    // alone in the second batch is also the one that sorts first, so a
    // per-batch ordering would put it last.
    let total = store::MAX_IN_PARAMS + 1;
    let mut keys = Vec::with_capacity(total);
    for i in 0..total {
        let name = if i + 1 == total {
            "Aaa second batch".to_string()
        } else {
            format!("Zzz {i:04}")
        };
        let req = request(
            &format!("https://github.com/o/r{i}"),
            &name,
            "s",
            "repo",
            &[],
            None,
        );
        upsert(&shared, &req, None, 1).await;
        keys.push(format!("github.com/o/r{i}"));
    }

    let tools = store::tools_by_keys(&shared, &keys).await.unwrap();
    assert_eq!(tools.len(), total);
    assert_eq!(tools[0].name, "Aaa second batch");
    let names: Vec<String> = tools.iter().map(|t| t.name.to_lowercase()).collect();
    let mut expected = names.clone();
    expected.sort();
    assert_eq!(names, expected, "the whole listing must be name-ordered");
}

#[tokio::test]
async fn locations_are_per_user_and_idempotent() {
    let user_db = db().await;
    let key = "github.com/o/r";

    let first = store::upsert_location(&user_db, 1, key, "Host", "/srv/x", &["Rust".into()], "n1")
        .await
        .unwrap();
    assert_eq!(first.host, "host", "host is folded to lowercase");
    assert_eq!(first.tags, vec!["rust".to_string()]);

    // Same (user, key, host, path): the row is refreshed, not duplicated.
    let again = store::upsert_location(&user_db, 1, key, "host", "/srv/x", &["cli".into()], "n2")
        .await
        .unwrap();
    assert_eq!(again.id, first.id);
    assert_eq!(again.tags, vec!["cli".to_string()]);
    assert_eq!(again.notes, "n2");

    // A different path is a different location.
    let other = store::upsert_location(&user_db, 1, key, "host", "/opt/x", &[], "")
        .await
        .unwrap();
    assert_ne!(other.id, first.id);

    // Another user's location is invisible to the first one's key list.
    store::upsert_location(&user_db, 2, "github.com/o/other", "host", "/srv/y", &[], "")
        .await
        .unwrap();
    assert_eq!(
        store::list_user_keys(&user_db, 1).await.unwrap(),
        vec![key.to_string()]
    );
    assert_eq!(
        store::list_user_keys(&user_db, 2).await.unwrap(),
        vec!["github.com/o/other".to_string()]
    );

    let map = store::locations_for_keys(&user_db, 1, &[key.to_string()])
        .await
        .unwrap();
    assert_eq!(map.get(key).map(Vec::len), Some(2));
}

#[tokio::test]
async fn delete_locations_forgets_for_one_user_only_and_keeps_the_sheet() {
    let shared = db().await;
    let user_db = db().await;
    let req = request("https://github.com/o/r", "R", "s", "repo", &[], None);
    upsert(&shared, &req, None, 1).await;
    own(&user_db, 1, &req, &[]).await;
    own(&user_db, 2, &req, &[]).await;

    let removed = store::delete_locations(&user_db, 1, "github.com/o/r")
        .await
        .unwrap();
    assert_eq!(removed, 1);
    assert!(store::list_user_keys(&user_db, 1).await.unwrap().is_empty());
    assert_eq!(store::list_user_keys(&user_db, 2).await.unwrap().len(), 1);
    assert!(
        store::get_tool_by_key(&shared, "github.com/o/r")
            .await
            .unwrap()
            .is_some(),
        "the shared sheet must survive: other shards are invisible from here"
    );
}

#[tokio::test]
async fn reindex_helpers_target_missing_and_stale_models() {
    let shared = db().await;
    let a = request("https://github.com/o/a", "A", "alpha", "repo", &["x"], None);
    let b = request("https://github.com/o/b", "B", "beta", "repo", &[], None);
    let (a_entry, _) = upsert(&shared, &a, None, 1).await;
    upsert(&shared, &b, Some((&[0.5f32, 0.5], "old-model")), 1).await;

    let keys = vec!["github.com/o/a".to_string(), "github.com/o/b".to_string()];

    // Missing only.
    let todo = store::tools_needing_embedding(&shared, &keys, None)
        .await
        .unwrap();
    assert_eq!(todo.len(), 1);
    assert_eq!(todo[0].0, a_entry.id);
    assert_eq!(todo[0].1, "A\nalpha\nx");

    // Missing + different model.
    let todo = store::tools_needing_embedding(&shared, &keys, Some("new-model"))
        .await
        .unwrap();
    assert_eq!(todo.len(), 2);

    store::set_embedding(&shared, a_entry.id, &[0.1, 0.2], "new-model")
        .await
        .unwrap();
    let todo = store::tools_needing_embedding(&shared, &keys, Some("new-model"))
        .await
        .unwrap();
    assert_eq!(todo.len(), 1, "only the stale-model row is left");
}

// --- search ---

#[tokio::test]
async fn a_user_never_sees_a_tool_they_do_not_own() {
    let shared = db().await;
    let user_db = db().await;
    let req = request(
        "https://github.com/o/r",
        "Parser",
        "parses json documents",
        "cli",
        &["json"],
        None,
    );
    upsert(&shared, &req, None, 1).await;
    own(&user_db, 1, &req, &[]).await;

    let opts = FindOptions::default();
    let mine = super::find_tools(&shared, &user_db, 1, "json parser", None, &opts)
        .await
        .unwrap();
    assert_eq!(mine.len(), 1);

    let theirs = super::find_tools(&shared, &user_db, 2, "json parser", None, &opts)
        .await
        .unwrap();
    assert!(
        theirs.is_empty(),
        "the shared row exists, but user 2 has no location for it"
    );
}

#[tokio::test]
async fn fts_channel_works_without_any_embedding() {
    let shared = db().await;
    let user_db = db().await;
    let a = request(
        "https://github.com/o/a",
        "Jsonnet",
        "parses and formats json documents",
        "cli",
        &["json", "format"],
        None,
    );
    let b = request(
        "https://github.com/o/b",
        "Kube",
        "deploys containers to a cluster",
        "cli",
        &["kubernetes"],
        None,
    );
    upsert(&shared, &a, None, 1).await;
    upsert(&shared, &b, None, 1).await;
    own(&user_db, 1, &a, &[]).await;
    own(&user_db, 1, &b, &[]).await;

    let hits = super::find_tools(&shared, &user_db, 1, "json", None, &FindOptions::default())
        .await
        .unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].tool.name, "Jsonnet");
    assert!(hits[0].score > 0.0);
    assert!(hits[0].fts_score != 0.0, "raw bm25 is exposed");
    assert_eq!(hits[0].vector_score, 0.0);
    assert_eq!(hits[0].locations.len(), 1);
}

#[tokio::test]
async fn vector_channel_finds_what_the_words_do_not() {
    let shared = db().await;
    let user_db = db().await;
    let a = request(
        "https://github.com/o/a",
        "Alpha",
        "alpha tool",
        "cli",
        &[],
        None,
    );
    let b = request(
        "https://github.com/o/b",
        "Beta",
        "beta tool",
        "cli",
        &[],
        None,
    );
    upsert(&shared, &a, Some((&[1.0f32, 0.0, 0.0], "m")), 1).await;
    upsert(&shared, &b, Some((&[0.0f32, 1.0, 0.0], "m")), 1).await;
    own(&user_db, 1, &a, &[]).await;
    own(&user_db, 1, &b, &[]).await;

    // A query whose words match nothing: only the vector channel can answer.
    let hits = super::find_tools(
        &shared,
        &user_db,
        1,
        "zzzz qqqq",
        Some(&[0.9f32, 0.1, 0.0]),
        &FindOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].tool.name, "Alpha");
    assert!(hits[0].vector_score > hits[1].vector_score);
    assert_eq!(hits[0].fts_score, 0.0, "no lexical match at all");
}

#[tokio::test]
async fn rrf_puts_a_tool_matched_by_both_channels_first() {
    let shared = db().await;
    let user_db = db().await;
    // `both` is the best vector match AND the only lexical match; `vec_only` is a
    // slightly better vector match than `fts_only`, which matches nothing.
    let both = request(
        "https://github.com/o/both",
        "Both",
        "grep across the repository",
        "cli",
        &["grep"],
        None,
    );
    let vec_only = request(
        "https://github.com/o/vec",
        "VecOnly",
        "unrelated words entirely",
        "cli",
        &[],
        None,
    );
    upsert(&shared, &both, Some((&[0.9f32, 0.1, 0.0], "m")), 1).await;
    upsert(&shared, &vec_only, Some((&[1.0f32, 0.0, 0.0], "m")), 1).await;
    own(&user_db, 1, &both, &[]).await;
    own(&user_db, 1, &vec_only, &[]).await;

    let hits = super::find_tools(
        &shared,
        &user_db,
        1,
        "grep",
        Some(&[1.0f32, 0.0, 0.0]),
        &FindOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(
        hits[0].tool.name, "Both",
        "two channels beat one better vector rank"
    );
    assert!(hits[0].score > hits[1].score);
}

#[tokio::test]
async fn kind_and_tag_filters_narrow_the_result_set() {
    let shared = db().await;
    let user_db = db().await;
    let cli = request(
        "https://github.com/o/cli",
        "CliTool",
        "search things quickly",
        "cli",
        &["search"],
        None,
    );
    let doc = request(
        "https://github.com/o/doc",
        "DocSite",
        "search things quickly",
        "doc",
        &["search"],
        None,
    );
    upsert(&shared, &cli, None, 1).await;
    upsert(&shared, &doc, None, 1).await;
    own(&user_db, 1, &cli, &["work", "linux"]).await;
    own(&user_db, 1, &doc, &["work"]).await;

    let all = super::find_tools(
        &shared,
        &user_db,
        1,
        "search",
        None,
        &FindOptions::default(),
    )
    .await
    .unwrap();
    assert_eq!(all.len(), 2);

    let only_cli = super::find_tools(
        &shared,
        &user_db,
        1,
        "search",
        None,
        &FindOptions {
            kind: Some("cli".to_string()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(only_cli.len(), 1);
    assert_eq!(only_cli[0].tool.name, "CliTool");

    // Tags are ANDed and matched against the caller's own location rows.
    let tagged = super::find_tools(
        &shared,
        &user_db,
        1,
        "search",
        None,
        &FindOptions {
            tags: vec!["Work".to_string(), "LINUX".to_string()],
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(tagged.len(), 1);
    assert_eq!(tagged[0].tool.name, "CliTool");

    let none = super::find_tools(
        &shared,
        &user_db,
        1,
        "search",
        None,
        &FindOptions {
            tags: vec!["absent".to_string()],
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert!(none.is_empty());
}

#[tokio::test]
async fn limit_is_clamped_and_defaults() {
    assert_eq!(FindOptions::default().effective_limit(), DEFAULT_FIND_LIMIT);
    assert_eq!(
        FindOptions {
            limit: 0,
            ..Default::default()
        }
        .effective_limit(),
        DEFAULT_FIND_LIMIT
    );
    assert_eq!(
        FindOptions {
            limit: 9999,
            ..Default::default()
        }
        .effective_limit(),
        MAX_FIND_LIMIT
    );
}
