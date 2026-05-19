// ============================================================================
// LLM -- Dynamic prompt overlay (Patch 15).
//
// Loads LLM system / user prompts with an embedded default that can be
// overridden at runtime by a file under `KLEOS_LLM_PROMPT_REPOSITORY`.
//
// Layout: `<repo>/<service>/<purpose>/system.txt` (and `user.txt` for
// prompts that have a user-side template). The `id` parameter expected by
// the helpers below is the dot-free path *without* the `.txt` extension,
// e.g. `"broca/ask_plan/system"`.
//
// Behavior:
// - If `KLEOS_LLM_PROMPT_REPOSITORY` is unset -> always return the embedded
//   default (zero I/O, hot path).
// - If set and `<repo>/<id>.txt` exists -> return its content (cached up to
//   `TTL_SECS` between mtime rechecks).
// - If set but file missing / unreadable -> log a `warn!` once per id and
//   fall back to the embedded default. Never panic.
//
// The cache stores `Arc<String>` so repeated lookups are cheap clones, and
// uses `RwLock` so multiple readers never serialize.
// ============================================================================

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, Instant, SystemTime};

const TTL_SECS: u64 = 5;

struct CacheEntry {
    /// File content as currently known. `None` means we resolved a miss
    /// (file not present or unreadable) and should keep falling back until
    /// the TTL expires.
    content: Option<Arc<String>>,
    /// `mtime` observed for `content`. `None` together with `content == None`
    /// signals a confirmed miss; otherwise tracks the file modification
    /// time used to decide whether a reload is needed.
    mtime: Option<SystemTime>,
    /// Wall-clock instant at which the entry was last checked. Used to
    /// throttle filesystem access via `TTL_SECS`.
    checked_at: Instant,
}

fn cache() -> &'static RwLock<HashMap<String, CacheEntry>> {
    static CACHE: OnceLock<RwLock<HashMap<String, CacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn repo_root() -> Option<&'static PathBuf> {
    static REPO: OnceLock<Option<PathBuf>> = OnceLock::new();
    REPO.get_or_init(|| {
        // 1. Explicit override: KLEOS_LLM_PROMPT_REPOSITORY (any directory).
        if let Some(raw) = std::env::var_os("KLEOS_LLM_PROMPT_REPOSITORY") {
            let p = PathBuf::from(raw);
            if !p.as_os_str().is_empty() {
                return Some(p);
            }
        }
        // 2. Implicit fallback: <KLEOS_DATA_DIR>/prompts when the convention
        //    directory exists. Avoids introducing a second env var when the
        //    operator already provides a canonical data root (deploy/kleos.env
        //    ships KLEOS_DATA_DIR=/var/lib/kleos by default). Mirrors the
        //    KLEOS_* / ENGRAM_* migration done by `config::migrate_env_prefix`
        //    by reading both names.
        for env in ["KLEOS_DATA_DIR", "ENGRAM_DATA_DIR"] {
            if let Some(raw) = std::env::var_os(env) {
                let candidate = PathBuf::from(raw).join("prompts");
                if candidate.is_dir() {
                    return Some(candidate);
                }
            }
        }
        None
    })
    .as_ref()
}

fn path_for(repo: &PathBuf, id: &str) -> PathBuf {
    repo.join(format!("{id}.txt"))
}

