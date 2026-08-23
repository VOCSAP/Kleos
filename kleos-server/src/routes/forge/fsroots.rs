//! Filesystem root resolver for server-side agent-forge compute routes.
//!
//! Paths supplied by callers are only forwarded to `agent_forge` tools when
//! they resolve to a location strictly inside one of the operator-configured
//! roots. This prevents path-traversal attacks where a caller could point
//! the server at arbitrary filesystem locations.
//!
//! Configuration: `KLEOS_FORGE_FS_ROOTS` -- colon-separated list of absolute
//! directory paths that the server is permitted to read. If the variable is
//! unset or empty, `resolve_within_roots` returns `None` for every input.
use std::path::PathBuf;

/// Canonicalize `path` and return it only when it lies under one of the roots
/// listed in `KLEOS_FORGE_FS_ROOTS` (colon-separated). Returns `None` if the
/// env var is unset or empty, `path` cannot be canonicalized (does not exist,
/// bad encoding, etc.), or the canonicalized path is not under any allowed
/// root. Thin wrapper that reads the process environment once and delegates the
/// matching to [`resolve_within_roots_in`].
pub fn resolve_within_roots(path: &str) -> Option<PathBuf> {
    let roots_raw = std::env::var("KLEOS_FORGE_FS_ROOTS").ok()?;
    resolve_within_roots_in(path, &roots_raw)
}

/// Pure core of [`resolve_within_roots`]: match `path` against the
/// colon-separated `roots_raw` allow-list with no environment access, so it is
/// deterministic and safe to exercise from parallel test threads. Returns
/// `None` when `roots_raw` is blank, `path` cannot be canonicalized, or the
/// canonicalized path is not under any listed root.
fn resolve_within_roots_in(path: &str, roots_raw: &str) -> Option<PathBuf> {
    if roots_raw.trim().is_empty() {
        return None;
    }

    // Canonicalize the candidate path so symlinks and `..` are resolved before
    // the prefix comparison. If the path does not exist, reject immediately.
    let candidate = std::fs::canonicalize(path).ok()?;

    // Check each configured root: canonicalize the root itself so that both
    // sides of the comparison are fully resolved. Skip roots that do not exist.
    for root_str in roots_raw.split(':') {
        let root_str = root_str.trim();
        if root_str.is_empty() {
            continue;
        }
        let root = match std::fs::canonicalize(root_str) {
            Ok(p) => p,
            Err(_) => continue,
        };
        if candidate.starts_with(&root) {
            return Some(candidate);
        }
    }

    None
}

// Unit tests for the resolver. They drive the pure `resolve_within_roots_in`
// helper with an explicit roots list instead of mutating the process-global
// `KLEOS_FORGE_FS_ROOTS`, so they stay deterministic and parallel-safe.
// Mutating a shared env var here used to race with concurrent `getenv` from
// other tests in the same binary and failed intermittently.
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: create a real temp directory whose canonical form we can use.
    fn make_tempdir() -> TempDir {
        tempfile::tempdir().expect("tempdir creation must succeed")
    }

    /// A path that is a direct child of a configured root is allowed.
    #[test]
    fn within_root_returns_some() {
        let dir = make_tempdir();
        let root = dir.path().to_str().expect("tempdir must be valid UTF-8");

        // Create a real file inside the root so canonicalize succeeds.
        let child = dir.path().join("file.txt");
        std::fs::write(&child, b"x").unwrap();

        let result = resolve_within_roots_in(child.to_str().unwrap(), root);

        assert!(result.is_some(), "child of configured root must resolve");
    }

    /// A path outside every configured root must be rejected.
    #[test]
    fn outside_root_returns_none() {
        let allowed = make_tempdir();
        let other = make_tempdir();

        // Create a real file in `other` so canonicalize does not fail.
        let child = other.path().join("file.txt");
        std::fs::write(&child, b"x").unwrap();

        let root = allowed.path().to_str().unwrap();
        let result = resolve_within_roots_in(child.to_str().unwrap(), root);

        assert!(result.is_none(), "path outside roots must be rejected");
    }

    /// An empty (unset-equivalent) roots list must always return None.
    #[test]
    fn empty_roots_returns_none() {
        let dir = make_tempdir();
        let child = dir.path().join("file.txt");
        std::fs::write(&child, b"x").unwrap();

        let result = resolve_within_roots_in(child.to_str().unwrap(), "");

        assert!(result.is_none(), "empty roots list must yield None");
    }
}
