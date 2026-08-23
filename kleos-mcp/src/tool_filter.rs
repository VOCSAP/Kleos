//! `tools/list` response filtering: a global exclusion overridable by a
//! per-project inclusion.
//!
//! This is NOT an authorization boundary. `dispatch_tool` (server-side) never
//! consults `registry()` or this filter, so a tool removed from `tools/list`
//! remains callable by name if a client already knows it (see
//! `docs/dev-notes/mcp-tool-filtering-design-todo.md` section 5.5). This
//! module only controls the *advertised* surface, to bound the context-budget
//! cost of tool definitions injected into a session. That is also why it
//! fails OPEN on any load/parse error: a fail-closed filter of a surface that
//! buys no security would only ever produce the worst outcome (a stray comma
//! erasing the entire Kleos MCP surface for every session on the host) for
//! zero benefit.
//!
//! Lives in the `kleos-mcp` bridge rather than `kleos-server` deliberately:
//! the server is shared across every project and host, while the bridge is a
//! per-project process that already resolves `KLEOS_SPACE` (see
//! `inject_space_for_tool_calls` in `lib.rs`), so it is the only place able to
//! carry a per-project override.
//!
//! Cascade (highest priority first), mirroring the `prompts-overrides` /
//! `lexicon-overrides` pattern minus their TTL cache -- pointless here since
//! the server never advertises `listChanged` and a client therefore never
//! re-queries `tools/list` within a session:
//! 1. `KLEOS_MCP_TOOLS_CONFIG` (explicit path) -- wins alone if set.
//! 2. `${KLEOS_DATA_DIR}/mcp-tools/<KLEOS_SPACE>.toml` if it exists -- unioned
//!    with `default.toml` only when the project file sets `inherit = true`.
//! 3. `${KLEOS_DATA_DIR}/mcp-tools/default.toml` if it exists.
//! 4. No filtering: the embedded `registry()` list passes through unchanged.
//!
//! `inherit` is only ever read from the project-scoped file. Setting
//! `inherit = true` inside `default.toml` itself is inert: there is nothing
//! above `default.toml` in the cascade for it to inherit from.

use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// One override file's filtering mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum FilterMode {
    /// `tools` is the exhaustive allowlist of what may be advertised.
    Allow,
    /// `tools` is removed from the embedded list; everything else passes.
    Deny,
}

/// Deserialized shape of one `mcp-tools/*.toml` file. Unknown TOML keys
/// (e.g. `schema_version`, reserved for future use) are ignored by serde's
/// default behavior rather than rejected.
#[derive(Debug, Deserialize)]
struct RawConfig {
    mode: FilterMode,
    #[serde(default)]
    tools: Vec<String>,
    #[serde(default)]
    inherit: bool,
}

/// One loaded and normalized override file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ToolFilter {
    mode: FilterMode,
    /// Normalized (`.` -> `_`) tool names from the config file.
    tools: HashSet<String>,
    inherit: bool,
}

/// Normalizes a config-file tool name the same way `registry()` normalizes
/// advertised names (`kleos-mcp/src/tools.rs:156`), so `memory.search` and
/// `memory_search` are always the same config key.
fn normalize(name: &str) -> String {
    name.replace('.', "_")
}

impl ToolFilter {
    fn from_raw(raw: RawConfig) -> Self {
        Self {
            mode: raw.mode,
            tools: raw.tools.iter().map(|t| normalize(t)).collect(),
            inherit: raw.inherit,
        }
    }

    /// Whether `normalized_name` (already normalized) survives this filter
    /// alone.
    fn allows(&self, normalized_name: &str) -> bool {
        match self.mode {
            FilterMode::Allow => self.tools.contains(normalized_name),
            FilterMode::Deny => !self.tools.contains(normalized_name),
        }
    }
}

/// One or more `ToolFilter`s in effect together. More than one entry only
/// happens under `inherit = true` (project file unioned with `default.toml`).
///
/// Union is defined at the tool-visibility level -- a tool survives if at
/// least one filter would show it -- rather than by merging the two `tools`
/// sets, which stays well-defined even when the project and default files
/// use different `mode`s. But a plain OR-of-any is not enough on its own:
/// it would let an `inherit = true` allowlist, or even another deny list,
/// silently resurrect a tool a `deny` filter elsewhere in the set was hiding
/// (union of two denies is their UNION, i.e. what's hidden shrinks to the
/// intersection of their lists -- backwards from what an operator reads in
/// the word "deny"). `deny` must stay subtractive end-to-end regardless of
/// inheritance, so every `Deny` filter in the set gets an additional
/// unanimous veto on top of the OR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveFilter(Vec<ToolFilter>);

