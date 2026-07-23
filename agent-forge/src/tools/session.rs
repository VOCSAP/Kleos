//! Session lifecycle tools: `checkpoint` snapshots the current git HEAD for
//! later rollback; `rollback` restores a named checkpoint; `session_learn`
//! records a mid-session discovery (optionally forwarding it to Kleos as a
//! skill); `session_recall` retrieves past learnings by keyword search.

use crate::db::Database;
use crate::json_io::Output;
use crate::tools::{ToolError, ToolResult};
use chrono::Utc;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use std::process::Command;
use uuid::Uuid;

/// Input for `checkpoint`. `name` and `description` drive the git snapshot. The
/// remaining fields drive slice emission: supplying `spec_id` turns this
/// checkpoint into a documentation boundary, and `components` and `conditions`
/// carry the knowledge-transfer prose only a model can write.
#[derive(Deserialize)]
pub struct CheckpointInput {
    /// Unique checkpoint name.
    pub name: Option<String>,
    /// Optional human description of the checkpoint.
    pub description: Option<String>,
    /// Spec this checkpoint documents. Absent means snapshot only, no emission.
    pub spec_id: Option<String>,
    /// One-line statement of what this slice did.
    pub intent: Option<String>,
    /// One entry per component touched: what it does and under what conditions.
    /// Required when emitting: a slice with no component prose is refused.
    pub components: Option<Vec<String>>,
    /// Non-obvious conditions: root causes, gotchas, documented limitations.
    pub conditions: Option<Vec<String>>,
    /// Set false to snapshot without emitting even when `spec_id` is present.
    pub emit: Option<bool>,
    /// Repository root that owns the Git snapshot and any emitted document.
    /// Direct CLI calls default to the current directory.
    pub repo_root: Option<String>,
}

/// Record the current `git rev-parse HEAD` value under `name` so the agent can
/// return to this point if subsequent edits go wrong, and -- when a `spec_id` is
/// supplied -- render this slice of the work into a committed document.
///
/// The snapshot is committed to the database BEFORE the emission steps run, and
/// that ordering is deliberate. The snapshot is the rollback safety net, so it
/// must survive a failure to render or screen the document; losing the ability
/// to roll back because a leak scan refused some prose would be the worse
/// outcome by far. A caller therefore cannot read `Err` as "nothing happened":
/// the checkpoint row may exist with `spec_id` and `slice_index` left NULL. That
/// state is benign. A NULL `slice_index` is ignored by the `MAX` used for slice
/// numbering, so it cannot consume a number a later slice needs, and the
/// checkpoint remains rollback-able by repository-scoped name exactly like a
/// snapshot-only one.
pub fn checkpoint(db: &Database, input: CheckpointInput) -> ToolResult {
    let mut input = input;
    // Cloned rather than moved out: `input` is passed whole to `emit_slice`
    // below, so it has to stay intact.
    let name = input
        .name
        .clone()
        .ok_or_else(|| ToolError::MissingField("name".into()))?;

    let id = format!("ckpt_{}", &Uuid::new_v4().to_string()[..8]);
    let now = Utc::now().timestamp();

    // The canonical Git root owns both the snapshot and emitted documentation.
    // Resolving it first keeps a failed snapshot from becoming a success row.
    let requested_root = input.repo_root.as_deref().unwrap_or(".");
    let (repo_root, git_ref) = repository_snapshot(requested_root)?;
    input.repo_root = Some(repo_root.clone());

    db.conn()
        .execute(
            r#"
            INSERT INTO checkpoints (id, name, created_at, git_ref, description, repo_root)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(repo_root, name) DO UPDATE SET
                id = excluded.id,
                created_at = excluded.created_at,
                git_ref = excluded.git_ref,
                files_snapshot = NULL,
                description = excluded.description,
                spec_id = NULL,
                slice_index = NULL
            "#,
            rusqlite::params![id, name, now, git_ref, input.description, repo_root],
        )
        .map_err(|e| ToolError::DatabaseError(e.to_string()))?;

    let Some(spec_id) = input.spec_id.clone() else {
        return Ok(Output::ok_with_id(
            id,
            format!("Checkpoint '{}' created", name),
        ));
    };
    if input.emit == Some(false) {
        return Ok(Output::ok_with_id(
            id,
            format!("Checkpoint '{}' created (emission suppressed)", name),
        ));
    }

    emit_slice(db, &input, &id, &name, &spec_id)
}

