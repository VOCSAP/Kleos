/// Brain absorption helpers for the server layer.
///
/// Brain absorption requires Arc<dyn BrainBackend> and EmbeddingProvider,
/// both of which are only available via AppState. This module contains the
/// server-side logic that ports Eidolon's absorber.rs concepts into Kleos.
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::RwLock;

use kleos_lib::embeddings::EmbeddingProvider;
use kleos_lib::services::brain::{AbsorbMemoryData, BrainBackend};

/// VOCSAP Patch 39 -- noise filter before brain absorption.
///
/// Upstream's absorb (#60) injects every stored memory into the global
/// Hopfield substrate without semantic filtering. We skip two classes of
/// noise before `brain.absorb`:
///   - `sidecar*` sources -- always filtered. Covers both the legacy
///     `sidecar-gate*` sources (gate prompts from the old gate.rs/watcher.rs
///     watcher, removed by upstream PR #217/commit e5526e80) and the current
///     sidecar's default `source = "sidecar"` (kleos-sidecar/src/main.rs:217).
///     Widened after the merge that reactivated the sidecar: the narrower
///     `sidecar-gate` prefix stopped matching anything and would have
///     silently re-opened the 2026-05-29 incident (619 raw sidecar turns
///     absorbed into the Hopfield substrate via `fast_extract_facts`, which
///     has no source filter of its own -- this guard is the only gate).
///   - memories below an importance threshold -- opt-in via the env var
///     `KLEOS_BRAIN_ABSORB_MIN_IMPORTANCE` (default 0.0 = inactive, so the
///     behaviour matches upstream except for the sidecar skip).
fn brain_absorb_min_importance() -> f64 {
    std::env::var("KLEOS_BRAIN_ABSORB_MIN_IMPORTANCE")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// Pure decision used by both call sites. Returns `false` when the memory
/// should be skipped. Split from the env lookup so it can be unit-tested
/// without touching process environment.
fn should_absorb_with_threshold(source: &str, importance: f64, min_importance: f64) -> bool {
    if source.starts_with("sidecar") {
        return false;
    }
    if importance < min_importance {
        return false;
    }
    true
}

/// Whether a memory should be absorbed into the brain, reading the importance
/// threshold from the environment.
fn should_absorb_to_brain(source: &str, importance: f64) -> bool {
    should_absorb_with_threshold(source, importance, brain_absorb_min_importance())
}

/// Absorb a single activity event into the brain.
///
/// This is called fire-and-forget from the activity route after process_activity
/// succeeds. Never fails the caller -- all errors are logged as warnings.
///
/// - `memory_id`: The id of the just-stored memory row (used as brain memory id)
/// - `content`: Pre-formatted content string
/// - `category`: "task" for task.* actions, "activity" for others
/// - `importance`: 6 for completed, 7 for blocked/error, 4 otherwise
/// - `source`: The agent name from the activity report
#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip(brain, embedder, content), fields(memory_id, category = %category, importance, source = %source))]
pub async fn absorb_activity_to_brain(
    brain: Arc<dyn BrainBackend>,
    embedder: Arc<RwLock<Option<Arc<dyn EmbeddingProvider>>>>,
    user_id: i64,
    memory_id: i64,
    content: String,
    category: String,
    importance: f64,
    source: String,
) {
    if !should_absorb_to_brain(&source, importance) {
        tracing::debug!(
            "brain_absorber: filtered activity memory {} (source={}, importance={})",
            memory_id,
            source,
            importance
        );
        return;
    }
    if !brain.is_ready() {
        tracing::debug!("brain_absorber: brain not ready, skipping absorption");
        return;
    }

    let embedder_guard = embedder.read().await;
    let embedder_ref = match embedder_guard.as_ref() {
        Some(e) => e.clone(),
        None => {
            tracing::warn!(
                "brain_absorber: embedder not ready, skipping absorption for memory {}",
                memory_id
            );
            return;
        }
    };
    drop(embedder_guard);

    let memory = AbsorbMemoryData {
        id: memory_id,
        content,
        category,
        source,
        importance,
        created_at: Utc::now().to_rfc3339(),
        tags: None,
    };

    match brain.absorb(embedder_ref.as_ref(), user_id, memory).await {
        Ok(()) => tracing::debug!("brain_absorber: absorbed activity memory id={}", memory_id),
        Err(e) => tracing::warn!(
            "brain_absorber: brain absorb failed for memory {}: {}",
            memory_id,
            e
        ),
    }
}

/// Absorb session completion data into the brain.
///
/// This is a portable adaptation of Eidolon's absorb_session function.
/// Callers are expected to extract the relevant data from session state
/// before calling this. All parameters are pre-processed strings.
///
/// - `session_short_id`: Short human-readable session identifier for logging
/// - `task`: Task description (will be truncated to 100 chars)
/// - `outcome`: One of "succeeded", "failed", "killed", "timed_out", "unknown"
/// - `agent`: Agent name
/// - `corrections`: Number of corrections applied during session
/// - `user_label`: Optional user label for category namespacing (e.g. "operator")
/// - `issue_lines`: Lines from output that represent blocked/gate issues (max 5 used)
/// - `discovery_lines`: Lines from output matching discovery keywords (max 10 used)
#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip(brain, embedder, task, issue_lines, discovery_lines), fields(session_short_id = %session_short_id, outcome = %outcome, agent = %agent, corrections))]
pub async fn absorb_session_to_brain(
    brain: Arc<dyn BrainBackend>,
    embedder: Arc<RwLock<Option<Arc<dyn EmbeddingProvider>>>>,
    user_id: i64,
    session_short_id: String,
    task: String,
    outcome: &str,
    agent: String,
    corrections: u32,
    user_label: Option<String>,
    issue_lines: Vec<String>,
    discovery_lines: Vec<String>,
) {
    if !brain.is_ready() {
        tracing::debug!("brain_absorber: brain not ready, skipping session absorption");
        return;
    }

    let importance: f64 = if outcome == "succeeded" { 6.0 } else { 7.0 };
    let task_excerpt: String = task.chars().take(100).collect();

    let summary = format!(
        "Kleos session ({}) for task \"{}\": {}. Agent: {}. Corrections: {}.",
        session_short_id, task_excerpt, outcome, agent, corrections,
    );

    let category_prefix = match user_label.as_deref() {
        Some(u) => format!("user:{}/", u),
        None => "system/".to_string(),
    };

    // 1. Absorb session summary
    absorb_one(
        &brain,
        &embedder,
        user_id,
        &summary,
        &format!("{}task", category_prefix),
        importance,
        "kleos-server",
    )
    .await;

    // 2. Absorb gate blocks as strong correction signals (importance 8)
    for line in issue_lines.iter().take(5) {
        let block_content = format!(
            "Gate blocked action in session {}: {}",
            session_short_id, line
        );
        absorb_one(
            &brain,
            &embedder,
            user_id,
            &block_content,
            &format!("{}issue", category_prefix),
            8.0,
            "kleos-server",
        )
        .await;
    }

    // 3. Extract and absorb key discoveries (importance 5)
    let used_discoveries: Vec<&String> = discovery_lines.iter().take(10).collect();
    if !used_discoveries.is_empty() {
        let task_excerpt_short: String = task.chars().take(80).collect();
        let discoveries = used_discoveries
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let discovery_content = format!(
            "Session {} discoveries for task \"{}\": {}",
            session_short_id, task_excerpt_short, discoveries,
        );
        absorb_one(
            &brain,
            &embedder,
            user_id,
            &discovery_content,
            &format!("{}discovery", category_prefix),
            5.0,
            "kleos-server",
        )
        .await;
        tracing::info!(
            "brain_absorber: absorbed {} discovery lines for session {}",
            used_discoveries.len(),
            session_short_id
        );
    }
}

/// Internal helper: absorb a single content string into the brain.
/// Best-effort -- logs warning on failure, never panics.
async fn absorb_one(
    brain: &Arc<dyn BrainBackend>,
    embedder: &Arc<RwLock<Option<Arc<dyn EmbeddingProvider>>>>,
    user_id: i64,
    content: &str,
    category: &str,
    importance: f64,
    source: &str,
) {
    if !should_absorb_to_brain(source, importance) {
        tracing::debug!(
            "brain_absorber: filtered memory (source={}, category={}, importance={})",
            source,
            category,
            importance
        );
        return;
    }
    let embedder_guard = embedder.read().await;
    let embedder_ref = match embedder_guard.as_ref() {
        Some(e) => e.clone(),
        None => {
            tracing::warn!(
                "brain_absorber: embedder not ready, skipping absorption of {:?}",
                kleos_lib::validation::truncate_on_char_boundary(content, 60)
            );
            return;
        }
    };
    drop(embedder_guard);

    // Generate a stable-ish id from content hash to avoid duplicate tracking
    let id = stable_id(content);

    let memory = AbsorbMemoryData {
        id,
        content: content.to_string(),
        category: category.to_string(),
        source: source.to_string(),
        importance,
        created_at: Utc::now().to_rfc3339(),
        tags: None,
    };

    match brain.absorb(embedder_ref.as_ref(), user_id, memory).await {
        Ok(()) => tracing::debug!("brain_absorber: absorbed id={} category={}", id, category),
        Err(e) => tracing::warn!(
            "brain_absorber: brain absorb failed (category={}): {}",
            category,
            e
        ),
    }
}

/// Generate a pseudo-unique i64 id from string content using a fast hash.
/// Using a simple FNV-1a-style fold to avoid pulling in uuid/rand here.
fn stable_id(content: &str) -> i64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in content.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    // Fold to positive i64
    (hash & 0x7fff_ffff_ffff_ffff) as i64
}