impl EffectiveFilter {
    fn allows(&self, normalized_name: &str) -> bool {
        self.0.iter().any(|f| f.allows(normalized_name))
            && self
                .0
                .iter()
                .all(|f| f.mode != FilterMode::Deny || f.allows(normalized_name))
    }

    /// Every distinct configured tool name across all filters in effect,
    /// used to warn about config entries that match nothing advertised.
    fn configured_tools(&self) -> HashSet<&str> {
        self.0
            .iter()
            .flat_map(|f| f.tools.iter().map(String::as_str))
            .collect()
    }
}

/// Reads `KLEOS_DATA_DIR` (or the legacy `ENGRAM_DATA_DIR` alias) and returns
/// `<dir>/mcp-tools`, matching the convention used by the lexicon and prompts
/// overlay channels. `None` when neither env var is set.
fn mcp_tools_dir() -> Option<PathBuf> {
    for env in ["KLEOS_DATA_DIR", "ENGRAM_DATA_DIR"] {
        if let Some(raw) = std::env::var_os(env) {
            let p = PathBuf::from(raw);
            if !p.as_os_str().is_empty() {
                return Some(p.join("mcp-tools"));
            }
        }
    }
    None
}

/// Loads and parses one override file. `Ok(None)` means the file does not
/// exist, which is not an error (callers fall through the cascade silently).
/// `Err` carries a human-readable message for the caller to fold into a
/// fail-open warning.
fn load_file(path: &Path) -> Result<Option<ToolFilter>, String> {
    if !path.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    let parsed: RawConfig =
        toml::from_str(&raw).map_err(|e| format!("failed to parse {}: {e}", path.display()))?;
    Ok(Some(ToolFilter::from_raw(parsed)))
}

/// Pure core of the cascade: given an explicit override path, the resolved
/// `mcp-tools` directory, and the current project space, resolves the
/// effective filter. Env-free so it is directly testable against a tempdir.
///
/// Returns `(None, warnings)` when nothing resolves (no filtering applies)
/// and `(Some(filter), warnings)` otherwise. Never "fails" in the Rust
/// sense -- every error becomes a warning string and the cascade degrades to
/// the next step (or to no filtering), consistent with the fail-open
/// contract.
fn resolve_filter(
    dir: Option<&Path>,
    explicit: Option<&Path>,
    space: Option<&str>,
) -> (Option<EffectiveFilter>, Vec<String>) {
    let mut warnings = Vec::new();

    // Step 1: explicit path wins alone, does not fall through on failure.
    if let Some(explicit) = explicit {
        return match load_file(explicit) {
            Ok(Some(f)) => (Some(EffectiveFilter(vec![f])), warnings),
            Ok(None) => {
                warnings.push(format!(
                    "KLEOS_MCP_TOOLS_CONFIG={} not found, no MCP tool filtering applied",
                    explicit.display()
                ));
                (None, warnings)
            }
            Err(e) => {
                warnings.push(format!("{e}, no MCP tool filtering applied"));
                (None, warnings)
            }
        };
    }

    let Some(dir) = dir else {
        return (None, warnings);
    };
    let default_path = dir.join("default.toml");

    // Step 2: project-scoped file, if a non-blank space is known.
    let space = space.map(str::trim).filter(|s| !s.is_empty());
    if let Some(space) = space {
        let project_path = dir.join(format!("{space}.toml"));
        match load_file(&project_path) {
            Ok(Some(project_filter)) => {
                if !project_filter.inherit {
                    return (Some(EffectiveFilter(vec![project_filter])), warnings);
                }
                return match load_file(&default_path) {
                    Ok(Some(default_filter)) => (
                        Some(EffectiveFilter(vec![project_filter, default_filter])),
                        warnings,
                    ),
                    // inherit=true but nothing to inherit from: not an error.
                    Ok(None) => (Some(EffectiveFilter(vec![project_filter])), warnings),
                    Err(e) => {
                        warnings.push(format!("{e}, ignoring inherit=true"));
                        (Some(EffectiveFilter(vec![project_filter])), warnings)
                    }
                };
            }
            Ok(None) => {
                // No project file: fall through to step 3 below.
            }
            Err(e) => {
                warnings.push(format!("{e}, falling back to default.toml"));
                // Fall through to step 3 below.
            }
        }
    }

    // Step 3: global default, if present.
    match load_file(&default_path) {
        Ok(Some(f)) => (Some(EffectiveFilter(vec![f])), warnings),
        Ok(None) => (None, warnings),
        Err(e) => {
            warnings.push(format!("{e}, no MCP tool filtering applied"));
            (None, warnings)
        }
    }
}

