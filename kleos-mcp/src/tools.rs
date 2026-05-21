//! MCP tool registry and dispatcher.
//!
//! All tools are derived from `kleos_client::ROUTES`. `registry()` emits one
//! `tools/list` entry per canonical name plus each alias; `dispatch()` looks
//! up the route and forwards the call through `app.client.call_route(...)`.

use crate::App;
use kleos_client::{find_by_name, ROUTES};
use serde_json::{json, Value};

/// Env var holding a CSV of glob-lite patterns. When set and non-empty,
/// `registry()` only emits routes whose canonical name matches at least one
/// pattern. `dispatch()` is intentionally left permissive so back-compat
/// callers can still address any route by name; the server remains the
/// authoritative enforcer via its scope check. See
/// `docs/dev-notes/local-patches.md` Patch 18.
const ALLOWLIST_ENV: &str = "KLEOS_MCP_TOOL_ALLOWLIST";

/// Reads `KLEOS_MCP_TOOL_ALLOWLIST` and returns the parsed patterns. Returns
/// `None` when the env var is absent or empty (= upstream behavior, all
/// routes exposed). Empty entries after splitting are dropped.
fn parse_allowlist() -> Option<Vec<String>> {
    let raw = std::env::var(ALLOWLIST_ENV).ok()?;
    let patterns: Vec<String> = raw
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if patterns.is_empty() {
        None
    } else {
        Some(patterns)
    }
}

/// Matches a canonical tool `name` against a single glob-lite `pattern`:
/// - `*` alone matches everything;
/// - `<prefix>.*` matches when `name` starts with `<prefix>.`;
/// - anything else is matched exactly.
fn matches_pattern(name: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix(".*") {
        return name == prefix || name.starts_with(&format!("{prefix}."));
    }
    name == pattern
}

/// True when `name` matches at least one allowlist pattern, or when the
/// allowlist is empty (= upstream behavior).
fn allowed(name: &str, allowlist: &Option<Vec<String>>) -> bool {
    match allowlist {
        None => true,
        Some(patterns) => patterns.iter().any(|p| matches_pattern(name, p)),
    }
}

/// Returns the full tool registry as JSON objects suitable for an MCP
/// `tools/list` response. Each route yields one entry per canonical name
/// plus one entry per alias (back-compat).
pub fn registry() -> Vec<Value> {
    let allowlist = parse_allowlist();
    let mut out = Vec::with_capacity(ROUTES.len() * 2);
    for route in ROUTES {
        if !allowed(route.name, &allowlist) {
            continue;
        }
        let schema: Value = serde_json::from_str(route.input_schema)
            .unwrap_or_else(|_| json!({ "type": "object", "additionalProperties": true }));
        out.push(json!({
            "name": route.name,
            "description": route.description,
            "inputSchema": schema,
        }));
        for alias in route.aliases {
            let schema_clone: Value = serde_json::from_str(route.input_schema)
                .unwrap_or_else(|_| json!({ "type": "object", "additionalProperties": true }));
            out.push(json!({
                "name": alias,
                "description": route.description,
                "inputSchema": schema_clone,
            }));
        }
    }
    out
}

/// Routes an MCP tool call to the registered HTTP route. The arguments are
/// passed straight through; path templates extract the relevant fields.
#[tracing::instrument(skip(app, args), fields(name = %name))]
pub async fn dispatch(app: &App, name: &str, args: Value) -> Result<Value, String> {
    let route = find_by_name(name).ok_or_else(|| format!("unknown tool: {name}"))?;
    app.client.call_route(route, args).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes tests that mutate the process-wide env, since `cargo test`
    /// runs unit tests in parallel by default.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn matches_exact() {
        assert!(matches_pattern("memory.store", "memory.store"));
        assert!(!matches_pattern("memory.store", "memory.recall"));
        assert!(!matches_pattern("memory.store", "memory"));
    }

    #[test]
    fn matches_suffix_wildcard() {
        assert!(matches_pattern("memory.store", "memory.*"));
        assert!(matches_pattern("memory.mark_forgotten", "memory.*"));
        // Bare prefix also matches the dotted form, but does not match an
        // unrelated route that happens to start with the same letters.
        assert!(matches_pattern("memory", "memory.*"));
        assert!(!matches_pattern("memories.recall", "memory.*"));
        assert!(!matches_pattern("memorystore", "memory.*"));
    }

    #[test]
    fn matches_star_alone() {
        assert!(matches_pattern("anything", "*"));
        assert!(matches_pattern("memory.store", "*"));
    }

    #[test]
    fn allowed_with_empty_allowlist_is_permissive() {
        assert!(allowed("memory.store", &None));
        assert!(allowed("admin.reset", &None));
    }

    #[test]
    fn allowed_with_patterns_filters() {
        let patterns = Some(vec!["memory.*".to_string(), "context.build".to_string()]);
        assert!(allowed("memory.store", &patterns));
        assert!(allowed("memory.recall", &patterns));
        assert!(allowed("context.build", &patterns));
        assert!(!allowed("context.build_stream", &patterns));
        assert!(!allowed("admin.reset", &patterns));
    }

    #[test]
    fn parse_allowlist_handles_whitespace_and_empties() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var(ALLOWLIST_ENV, " memory.* , , context.build , ");
        let p = parse_allowlist().expect("expected Some");
        assert_eq!(p, vec!["memory.*".to_string(), "context.build".to_string()]);
        std::env::remove_var(ALLOWLIST_ENV);
        assert!(parse_allowlist().is_none());
        std::env::set_var(ALLOWLIST_ENV, "");
        assert!(parse_allowlist().is_none());
        std::env::remove_var(ALLOWLIST_ENV);
    }

    #[test]
    fn registry_includes_aliases_only_for_allowed_canonicals() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::env::set_var(ALLOWLIST_ENV, "memory.store");
        let r = registry();
        let names: Vec<&str> = r
            .iter()
            .filter_map(|v| v.get("name").and_then(|n| n.as_str()))
            .collect();
        // memory.store canonical + its alias memory_store must be present.
        assert!(names.contains(&"memory.store"));
        assert!(names.contains(&"memory_store"));
        // memory.recall (canonical not allowlisted) and its alias must be absent.
        assert!(!names.contains(&"memory.recall"));
        assert!(!names.contains(&"memory_recall"));
        std::env::remove_var(ALLOWLIST_ENV);
    }
}
