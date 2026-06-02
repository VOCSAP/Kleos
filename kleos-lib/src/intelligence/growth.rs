//! Growth reflection -- LLM-backed self-reflection and growth tracking.
//!
//! Observes recent activity, generates observations about patterns, and
//! stores them as growth memories.

use crate::brain::dream::types::DreamCycleResult;
use crate::config::Config;
use crate::cred::{has_secret_patterns, CreddClient};
use crate::db::Database;
use crate::intelligence::llm::{call_llm, is_llm_available};
use crate::intelligence::types::{
    GrowthObservation, GrowthReflectRequest, GrowthReflectResult, LlmOptions,
};
use crate::{EngError, Result};
use rusqlite::OptionalExtension;
use tracing::{info, warn};

fn rusqlite_to_eng_error(err: rusqlite::Error) -> EngError {
    EngError::DatabaseMessage(err.to_string())
}

// Patch 17c -- optional external reject patterns for growth observations.
// Loads `<prompt_repo>/growth/reject_patterns.txt` at every validation call
// (no caching: dreamer fires every 30 min, hot-tunable matters more than
// micro-perf). Format: one substring per line, case-insensitive, blank lines
// and lines starting with `#` ignored. Any match rejects the observation
// (treated as NOTHING). Resolution mirrors `kleos-lib::llm::prompts`:
// `KLEOS_LLM_PROMPT_REPOSITORY` first, then `$KLEOS_DATA_DIR/prompts`.
fn growth_reject_patterns() -> Vec<String> {
    use std::path::PathBuf;
    let repo: Option<PathBuf> = std::env::var_os("KLEOS_LLM_PROMPT_REPOSITORY")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| {
            for env in ["KLEOS_DATA_DIR", "ENGRAM_DATA_DIR"] {
                if let Some(raw) = std::env::var_os(env) {
                    let candidate = PathBuf::from(raw).join("prompts");
                    if candidate.is_dir() {
                        return Some(candidate);
                    }
                }
            }
            None
        });
    let Some(repo) = repo else {
        return Vec::new();
    };
    let path = repo.join("growth").join("reject_patterns.txt");
    match std::fs::read_to_string(&path) {
        Ok(content) => content
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| l.to_lowercase())
            .collect(),
        Err(_) => Vec::new(),
    }
}

