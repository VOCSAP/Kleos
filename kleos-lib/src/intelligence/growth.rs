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

#[tracing::instrument(skip(db), fields(limit))]
pub async fn list_observations(db: &Database, limit: usize) -> Result<Vec<GrowthObservation>> {
    db.read(move |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT id, content, source, importance, created_at \
                 FROM memories \
                 WHERE category = 'growth' AND is_forgotten = 0 \
                 ORDER BY created_at DESC LIMIT ?1",
            )
            .map_err(rusqlite_to_eng_error)?;

        let observations = stmt
            .query_map(rusqlite::params![limit as i64], |row| {
                Ok(GrowthObservation {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    source: row.get(2)?,
                    importance: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })
            .map_err(rusqlite_to_eng_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(rusqlite_to_eng_error)?;

        Ok(observations)
    })
    .await
}

#[tracing::instrument(skip(db))]
pub async fn materialize(db: &Database, observation_id: i64, user_id: i64) -> Result<i64> {
    db.write(move |conn| {
        let result: Option<(String, String)> = conn
            .query_row(
                "SELECT content, source FROM memories WHERE id = ?1 AND category = 'growth'",
                rusqlite::params![observation_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(rusqlite_to_eng_error)?;

        let (content, source) = result.ok_or_else(|| {
            EngError::NotFound(format!("growth observation {} not found", observation_id))
        })?;

        conn.execute(
            "INSERT INTO memories (content, category, source, importance, version, is_latest, \
             source_count, is_static, is_forgotten, confidence, status, \
             created_at, updated_at) \
             VALUES (?1, 'insight', ?2, 8, 1, 1, 1, 1, 0, 1.0, 'approved', \
             datetime('now'), datetime('now'))",
            rusqlite::params![content, source],
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
            let truncated = if existing.len() > 4000 {
                &existing[..4000]
            } else {
                existing
            };
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
                     AND created_at > datetime('now', '-24 hours')",
                    rusqlite::params![prefix_clone],
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
    let (memory_id, reflection_id) = db
        .write(move |conn| {
            let trimmed_refl = trimmed_for_closure.clone();
            conn.execute(
                "INSERT INTO memories (content, category, source, importance, version, is_latest, \
                 source_count, is_static, is_forgotten, is_archived, confidence, status, \
                 created_at, updated_at) \
                 VALUES (?1, 'growth', ?2, 7, 1, 1, 1, 1, 0, 1, 1.0, 'approved', \
                 datetime('now'), datetime('now'))",
                rusqlite::params![trimmed_for_closure, source_c],
            )
            .map_err(rusqlite_to_eng_error)?;

            let memory_id = conn.last_insert_rowid();

            conn.execute(
                "INSERT INTO reflections (content, reflection_type, source_memory_ids, \
                 confidence, created_at) \
                 VALUES (?1, 'growth', ?2, 1.0, datetime('now'))",
                rusqlite::params![trimmed_refl, format!("[{}]", memory_id)],
            )
            .map_err(rusqlite_to_eng_error)?;

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

/// Self-reflection for Engram -- called periodically (e.g., every hour).
/// Gathers memory stats, builds context, and generates a growth observation.
#[tracing::instrument(skip(db))]
pub async fn self_reflect(db: &Database, user_id: i64) -> Result<GrowthReflectResult> {
    // Min activity threshold: 50 new memories in last hour
    let recent_count: i64 = db
        .read(move |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM memories \
                 WHERE created_at > datetime('now', '-1 hour')",
                [],
                |row| row.get(0),
            )
            .map_err(rusqlite_to_eng_error)
        })
        .await?;

    if recent_count < 50 {
        return Ok(GrowthReflectResult {
            observation: None,
            stored_memory_id: None,
            reflection_id: None,
        });
    }

    // 15% probability gate
    if rand::random::<f64>() > 0.15 {
        return Ok(GrowthReflectResult {
            observation: None,
            stored_memory_id: None,
            reflection_id: None,
        });
    }

    // Build context from memory stats
    let (total, never_accessed, growth_count, avg_importance): (i64, i64, i64, f64) = db
        .read(move |conn| {
            conn.query_row(
                "SELECT COUNT(*) as total, \
                        SUM(CASE WHEN access_count = 0 THEN 1 ELSE 0 END) as never_accessed, \
                        SUM(CASE WHEN category = 'growth' THEN 1 ELSE 0 END) as growth_count, \
                        AVG(importance) as avg_importance \
                 FROM memories WHERE is_forgotten = 0",
                rusqlite::params![],
                |row| {
                    Ok((
                        row.get::<_, i64>(0).unwrap_or(0),
                        row.get::<_, i64>(1).unwrap_or(0),
                        row.get::<_, i64>(2).unwrap_or(0),
                        row.get::<_, f64>(3).unwrap_or(0.0),
                    ))
                },
            )
            .map_err(rusqlite_to_eng_error)
        })
        .await?;

    let context = vec![
        format!(
            "Memory stats: {} total, {} never accessed, {} growth entries, avg importance {:.1}",
            total, never_accessed, growth_count, avg_importance
        ),
        format!("New memories in last hour: {}", recent_count),
    ];

    // Get existing growth for anti-repeat
    let existing_lines: Vec<String> = db
        .read(move |conn| {
            // Anti-repeat lookup: match every `<service>-growth` source, since
            // the source is built dynamically from `req.service` at write time
            // (see line ~288: `format!("{}-growth", req.service)`). This covers
            // the legacy `engram-growth` rows, the post-rebrand `kleos-growth`
            // rows, and any future per-service growth stream (e.g. `claude-code-growth`)
            // without needing the SELECT to be updated each time a new caller
            // is added. The `category = 'growth'` filter already constrains the
            // result set to growth memories.
            let mut stmt = conn
                .prepare(
                    "SELECT content FROM memories \
                     WHERE category = 'growth' \
                       AND source LIKE '%-growth' \
                       AND is_forgotten = 0 \
                     ORDER BY created_at DESC LIMIT 10",
                )
                .map_err(rusqlite_to_eng_error)?;

            let lines = stmt
                .query_map([], |row| {
                    let content: String = row.get(0)?;
                    Ok(format!("- {}", content))
                })
                .map_err(rusqlite_to_eng_error)?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(rusqlite_to_eng_error)?;

            Ok(lines)
        })
        .await?;

    let req = GrowthReflectRequest {
        service: "kleos".to_string(),
        context,
        existing_growth: if existing_lines.is_empty() {
            None
        } else {
            Some(existing_lines.join("\n"))
        },
        prompt_override: None,
    };

    reflect(db, &req, user_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_observation_valid() {
        assert!(validate_observation(
            "I noticed that memory access patterns shift during weekday evenings."
        ));
    }

    #[test]
    fn test_validate_observation_too_short() {
        assert!(!validate_observation("short"));
    }

    #[test]
    fn test_validate_observation_nothing() {
        assert!(!validate_observation("NOTHING"));
    }

    #[test]
    fn test_validate_observation_meta() {
        assert!(!validate_observation("I don't see anything interesting"));
        assert!(!validate_observation("There is nothing notable"));
    }

    #[test]
    fn test_get_prompt_override() {
        let p = get_prompt_for_service("engram", Some("Custom prompt"));
        assert_eq!(p, "Custom prompt");
    }

    #[test]
    fn test_get_prompt_default() {
        let p = get_prompt_for_service("unknown_service", None);
        assert!(p.contains("self-reflection process"));
    }
}
