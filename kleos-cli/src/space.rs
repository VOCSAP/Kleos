//! Patch 33 -- client-side helpers for the spaces partitioning
//! convention. The resolution logic mirrors the bash helper at
//! `hooks/full/lib-kleos-space.sh`; a parity test
//! (`tests/space-resolution-parity.sh`) keeps the two in sync.
//!
//! Resolution order for a `cwd`:
//!   1. walk up looking for a `.kleos-space` marker -> first non-comment,
//!      non-blank line, normalized.
//!   2. walk up looking for `.git/` -> basename of its parent, normalized.
//!   3. basename of cwd, normalized.
//!
//! Names are normalized via [`normalize_space_name`]: lowercase, trimmed,
//! only `[a-z0-9_-]` retained.
//!
//! The marker file is IMMUTABLE once created (CONST VIOLATION rule by the
//! operator). The CLI never rewrites an existing `.kleos-space`.

use std::path::{Path, PathBuf};

/// Max number of parent directories walked when searching for marker /
/// .git. Defends against pathological fs loops.
const MAX_WALK_UP: usize = 50;

/// Normalize a free-form space name to the canonical form (lowercase,
/// trimmed, only `[a-z0-9_-]` retained). Identical implementation to
/// `kleos_lib::space::normalize_space_name` and the bash helper.
pub fn normalize_space_name(input: &str) -> String {
    input
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect()
}

/// Walk up from `start` looking for `needle`. Returns the absolute path
/// of the first hit, or `None` if not found within `MAX_WALK_UP` levels.
fn find_walk_up(start: &Path, needle: &str) -> Option<PathBuf> {
    let mut cur = start.canonicalize().ok().or_else(|| Some(start.to_path_buf()))?;
    for _ in 0..MAX_WALK_UP {
        let candidate = cur.join(needle);
        if candidate.exists() {
            return Some(candidate);
        }
        match cur.parent() {
            Some(parent) if parent != cur => cur = parent.to_path_buf(),
            _ => return None,
        }
    }
    None
}

/// Resolve the project's space name from `cwd` using the marker -> git ->
/// cwd cascade. Returns `Some(name)` in all cases except when the cwd
/// itself has no valid basename (rare).
pub fn resolve_project_name(cwd: &Path) -> Option<String> {
    if let Some(marker) = find_walk_up(cwd, ".kleos-space") {
        if let Ok(content) = std::fs::read_to_string(&marker) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                let n = normalize_space_name(trimmed);
                if !n.is_empty() {
                    return Some(n);
                }
            }
        }
    }
    if let Some(git_dir) = find_walk_up(cwd, ".git") {
        if let Some(parent) = git_dir.parent() {
            if let Some(name) = parent.file_name().and_then(|s| s.to_str()) {
                let n = normalize_space_name(name);
                if !n.is_empty() {
                    return Some(n);
                }
            }
        }
    }
    cwd.file_name()
        .and_then(|s| s.to_str())
        .map(normalize_space_name)
        .filter(|s| !s.is_empty())
}

/// Compute the `(space_id, space)` pair to inject into a request body
/// for a given invocation. Hierarchy (first hit wins):
///
///   1. `--no-space`           -> `(None, Some("default"))` (force server alias)
///   2. `--space-id <N>`       -> `(Some(N), None)`
///   3. `--space <name>`       -> `(None, Some(name))`
///   4. `$KLEOS_SPACE` env var -> `(None, Some(env))`
///   5. cwd resolution         -> `(None, Some(resolved))`
///   6. no resolution          -> `(None, None)` (server defaults)
pub fn determine_space_for_request(
    no_space: bool,
    space_flag: Option<&str>,
    space_id_flag: Option<i64>,
) -> (Option<i64>, Option<String>) {
    if no_space {
        return (None, Some("default".to_string()));
    }
    if let Some(id) = space_id_flag {
        return (Some(id), None);
    }
    if let Some(name) = space_flag {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            return (None, Some(trimmed.to_string()));
        }
    }
    if let Ok(env) = std::env::var("KLEOS_SPACE") {
        let trimmed = env.trim();
        if !trimmed.is_empty() {
            return (None, Some(trimmed.to_string()));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(name) = resolve_project_name(&cwd) {
            return (None, Some(name));
        }
    }
    (None, None)
}

/// Inject the resolved space into a JSON body in place. Skips when both
/// outputs are `None` (server will default appropriately).
pub fn inject_space_into_body(
    body: &mut serde_json::Value,
    space_id: Option<i64>,
    space: Option<String>,
) {
    let serde_json::Value::Object(map) = body else { return };
    if let Some(id) = space_id {
        map.insert("space_id".to_string(), serde_json::json!(id));
    }
    if let Some(name) = space {
        map.insert("space".to_string(), serde_json::json!(name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_matches_lib() {
        assert_eq!(normalize_space_name("Kleos VOCSAP"), "kleosvocsap");
        assert_eq!(normalize_space_name(" Foo-Bar_42 "), "foo-bar_42");
        assert_eq!(normalize_space_name("a/b\\c"), "abc");
        assert_eq!(normalize_space_name(""), "");
    }

    #[test]
    fn determine_no_space_takes_precedence() {
        let (id, name) = determine_space_for_request(true, Some("kleos"), Some(42));
        assert_eq!(id, None);
        assert_eq!(name.as_deref(), Some("default"));
    }

    #[test]
    fn determine_space_id_beats_name() {
        let (id, name) = determine_space_for_request(false, Some("kleos"), Some(42));
        assert_eq!(id, Some(42));
        assert_eq!(name, None);
    }

    #[test]
    fn determine_explicit_name_used() {
        let (id, name) = determine_space_for_request(false, Some("kleos"), None);
        assert_eq!(id, None);
        assert_eq!(name.as_deref(), Some("kleos"));
    }
}