/// Resolve a caller path to its canonical Git top-level and current commit.
fn repository_snapshot(repo_root: &str) -> Result<(String, String), ToolError> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel", "HEAD"])
        .current_dir(repo_root)
        .output()
        .map_err(|error| ToolError::IoError(format!("cannot inspect repo_root: {error}")))?;
    if !output.status.success() {
        return Err(ToolError::InvalidValue(format!(
            "repo_root is not a Git checkout: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| ToolError::InvalidValue(format!("Git output is not UTF-8: {error}")))?;
    let mut lines = stdout.lines();
    let reported_root = lines
        .next()
        .filter(|root| !root.trim().is_empty())
        .ok_or_else(|| ToolError::InvalidValue("Git did not report a repository root".into()))?;
    let git_ref = lines
        .next()
        .filter(|git_ref| !git_ref.trim().is_empty())
        .ok_or_else(|| ToolError::InvalidValue("Git did not report a HEAD commit".into()))?
        .to_string();
    let canonical_root = std::fs::canonicalize(reported_root)
        .map_err(|error| ToolError::IoError(format!("cannot canonicalize repo_root: {error}")))?;
    let canonical_root = canonical_root.into_os_string().into_string().map_err(|_| {
        ToolError::InvalidValue("canonical repository path is not valid UTF-8".into())
    })?;
    Ok((canonical_root, git_ref))
}

/// Render this checkpoint's slice document, refuse it if the leak scan trips,
/// write it beside the code, and record the slice number on the checkpoint row.
/// Compiled only under the `fluency` feature.
#[cfg(feature = "fluency")]
fn emit_slice(
    db: &Database,
    input: &CheckpointInput,
    id: &str,
    name: &str,
    spec_id: &str,
) -> ToolResult {
    use crate::emit::gatekeeper::{guard_no_leaks, is_public_repo};
    use crate::emit::model::load_spec_record;
    use crate::emit::paths::{slice_path, slices_dir, slugify};
    use crate::emit::render::{render_slice, SliceContent};
    use crate::emit::trust::derive_trust;
    use std::path::PathBuf;

    let repo_root: PathBuf = input
        .repo_root
        .clone()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    // Teaching is the point of a slice. An empty components list would render a
    // document whose every section reads "None recorded" -- hollow prose that
    // LOOKS like documentation, which is worse than none. Refuse it the same
    // way a leaking slice is refused: the snapshot above survives, no file is
    // written, and the error names exactly what a real slice needs. In a CLI
    // invoked by an agent, a refusal that names the missing prose IS the ask.
    let has_prose = input
        .components
        .as_ref()
        .is_some_and(|c| c.iter().any(|s| !s.trim().is_empty()));
    if !has_prose {
        return Err(ToolError::InvalidValue(
            "refusing to emit a hollow slice: `components` is empty. Describe each \
             component this slice touched -- what it does and under what conditions. \
             Pass `emit: false` to snapshot without a document."
                .into(),
        ));
    }

    let record = load_spec_record(db, spec_id)?;
    let trust = derive_trust(&record.verifications);
    let spec_slug = slugify(&record.task_description);

    // Slice numbering is per spec and one-based, so 001 is the first slice.
    let next_index: i64 = db
        .conn()
        .query_row(
            "SELECT COALESCE(MAX(slice_index), 0) + 1 FROM checkpoints WHERE spec_id = ?1",
            rusqlite::params![spec_id],
            |row| row.get(0),
        )
        .map_err(|e| ToolError::DatabaseError(e.to_string()))?;

    let content = SliceContent {
        intent: input.intent.clone().unwrap_or_else(|| name.to_string()),
        components: input.components.clone().unwrap_or_default(),
        conditions: input.conditions.clone().unwrap_or_default(),
    };
    let body = render_slice(next_index, &content, &record, trust);

    guard_no_leaks(&body)?;

    let dir = slices_dir(&repo_root, &spec_slug);
    std::fs::create_dir_all(&dir).map_err(|e| ToolError::IoError(e.to_string()))?;
    let path = slice_path(
        &repo_root,
        &spec_slug,
        next_index,
        &slugify(&content.intent),
    );
    std::fs::write(&path, &body).map_err(|e| ToolError::IoError(e.to_string()))?;

    db.conn()
        .execute(
            "UPDATE checkpoints SET spec_id = ?1, slice_index = ?2 WHERE id = ?3",
            rusqlite::params![spec_id, next_index, id],
        )
        .map_err(|e| ToolError::DatabaseError(e.to_string()))?;

    let mut output = Output::ok_with_id(
        id.to_string(),
        format!(
            "Checkpoint '{}' created, slice {:03} emitted",
            name, next_index
        ),
    );
    output.data = Some(serde_json::json!({
        "slice_path": path.to_string_lossy(),
        "slice_index": next_index,
        "requires_screening": is_public_repo(&repo_root),
    }));
    Ok(output)
}

/// Stand-in for `emit_slice` in builds without the `fluency` feature.
///
/// The snapshot has already been committed by the time this runs, so refusing
/// with an error would misreport what happened. Reporting success while
/// silently discarding the request would be worse still: the caller asked for a
/// document and would have no way to learn it was never written. This reports
/// the snapshot honestly and names the feature that would satisfy the request.
#[cfg(not(feature = "fluency"))]
fn emit_slice(
    _db: &Database,
    _input: &CheckpointInput,
    id: &str,
    name: &str,
    _spec_id: &str,
) -> ToolResult {
    let mut output = Output::ok_with_id(
        id.to_string(),
        format!(
            "Checkpoint '{}' created. Slice emission was requested but this build \
             does not include the emission layer; rebuild agent-forge with \
             `--features fluency` to write slice documents.",
            name
        ),
    );
    output.data = Some(serde_json::json!({
        "emitted": false,
        "reason": "the fluency feature is not enabled in this build",
    }));
    Ok(output)
}

/// Input for `rollback`: the name of a previously created checkpoint to restore.
#[derive(Deserialize)]
pub struct RollbackInput {
    /// Checkpoint name to restore.
    pub checkpoint_name: Option<String>,
    /// Repository whose working tree will be restored.
    pub repo_root: Option<String>,
}

/// Look up the git hash stored under `checkpoint_name` and run `git checkout`
/// to restore the working tree to that commit.
pub fn rollback(db: &Database, input: RollbackInput) -> ToolResult {
    let name = input
        .checkpoint_name
        .ok_or_else(|| ToolError::MissingField("checkpoint_name".into()))?;
    let requested_root = input.repo_root.as_deref().unwrap_or(".");
    let (repo_root, _) = repository_snapshot(requested_root)?;

    let git_ref: Option<String> = db
        .conn()
        .query_row(
            "SELECT git_ref FROM checkpoints WHERE name = ?1 AND repo_root = ?2",
            rusqlite::params![name, repo_root],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| ToolError::DatabaseError(error.to_string()))?;

    if let Some(ref git_hash) = git_ref {
        // FORGE-1 fix: refuse to checkout over a dirty working tree. `git checkout
        // <hash>` aborts when there are uncommitted changes, so we detect the
        // condition early and report a clear error rather than returning Ok on a
        // failed checkout. A dirty tree also makes rollback semantics ambiguous --
        // the agent must commit or stash first.
        let porcelain = Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&repo_root)
            .output()
            .map_err(|e| ToolError::IoError(e.to_string()))?;

        if !porcelain.stdout.is_empty() {
            return Err(ToolError::IoError(
                "Working tree is dirty -- commit or stash changes before rolling back".into(),
            ));
        }

        let status = Command::new("git")
            .args(["checkout", git_hash])
            .current_dir(&repo_root)
            .status()
            .map_err(|e| ToolError::IoError(e.to_string()))?;

        if !status.success() {
            return Err(ToolError::IoError("git checkout failed".into()));
        }

        // `git checkout <hash>` produces a detached HEAD. Report this explicitly
        // so the caller knows branch tracking is suspended until they run
        // `git checkout -b <branch>` or `git switch -`.
        return Ok(Output::ok(format!(
            "Rolled back to checkpoint '{}' (detached HEAD at {}). \
            Run `git checkout -b <branch>` or `git switch -` to reattach.",
            name, git_hash
        )));
    }

    let legacy_exists: bool = db
        .conn()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM checkpoints WHERE name = ?1 AND repo_root IS NULL)",
            rusqlite::params![name],
            |row| row.get(0),
        )
        .map_err(|error| ToolError::DatabaseError(error.to_string()))?;
    if legacy_exists {
        return Err(ToolError::InvalidValue(format!(
            "Checkpoint '{name}' predates repository scoping and cannot be restored; create it again in this repository"
        )));
    }
    Err(ToolError::InvalidValue(format!(
        "Checkpoint not found in this repository: {name}"
    )))
}

