//! Smoke tests for kleos-mcp tool registry and MCP protocol surface.
//!
//! These tests verify the JSON-RPC envelope shape and tool registry via the
//! server-side `POST /mcp` endpoint. The server test harness provides an
//! in-memory database with auth, so each test bootstraps a key and sends
//! authenticated JSON-RPC requests.
//!
//! Live wire tests that exercise actual tool execution belong in the
//! kleos-server integration test suite, not here.

use kleos_mcp::tools::registry;

/// Build a flat list of visible tool names from the curated registry.
fn registry_names() -> Vec<String> {
    registry()
        .into_iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_owned))
        .collect()
}

/// The tool registry must include the core daily-driver tools.
#[test]
fn registry_includes_core_tools() {
    let names = registry_names();
    for required in [
        "memory.store",
        "memory.search",
        "memory.recall",
        "activity.report",
        "prompts.generate",
        "context.get_header",
        "tasks.list",
        "broca.feed",
        "soma.list_agents",
        "loom.list_runs",
        "thymus.get_metrics",
        "handoffs.store",
        "scratchpad.put",
        "skills.find_skills",
        "agents.verify",
        "mcp_schema.get",
    ] {
        assert!(
            names.iter().any(|name| name == required),
            "{required} must be in registry, got {:?}",
            names
        );
    }
}

/// The tool registry must keep important compatibility aliases for existing clients.
#[test]
fn registry_includes_daily_workflow_aliases() {
    let names = registry_names();
    for alias in [
        "memory_store",
        "memory_search",
        "memory_recall",
        "context.generate_prompt",
        "context.get_header",
        "services.chiasm_create_task",
        "tasks.update",
        "services.soma_register",
        "handoffs.dump",
    ] {
        assert!(
            names.iter().any(|name| name == alias),
            "{alias} must be in registry, got {:?}",
            names
        );
    }
}

/// The tool registry must hide the admin and generated long-tail tools by default.
#[test]
fn registry_excludes_long_tail_tools() {
    let names = registry_names();
    for excluded in [
        "admin.backfill_facts",
        "security.create_api_key",
        "graph.create_entity",
        "docs.openapi",
        "well_known.llms_txt",
        "memory.store_memory",
        "mcp_schema.dispatch",
        "tasks.create_task",
        "skills.create_skill",
    ] {
        assert!(
            names.iter().all(|name| name != excluded),
            "{excluded} must not be in registry, got {:?}",
            names
        );
    }
}
