//! Per-route tests for the toolbox catalog (Patch 53).
//!
//! Sharded mode on purpose: the shared sheets live in the main database and the
//! locations in each user's shard, so these tests also prove the two tables are
//! never joined in SQL. No embedder is loaded, so retrieval runs on the FTS
//! channel alone -- which is exactly the degraded path a server without an
//! embedding provider takes.

mod common;

use axum::http::StatusCode;
use common::{bootstrap_admin_key, delete, get, post, seed_user, test_app_with_sharding};
use serde_json::{json, Value};

/// A complete sheet for a fictional tool, as an indexing client would send it.
fn sheet(remote: &str, name: &str, kind: &str, summary: &str, path: &str) -> Value {
    json!({
        "key": { "git_remote": remote, "local_path": path, "host": "workstation" },
        "kind": kind,
        "name": name,
        "summary": summary,
        "body": format!("# {name}\n\nFull sheet."),
        "keywords": ["json", "parser", "analyse"],
        "commit": { "sha": "abc123", "time": 1_700_000_000 },
        "tags": ["local"],
        "notes": "cloned for the parser work"
    })
}

#[tokio::test]
async fn index_find_get_forget_round_trip() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;

    let (status, body) = post(
        &app,
        "/toolbox/entries",
        &key,
        sheet(
            "git@github.com:vocsap/jsonsurf.git",
            "jsonsurf",
            "cli",
            "Streaming JSON parser for huge files.",
            "/srv/tools/jsonsurf",
        ),
    )
    .await;
    assert!(status.is_success(), "index failed {status}: {body}");
    assert_eq!(body["outcome"], "inserted");
    assert_eq!(body["tool_key"], "github.com/vocsap/jsonsurf");
    assert_eq!(body["key_kind"], "git");
    assert_eq!(body["embedded"], false, "no embedder is loaded in tests");
    let id = body["id"].as_i64().expect("index returned an id");
    assert!(body["location_id"].as_i64().is_some(), "location recorded");

    // Re-indexing the same sheet is a no-op on the shared row.
    let (status, body) = post(
        &app,
        "/toolbox/entries",
        &key,
        sheet(
            "https://github.com/VOCSAP/jsonsurf.git",
            "jsonsurf",
            "cli",
            "Streaming JSON parser for huge files.",
            "/srv/tools/jsonsurf",
        ),
    )
    .await;
    assert!(status.is_success(), "re-index failed {status}: {body}");
    assert_eq!(body["id"], id, "the https remote resolves to the same key");
    assert_eq!(body["outcome"], "unchanged");

    // find
    let (status, body) = post(
        &app,
        "/toolbox/find",
        &key,
        json!({ "query": "a-t-on un parser JSON ?" }),
    )
    .await;
    assert!(status.is_success(), "find failed {status}: {body}");
    assert_eq!(body["count"], 1, "expected one hit: {body}");
    assert_eq!(body["embedded_query"], false);
    assert_eq!(body["reranked"], false);
    let hit = &body["results"][0];
    assert_eq!(hit["tool"]["id"], id);
    assert_eq!(hit["locations"][0]["local_path"], "/srv/tools/jsonsurf");
    assert_eq!(hit["locations"][0]["tags"][0], "local");

    // list
    let (status, body) = get(&app, "/toolbox/entries", &key).await;
    assert!(status.is_success(), "list failed {status}: {body}");
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["tool"]["name"], "jsonsurf");

    // get by id
    let (status, body) = get(&app, &format!("/toolbox/entries/{id}"), &key).await;
    assert!(status.is_success(), "get failed {status}: {body}");
    assert_eq!(body["tool"]["tool_key"], "github.com/vocsap/jsonsurf");
    assert_eq!(body["locations"][0]["host"], "workstation");

    // forget: the caller's locations go, the shared sheet stays.
    let (status, body) = delete(&app, &format!("/toolbox/entries/{id}"), &key).await;
    assert!(status.is_success(), "forget failed {status}: {body}");
    assert_eq!(body["deleted"], 1);

    let (status, _) = get(&app, &format!("/toolbox/entries/{id}"), &key).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a forgotten tool is gone for its former owner"
    );
    let (_, body) = post(
        &app,
        "/toolbox/find",
        &key,
        json!({ "query": "parser json" }),
    )
    .await;
    assert_eq!(body["count"], 0, "forgotten tools drop out of search");
}