/// Input for `session_learn`: the insight to record plus optional context,
/// tags, spec linkage, and a flag to simultaneously capture it as a Kleos skill.
#[derive(Deserialize)]
pub struct SessionLearnInput {
    pub discovery: Option<String>,
    pub context: Option<String>,
    pub tags: Option<Vec<String>>,
    pub capture_as_skill: Option<bool>,
    pub spec_id: Option<String>,
}

/// Largest recall page accepted from CLI or MCP callers.
const MAX_SESSION_RECALL_LIMIT: usize = 100;

/// Persist a mid-session discovery to the `session_learns` table. If
/// `capture_as_skill` is true, also forward the discovery text to the Kleos
/// skill capture endpoint (best-effort -- failures are logged but do not abort).
pub fn session_learn(db: &Database, input: SessionLearnInput) -> ToolResult {
    let capture_as_skill = input.capture_as_skill;
    let spec_id = input.spec_id;

    let discovery = input
        .discovery
        .ok_or_else(|| ToolError::MissingField("discovery".into()))?;
    if discovery.trim().is_empty() {
        return Err(ToolError::InvalidValue(
            "discovery must contain non-whitespace text".into(),
        ));
    }

    let id = format!("learn_{}", &Uuid::new_v4().to_string()[..8]);
    let now = Utc::now().timestamp();

    db.conn()
        .execute(
            r#"
            INSERT INTO session_learns (id, created_at, discovery, context, tags, spec_id)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            rusqlite::params![
                id,
                now,
                discovery,
                input.context,
                input.tags.map(|t| serde_json::to_string(&t).unwrap()),
                spec_id,
            ],
        )
        .map_err(|e| ToolError::DatabaseError(e.to_string()))?;

    let mut skill_info = None;
    if capture_as_skill.unwrap_or(false) {
        if let Ok(client) = crate::kleos_client::KleosClient::new() {
            match client.capture_skill(&discovery, Some("agent-forge")) {
                Ok(v) => {
                    skill_info = Some(v);
                }
                Err(e) => {
                    // Best-effort: log but don't fail the session_learn
                    eprintln!("warning: skill capture failed: {}", e);
                }
            }
        }
    }

    let mut output = Output::ok_with_id(id, "Learning recorded");
    if let Some(info) = skill_info {
        output.data = Some(serde_json::json!({
            "skill_captured": true,
            "skill": info,
        }));
    }
    Ok(output)
}