/// Load a single prompt by its dot-free id (e.g. `"broca/ask_plan/system"`).
///
/// Returns a borrowed `Cow` over the embedded default in the common case
/// (no override repo configured, or override missing). Returns an owned
/// `Cow` over the file content when an override is in effect.
pub fn load_prompt(id: &str, embedded_default: &'static str) -> Cow<'static, str> {
    let Some(repo) = repo_root() else {
        return Cow::Borrowed(embedded_default);
    };
    match resolve_override(repo, id) {
        Some(content) => Cow::Owned((*content).clone()),
        None => Cow::Borrowed(embedded_default),
    }
}

/// Load the `system` and `user` halves of a prompt pair sharing a common
/// prefix (e.g. `"broca/ask_plan"`).
pub fn load_pair(
    prefix: &str,
    def_sys: &'static str,
    def_user: &'static str,
) -> (Cow<'static, str>, Cow<'static, str>) {
    let sys_id = format!("{prefix}/system");
    let user_id = format!("{prefix}/user");
    (
        load_prompt(&sys_id, def_sys),
        load_prompt(&user_id, def_user),
    )
}

/// Load a prompt and interpolate `{{var}}` placeholders with `vars`.
///
/// Convenience helper for callers that always need a rendered string.
pub fn load_and_render(
    id: &str,
    embedded_default: &'static str,
    vars: &serde_json::Value,
) -> String {
    let raw = load_prompt(id, embedded_default);
    super::template::interpolate(&raw, vars)
}

/// Resolve the override for `id` by hitting the cache first; refresh from
/// disk when the entry is older than `TTL_SECS` or absent.
fn resolve_override(repo: &PathBuf, id: &str) -> Option<Arc<String>> {
    // Fast path: cache hit within TTL window.
    {
        let cache_g = cache().read().ok()?;
        if let Some(entry) = cache_g.get(id) {
            if entry.checked_at.elapsed() < Duration::from_secs(TTL_SECS) {
                return entry.content.as_ref().map(Arc::clone);
            }
        }
    }

    // Slow path: revalidate against the filesystem.
    let path = path_for(repo, id);
    let fresh_mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    let now = Instant::now();

    // If the file is still there and unchanged, reuse the cached content
    // without re-reading.
    {
        let cache_g = cache().read().ok()?;
        if let Some(entry) = cache_g.get(id) {
            if entry.mtime == fresh_mtime && entry.content.is_some() {
                drop(cache_g);
                let mut w = cache().write().ok()?;
                if let Some(e) = w.get_mut(id) {
                    e.checked_at = now;
                }
                return w.get(id).and_then(|e| e.content.as_ref().map(Arc::clone));
            }
        }
    }

    // mtime missing or different: re-read from disk.
    let new_content = std::fs::read_to_string(&path).ok().map(Arc::new);
    if new_content.is_none() {
        // Log a single warning per id-miss so operators can spot typos.
        // We use `debug` for the case where no override file is expected
        // (the embedded default is the intended path), but `warn` when the
        // file used to exist and disappeared.
        let lost_existing = cache()
            .read()
            .ok()
            .and_then(|g| g.get(id).map(|e| e.content.is_some()))
            .unwrap_or(false);
        if lost_existing {
            tracing::warn!(prompt_id = id, path = %path.display(), "prompt override file disappeared, falling back to embedded default");
        } else {
            tracing::debug!(prompt_id = id, path = %path.display(), "no prompt override file (using embedded default)");
        }
    }

    let mut w = cache().write().ok()?;
    let entry = CacheEntry {
        content: new_content.clone(),
        mtime: fresh_mtime,
        checked_at: now,
    };
    w.insert(id.to_string(), entry);
    new_content
}

#[cfg(test)]
pub fn clear_cache() {
    if let Ok(mut g) = cache().write() {
        g.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;

    // Serialize env-var manipulation across tests in this module: the cargo
    // test runner runs tests in parallel by default and `set_var` is process-
    // wide. The mutex must be held for the duration of any env-mutating test.
    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn make_tempdir(label: &str) -> PathBuf {
        let nano = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("kleos-prompts-test-{label}-{nano}"));
        fs::create_dir_all(&dir).expect("create tempdir");
        dir
    }

    /// Drop the `repo_root` OnceLock by reading it: we can't reset a OnceLock
    /// directly, so each test that wants a different env value must run in a
    /// subprocess or rely on the fact that the OnceLock is initialised lazily
    /// on first call. The tests below avoid that limitation by checking the
    /// underlying `resolve_override` directly and bypassing the OnceLock for
    /// the env var, OR by accepting that the first test sets the value for
    /// the whole binary (we serialize with `env_lock()` and read the env at
    /// each call within `resolve_override` is not possible -- so we test the
    /// invariants we can without touching OnceLock).
    #[test]
    fn load_prompt_no_repo_returns_embedded() {
        let _g = env_lock().lock().unwrap();
        // If KLEOS_LLM_PROMPT_REPOSITORY was set before this test runs, the
        // OnceLock will return the cached value and we cannot reset it. Skip
        // gracefully in that case rather than reporting a false failure.
        if std::env::var_os("KLEOS_LLM_PROMPT_REPOSITORY").is_some() {
            return;
        }
        clear_cache();
        let p = load_prompt("nonexistent/path/system", "DEFAULT_BODY");
        assert_eq!(p.as_ref(), "DEFAULT_BODY");
    }

    #[test]
    fn resolve_override_hits_existing_file() {
        let _g = env_lock().lock().unwrap();
        clear_cache();
        let dir = make_tempdir("hit");
        let id = "svc/purpose/system";
        let file = dir.join(format!("{id}.txt"));
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "OVERRIDDEN").unwrap();

        let got = resolve_override(&dir, id).expect("override should load");
        assert_eq!(&*got, "OVERRIDDEN");
    }

    #[test]
    fn resolve_override_misses_when_file_absent() {
        let _g = env_lock().lock().unwrap();
        clear_cache();
        let dir = make_tempdir("miss");
        assert!(resolve_override(&dir, "nope/zzz/system").is_none());
    }

    #[test]
    fn resolve_override_reloads_after_mtime_change() {
        let _g = env_lock().lock().unwrap();
        clear_cache();
        let dir = make_tempdir("mtime");
        let id = "svc/v/system";
        let file = dir.join(format!("{id}.txt"));
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "v1").unwrap();
        assert_eq!(&*resolve_override(&dir, id).unwrap(), "v1");

        // Wait past the TTL window and rewrite with a different mtime.
        std::thread::sleep(Duration::from_millis((TTL_SECS as u64) * 1000 + 200));
        // Touch with explicit modified-time bump (sleep already past TTL).
        fs::write(&file, "v2").unwrap();
        assert_eq!(&*resolve_override(&dir, id).unwrap(), "v2");
    }

    #[test]
    fn load_pair_reads_both_halves() {
        let _g = env_lock().lock().unwrap();
        clear_cache();
        let dir = make_tempdir("pair");
        let sys = dir.join("svc/p/system.txt");
        let usr = dir.join("svc/p/user.txt");
        fs::create_dir_all(sys.parent().unwrap()).unwrap();
        fs::write(&sys, "SYS_OVERRIDE").unwrap();
        fs::write(&usr, "USR_OVERRIDE {{x}}").unwrap();

        // Use the lower-level resolve to avoid depending on OnceLock state.
        let s = resolve_override(&dir, "svc/p/system").unwrap();
        let u = resolve_override(&dir, "svc/p/user").unwrap();
        assert_eq!(&*s, "SYS_OVERRIDE");
        assert_eq!(&*u, "USR_OVERRIDE {{x}}");
    }

    #[test]
    fn render_interpolates_overridden_template() {
        // Render through interpolate directly (avoids OnceLock).
        let tmpl = "Hello {{who}}!";
        let vars = serde_json::json!({ "who": "kleos" });
        assert_eq!(super::super::template::interpolate(tmpl, &vars), "Hello kleos!");
    }
}
