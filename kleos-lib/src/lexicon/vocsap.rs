// ============================================================================
// Lexicon -- VOCSAP-only admin surface (Patch 38).
//
// Lateral module: upstream's dead-code pass (commit 25ff58a9) removed
// `validate_override_repo` and `reload_overrides` from `lexicon/mod.rs`
// because upstream had no caller for them. VOCSAP's admin routes
// (`POST /admin/lexicon/validate`, `POST /admin/lexicon/reload`,
// kleos-server/src/routes/admin/mod.rs) still call them, so they are
// re-hosted here to keep the delta on the upstream-owned `mod.rs`/`cache.rs`
// files to a single re-export line and one `#[cfg(test)]` removal (the
// override cache still needs clearing at runtime, not just in tests).
// ============================================================================

use super::loader;
use super::{cache, embedded, EMBEDDED_LANGS};

/// One successfully-parsed lexicon file (embedded baseline or override).
#[derive(Debug, Clone, serde::Serialize)]
pub struct LexiconValidationEntry {
    pub file: String,
    pub language: String,
    pub class_count: usize,
}

/// Result of a parse error for one file in the override repo.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LexiconValidationError {
    pub file: String,
    pub error: String,
}

/// Outcome of a `validate_override_repo` call.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LexiconValidationReport {
    pub repo_path: Option<String>,
    pub loaded: usize,
    pub ok: Vec<LexiconValidationEntry>,
    pub errors: Vec<LexiconValidationError>,
}

/// Walk every `<lang>.toml` file in the configured override repo (if any)
/// and parse it. Returns counts and per-file diagnostics. Embedded
/// baselines are always considered valid (they would have panicked at
/// build time via `OnceLock::get_or_init` otherwise) and are reported
/// separately under `ok` with the file label `embedded:<lang>`.
pub fn validate_override_repo() -> LexiconValidationReport {
    let repo = cache::repo_root();
    let repo_path = repo.map(|p| p.display().to_string());
    let mut ok: Vec<LexiconValidationEntry> = Vec::new();
    let mut errors: Vec<LexiconValidationError> = Vec::new();

    // Embedded baselines first (constant-time, guaranteed valid).
    for lang in EMBEDDED_LANGS {
        if let Some(parsed) = embedded(lang) {
            ok.push(LexiconValidationEntry {
                file: format!("embedded:{lang}.toml"),
                language: parsed.language.clone(),
                class_count: parsed.classes.len(),
            });
        }
    }

    // Override files in the repo, if configured.
    if let Some(repo) = repo {
        if let Ok(entries) = std::fs::read_dir(repo) {
            for entry in entries.flatten() {
                if entry.path().extension().and_then(|e| e.to_str()) != Some("toml") {
                    continue;
                }
                let file_label = entry.path().display().to_string();
                let stem = entry
                    .path()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                match std::fs::read_to_string(entry.path())
                    .map_err(|e| e.to_string())
                    .and_then(|src| loader::parse(&src).map_err(|e| e.to_string()))
                {
                    Ok(parsed) => {
                        ok.push(LexiconValidationEntry {
                            file: file_label,
                            language: if parsed.language.is_empty() {
                                stem
                            } else {
                                parsed.language
                            },
                            class_count: parsed.classes.len(),
                        });
                    }
                    Err(e) => {
                        errors.push(LexiconValidationError {
                            file: file_label,
                            error: e,
                        });
                    }
                }
            }
        }
    }

    LexiconValidationReport {
        loaded: ok.len(),
        repo_path,
        ok,
        errors,
    }
}

/// Force-clear the lexicon override cache so the next `word_class` call
/// re-reads from disk. Used by `POST /admin/lexicon/reload`.
pub fn reload_overrides() {
    cache::clear_cache();
}