#[tokio::test]
async fn one_user_never_sees_another_users_catalog() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let admin = bootstrap_admin_key(&app).await;
    let (_bob_id, bob) = seed_user(&app, &admin, "bob").await;

    let (status, body) = post(
        &app,
        "/toolbox/entries",
        &admin,
        sheet(
            "git@github.com:vocsap/jsonsurf.git",
            "jsonsurf",
            "cli",
            "Streaming JSON parser for huge files.",
            "/srv/tools/jsonsurf",
        ),
    )
    .await;
    assert!(status.is_success(), "index failed {status}: {body}");
    let id = body["id"].as_i64().expect("index returned an id");

    // The shared row exists, but Bob has no location for that key.
    let (_, body) = post(
        &app,
        "/toolbox/find",
        &bob,
        json!({ "query": "parser json" }),
    )
    .await;
    assert_eq!(
        body["count"], 0,
        "search must not leak another user's tools"
    );

    let (_, body) = get(&app, "/toolbox/entries", &bob).await;
    assert_eq!(body["total"], 0, "listing must not leak either");

    let (status, _) = get(&app, &format!("/toolbox/entries/{id}"), &bob).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "fetching by id must be indistinguishable from a missing id"
    );
    let (status, _) = delete(&app, &format!("/toolbox/entries/{id}"), &bob).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "and so must deleting");

    // Once Bob indexes the same tool he shares the sheet -- same row, his own
    // location -- and the owner's location is still not visible to him.
    let (status, body) = post(
        &app,
        "/toolbox/entries",
        &bob,
        sheet(
            "https://github.com/vocsap/jsonsurf",
            "jsonsurf",
            "cli",
            "Streaming JSON parser for huge files.",
            "/home/bob/src/jsonsurf",
        ),
    )
    .await;
    assert!(status.is_success(), "bob's index failed {status}: {body}");
    assert_eq!(body["id"], id, "same canonical key, same shared sheet");

    let (_, body) = get(&app, &format!("/toolbox/entries/{id}"), &bob).await;
    let locations = body["locations"].as_array().expect("locations array");
    assert_eq!(locations.len(), 1, "only Bob's own location: {body}");
    assert_eq!(locations[0]["local_path"], "/home/bob/src/jsonsurf");

    // Bob forgetting the tool leaves the owner's location untouched.
    let (status, _) = delete(&app, &format!("/toolbox/entries/{id}"), &bob).await;
    assert!(status.is_success());
    let (status, body) = get(&app, &format!("/toolbox/entries/{id}"), &admin).await;
    assert!(status.is_success(), "owner still owns it {status}: {body}");
    assert_eq!(body["locations"][0]["local_path"], "/srv/tools/jsonsurf");
}