/// Input for `session_recall`: a keyword to search past learnings and a result cap.
#[derive(Deserialize)]
pub struct SessionRecallInput {
    pub query: Option<String>,
    pub limit: Option<usize>,
}

/// Search `session_learns` for rows whose `discovery` text contains `query`,
/// returning the most-recent matches up to `limit` (default 10).
pub fn session_recall(db: &Database, input: SessionRecallInput) -> ToolResult {
    let query = input.query.unwrap_or_default();
    let limit = input.limit.unwrap_or(10);
    if !(1..=MAX_SESSION_RECALL_LIMIT).contains(&limit) {
        return Err(ToolError::InvalidValue(format!(
            "limit must be between 1 and {MAX_SESSION_RECALL_LIMIT}"
        )));
    }

    let mut stmt = db
        .conn()
        .prepare(
            r#"
            SELECT id, discovery, context, tags
            FROM session_learns
            WHERE discovery LIKE ?1
            ORDER BY created_at DESC
            LIMIT ?2
            "#,
        )
        .map_err(|e| ToolError::DatabaseError(e.to_string()))?;

    let pattern = format!("%{}%", query);
    let rows = stmt
        .query_map(rusqlite::params![pattern, limit], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "discovery": row.get::<_, String>(1)?,
                "context": row.get::<_, Option<String>>(2)?,
                "tags": row.get::<_, Option<String>>(3)?,
            }))
        })
        .map_err(|e| ToolError::DatabaseError(e.to_string()))?;

    let results: Vec<_> = rows.filter_map(|r| r.ok()).collect();

    let mut output = Output::ok(format!("Found {} learnings", results.len()));
    output.data = Some(serde_json::json!({ "results": results }));
    Ok(output)
}

