//! The `review` tool. Assembles a spec's full record into a reviewer-facing
//! document, leading with what is unverified because that is where review
//! effort belongs.

use crate::db::Database;
use crate::emit::gatekeeper::{guard_no_leaks, is_public_repo};
use crate::emit::model::load_spec_record;
use crate::emit::paths::{record_path, slugify, spec_dir};
use crate::emit::render::render_record;
use crate::emit::trust::{derive_trust, Trust};
use crate::json_io::Output;
use crate::tools::{ToolError, ToolResult};
use serde::Deserialize;
use std::path::PathBuf;

/// Input for `review`: which spec to assemble, where the repository lives, and
/// whether to persist the rendered record alongside returning it.
#[derive(Deserialize)]
pub struct ReviewInput {
    /// The spec to assemble.
    pub spec_id: Option<String>,
    /// Repository root. Defaults to the current directory.
    pub repo_root: Option<String>,
    /// Whether to write `record.md` to disk. Defaults to true.
    pub write: Option<bool>,
}

/// Assemble a spec's record for review. The banner leads with the trust tier so
/// a reviewer sees where the evidence is thin before reading anything else.
pub fn review(db: &Database, input: ReviewInput) -> ToolResult {
    let spec_id = input
        .spec_id
        .ok_or_else(|| ToolError::MissingField("spec_id".into()))?;

    let repo_root: PathBuf = input
        .repo_root
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    let record = load_spec_record(db, &spec_id)?;
    let trust = derive_trust(&record.verifications);

    // Both variants state the trust label verbatim and then add guidance the
    // label does not carry. Restating the label's own content in the surrounding
    // sentence, as an earlier version did, made the first thing a reviewer reads
    // say one fact three times.
    let banner = match trust {
        Trust::Unverified => format!(
            "> **Review priority:** {}. No verification run for this spec has \
             passed, so every decision below is unproved. Read them closely.\n\n",
            trust.label()
        ),
        Trust::SpecVerified => format!(
            "> **Review priority:** {}. The criteria were exercised, so read the \
             decisions below for judgment rather than for correctness.\n\n",
            trust.label()
        ),
    };

    let body = format!("{}{}", banner, render_record(&record, trust));

    guard_no_leaks(&body)?;

    let mut data = serde_json::json!({
        "review": body,
        "trust": format!("{:?}", trust),
        "requires_screening": is_public_repo(&repo_root),
    });

    if input.write.unwrap_or(true) {
        let slug = slugify(&record.task_description);
        std::fs::create_dir_all(spec_dir(&repo_root, &slug))
            .map_err(|e| ToolError::IoError(e.to_string()))?;
        let path = record_path(&repo_root, &slug);
        std::fs::write(&path, &body).map_err(|e| ToolError::IoError(e.to_string()))?;
        data["record_path"] = serde_json::json!(path.to_string_lossy());
    }

    let mut output = Output::ok(format!("Review assembled for {}", spec_id));
    output.data = Some(data);
    Ok(output)
}

#[cfg(test)]
/// Tests for the review assembler.
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// Create a database holding one spec whose verification failed.
    fn db_unverified(dir: &std::path::Path) -> Database {
        let db = Database::open(&dir.join("forge.db")).unwrap();
        db.conn()
            .execute_batch(
                r#"
                INSERT INTO specs (id, created_at, task_description, task_type,
                                   acceptance_criteria, status)
                VALUES ('spec_1', 1, 'Add a thing', 'feature', '["it works"]', 'active');

                INSERT INTO verifications (id, spec_id, created_at, command,
                                           exit_code, success, criteria_index)
                VALUES ('ver_1', 'spec_1', 1, 'cargo test', 1, 0, 0);
                "#,
            )
            .unwrap();
        db
    }

    /// A missing spec_id is a MissingField error.
    #[test]
    fn requires_spec_id() {
        let dir = tempdir().unwrap();
        let db = Database::open(&dir.path().join("forge.db")).unwrap();
        assert!(matches!(
            review(
                &db,
                ReviewInput {
                    spec_id: None,
                    repo_root: None,
                    write: None
                }
            ),
            Err(ToolError::MissingField(_))
        ));
    }

    /// A spec with only failing verifications is reported as unverified and
    /// carries the review banner that says so.
    #[test]
    fn unverified_spec_leads_with_a_warning() {
        let dir = tempdir().unwrap();
        let db = db_unverified(dir.path());
        let out = review(
            &db,
            ReviewInput {
                spec_id: Some("spec_1".into()),
                repo_root: Some(dir.path().to_string_lossy().to_string()),
                write: Some(false),
            },
        )
        .unwrap();
        let body = out.data.unwrap()["review"].as_str().unwrap().to_string();
        assert!(body.starts_with("> **Review priority:**"));
        assert!(body.contains("not independently verified"));
    }

    /// With write enabled the record document lands on disk.
    #[test]
    fn write_persists_the_record() {
        let dir = tempdir().unwrap();
        let db = db_unverified(dir.path());
        let out = review(
            &db,
            ReviewInput {
                spec_id: Some("spec_1".into()),
                repo_root: Some(dir.path().to_string_lossy().to_string()),
                write: Some(true),
            },
        )
        .unwrap();
        let path = out.data.unwrap()["record_path"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(std::fs::read_to_string(path)
            .unwrap()
            .contains("# Record: Add a thing"));
    }

    /// A review containing a concrete local home path is refused before the
    /// public record directory or file can be created.
    #[test]
    fn write_refuses_local_home_paths_before_persistence() {
        let dir = tempdir().unwrap();
        let db = db_unverified(dir.path());
        db.conn()
            .execute(
                "INSERT INTO verifications (id, spec_id, created_at, command, \
                 exit_code, success, criteria_index) \
                 VALUES ('ver_2', 'spec_1', 2, '/home/alice/private/check', 0, 1, 0)",
                [],
            )
            .unwrap();

        let error = review(
            &db,
            ReviewInput {
                spec_id: Some("spec_1".into()),
                repo_root: Some(dir.path().to_string_lossy().to_string()),
                write: Some(true),
            },
        )
        .err()
        .expect("local home path must prevent review persistence");

        assert!(error.to_string().contains("absolute home path"));
        assert!(!dir
            .path()
            .join("docs/agent-forge/work/add-a-thing/record.md")
            .exists());
    }
}