#[tokio::test]
async fn find_filters_by_kind_and_tags() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;

    let mut doc = sheet(
        "https://example.org/handbook",
        "handbook",
        "doc",
        "Team handbook covering the JSON conventions.",
        "",
    );
    doc["key"] = json!({ "url": "https://example.org/handbook", "host": "workstation" });
    doc["tags"] = json!(["reference"]);
    let (status, body) = post(&app, "/toolbox/entries", &key, doc).await;
    assert!(status.is_success(), "index failed {status}: {body}");
    assert_eq!(body["key_kind"], "url");
    assert_eq!(body["tool_key"], "example.org/handbook");

    let (status, body) = post(
        &app,
        "/toolbox/entries",
        &key,
        sheet(
            "git@github.com:vocsap/jsonsurf.git",
            "jsonsurf",
            "cli",
            "Streaming JSON parser for huge files.",
            "/srv/tools/jsonsurf",
        ),
    )
    .await;
    assert!(status.is_success(), "index failed {status}: {body}");

    let (_, body) = post(&app, "/toolbox/find", &key, json!({ "query": "json" })).await;
    assert_eq!(body["count"], 2, "both sheets mention json: {body}");

    let (_, body) = post(
        &app,
        "/toolbox/find",
        &key,
        json!({ "query": "json", "kind": "cli" }),
    )
    .await;
    assert_eq!(body["count"], 1);
    assert_eq!(body["results"][0]["tool"]["name"], "jsonsurf");

    let (_, body) = post(
        &app,
        "/toolbox/find",
        &key,
        json!({ "query": "json", "tags": ["reference"] }),
    )
    .await;
    assert_eq!(body["count"], 1);
    assert_eq!(body["results"][0]["tool"]["name"], "handbook");

    let (_, body) = get(&app, "/toolbox/entries?kind=doc", &key).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["tool"]["kind"], "doc");
}

#[tokio::test]
async fn malformed_requests_are_rejected() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;

    // No identity field at all: no canonical key can be computed.
    let (status, _) = post(
        &app,
        "/toolbox/entries",
        &key,
        json!({ "kind": "cli", "name": "nameless", "summary": "no key" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Blank name.
    let (status, _) = post(
        &app,
        "/toolbox/entries",
        &key,
        json!({ "key": { "url": "https://example.org/x" }, "kind": "doc",
                "name": "   ", "summary": "blank name" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Empty query.
    let (status, _) = post(&app, "/toolbox/find", &key, json!({ "query": "  " })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // An unknown id is a 404, not a 500.
    let (status, _) = get(&app, "/toolbox/entries/424242", &key).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn space_fields_injected_by_the_mcp_bridge_are_ignored() {
    // kleos-mcp appends `space` to every call it dispatches; the toolbox has no
    // spaces, so the handlers must ignore it rather than 400.
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;

    let mut payload = sheet(
        "git@github.com:vocsap/jsonsurf.git",
        "jsonsurf",
        "cli",
        "Streaming JSON parser for huge files.",
        "/srv/tools/jsonsurf",
    );
    payload["space"] = json!("kleos");
    payload["space_id"] = json!(3);
    let (status, body) = post(&app, "/toolbox/entries", &key, payload).await;
    assert!(
        status.is_success(),
        "index with space failed {status}: {body}"
    );

    let (status, body) = post(
        &app,
        "/toolbox/find",
        &key,
        json!({ "query": "json parser", "space": "kleos" }),
    )
    .await;
    assert!(
        status.is_success(),
        "find with space failed {status}: {body}"
    );
    assert_eq!(body["count"], 1);

    // GET routes receive it as a query parameter.
    let (status, body) = get(&app, "/toolbox/entries?space=kleos", &key).await;
    assert!(
        status.is_success(),
        "list with space failed {status}: {body}"
    );
    assert_eq!(body["total"], 1);
}

#[tokio::test]
async fn reindex_is_a_no_op_without_an_embedder() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;

    let (status, _) = post(
        &app,
        "/toolbox/entries",
        &key,
        sheet(
            "git@github.com:vocsap/jsonsurf.git",
            "jsonsurf",
            "cli",
            "Streaming JSON parser for huge files.",
            "/srv/tools/jsonsurf",
        ),
    )
    .await;
    assert!(status.is_success());

    let (status, body) = post(&app, "/toolbox/reindex", &key, json!({})).await;
    assert!(status.is_success(), "reindex failed {status}: {body}");
    assert_eq!(body["embedder"], false);
    assert_eq!(body["embedded"], 0);
    assert_eq!(body["candidates"], 0);
}