#[cfg(test)]
/// Tests for checkpoint slice emission.
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::tempdir;

    /// Create a database holding one spec with a chosen approach.
    fn db_with_spec(dir: &Path) -> Database {
        assert!(Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir)
            .status()
            .unwrap()
            .success());
        commit_state(dir, "baseline");
        let db = Database::open(&dir.join("forge.db")).unwrap();
        db.conn()
            .execute_batch(
                r#"
                INSERT INTO specs (id, created_at, task_description, task_type,
                                   acceptance_criteria, status)
                VALUES ('spec_1', 1, 'Add a thing', 'feature', '["it works"]', 'active');

                INSERT INTO approaches (id, spec_id, created_at, name, description,
                                        pros, cons, score, chosen)
                VALUES ('appr_1', 'spec_1', 1, 'Direct', 'Do it directly',
                        '[]', '[]', 8.0, 1);
                "#,
            )
            .unwrap();
        db
    }

    /// Commit one state-file revision and return the resulting Git object ID.
    fn commit_state(repo: &Path, contents: &str) -> String {
        std::fs::write(repo.join("state.txt"), contents).unwrap();
        assert!(Command::new("git")
            .args(["add", "state.txt"])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .args([
                "-c",
                "user.name=Agent Forge Test",
                "-c",
                "user.email=agent-forge@example.invalid",
                "commit",
                "-q",
                "-m",
                "test state",
            ])
            .current_dir(repo)
            .status()
            .unwrap()
            .success());
        String::from_utf8(
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(repo)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .to_string()
    }

    /// Checkpoint and rollback Git operations stay rooted in the requested
    /// repository even when the Agent-Forge process runs somewhere else.
    #[test]
    fn checkpoint_and_rollback_use_repo_root() {
        let repo = tempdir().unwrap();
        let db_dir = tempdir().unwrap();
        assert!(Command::new("git")
            .args(["init", "-q"])
            .current_dir(repo.path())
            .status()
            .unwrap()
            .success());
        let first_ref = commit_state(repo.path(), "first");
        let db = db_with_spec(db_dir.path());

        checkpoint(
            &db,
            CheckpointInput {
                name: Some("rooted".into()),
                description: None,
                spec_id: None,
                intent: None,
                components: None,
                conditions: None,
                emit: None,
                repo_root: Some(repo.path().to_string_lossy().to_string()),
            },
        )
        .unwrap();
        let stored_ref: Option<String> = db
            .conn()
            .query_row(
                "SELECT git_ref FROM checkpoints WHERE name = 'rooted'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_ref.as_deref(), Some(first_ref.as_str()));

        let second_ref = commit_state(repo.path(), "second");
        assert_ne!(first_ref, second_ref);
        rollback(
            &db,
            RollbackInput {
                checkpoint_name: Some("rooted".into()),
                repo_root: Some(repo.path().to_string_lossy().to_string()),
            },
        )
        .unwrap();
        let restored_ref = String::from_utf8(
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(repo.path())
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        assert_eq!(restored_ref.trim(), first_ref);
    }

    /// Identical checkpoint names remain isolated across repository roots, and
    /// rollback selects only the snapshot owned by the requested repository.
    #[test]
    fn checkpoint_names_are_repository_scoped() {
        let first_repo = tempdir().unwrap();
        let second_repo = tempdir().unwrap();
        let db_dir = tempdir().unwrap();
        for repo in [first_repo.path(), second_repo.path()] {
            assert!(Command::new("git")
                .args(["init", "-q"])
                .current_dir(repo)
                .status()
                .unwrap()
                .success());
        }
        let first_ref = commit_state(first_repo.path(), "first repository");
        let second_ref = commit_state(second_repo.path(), "second repository");
        assert_ne!(first_ref, second_ref);
        let db = db_with_spec(db_dir.path());

        for repo in [first_repo.path(), second_repo.path()] {
            checkpoint(
                &db,
                CheckpointInput {
                    name: Some("shared".into()),
                    description: None,
                    spec_id: None,
                    intent: None,
                    components: None,
                    conditions: None,
                    emit: None,
                    repo_root: Some(repo.to_string_lossy().to_string()),
                },
            )
            .unwrap();
        }
        let scoped_rows: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM checkpoints WHERE name = 'shared'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(scoped_rows, 2);

        let advanced_ref = commit_state(second_repo.path(), "advanced");
        assert_ne!(advanced_ref, second_ref);
        rollback(
            &db,
            RollbackInput {
                checkpoint_name: Some("shared".into()),
                repo_root: Some(second_repo.path().to_string_lossy().to_string()),
            },
        )
        .unwrap();
        let restored_ref = repository_snapshot(second_repo.path().to_str().unwrap())
            .unwrap()
            .1;
        assert_eq!(restored_ref, second_ref);
    }

    /// Repository resolution stores the canonical top-level even when a caller
    /// starts Agent-Forge from a nested directory.
    #[test]
    fn repository_snapshot_canonicalizes_nested_paths() {
        let repo = tempdir().unwrap();
        assert!(Command::new("git")
            .args(["init", "-q"])
            .current_dir(repo.path())
            .status()
            .unwrap()
            .success());
        let expected_ref = commit_state(repo.path(), "nested");
        let nested = repo.path().join("nested");
        std::fs::create_dir(&nested).unwrap();

        let (root, git_ref) = repository_snapshot(nested.to_str().unwrap()).unwrap();
        let expected_root = std::fs::canonicalize(repo.path())
            .unwrap()
            .into_os_string()
            .into_string()
            .unwrap();
        assert_eq!(root, expected_root);
        assert_eq!(git_ref, expected_ref);
    }

    /// Learning rejects hollow discoveries before writing a database row.
    #[test]
    fn session_learn_rejects_blank_discovery() {
        let dir = tempdir().unwrap();
        let db = db_with_spec(dir.path());
        let error = session_learn(
            &db,
            SessionLearnInput {
                discovery: Some("   ".into()),
                context: None,
                tags: None,
                capture_as_skill: Some(false),
                spec_id: Some("spec_1".into()),
            },
        )
        .err()
        .expect("blank learning must fail");
        assert!(error.to_string().contains("non-whitespace"));
    }

    /// Recall rejects empty and excessive page sizes at the shared typed boundary.
    #[test]
    fn session_recall_rejects_invalid_limits() {
        let dir = tempdir().unwrap();
        let db = db_with_spec(dir.path());
        for limit in [0, MAX_SESSION_RECALL_LIMIT + 1] {
            let error = session_recall(
                &db,
                SessionRecallInput {
                    query: None,
                    limit: Some(limit),
                },
            )
            .err()
            .expect("invalid recall limit must fail");
            assert!(error.to_string().contains("limit must be between"));
        }
    }

    /// Build a checkpoint input that requests emission for `spec_1`.
    fn emitting_input(repo: &Path, name: &str) -> CheckpointInput {
        CheckpointInput {
            name: Some(name.into()),
            description: None,
            spec_id: Some("spec_1".into()),
            intent: Some("wire it up".into()),
            components: Some(vec!["Renderer -- builds markdown".into()]),
            conditions: Some(vec!["Empty specs still render".into()]),
            emit: Some(true),
            repo_root: Some(repo.to_string_lossy().to_string()),
        }
    }

    /// A checkpoint without a spec_id stays a plain git snapshot and writes no files.
    #[test]
    fn checkpoint_without_spec_id_emits_nothing() {
        let dir = tempdir().unwrap();
        let db = db_with_spec(dir.path());
        let out = checkpoint(
            &db,
            CheckpointInput {
                name: Some("plain".into()),
                description: None,
                spec_id: None,
                intent: None,
                components: None,
                conditions: None,
                emit: None,
                repo_root: Some(dir.path().to_string_lossy().to_string()),
            },
        )
        .unwrap();
        assert!(out.success);
        assert!(out.data.is_none());
        assert!(!dir.path().join("docs/agent-forge").exists());
    }

    /// A checkpoint carrying a spec_id writes a numbered slice document.
    #[cfg(feature = "fluency")]
    #[test]
    fn checkpoint_with_spec_id_writes_a_slice() {
        let dir = tempdir().unwrap();
        let db = db_with_spec(dir.path());
        let out = checkpoint(&db, emitting_input(dir.path(), "first")).unwrap();
        assert!(out.success);

        let data = out.data.expect("emission data");
        let path = data["slice_path"].as_str().unwrap();
        assert_eq!(data["slice_index"].as_i64().unwrap(), 1);

        let body = std::fs::read_to_string(path).unwrap();
        assert!(body.contains("# Slice 001: wire it up"));
        assert!(body.contains("Renderer -- builds markdown"));
        assert!(body.contains("## Decision: Direct"));
    }

    /// Slice indices increment per spec across successive checkpoints.
    #[cfg(feature = "fluency")]
    #[test]
    fn slice_indices_increment() {
        let dir = tempdir().unwrap();
        let db = db_with_spec(dir.path());
        checkpoint(&db, emitting_input(dir.path(), "first")).unwrap();
        let out = checkpoint(&db, emitting_input(dir.path(), "second")).unwrap();
        assert_eq!(out.data.unwrap()["slice_index"].as_i64().unwrap(), 2);
    }

    /// A checkpoint that requests emission without component prose is refused:
    /// no document is written, the error names the missing field, and the
    /// snapshot half of the checkpoint survives so rollback still works. A
    /// whitespace-only entry counts as empty, so the laziest possible caller
    /// cannot satisfy the gate with `[""]`.
    #[cfg(feature = "fluency")]
    #[test]
    fn hollow_slice_is_refused() {
        let dir = tempdir().unwrap();
        let db = db_with_spec(dir.path());

        let mut input = emitting_input(dir.path(), "hollow");
        input.components = Some(vec![]);
        let err = checkpoint(&db, input).err().unwrap();
        assert!(err.to_string().contains("components"));

        let mut input = emitting_input(dir.path(), "hollow-blank");
        input.components = Some(vec!["   ".into()]);
        assert!(checkpoint(&db, input).is_err());

        assert!(!dir.path().join("docs/agent-forge/work").exists());

        // Both snapshots persisted despite the refusals, with no slice number
        // consumed.
        let snapshots: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM checkpoints WHERE slice_index IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(snapshots, 2);
    }

    /// Content that trips the leak scan is refused and no file is written.
    /// The error message is asserted, not merely the fact of an error, so this
    /// test pins the leak guard specifically rather than passing on any failure
    /// that happens to occur earlier in the function.
    #[cfg(feature = "fluency")]
    #[test]
    fn leaking_content_is_refused() {
        let dir = tempdir().unwrap();
        let db = db_with_spec(dir.path());
        let mut input = emitting_input(dir.path(), "leaky");
        input.components = Some(vec!["Talks to 10.0.0.1 directly".into()]);

        let err = checkpoint(&db, input).err().unwrap();
        assert!(err.to_string().contains("refusing to emit"));
        assert!(!dir.path().join("docs/agent-forge/work").exists());
    }

    /// Without the `fluency` feature a checkpoint that requests emission still
    /// snapshots, writes no files, and says so rather than reporting a silent
    /// success that would leave the caller believing a document exists.
    #[cfg(not(feature = "fluency"))]
    #[test]
    fn checkpoint_reports_emission_not_compiled_in() {
        let dir = tempdir().unwrap();
        let db = db_with_spec(dir.path());
        let out = checkpoint(&db, emitting_input(dir.path(), "first")).unwrap();

        assert!(out.success);
        assert!(out.message.contains("--features fluency"));
        assert_eq!(out.data.unwrap()["emitted"], serde_json::json!(false));
        assert!(!dir.path().join("docs/agent-forge").exists());
    }

    /// `emit: false` suppresses the document while still taking the snapshot.
    /// The checkpoint row keeps a NULL `slice_index`, so a suppressed checkpoint
    /// cannot consume a slice number that a later real slice would need.
    #[test]
    fn emit_false_suppresses_the_document() {
        let dir = tempdir().unwrap();
        let db = db_with_spec(dir.path());
        let mut input = emitting_input(dir.path(), "suppressed");
        input.emit = Some(false);

        let out = checkpoint(&db, input).unwrap();
        assert!(out.success);
        assert!(out.data.is_none());
        assert!(!dir.path().join("docs/agent-forge/work").exists());

        let indexed: i64 = db
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM checkpoints WHERE slice_index IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(indexed, 0);
    }
}