#[cfg(test)]
mod tests {
    use super::should_absorb_with_threshold;

    #[test]
    fn sidecar_gate_source_is_always_filtered() {
        // Threshold inactive (0.0): sidecar-gate still skipped regardless of importance.
        assert!(!should_absorb_with_threshold("sidecar-gate:deploy", 9.0, 0.0));
    }

    #[test]
    fn sidecar_gate_exact_prefix_is_filtered() {
        assert!(!should_absorb_with_threshold("sidecar-gate", 9.0, 0.0));
    }

    #[test]
    fn normal_source_passes_with_inactive_threshold() {
        assert!(should_absorb_with_threshold("claude-session", 4.0, 0.0));
        assert!(should_absorb_with_threshold("kleos-server", 6.0, 0.0));
    }

    #[test]
    fn below_threshold_is_filtered() {
        assert!(!should_absorb_with_threshold("claude-session", 3.0, 5.0));
    }

    #[test]
    fn at_threshold_passes() {
        // Skip only when strictly below the threshold.
        assert!(should_absorb_with_threshold("claude-session", 5.0, 5.0));
    }

    #[test]
    fn above_threshold_passes() {
        assert!(should_absorb_with_threshold("claude-session", 6.0, 5.0));
    }

    #[test]
    fn sidecar_gate_wins_over_high_importance_and_threshold() {
        // sidecar-gate is filtered even above an active threshold.
        assert!(!should_absorb_with_threshold("sidecar-gate:x", 10.0, 5.0));
    }

    #[test]
    fn new_sidecar_default_source_is_filtered() {
        // The post-#217 sidecar (gate.rs/watcher.rs removed upstream) posts
        // observations with source="sidecar" (kleos-sidecar/src/main.rs:217),
        // not "sidecar-gate". The Patch 39 guard must still catch it.
        assert!(!should_absorb_with_threshold("sidecar", 9.0, 0.0));
    }

    #[test]
    fn sidecar_gate_still_filtered_after_widening() {
        // Both the legacy and current sidecar source forms are covered by
        // the same "sidecar" prefix -- no regression on the original guard.
        assert!(!should_absorb_with_threshold("sidecar-gate", 9.0, 0.0));
        assert!(!should_absorb_with_threshold("sidecar-gate:deploy", 9.0, 0.0));
    }

    #[test]
    fn normal_source_still_passes_after_widening() {
        // A source that merely contains "sidecar" as a substring (not a
        // prefix) must not be caught by the widened guard.
        assert!(should_absorb_with_threshold("claude-session", 4.0, 0.0));
        assert!(should_absorb_with_threshold("not-a-sidecar-thing", 4.0, 0.0));
    }
}
