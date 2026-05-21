// ============================================================================
// Gate -- Cascade operator-first pour patterns (Patch 19b).
//
// Loader generique avec cascade exclusive : fichier dedie > env var > defaults
// Rust. Premier niveau present gagne (presence = priorite, MEME vide).
//
// Utilise pour deux familles de patterns gate :
//   - `blocked_patterns`            : matche -> requete bloquee
//   - `require_approval_patterns`   : matche -> requires_approval=true
//                                              (status DB `pending_approval`)
//
// Format fichier (commun aux deux familles) :
//   - 1 pattern par ligne, `contains` case-insensitive (semantique preservee)
//   - Lignes commencant par `#` ignorees (commentaires libres)
//   - Lignes blanches ignorees
//   - `trim()` applique avant comparaison
//
// Semantique vide :
//   - Fichier present mais 0 ligne non-commentaire -> `Vec::new()` retournee,
//     cascade s'arrete au fichier (l'operateur a explicitement decide de
//     desactiver ce niveau).
//   - Env var set a chaine vide -> `Vec::new()` retournee, cascade s'arrete a
//     l'env (semantique parallele).
//
// Cache TTL 5s par chemin de fichier (parite avec
// `kleos-lib/src/llm/prompts.rs::TTL_SECS` pose par Patch 17c). Le cache evite
// la relecture I/O sur appels rapproches mais reste hot-tunable sous 5s.
// ============================================================================

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

const TTL_SECS: u64 = 5;

struct CacheEntry {
    patterns: Vec<String>,
    checked_at: Instant,
}

fn cache() -> &'static RwLock<HashMap<PathBuf, CacheEntry>> {
    static CACHE: OnceLock<RwLock<HashMap<PathBuf, CacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Cascade exclusive: file > env > defaults (first present wins).
///
/// - `file = Some(path)` AND path exists -> file contents win (even if empty
///   after parsing, meaning the operator explicitly set zero patterns).
/// - `file = Some(path)` AND path missing -> fall through to env.
/// - `env = Some(s)` -> CSV split (empty string yields `Vec::new()`).
/// - otherwise -> clone `defaults`.
pub fn load(file: Option<&Path>, env: Option<&str>, defaults: &[String]) -> Vec<String> {
    if let Some(path) = file {
        if let Some(patterns) = load_file_cached(path) {
            return patterns;
        }
    }
    if let Some(s) = env {
        return parse_csv(s);
    }
    defaults.to_vec()
}

fn parse_csv(s: &str) -> Vec<String> {
    s.split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

fn parse_lines(raw: &str) -> Vec<String> {
    raw.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect()
}

/// Load `path` through the TTL cache. Returns `None` only when the file does
/// not exist (or is unreadable), so the caller can fall through to env.
fn load_file_cached(path: &Path) -> Option<Vec<String>> {
    // Fast path: cache hit within TTL window.
    {
        if let Ok(cache_g) = cache().read() {
            if let Some(entry) = cache_g.get(path) {
                if entry.checked_at.elapsed() < Duration::from_secs(TTL_SECS) {
                    return Some(entry.patterns.clone());
                }
            }
        }
    }
    // Slow path: re-read from disk.
    let raw = std::fs::read_to_string(path).ok()?;
    let patterns = parse_lines(&raw);
    if let Ok(mut w) = cache().write() {
        w.insert(
            path.to_path_buf(),
            CacheEntry {
                patterns: patterns.clone(),
                checked_at: Instant::now(),
            },
        );
    }
    Some(patterns)
}

/// Test-only helper: drop any cached entry for `path` so a subsequent `load`
/// re-reads from disk. Used by tests that mutate a fixture file between calls.
#[cfg(test)]
fn invalidate_cache_for(path: &Path) {
    if let Ok(mut w) = cache().write() {
        w.remove(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_file(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("tmp file");
        f.write_all(content.as_bytes()).expect("write");
        f.flush().expect("flush");
        f
    }

    #[test]
    fn load_returns_defaults_when_no_overrides() {
        let defaults = vec!["rm -rf /".to_string(), "shutdown".to_string()];
        let got = load(None, None, &defaults);
        assert_eq!(got, defaults);
    }

    #[test]
    fn load_file_wins_over_env_and_defaults() {
        let f = write_file("apt install\ndocker rm\n");
        let env = "ignored,csv";
        let defaults = vec!["also-ignored".to_string()];
        invalidate_cache_for(f.path());
        let got = load(Some(f.path()), Some(env), &defaults);
        assert_eq!(got, vec!["apt install".to_string(), "docker rm".to_string()]);
    }

    #[test]
    fn load_empty_file_is_intentional_zero_patterns() {
        // Operator created the file deliberately (touch). Comments only also
        // counts as "intentional zero".
        let f = write_file("# Patterns require_approval\n# (empty on purpose)\n");
        invalidate_cache_for(f.path());
        let env = "would,not,reach,here";
        let defaults = vec!["nor-defaults".to_string()];
        let got = load(Some(f.path()), Some(env), &defaults);
        assert!(got.is_empty(), "file present (even empty) must skip env+defaults, got {got:?}");
    }

    #[test]
    fn load_missing_file_falls_through_to_env() {
        let nonexistent = PathBuf::from("/nonexistent/path/that/does/not/exist.txt");
        let got = load(Some(&nonexistent), Some("from-env"), &["from-defaults".to_string()]);
        assert_eq!(got, vec!["from-env".to_string()]);
    }

    #[test]
    fn load_env_wins_when_no_file_arg() {
        let got = load(None, Some("a, b ,c"), &["default".to_string()]);
        assert_eq!(got, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }

    #[test]
    fn load_empty_env_is_intentional_zero_patterns() {
        // Env var set to "" -> CSV split yields one empty segment, filtered
        // out -> Vec::new(). Defaults never reached.
        let defaults = vec!["should-be-skipped".to_string()];
        let got = load(None, Some(""), &defaults);
        assert!(got.is_empty(), "empty env must skip defaults, got {got:?}");
    }

    #[test]
    fn parse_lines_skips_comments_and_blanks() {
        let raw = "# header comment\n\napt install\n  docker rm   \n# trailing\n";
        let got = parse_lines(raw);
        assert_eq!(got, vec!["apt install".to_string(), "docker rm".to_string()]);
    }

    #[test]
    fn parse_csv_trims_and_drops_empty_segments() {
        let got = parse_csv(" a , , b,c , ");
        assert_eq!(got, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }

    #[test]
    fn cache_serves_stale_content_within_ttl_then_rereads_after_expiry() {
        let mut f = NamedTempFile::new().expect("tmp");
        f.write_all(b"first\n").unwrap();
        f.flush().unwrap();
        invalidate_cache_for(f.path());

        let first = load(Some(f.path()), None, &[]);
        assert_eq!(first, vec!["first".to_string()]);

        // Overwrite while cache is still warm.
        std::fs::write(f.path(), b"second\n").unwrap();
        let cached = load(Some(f.path()), None, &[]);
        assert_eq!(cached, vec!["first".to_string()], "cache should serve stale within TTL");

        // Force expiry by dropping the cache entry (much faster than sleeping
        // TTL_SECS in a unit test; we already exercise the TTL branch above).
        invalidate_cache_for(f.path());
        let after = load(Some(f.path()), None, &[]);
        assert_eq!(after, vec!["second".to_string()]);
    }
}
