//! The memory JSON shape carries `is_superseded`, so a replica that reads
//! GET /list can drop a retired memory it projected earlier.

mod common;

use axum::Router;
use common::{bootstrap_admin_key, get, post, test_app_with_sharding};
use serde_json::{json, Value};

fn listed(body: &Value, id: i64) -> Option<&Value> {
    body["results"]
        .as_array()
        .expect("results array")
        .iter()
        .find(|row| row["id"].as_i64() == Some(id))
}

async fn store(app: &Router, key: &str, content: &str) -> i64 {
    let (status, stored) = post(
        app,
        "/store",
        key,
        json!({ "content": content, "category": "test" }),
    )
    .await;
    assert!(status.is_success(), "store failed: {status} {stored}");
    stored["id"].as_i64().expect("numeric id")
}

#[tokio::test]
async fn list_reports_is_superseded_as_a_boolean() {
    let (app, state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;
    let id = store(&app, &key, "replica flag probe").await;

    let (status, body) = get(&app, "/list", &key).await;
    assert!(status.is_success(), "list failed: {status} {body}");
    let row =
        listed(&body, id).unwrap_or_else(|| panic!("stored memory missing from /list: {body}"));
    assert_eq!(row["is_superseded"], json!(false), "{row}");

    // Retire the row the way deduplicate does: still the latest version, so
    // GET /list keeps returning it and the replica must read the flag.
    let owner = row["user_id"].as_i64().expect("user_id");
    let shard = kleos_server::extractors::resolve_db_for_user(&state, owner)
        .await
        .expect("resolve shard");
    shard
        .write(move |conn| {
            conn.execute(
                "UPDATE memories SET is_superseded = 1 WHERE id = ?1",
                rusqlite::params![id],
            )?;
            Ok(())
        })
        .await
        .expect("mark retired");

    let (status, body) = get(&app, "/list", &key).await;
    assert!(status.is_success(), "list failed: {status} {body}");
    let row = listed(&body, id)
        .unwrap_or_else(|| panic!("retired latest row missing from /list: {body}"));
    assert_eq!(row["is_superseded"], json!(true), "{row}");
}

async fn retire(state: &kleos_server::state::AppState, owner: i64, id: i64) {
    let shard = kleos_server::extractors::resolve_db_for_user(state, owner)
        .await
        .expect("resolve shard");
    shard
        .write(move |conn| {
            conn.execute(
                "UPDATE memories SET is_superseded = 1 WHERE id = ?1",
                rusqlite::params![id],
            )?;
            Ok(())
        })
        .await
        .expect("mark retired");
}

fn ids(rows: &Value) -> Vec<i64> {
    rows.as_array()
        .expect("results array")
        .iter()
        .filter_map(|row| row["id"].as_i64())
        .collect()
}

#[tokio::test]
async fn list_hides_a_retired_memory_only_when_asked() {
    let (app, state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;
    let live = store(&app, &key, "listing keeper hollowmere").await;
    let retired = store(&app, &key, "listing retired ironbridge").await;
    let (_, body) = get(&app, "/list", &key).await;
    let owner = listed(&body, live).expect("live listed")["user_id"]
        .as_i64()
        .expect("user_id");
    retire(&state, owner, retired).await;

    let (status, raw) = get(&app, "/list", &key).await;
    assert!(status.is_success(), "list failed: {status} {raw}");
    assert!(
        ids(&raw["results"]).contains(&retired),
        "raw listing keeps it: {raw}"
    );

    let (status, live_only) = get(&app, "/list?exclude_superseded=true", &key).await;
    assert!(status.is_success(), "list failed: {status} {live_only}");
    let got = ids(&live_only["results"]);
    assert!(got.contains(&live), "{live_only}");
    assert!(
        !got.contains(&retired),
        "retired memory listed: {live_only}"
    );
}

fn tool_call(tool: &str, id: i64, arguments: Value) -> Value {
    json!({
        "jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": { "name": tool, "arguments": arguments }
    })
}

async fn live_and_retired(
    app: &axum::Router,
    state: &kleos_server::state::AppState,
    key: &str,
) -> (i64, i64) {
    let live = store(app, key, "mcp keeper juniper").await;
    let retired = store(app, key, "mcp retired kestrel").await;
    let (_, body) = get(app, "/list", key).await;
    let owner = listed(&body, live).expect("live listed")["user_id"]
        .as_i64()
        .expect("user_id");
    retire(state, owner, retired).await;
    (live, retired)
}

#[tokio::test]
async fn the_memory_list_tool_hides_retired_memories_by_default() {
    let (app, state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;
    let (live, retired) = live_and_retired(&app, &state, &key).await;

    for tool in ["memory_list", "memory.list"] {
        let call = tool_call(tool, 1, json!({ "limit": 50 }));
        let (status, resp) = post(&app, "/mcp", &key, call).await;
        assert!(status.is_success(), "mcp failed: {status} {resp}");
        assert_eq!(resp["result"]["isError"], json!(false), "{tool}: {resp}");
        let got = ids(&resp["result"]["structuredContent"]["results"]);
        assert!(got.contains(&live), "{tool}: {resp}");
        assert!(
            !got.contains(&retired),
            "retired memory served over MCP by {tool}: {resp}"
        );
    }

    let batch = json!([
        tool_call("memory_list", 2, json!({ "limit": 50 })),
        tool_call(
            "memory_list",
            3,
            json!({ "limit": 50, "exclude_superseded": false })
        )
    ]);
    let (status, resp) = post(&app, "/mcp", &key, batch).await;
    assert!(status.is_success(), "mcp batch failed: {status} {resp}");
    let by_id = |id: i64| {
        let row = resp
            .as_array()
            .expect("batch response")
            .iter()
            .find(|r| r["id"] == json!(id))
            .unwrap_or_else(|| panic!("no response {id}: {resp}"));
        ids(&row["result"]["structuredContent"]["results"])
    };
    assert!(
        !by_id(2).contains(&retired),
        "batched call served it: {resp}"
    );
    assert!(
        by_id(3).contains(&retired),
        "explicit opt-out keeps the raw listing: {resp}"
    );
}

#[tokio::test]
async fn memory_list_arguments_never_bypass_the_filter() {
    let (app, state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;
    let (live, retired) = live_and_retired(&app, &state, &key).await;

    for arguments in [Value::Null, json!({ "exclude_superseded": null })] {
        let call = tool_call("memory_list", 1, arguments.clone());
        let (status, resp) = post(&app, "/mcp", &key, call).await;
        assert!(status.is_success(), "mcp failed: {status} {resp}");
        let got = ids(&resp["result"]["structuredContent"]["results"]);
        assert!(got.contains(&live), "{arguments}: {resp}");
        assert!(
            !got.contains(&retired),
            "{arguments} bypassed the filter: {resp}"
        );
    }

    for arguments in [
        json!([]),
        json!("limit=50"),
        json!(7),
        json!({ "exclude_superseded": "false" }),
        json!({ "exclude_superseded": 0 }),
    ] {
        let call = tool_call("memory_list", 1, arguments.clone());
        let (status, resp) = post(&app, "/mcp", &key, call).await;
        assert!(status.is_success(), "mcp failed: {status} {resp}");
        assert_eq!(resp["error"]["code"], json!(-32602), "{arguments}: {resp}");
        assert!(
            resp.get("result").is_none(),
            "{arguments} was listed: {resp}"
        );
    }
}

#[tokio::test]
async fn a_corrected_memory_is_retired_and_replaced_in_the_listing() {
    let (app, _state, _tmp) = test_app_with_sharding().await;
    let key = bootstrap_admin_key(&app).await;
    let id = store(&app, &key, "the deploy port is 4100").await;

    let (status, corrected) = post(
        &app,
        "/correct",
        &key,
        json!({ "memory_id": id, "content": "the deploy port is 4200" }),
    )
    .await;
    assert!(status.is_success(), "correct failed: {status} {corrected}");
    let copy = corrected["id"].as_i64().expect("corrected id");

    let (status, original) = get(&app, &format!("/memory/{id}"), &key).await;
    assert!(status.is_success(), "get failed: {status} {original}");
    assert_eq!(original["is_superseded"], json!(true), "{original}");
    assert_eq!(original["is_latest"], json!(false), "{original}");

    let (status, body) = get(&app, "/list", &key).await;
    assert!(status.is_success(), "list failed: {status} {body}");
    assert!(
        listed(&body, id).is_none(),
        "corrected original still listed: {body}"
    );
    let row =
        listed(&body, copy).unwrap_or_else(|| panic!("correction missing from /list: {body}"));
    assert_eq!(row["parent_memory_id"], json!(id), "{row}");
    assert_eq!(row["is_superseded"], json!(false), "{row}");
}