/// Env-facing wrapper around [`resolve_filter`]: reads `KLEOS_MCP_TOOLS_CONFIG`,
/// the `mcp-tools` directory, and `KLEOS_SPACE` (the same var
/// `inject_space_for_tool_calls` already reads in `lib.rs`).
fn load_effective_filter() -> (Option<EffectiveFilter>, Vec<String>) {
    let explicit = std::env::var_os("KLEOS_MCP_TOOLS_CONFIG").map(PathBuf::from);
    let dir = mcp_tools_dir();
    let space = std::env::var("KLEOS_SPACE").ok();
    resolve_filter(dir.as_deref(), explicit.as_deref(), space.as_deref())
}

/// Filters the `result.tools` array of a `tools/list` response in place.
/// Returns the filtered response and any "unknown tool" warnings for config
/// entries that matched no currently-advertised tool (the anti-typo net
/// promised by the fail-open contract).
fn apply(mut resp: Value, filter: &EffectiveFilter) -> (Value, Vec<String>) {
    let mut warnings = Vec::new();
    let Some(tools) = resp.pointer_mut("/result/tools").and_then(Value::as_array_mut) else {
        return (resp, warnings);
    };

    let advertised: HashSet<String> = tools
        .iter()
        .filter_map(|t| t.get("name").and_then(Value::as_str).map(str::to_string))
        .collect();
    for configured in filter.configured_tools() {
        if !advertised.contains(configured) {
            warnings.push(format!("unknown tool in mcp-tools config: {configured}"));
        }
    }

    tools.retain(|t| {
        // Fail open: a tool entry with no `name` (or a non-string one) is
        // something this module cannot classify, not something it can rule
        // out -- consistent with the rest of the module's fail-open
        // contract, keep it rather than silently dropping it.
        t.get("name")
            .and_then(Value::as_str)
            .map_or(true, |n| filter.allows(n))
    });

    (resp, warnings)
}

