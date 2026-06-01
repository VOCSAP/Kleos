//! MCP tool registry and dispatcher.
//!
//! The server route table contains both daily-driver tools and a very large
//! auto-generated long tail. `registry()` intentionally exposes only the
//! daily-use surface for MCP clients, while still deriving every entry from
//! `kleos_client::ROUTES` so schemas and descriptions stay source-aligned.

use crate::App;
use kleos_client::{find_by_name, Route};
use serde_json::{json, Value};

/// The curated daily-driver tool names exposed through `tools/list`.
///
/// Canonical names and selected aliases both appear here when they are part
/// of the normal human workflow or preserve compatibility with existing MCP
/// client setups.
const DAILY_TOOL_NAMES: &[&str] = &[
    "memory.store",
    "memory_store",
    "memory.search",
    "memory_search",
    "memory_search_preset",
    "memory.get",
    "memory.list",
    "memory_list",
    "memory.recall",
    "memory_recall",
    "skill.search",
    "skill_search",
    "skill.execute",
    "skill_execute",
    "skills.find_skills",
    "skills.usage_stats",
    "activity.report",
    "tasks.list",
    "tasks.create",
    "services.chiasm_create_task",
    "tasks.feed",
    "tasks.get_task",
    "tasks.update_task",
    "tasks.update",
    "services.chiasm_update_task",
    "broca.feed",
    "axon.list_events",
    "services.axon_consume",
    "soma.list_agents",
    "soma.create_agent",
    "soma.register",
    "services.soma_register",
    "loom.list_runs",
    "thymus.get_metrics",
    "handoffs.store",
    "handoffs.dump",
    "handoffs.list",
    "handoffs.latest",
    "handoffs.search",
    "sessions.get",
    "sessions.append",
    "sessions.list_sessions",
    "sessions.create_session",
    "sessions.stream",
    "scratchpad.list",
    "scratchpad.put",
    "scratchpad.delete_key",
    "scratchpad.delete_session",
    "scratchpad.promote",
    "prompts.generate",
    "context.generate_prompt",
    "prompts.header",
    "context.get_header",
    "mcp_schema.get",
    "errors.report",
    "agents.verify",
];

/// Parse one route's schema, falling back to an object-shaped schema on bad metadata.
fn route_schema(route: &Route) -> Value {
    serde_json::from_str(route.input_schema)
        .unwrap_or_else(|_| json!({ "type": "object", "additionalProperties": true }))
}

/// Build one MCP tool entry from the chosen visible tool name and backing route metadata.
fn registry_entry(name: &str, route: &Route) -> Value {
    json!({
        "name": name,
        "description": route.description,
        "inputSchema": route_schema(route),
    })
}

/// Returns the curated tool registry as JSON objects suitable for an MCP
/// `tools/list` response.
pub fn registry() -> Vec<Value> {
    DAILY_TOOL_NAMES
        .iter()
        .filter_map(|name| {
            find_by_name(name)
                .map(|route| registry_entry(name, route))
                .or_else(|| {
                    tracing::warn!(tool = %name, "daily MCP tool is missing from route registry");
                    None
                })
        })
        .collect()
}

/// Routes an MCP tool call to the registered HTTP route. The arguments are
/// passed straight through; path templates extract the relevant fields.
///
/// Patch 33: if the args object lacks both `space` and `space_id`, try to
/// auto-inject the project's space name from the per-session file written
/// by `session-start-kleos.sh`. This lets MCP tool calls inherit the
/// session's project scope transparently (matching the convention that
/// `kleos-cli` enforces via env / cwd resolution).
#[tracing::instrument(skip(app, args), fields(name = %name))]
pub async fn dispatch(app: &App, name: &str, args: Value) -> Result<Value, String> {
    let route = find_by_name(name).ok_or_else(|| format!("unknown tool: {name}"))?;
    let args = maybe_inject_space(args);
    app.client.call_route(route, args).await
}

/// Read the per-session space file written by `session-start-kleos.sh`
/// (path: `$HOME/.kleos/sessions/$CLAUDE_SESSION_ID/space_name`). Returns
/// the trimmed first line on success, `None` otherwise.
fn read_session_space_name() -> Option<String> {
    let sid = std::env::var("CLAUDE_SESSION_ID")
        .or_else(|_| std::env::var("CLAUDE_CODE_SESSION_ID"))
        .ok()?;
    if sid.trim().is_empty() {
        return None;
    }
    let home = std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())?;
    let path = std::path::PathBuf::from(home)
        .join(".kleos")
        .join("sessions")
        .join(sid.trim())
        .join("space_name");
    let raw = std::fs::read_to_string(&path).ok()?;
    let first = raw.lines().next()?.trim().to_string();
    if first.is_empty() {
        None
    } else {
        Some(first)
    }
}

/// If `args` is a JSON object and contains neither `space` nor `space_id`,
/// inject `space = <session_space>` so the server-side
/// `normalize_space_input` resolves it. Non-object args (numbers, strings,
/// arrays) and args with an explicit space are returned untouched.
fn maybe_inject_space(mut args: Value) -> Value {
    let Some(map) = args.as_object_mut() else {
        return args;
    };
    if map.contains_key("space") || map.contains_key("space_id") {
        return args;
    }
    if let Some(name) = read_session_space_name() {
        map.insert("space".to_string(), Value::String(name));
    }
    args
}