#[tracing::instrument(skip(db), fields(limit, space_id, include_unscoped))]
pub async fn list_observations(
    db: &Database,
    limit: usize,
    // Patch 33 -- optional space filter (#3028 fix). None preserves the
    // upstream behaviour (no filter; observations from all spaces are
    // returned). Some(N) applies the inclusive partitioning convention:
    //   include_unscoped = Some(true)  -> space N + default + legacy NULL
    //   include_unscoped = Some(false) / None -> strict space N only
    space_id: Option<i64>,
    include_unscoped: Option<bool>,
    user_id: i64,
) -> Result<Vec<GrowthObservation>> {
    // Patch 33 -- build (extra_clause, params) where positional indices
    // match the order in `params_vec`. LIMIT is always the last param.
    let (extra_clause, params_vec): (String, Vec<rusqlite::types::Value>) = match space_id {
        Some(sid) if matches!(include_unscoped, Some(true)) => (
            " AND (space_id = ?1 \
                OR space_id = (SELECT id FROM spaces \
                               WHERE user_id = ?2 AND name = 'default' LIMIT 1) \
                OR space_id IS NULL)"
                .to_string(),
            vec![
                rusqlite::types::Value::Integer(sid),
                rusqlite::types::Value::Integer(user_id),
            ],
        ),
        Some(sid) => (
            " AND space_id = ?1".to_string(),
            vec![rusqlite::types::Value::Integer(sid)],
        ),
        None => (String::new(), Vec::new()),
    };
    let limit_idx = params_vec.len() + 1;
    let sql = format!(
        "SELECT id, content, source, importance, created_at \
         FROM memories \
         WHERE category = 'growth' AND is_forgotten = 0{extra_clause} \
         ORDER BY created_at DESC LIMIT ?{limit_idx}"
    );
    db.read(move |conn| {
        let mut stmt = conn.prepare(&sql).map_err(rusqlite_to_eng_error)?;
        let mut all_params: Vec<rusqlite::types::Value> = params_vec;
        all_params.push(rusqlite::types::Value::Integer(limit as i64));
        let observations = stmt
            .query_map(
                rusqlite::params_from_iter(all_params.iter().cloned()),
                |row| {
                    Ok(GrowthObservation {
                        id: row.get(0)?,
                        content: row.get(1)?,
                        source: row.get(2)?,
                        importance: row.get(3)?,
                        created_at: row.get(4)?,
                    })
                },
            )
            .map_err(rusqlite_to_eng_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(rusqlite_to_eng_error)?;

        Ok(observations)
    })
    .await
}

#[tracing::instrument(skip(db))]
/// Converts one growth observation into an insight memory.
pub async fn materialize(db: &Database, observation_id: i64, user_id: i64) -> Result<i64> {
    db.write(move |conn| {
        // Patch 33 -- also read the source observation's space_id so the
        // promoted insight stays in the same project bucket (no
        // cross-project leak when materialize is called from a
        // tenant-shared dreamer).
        let result: Option<(String, String, Option<i64>)> = conn
            .query_row(
                "SELECT content, source, space_id FROM memories \
                 WHERE id = ?1 AND category = 'growth'",
                rusqlite::params![observation_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;

        let (content, source, source_space_id) = result.ok_or_else(|| {
            EngError::NotFound(format!("growth observation {} not found", observation_id))
        })?;

        // Patch 33 -- propagate space_id of the source observation. NULL
        // is preserved when the source is a pre-v57 legacy observation;
        // the user_id arg is reserved for a future enhancement that may
        // also stamp user_id on the insight.
        let _ = user_id;
        conn.execute(
            "INSERT INTO memories (content, category, source, importance, version, is_latest, \
             source_count, is_static, is_forgotten, confidence, status, space_id, \
             created_at, updated_at) \
             VALUES (?1, 'insight', ?2, 8, 1, 1, 1, 1, 0, 1.0, 'approved', ?3, \
             datetime('now'), datetime('now'))",
            rusqlite::params![content, source, source_space_id],
        )
        .map_err(rusqlite_to_eng_error)?;

        Ok(conn.last_insert_rowid())
    })
    .await
}

/// Service-specific reflection prompts.
// Patch 15 -- embedded defaults for the growth/* reflection prompts.
const GROWTH_KLEOS_REFLECTION_DEFAULT: &str =
    include_str!("../../prompts/growth/kleos_reflection/system.txt");
const GROWTH_CLAUDE_CODE_REFLECTION_DEFAULT: &str =
    include_str!("../../prompts/growth/claude_code_reflection/system.txt");
const GROWTH_EIDOLON_REFLECTION_DEFAULT: &str =
    include_str!("../../prompts/growth/eidolon_reflection/system.txt");
const GROWTH_DEFAULT_REFLECTION_DEFAULT: &str =
    include_str!("../../prompts/growth/default_reflection/system.txt");

// Patch 16 -- embedded defaults for the per-service rules suffix and user
// templates. Colocated with the existing system prompts (Option C of plan
// mossy-launching-origami): the same Rules text is duplicated across the
// four services so an operator can tune one without touching the others.
const GROWTH_KLEOS_REFLECTION_SUFFIX_DEFAULT: &str =
    include_str!("../../prompts/growth/kleos_reflection/system_suffix.txt");
const GROWTH_CLAUDE_CODE_REFLECTION_SUFFIX_DEFAULT: &str =
    include_str!("../../prompts/growth/claude_code_reflection/system_suffix.txt");
const GROWTH_EIDOLON_REFLECTION_SUFFIX_DEFAULT: &str =
    include_str!("../../prompts/growth/eidolon_reflection/system_suffix.txt");
const GROWTH_DEFAULT_REFLECTION_SUFFIX_DEFAULT: &str =
    include_str!("../../prompts/growth/default_reflection/system_suffix.txt");
const GROWTH_KLEOS_REFLECTION_USER_DEFAULT: &str =
    include_str!("../../prompts/growth/kleos_reflection/user.txt");
const GROWTH_CLAUDE_CODE_REFLECTION_USER_DEFAULT: &str =
    include_str!("../../prompts/growth/claude_code_reflection/user.txt");
const GROWTH_EIDOLON_REFLECTION_USER_DEFAULT: &str =
    include_str!("../../prompts/growth/eidolon_reflection/user.txt");
const GROWTH_DEFAULT_REFLECTION_USER_DEFAULT: &str =
    include_str!("../../prompts/growth/default_reflection/user.txt");

struct ServicePromptPaths {
    system_id: &'static str,
    system_default: &'static str,
    suffix_id: &'static str,
    suffix_default: &'static str,
    user_id: &'static str,
    user_default: &'static str,
}

// Map the dispatch key to a canonical prompt slug. The kleos / engram
// aliasing collapses to the same canonical files so legacy growth rows
// tagged "engram" still pick up the same reflection prompt as "kleos" rows.
fn service_prompt_paths(service: &str) -> ServicePromptPaths {
    match service {
        "engram" | "kleos" => ServicePromptPaths {
            system_id: "growth/kleos_reflection/system",
            system_default: GROWTH_KLEOS_REFLECTION_DEFAULT,
            suffix_id: "growth/kleos_reflection/system_suffix",
            suffix_default: GROWTH_KLEOS_REFLECTION_SUFFIX_DEFAULT,
            user_id: "growth/kleos_reflection/user",
            user_default: GROWTH_KLEOS_REFLECTION_USER_DEFAULT,
        },
        "claude-code" => ServicePromptPaths {
            system_id: "growth/claude_code_reflection/system",
            system_default: GROWTH_CLAUDE_CODE_REFLECTION_DEFAULT,
            suffix_id: "growth/claude_code_reflection/system_suffix",
            suffix_default: GROWTH_CLAUDE_CODE_REFLECTION_SUFFIX_DEFAULT,
            user_id: "growth/claude_code_reflection/user",
            user_default: GROWTH_CLAUDE_CODE_REFLECTION_USER_DEFAULT,
        },
        "eidolon" => ServicePromptPaths {
            system_id: "growth/eidolon_reflection/system",
            system_default: GROWTH_EIDOLON_REFLECTION_DEFAULT,
            suffix_id: "growth/eidolon_reflection/system_suffix",
            suffix_default: GROWTH_EIDOLON_REFLECTION_SUFFIX_DEFAULT,
            user_id: "growth/eidolon_reflection/user",
            user_default: GROWTH_EIDOLON_REFLECTION_USER_DEFAULT,
        },
        _ => ServicePromptPaths {
            system_id: "growth/default_reflection/system",
            system_default: GROWTH_DEFAULT_REFLECTION_DEFAULT,
            suffix_id: "growth/default_reflection/system_suffix",
            suffix_default: GROWTH_DEFAULT_REFLECTION_SUFFIX_DEFAULT,
            user_id: "growth/default_reflection/user",
            user_default: GROWTH_DEFAULT_REFLECTION_USER_DEFAULT,
        },
    }
}

fn get_prompt_for_service(service: &str, prompt_override: Option<&str>) -> String {
    if let Some(override_prompt) = prompt_override {
        return override_prompt.to_string();
    }
    let paths = service_prompt_paths(service);
    crate::llm::prompts::load_prompt(paths.system_id, paths.system_default).into_owned()
}

/// Validate that an observation is meaningful (not empty, not meta-commentary).
fn validate_observation(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.len() < 10 || trimmed.len() > 500 {
        return false;
    }
    if trimmed.to_uppercase() == "NOTHING" {
        return false;
    }
    if trimmed.starts_with("I don't") || trimmed.starts_with("There is nothing") {
        return false;
    }
    // Patch 17c -- optional external reject patterns (hot-tunable via file).
    let patterns = growth_reject_patterns();
    if !patterns.is_empty() {
        let lower = trimmed.to_lowercase();
        if patterns.iter().any(|p| lower.contains(p)) {
            return false;
        }
    }
    true
}

/// Resolves an observation through secret redaction when needed.
async fn resolve_growth_observation(
    db: &Database,
    service: &str,
    observation: &str,
    user_id: i64,
) -> Result<String> {
    if !has_secret_patterns(observation) {
        return Ok(observation.to_string());
    }

    let config = Config::from_env();
    let credd = CreddClient::from_config(&config);
    credd.resolve_text(db, user_id, service, observation).await
}

/// Build context lines from a dream cycle result for growth reflection.
///
/// Extracts per-stage telemetry (items processed/changed) from the
/// `DreamCycleResult` and formats them into human-readable lines that
/// describe what the consolidation cycle did. These are prepended to the
/// recent-memory context so the LLM reflects on both what happened in
/// the brain and what the agent recently experienced.
pub fn build_dream_context(
    result: &DreamCycleResult,
    pattern_count: usize,
    edge_count: usize,
) -> Vec<String> {
    let mut lines = Vec::with_capacity(result.stages.len() + 2);
    lines.push(format!(
        "Dream cycle completed in {}ms",
        result.total_duration_ms
    ));

    for stage in &result.stages {
        lines.push(format!(
            "Stage '{}': processed {}, changed {}",
            stage.stage, stage.items_processed, stage.items_changed
        ));
    }

    lines.push(format!(
        "Current substrate: {} patterns, {} edges",
        pattern_count, edge_count
    ));
    lines
}

/// Perform a growth reflection -- observe recent activity and generate an observation.
#[tracing::instrument(skip(db, req), fields(service = %req.service, context_len = req.context.len(), user_id))]
pub async fn reflect(
    db: &Database,
    req: &GrowthReflectRequest,
    user_id: i64,
) -> Result<GrowthReflectResult> {
    if req.context.is_empty() {
        return Err(crate::EngError::InvalidInput(
            "context array is required and must not be empty".to_string(),
        ));
    }

    if !is_llm_available() {
        warn!(service = %req.service, "growth_reflect_skipped: llm_unavailable");
        return Ok(GrowthReflectResult {
            observation: None,
            stored_memory_id: None,
            reflection_id: None,
        });
    }

    let system_prompt = get_prompt_for_service(&req.service, req.prompt_override.as_deref());
    let paths = service_prompt_paths(&req.service);
    let suffix = crate::llm::prompts::load_prompt(paths.suffix_id, paths.suffix_default);
    let full_system = format!(
        "{}\n{}",
        system_prompt.trim_end(),
        suffix.trim_end()
    );

    let existing_block = match req.existing_growth.as_deref() {
        Some(existing) => {
            // Patch 39: utf-8 safe truncation -- raw &existing[..4000]
            // panics when byte 4000 lands inside a multi-byte glyph
            // (frequent on FR/CJK/emoji content). Caps at 4000 bytes
            // or the largest prefix ending on a char boundary below.
            let truncated = crate::str_safe::truncate_at_char(existing, 4000);
            format!(
                "Things I already know (do NOT repeat these):\n{}\n\n",
                truncated
            )
        }
        None => String::new(),
    };
    let context_joined = req.context.join("\n");
    let vars = serde_json::json!({
        "context": context_joined,
        "existing_block": existing_block,
    });
    let user_prompt = crate::llm::prompts::load_and_render(
        paths.user_id,
        paths.user_default,
        &vars,
    );
    let user_prompt = user_prompt.trim_end().to_string();

    let opts = LlmOptions {
        temperature: 0.7,
        max_tokens: 300,
    };

    let response = match call_llm(&full_system, &user_prompt, Some(opts)).await {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, service = %req.service, "growth_reflect_failed");
            return Ok(GrowthReflectResult {
                observation: None,
                stored_memory_id: None,
                reflection_id: None,
            });
        }
    };

    let trimmed = response.trim().to_string();

    if !validate_observation(&trimmed) {
        info!(service = %req.service, "growth_nothing_observed");
        return Ok(GrowthReflectResult {
            observation: None,
            stored_memory_id: None,
            reflection_id: None,
        });
    }

    let trimmed = resolve_growth_observation(db, &req.service, &trimmed, user_id).await?;

    // Dedup: skip if a growth memory with same 200-char prefix exists in last 24h
    let prefix: String = trimmed.chars().take(200).collect();
    let prefix_clone = prefix.clone();
    let is_dup: bool = db
        .read(move |conn| {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM memories WHERE category = 'growth' \
                     AND substr(content, 1, 200) = ?1 \
                     AND user_id = ?2 \
                     AND created_at > datetime('now', '-24 hours')",
                    rusqlite::params![prefix_clone, user_id],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            Ok(count > 0)
        })
        .await?;
    if is_dup {
        info!(service = %req.service, "growth_duplicate_skipped");
        return Ok(GrowthReflectResult {
            observation: None,
            stored_memory_id: None,
            reflection_id: None,
        });
    }

    // Store as growth memory
    let source = format!("{}-growth", req.service);

    let trimmed_for_closure = trimmed.clone();
    let source_c = source.clone();
    // Patch 33 -- stamp space_id so the observation stays in the
    // project bucket of the context that generated it (#3028 fix).
    let space_id_for_closure = req.space_id;
    let (memory_id, reflection_id) = db
        .write(move |conn| {
            let trimmed_refl = trimmed_for_closure.clone();
            conn.execute(
                "INSERT INTO memories (content, category, source, importance, version, is_latest, \
                 source_count, is_static, is_forgotten, is_archived, confidence, status, space_id, user_id, \
                 created_at, updated_at) \
                 VALUES (?1, 'growth', ?2, 7, 1, 1, 1, 1, 0, 1, 1.0, 'approved', ?3, ?4, \
                 datetime('now'), datetime('now'))",
                rusqlite::params![trimmed_for_closure, source_c, space_id_for_closure, user_id],
            )
            .map_err(rusqlite_to_eng_error)?;

            let memory_id = conn.last_insert_rowid();

            conn.execute(
                "INSERT INTO reflections (content, reflection_type, source_memory_ids, \
                 confidence, user_id, created_at) \
                 VALUES (?1, 'growth', ?2, 1.0, ?3, datetime('now'))",
                rusqlite::params![trimmed_refl, format!("[{}]", memory_id), user_id],
            )?;

            let reflection_id = conn.last_insert_rowid();

            Ok((memory_id, reflection_id))
        })
        .await?;

    info!(
        service = %req.service,
        memory_id,
        reflection_id,
        observation = %trimmed.chars().take(80).collect::<String>(),
        "growth_observation_stored"
    );

    Ok(GrowthReflectResult {
        observation: Some(trimmed),
        stored_memory_id: Some(memory_id),
        reflection_id: Some(reflection_id),
    })
}

/// Tests the growth reflection helpers and validation rules.
#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that valid observations pass validation.
    #[test]
    fn test_validate_observation_valid() {
        assert!(validate_observation(
            "I noticed that memory access patterns shift during weekday evenings."
        ));
    }

    /// Verifies that short observations are rejected.
    #[test]
    fn test_validate_observation_too_short() {
        assert!(!validate_observation("short"));
    }

    /// Verifies that the literal NOTHING is rejected.
    #[test]
    fn test_validate_observation_nothing() {
        assert!(!validate_observation("NOTHING"));
    }

    /// Verifies that meta-commentary is rejected.
    #[test]
    fn test_validate_observation_meta() {
        assert!(!validate_observation("I don't see anything interesting"));
        assert!(!validate_observation("There is nothing notable"));
    }

    /// Verifies that a prompt override is returned unchanged.
    #[test]
    fn test_get_prompt_override() {
        let p = get_prompt_for_service("engram", Some("Custom prompt"));
        assert_eq!(p, "Custom prompt");
    }

    /// Verifies that the default service prompt includes the expected guidance.
    #[test]
    fn test_get_prompt_default() {
        let p = get_prompt_for_service("unknown_service", None);
        assert!(p.contains("self-reflection process"));
    }
}