/// Single public entry point: filters a `tools/list` JSON-RPC response
/// according to the resolved cascade, logging any fail-open warning. Detects
/// a `tools/list` response structurally (presence of a `result.tools` array)
/// rather than by tracking the request method, so the call site in
/// `handle_jsonrpc` stays a one-line, method-agnostic post-processing step;
/// every other response shape (including errors) passes through unchanged.
pub fn filter_tools_list_response(resp: Value) -> Value {
    if resp.pointer("/result/tools").and_then(Value::as_array).is_none() {
        return resp;
    }
    let (filter, load_warnings) = load_effective_filter();
    for w in &load_warnings {
        tracing::warn!("mcp tool filter: {w}");
    }
    let Some(filter) = filter else {
        return resp;
    };
    let (resp, unknown_warnings) = apply(resp, &filter);
    for w in &unknown_warnings {
        tracing::warn!("mcp tool filter: {w}");
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn write(dir: &Path, name: &str, contents: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    fn tools_list_response(names: &[&str]) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "tools": names.iter().map(|n| json!({"name": n, "description": "d", "inputSchema": {}})).collect::<Vec<_>>()
            }
        })
    }

    #[test]
    fn normalizes_dot_and_underscore_aliases_the_same() {
        assert_eq!(normalize("memory.search"), "memory_search");
        assert_eq!(normalize("memory_search"), "memory_search");
    }

    #[test]
    fn mode_allow_keeps_only_listed_tools() {
        let dir = tempdir().unwrap();
        write(dir.path(), "default.toml", "mode = \"allow\"\ntools = [\"memory_search\"]\n");
        let (filter, warnings) = resolve_filter(Some(dir.path()), None, None);
        assert!(warnings.is_empty());
        let filter = filter.expect("filter must resolve");
        let (resp, unknown) = apply(
            tools_list_response(&["memory_search", "memory_store"]),
            &filter,
        );
        assert!(unknown.is_empty());
        let names: Vec<&str> = resp["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["memory_search"]);
    }

    #[test]
    fn mode_deny_removes_only_listed_tools() {
        let dir = tempdir().unwrap();
        write(dir.path(), "default.toml", "mode = \"deny\"\ntools = [\"memory_store\"]\n");
        let (filter, _) = resolve_filter(Some(dir.path()), None, None);
        let filter = filter.expect("filter must resolve");
        let (resp, _) = apply(
            tools_list_response(&["memory_search", "memory_store"]),
            &filter,
        );
        let names: Vec<&str> = resp["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["memory_search"]);
    }

    #[test]
    fn config_dot_alias_matches_underscore_advertised_name() {
        let dir = tempdir().unwrap();
        write(dir.path(), "default.toml", "mode = \"allow\"\ntools = [\"memory.search\"]\n");
        let (filter, _) = resolve_filter(Some(dir.path()), None, None);
        let filter = filter.expect("filter must resolve");
        let (resp, unknown) = apply(tools_list_response(&["memory_search"]), &filter);
        assert!(unknown.is_empty(), "dot-form config entry must match underscore-form advertised name");
        assert_eq!(resp["result"]["tools"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn project_file_replaces_default_when_inherit_is_false() {
        let dir = tempdir().unwrap();
        write(dir.path(), "default.toml", "mode = \"allow\"\ntools = [\"memory_search\"]\n");
        write(
            dir.path(),
            "myproj.toml",
            "mode = \"allow\"\ntools = [\"forge_spec_task\"]\n",
        );
        let (filter, _) = resolve_filter(Some(dir.path()), None, Some("myproj"));
        let filter = filter.expect("filter must resolve");
        let (resp, _) = apply(
            tools_list_response(&["memory_search", "forge_spec_task"]),
            &filter,
        );
        let names: Vec<&str> = resp["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["forge_spec_task"], "project file must replace default, not intersect");
    }

    #[test]
    fn deny_survives_inherit_project_deny_over_default_deny() {
        let dir = tempdir().unwrap();
        write(dir.path(), "default.toml", "mode = \"deny\"\ntools = [\"memory_store\"]\n");
        write(
            dir.path(),
            "myproj.toml",
            "mode = \"deny\"\ninherit = true\ntools = [\"forge_spec_task\"]\n",
        );
        let (filter, _) = resolve_filter(Some(dir.path()), None, Some("myproj"));
        let filter = filter.expect("filter must resolve");
        let (resp, _) = apply(
            tools_list_response(&["memory_search", "memory_store", "forge_spec_task"]),
            &filter,
        );
        let mut names: Vec<&str> = resp["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        names.sort_unstable();
        assert_eq!(
            names,
            vec!["memory_search"],
            "both deny lists must stay hidden under inherit -- union of two denies is their union, not their intersection"
        );
    }

    #[test]
    fn deny_survives_inherit_project_allow_over_default_deny() {
        let dir = tempdir().unwrap();
        write(dir.path(), "default.toml", "mode = \"deny\"\ntools = [\"memory_store\"]\n");
        write(
            dir.path(),
            "myproj.toml",
            "mode = \"allow\"\ninherit = true\ntools = [\"memory_store\", \"forge_spec_task\"]\n",
        );
        let (filter, _) = resolve_filter(Some(dir.path()), None, Some("myproj"));
        let filter = filter.expect("filter must resolve");
        let (resp, _) = apply(
            tools_list_response(&["memory_search", "memory_store", "forge_spec_task"]),
            &filter,
        );
        let names: Vec<&str> = resp["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(
            !names.contains(&"memory_store"),
            "a project allowlist under inherit must not be able to resurrect a globally-denied tool: got {names:?}"
        );
        assert!(names.contains(&"forge_spec_task"));
    }

    #[test]
    fn globally_denied_tool_stays_invisible_regardless_of_project_config() {
        let filter = EffectiveFilter(vec![
            ToolFilter {
                mode: FilterMode::Allow,
                tools: ["forge_spec_task".to_string()].into_iter().collect(),
                inherit: true,
            },
            ToolFilter {
                mode: FilterMode::Deny,
                tools: ["memory_store".to_string()].into_iter().collect(),
                inherit: false,
            },
        ]);
        assert!(!filter.allows("memory_store"), "deny must win regardless of any allow filter in the same set");
        assert!(filter.allows("forge_spec_task"));
    }

    #[test]
    fn inherit_true_unions_project_and_default() {
        let dir = tempdir().unwrap();
        write(dir.path(), "default.toml", "mode = \"allow\"\ntools = [\"memory_search\"]\n");
        write(
            dir.path(),
            "myproj.toml",
            "mode = \"allow\"\ninherit = true\ntools = [\"forge_spec_task\"]\n",
        );
        let (filter, _) = resolve_filter(Some(dir.path()), None, Some("myproj"));
        let filter = filter.expect("filter must resolve");
        let (resp, _) = apply(
            tools_list_response(&["memory_search", "forge_spec_task", "handoffs_search"]),
            &filter,
        );
        let mut names: Vec<&str> = resp["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        names.sort_unstable();
        assert_eq!(names, vec!["forge_spec_task", "memory_search"]);
    }

    #[test]
    fn missing_config_file_fails_open_without_warning() {
        let dir = tempdir().unwrap();
        // No files written at all.
        let (filter, warnings) = resolve_filter(Some(dir.path()), None, None);
        assert!(filter.is_none());
        assert!(
            warnings.is_empty(),
            "an absent optional file is not an error worth warning about"
        );
    }

    #[test]
    fn malformed_toml_fails_open_with_warning() {
        let dir = tempdir().unwrap();
        write(dir.path(), "default.toml", "this is not valid toml {{{");
        let (filter, warnings) = resolve_filter(Some(dir.path()), None, None);
        assert!(filter.is_none(), "malformed config must fail open, not filter anything");
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("failed to parse"));
    }

    #[test]
    fn explicit_config_missing_does_not_fall_through_to_cascade() {
        let dir = tempdir().unwrap();
        write(dir.path(), "default.toml", "mode = \"allow\"\ntools = [\"memory_search\"]\n");
        let explicit = dir.path().join("does-not-exist.toml");
        let (filter, warnings) = resolve_filter(Some(dir.path()), Some(&explicit), None);
        assert!(
            filter.is_none(),
            "an explicit but missing KLEOS_MCP_TOOLS_CONFIG must not silently fall back to default.toml"
        );
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("not found"));
    }

    #[test]
    fn unknown_tool_in_config_warns_but_still_filters() {
        let dir = tempdir().unwrap();
        write(
            dir.path(),
            "default.toml",
            "mode = \"allow\"\ntools = [\"memory_search\", \"totally_made_up\"]\n",
        );
        let (filter, _) = resolve_filter(Some(dir.path()), None, None);
        let filter = filter.expect("filter must resolve");
        let (resp, unknown) = apply(tools_list_response(&["memory_search"]), &filter);
        assert_eq!(unknown, vec!["unknown tool in mcp-tools config: totally_made_up"]);
        assert_eq!(resp["result"]["tools"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn malformed_tool_entry_fails_open_and_is_kept() {
        let dir = tempdir().unwrap();
        write(dir.path(), "default.toml", "mode = \"deny\"\ntools = [\"memory_store\"]\n");
        let (filter, _) = resolve_filter(Some(dir.path()), None, None);
        let filter = filter.expect("filter must resolve");
        let resp = json!({
            "jsonrpc": "2.0", "id": 1,
            "result": { "tools": [
                {"description": "no name field at all"},
                {"name": 42, "description": "name is not a string"},
                {"name": "memory_search", "description": "normal entry"},
            ]}
        });
        let (resp, _) = apply(resp, &filter);
        assert_eq!(
            resp["result"]["tools"].as_array().unwrap().len(),
            3,
            "entries this module cannot classify must be kept (fail open), not dropped"
        );
    }

    #[test]
    fn non_tools_list_response_passes_through_unchanged() {
        let resp = json!({"jsonrpc": "2.0", "id": 1, "result": {"ok": true}});
        assert_eq!(filter_tools_list_response(resp.clone()), resp);
    }

    #[test]
    fn blank_space_behaves_like_unset_space() {
        let dir = tempdir().unwrap();
        write(dir.path(), "default.toml", "mode = \"allow\"\ntools = [\"memory_search\"]\n");
        let (filter, _) = resolve_filter(Some(dir.path()), None, Some("   "));
        assert!(filter.is_some(), "blank space must fall through to default.toml");
    }
}
