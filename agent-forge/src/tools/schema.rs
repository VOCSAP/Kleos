//! `schema` tool -- emits the machine-readable JSON Schema for a subcommand's
//! input struct via `schemars`. Complements `help`, which is human-prose:
//!
//!   agent-forge help spec-task         -> prose with REQUIRED/OPTIONAL/EXAMPLE
//!   agent-forge schema --command spec-task -> JSON Schema (Draft 7)
//!
//! When the schema diverges from the help text, the schema wins (it is
//! derived directly from the Rust struct that the dispatch path actually
//! parses).

use schemars::{schema_for, JsonSchema};
use serde_json::Value;

use crate::tools;

/// Render a `schemars`-derived schema to a pretty-printed JSON string. Keeps
/// the formatting consistent for the stdout output regardless of caller.
fn render<T: JsonSchema>() -> String {
    let schema = schema_for!(T);
    // schemars produces a `RootSchema` -> serde_json -> pretty string.
    let value: Value = serde_json::to_value(&schema)
        .expect("RootSchema serializes to JSON by construction");
    serde_json::to_string_pretty(&value)
        .expect("Value pretty-prints by construction")
}

/// Look up the JSON Schema for one subcommand by its kebab-case name. Returns
/// `None` when the name does not map to a known input struct (e.g. `help`,
/// `schema`, or any future subcommand whose author forgot to register here).
///
/// When adding a new subcommand: derive `JsonSchema` on its `*Input` struct
/// and add a match arm below. `help.rs::KNOWN_SUBCOMMANDS` is the human-side
/// inventory; this match arm is the machine-side inventory.
pub fn for_command(name: &str) -> Option<String> {
    Some(match name {
        "spec-task" => render::<tools::spec::SpecTaskInput>(),
        "update-spec" => render::<tools::spec::UpdateSpecInput>(),
        "list-specs" => render::<tools::spec::ListSpecsInput>(),
        "get-spec" => render::<tools::spec::GetSpecInput>(),
        "log-hypothesis" => render::<tools::hypothesis::LogHypothesisInput>(),
        "log-outcome" => render::<tools::hypothesis::LogOutcomeInput>(),
        "recall-errors" => render::<tools::hypothesis::RecallErrorsInput>(),
        "verify" => render::<tools::verify::VerifyInput>(),
        "challenge-code" => render::<tools::verify::ChallengeCodeInput>(),
        "session-diff" => render::<tools::verify::SessionDiffInput>(),
        "comment-check" => render::<tools::comments::CommentCheckInput>(),
        "checkpoint" => render::<tools::session::CheckpointInput>(),
        "rollback" => render::<tools::session::RollbackInput>(),
        "session-learn" => render::<tools::session::SessionLearnInput>(),
        "session-recall" => render::<tools::session::SessionRecallInput>(),
        "think" => render::<tools::think::ThinkInput>(),
        "declare-unknowns" => render::<tools::think::DeclareUnknownsInput>(),
        "consider-approaches" => render::<tools::approaches::ConsiderApproachesInput>(),
        "repo-map" => render::<tools::ast::repo_map::RepoMapInput>(),
        "search-code" => render::<tools::ast::search::SearchCodeInput>(),
        "skill-search" => render::<tools::skills::SkillSearchInput>(),
        "skill-capture" => render::<tools::skills::SkillCaptureInput>(),
        "skill-record-exec" => render::<tools::skills::SkillRecordExecInput>(),
        "skill-fix" => render::<tools::skills::SkillFixInput>(),
        "skill-derive" => render::<tools::skills::SkillDeriveInput>(),
        "skill-lineage" => render::<tools::skills::SkillLineageInput>(),
        "stats" => render::<tools::stats::StatsInput>(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that every name in `help::KNOWN_SUBCOMMANDS` has a schema
    /// dispatch arm. If this test fails after adding a new subcommand, append
    /// the matching arm to `for_command` above.
    #[test]
    fn every_known_subcommand_has_schema() {
        for name in crate::tools::help::KNOWN_SUBCOMMANDS {
            assert!(
                for_command(name).is_some(),
                "help::KNOWN_SUBCOMMANDS lists '{}' but schema::for_command returns None",
                name
            );
        }
    }

    /// Sanity-check that one schema is valid parseable JSON with the expected
    /// top-level shape (a JSON Schema document has `$schema` or `type`).
    #[test]
    fn schema_is_valid_json() {
        let s = for_command("spec-task").expect("spec-task has schema");
        let v: Value = serde_json::from_str(&s).expect("schema parses as JSON");
        assert!(v.is_object(), "schema is a JSON object");
        // schemars emits `$schema`, `title`, and `type` at the root.
        assert!(v.get("$schema").is_some() || v.get("type").is_some());
    }

    #[test]
    fn unknown_command_returns_none() {
        assert!(for_command("nope").is_none());
        assert!(for_command("help").is_none());
        assert!(for_command("schema").is_none());
    }
}
