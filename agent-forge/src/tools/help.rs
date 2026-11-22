//! `help` tool -- prints structured documentation about agent-forge to stdout.
//! Bootstraps an AI agent's discovery of the workflow without forcing it to
//! discover input fields by trial-and-error (which produces the iterative
//! "Missing required field" -> "Invalid value: minimum N items" cascade).
//!
//! Two-tier doc:
//!   - `agent-forge help`                 -> overview + workflow + subcmd list
//!   - `agent-forge help <subcommand>`    -> human-readable input schema for one
//!
//! For machine-readable JSON Schema, see `agent-forge schema --command <X>`
//! (Phase 2, schemars-derived).

/// Top-level overview emitted by `agent-forge help` with no argument.
pub const OVERVIEW: &str = r#"agent-forge -- structured reasoning workflow for AI coding agents

WORKFLOW
  1. spec-task            Define the task before any code touches disk
  2. consider-approaches  (optional) Compare design alternatives explicitly
  3. log-hypothesis       Before any bugfix, declare your root-cause hypothesis
  4. <edit code>          Implementation (gated by enforce-agent-forge hook)
  5. verify               Run tests/build commands, persist pass/fail per criterion
  6. log-outcome          Close the hypothesis: correct / incorrect / partial
  7. update-spec          Transition spec to completed / failed / blocked

DISCOVERY
  agent-forge help                    Print this overview
  agent-forge help <subcommand>       Detailed input schema for one subcommand
  agent-forge schema --command <X>    Machine-readable JSON Schema (schemars)

SUBCOMMANDS BY PHASE
  Spec lifecycle:    spec-task, update-spec, list-specs, get-spec
  Hypothesis:        log-hypothesis, log-outcome, recall-errors
  Verify:            verify, challenge-code, comment-check, session-diff
  Session:           checkpoint, rollback, session-learn, session-recall
  Reasoning:         think, declare-unknowns, consider-approaches
  Code search (AST): repo-map, search-code
  Kleos skills:      skill-search, skill-capture, skill-record-exec,
                     skill-fix, skill-derive, skill-lineage
  Observability:     stats

INVOCATION
  agent-forge --input <file.json> --output <file.json> <subcommand>

  Every subcommand reads its body from --input and writes a uniform envelope
  {success, id?, message, data?} to --output. `help` and `schema` are the
  only subcommands that print to stdout instead and ignore --input/--output.

COMMON PITFALLS (see ~/.claude/rules/agent-forge.md)
  - task_type "fix" is INVALID. Use "bugfix".
  - edge_cases requires >= 3 items.
  - acceptance_criteria requires >= 2 items.
  - interface_contract must be a STRING, not a nested JSON object.
  - verify accepts {steps: [{command, ...}]} for multi-step runs (preferred
    over chaining with &&, since && masks which step failed).
  - confidence is in [0.0, 1.0] (not 0-100).
"#;

/// Look up the help text for one subcommand by its kebab-case name (the form
/// printed by the overview). Returns `None` when the name is not recognised.
pub fn per_subcmd(name: &str) -> Option<&'static str> {
    Some(match name {
        "spec-task" => SPEC_TASK,
        "update-spec" => UPDATE_SPEC,
        "list-specs" => LIST_SPECS,
        "get-spec" => GET_SPEC,
        "log-hypothesis" => LOG_HYPOTHESIS,
        "log-outcome" => LOG_OUTCOME,
        "recall-errors" => RECALL_ERRORS,
        "verify" => VERIFY,
        "challenge-code" => CHALLENGE_CODE,
        "comment-check" => COMMENT_CHECK,
        "session-diff" => SESSION_DIFF,
        "checkpoint" => CHECKPOINT,
        "rollback" => ROLLBACK,
        "session-learn" => SESSION_LEARN,
        "session-recall" => SESSION_RECALL,
        "think" => THINK,
        "declare-unknowns" => DECLARE_UNKNOWNS,
        "consider-approaches" => CONSIDER_APPROACHES,
        "repo-map" => REPO_MAP,
        "search-code" => SEARCH_CODE,
        "skill-search" => SKILL_SEARCH,
        "skill-capture" => SKILL_CAPTURE,
        "skill-record-exec" => SKILL_RECORD_EXEC,
        "skill-fix" => SKILL_FIX,
        "skill-derive" => SKILL_DERIVE,
        "skill-lineage" => SKILL_LINEAGE,
        "stats" => STATS,
        _ => return None,
    })
}

/// List of recognised subcommand names (used by the unknown-subcommand error
/// path to suggest valid names).
pub const KNOWN_SUBCOMMANDS: &[&str] = &[
    "spec-task",
    "update-spec",
    "list-specs",
    "get-spec",
    "log-hypothesis",
    "log-outcome",
    "recall-errors",
    "verify",
    "challenge-code",
    "comment-check",
    "session-diff",
    "checkpoint",
    "rollback",
    "session-learn",
    "session-recall",
    "think",
    "declare-unknowns",
    "consider-approaches",
    "repo-map",
    "search-code",
    "skill-search",
    "skill-capture",
    "skill-record-exec",
    "skill-fix",
    "skill-derive",
    "skill-lineage",
    "stats",
];

// ---------------------------------------------------------------------------
// Per-subcommand help blocks. Source-of-truth is the matching `*Input` struct
// in `tools/<file>.rs`. When that struct changes, update the block below and
// keep `agent-forge schema --command <X>` (schemars-derived) as the
// machine-checkable cross-reference.
// ---------------------------------------------------------------------------

const SPEC_TASK: &str = r#"spec-task -- Define a new task spec before any code is written

REQUIRED:
  task_description     string         Plain-English description of what to build
  task_type            string enum    One of: feature, bugfix, refactor,
                                      enhancement, test, docs ("fix" is INVALID)
  acceptance_criteria  array<string>  Minimum 2 items
  interface_contract   string         Free prose: files touched, fn signatures,
                                      expected behaviour (NOT a JSON object)
  edge_cases           array<string>  Minimum 3 items

OPTIONAL:
  files_to_touch       array<string>  Paths the task is expected to modify
  dependencies         string         Free-form notes on inter-task ordering

RETURNS:
  id                   spec_xxxxxxxx
  message              "Spec created"
  data.related_skills  (best-effort) Kleos skill suggestions for this task

SIDE EFFECT:
  Sets the session-active marker so the enforce-agent-forge hook unblocks
  Write/Edit/MultiEdit until the spec is closed.

EXAMPLE:
  {
    "task_description": "Add help and schema subcommands to agent-forge",
    "task_type": "feature",
    "acceptance_criteria": [
      "agent-forge help prints overview to stdout",
      "agent-forge help spec-task prints input schema"
    ],
    "interface_contract": "main.rs adds Help/Schema variants; tools/help.rs new module; --input/--output become Option to allow help without files",
    "edge_cases": [
      "subcommand name does not exist",
      "no subcommand argument provided",
      "stdout encoding on Windows console"
    ]
  }
"#;

const UPDATE_SPEC: &str = r#"update-spec -- Transition an existing spec to a new status

REQUIRED:
  spec_id   string       The spec_xxxxxxxx ID returned by spec-task
  status    string enum  One of: active, completed, failed, blocked

OPTIONAL:
  note      string       Status note (recorded with the transition)

SIDE EFFECT:
  Sets completed_at automatically when status is "completed" or "failed".

EXAMPLE:
  {"spec_id": "spec_a3b4c5d6", "status": "completed", "note": "verified on LXC 121"}
"#;

const LIST_SPECS: &str = r#"list-specs -- List specs ordered by creation time descending

OPTIONAL:
  status    string enum  Filter to one of: active, completed, failed, blocked
  limit     integer      Result cap (default 20)

RETURNS:
  data.specs  array of {id, task_description, task_type, status, created_at,
                        completed_at, status_note}

EXAMPLE:
  {"status": "active", "limit": 10}
"#;

const GET_SPEC: &str = r#"get-spec -- Fetch a spec with all linked hypotheses/approaches/learnings/verifications

REQUIRED:
  spec_id   string  The spec_xxxxxxxx ID

RETURNS:
  data.spec            Full spec row
  data.hypotheses      Linked hypotheses
  data.approaches      Linked approaches
  data.learnings       Linked session_learns
  data.verifications   Linked verification records (pass/fail per step)

EXAMPLE:
  {"spec_id": "spec_a3b4c5d6"}
"#;

const LOG_HYPOTHESIS: &str = r#"log-hypothesis -- Record a root-cause hypothesis BEFORE touching any code

REQUIRED:
  bug_description   string   Observed symptoms, error message, repro steps
  hypothesis        string   Proposed root cause and reasoning

OPTIONAL:
  confidence        number   In [0.0, 1.0]. Default 0.7. NOT 0-100.
  spec_id           string   Link this hypothesis to a spec

RETURNS:
  id        hyp_xxxxxxxx
  message   "Hypothesis logged"

SIDE EFFECT:
  Sets the session-active marker so the enforce-agent-forge hook permits
  subsequent edits while the hypothesis is open.

EXAMPLE:
  {
    "bug_description": "TUI shows duplicate approvals for one Bash command",
    "hypothesis": "PreToolUse hooks run in parallel; even when one blocks, kleos-sh still posts /gate/check, creating an approval downstream",
    "confidence": 0.85
  }
"#;

const LOG_OUTCOME: &str = r#"log-outcome -- Close a hypothesis once the fix is verified or disproved

REQUIRED:
  hypothesis_id   string       The hyp_xxxxxxxx ID returned by log-hypothesis
  outcome         string enum  One of: correct, incorrect, partial

OPTIONAL:
  notes           string       Explanation of why the outcome was reached

EXAMPLE:
  {"hypothesis_id": "hyp_a3b4c5d6", "outcome": "correct", "notes": "confirmed by tcpdump"}
"#;

const RECALL_ERRORS: &str = r#"recall-errors -- LIKE search over past hypothesis records to avoid repeats

OPTIONAL:
  query   string   Keyword to match against bug_description and hypothesis
  limit   integer  Result cap (default 10)

RETURNS:
  data.results  array of {id, bug_description, hypothesis, outcome, notes}

EXAMPLE:
  {"query": "rate limit", "limit": 5}
"#;

const VERIFY: &str = r#"verify -- Run commands and persist pass/fail per spec criterion

EITHER provide a single command:
  command              string   Shell-free command line (argv split on whitespace)
  expected_exit_code   integer  Default 0

OR a list of steps (preferred for multi-command verification):
  steps   array<{command, expected_exit_code?, label?}>

OPTIONAL:
  spec_id          string   Link results to a spec
  criteria_index   integer  Which acceptance criterion this verifies
  skill_id         integer  Record an execution against a Kleos skill
  timeout_secs     integer  Per-step timeout (kill child on expiry)

SECURITY:
  Each command is parsed into argv and executed WITHOUT a shell, so neither
  pipes nor redirects work. Use --steps for sequences.

EXAMPLE:
  {
    "spec_id": "spec_a3b4c5d6",
    "steps": [
      {"command": "cargo check -p kleos-lib", "label": "check"},
      {"command": "cargo test -p kleos-lib --lib", "label": "unit-tests"}
    ]
  }
"#;

const CHALLENGE_CODE: &str = r#"challenge-code -- Build an adversarial review prompt for a file

REQUIRED:
  file_path   string  Path to the file to review

OPTIONAL:
  focus_areas array<string>  Default: security, performance, error_handling,
                             edge_cases, comment_coverage

RETURNS:
  data.prompt           Adversarial review prompt for the agent to apply
  data.comment_report   Mechanical comment-coverage scan (undocumented decls)
  data.lines            File line count

EXAMPLE:
  {"file_path": "kleos-lib/src/gate/validator.rs", "focus_areas": ["security", "edge_cases"]}
"#;

const COMMENT_CHECK: &str = r#"comment-check -- Scan a file for declarations missing a leading comment

REQUIRED:
  file_path   string  Path to the file to scan

SUPPORTED LANGUAGES:
  Rust (.rs), C family (.c .cpp .h .hpp .ts .tsx .js .jsx .go .java .kt .swift),
  Python (.py). Other extensions return zero findings.

RETURNS:
  data.total     Total declarations found
  data.missing   Count of declarations without a leading comment
  data.findings  Per-declaration {line, item, declaration} list

EXAMPLE:
  {"file_path": "agent-forge/src/tools/help.rs"}
"#;

const SESSION_DIFF: &str = r#"session-diff -- Summarise git changes before declaring a task done

OPTIONAL:
  base   string  Git ref to diff against (default "HEAD~10")

SECURITY:
  The ref is validated against [a-zA-Z0-9-_.~/^:@{}] and rejected if it
  starts with '-' (flag injection guard).

RETURNS:
  data.files  List of changed file paths
  data.stat   git diff --stat output

EXAMPLE:
  {"base": "main"}
"#;

const CHECKPOINT: &str = r#"checkpoint -- Snapshot current git HEAD under a name for later rollback

REQUIRED:
  name          string   Human-readable name of the checkpoint

OPTIONAL:
  description   string   Free-form description

RETURNS:
  id       ckpt_xxxxxxxx
  message  "Checkpoint '<name>' created"

EXAMPLE:
  {"name": "before-refactor", "description": "kleos-lib gate validator rewrite"}
"#;

const ROLLBACK: &str = r#"rollback -- Restore the working tree to a previously created checkpoint

REQUIRED:
  checkpoint_name   string  Name passed to a prior `checkpoint` call

EFFECT:
  Runs `git checkout <hash>` where <hash> was the HEAD at checkpoint creation.

EXAMPLE:
  {"checkpoint_name": "before-refactor"}
"#;

const SESSION_LEARN: &str = r#"session-learn -- Persist a mid-session discovery (optionally as Kleos skill)

REQUIRED:
  discovery          string         The non-obvious fact / insight discovered

OPTIONAL:
  context            string         Where this was observed
  tags               array<string>  Categorisation tags
  spec_id            string         Link to a spec
  capture_as_skill   boolean        Also forward to Kleos skill capture endpoint

EXAMPLE:
  {
    "discovery": "Axum TimeoutLayer at route level overrides global; shortest cap wins",
    "tags": ["axum", "rust", "timeout"],
    "capture_as_skill": true
  }
"#;

const SESSION_RECALL: &str = r#"session-recall -- LIKE search over past session_learns

OPTIONAL:
  query   string   Keyword to match against discovery text
  limit   integer  Result cap (default 10)

RETURNS:
  data.results  array of {id, discovery, context, tags}

EXAMPLE:
  {"query": "timeout", "limit": 5}
"#;

const THINK: &str = r#"think -- Emit a structured five-step reasoning prompt (no DB write)

REQUIRED:
  problem       string         The problem to reason about

OPTIONAL:
  constraints   array<string>  Hard constraints (cost, time, dependencies)
  context       string         What is already known

RETURNS:
  data.prompt   Five-step "know / find out / options / tradeoffs / recommend"
                prompt suitable for chain-of-thought reasoning.

EXAMPLE:
  {
    "problem": "Long-poll vs SSE for the approval TUI",
    "constraints": ["1-3 clients only", "rare events (few/day)"],
    "context": "Patch 20c shipped long-poll but rate-limit still saturates on bursts"
  }
"#;

const DECLARE_UNKNOWNS: &str = r#"declare-unknowns -- Surface blocking vs non-blocking unknowns explicitly

REQUIRED:
  unknowns   array<{description, blocking, resolution_hint?}>
             Minimum 1 item. blocking=true items halt forward progress.

RETURNS:
  data.blocking      Items that must be resolved before proceeding
  data.non_blocking  Items that can be deferred with caution
  data.action        "STOP: ..." or "OK: ..." directive

EXAMPLE:
  {
    "unknowns": [
      {"description": "Does Patch 25 ship with KLEOS_DATA_DIR set?", "blocking": true,
       "resolution_hint": "Check /etc/kleos/kleos.env on LXC 121"},
      {"description": "Are there other consumers of growth_reject_patterns?", "blocking": false}
    ]
  }
"#;

const CONSIDER_APPROACHES: &str = r#"consider-approaches -- Compare two-or-more design alternatives explicitly

REQUIRED:
  problem      string                    Statement of what is being decided
  approaches   array<{name, description, pros?, cons?, score?}>
               Minimum 2 items.

OPTIONAL:
  spec_id        string   Link the decision to a spec (validated for existence)
  chosen_index   integer  0-based index into approaches of the chosen option

RETURNS:
  data.ids                 Stored approach IDs (one per item)
  data.comparison_prompt   Prompt comparing pros/cons for agent reasoning

EXAMPLE:
  {
    "problem": "Help impl: hardcoded strings vs schemars-derived",
    "approaches": [
      {"name": "hardcoded", "description": "tools/help.rs with const strings",
       "pros": ["simple", "no new dep"], "cons": ["can diverge from struct"]},
      {"name": "schemars-derived", "description": "JsonSchema on every Input struct",
       "pros": ["always accurate"], "cons": ["20 structs to derive", "ugly output"]}
    ],
    "chosen_index": 0
  }
"#;

const REPO_MAP: &str = r#"repo-map -- AST-derived ranked symbol map of a repository

REQUIRED:
  path   string  Directory root to scan

OPTIONAL:
  focus       array<string>  Path fragments to boost (e.g. ["src/server"])
  max_tokens  integer        Cap on output size (default 4000)

RETURNS:
  data.map   Newline-delimited symbol map (file:line  kind  name) sorted
             by importance, truncated to fit max_tokens.

SUPPORTED LANGUAGES:
  Anything tree-sitter can parse (Rust, TS/JS, Python, Go, C, JSON).

EXAMPLE:
  {"path": "kleos-lib", "focus": ["src/gate"], "max_tokens": 6000}
"#;

const SEARCH_CODE: &str = r#"search-code -- AST-based symbol search (case-insensitive name match)

REQUIRED:
  query   string  Fragment to find in symbol names

OPTIONAL:
  path          string   Directory root (default ".")
  symbol_type   string   Kind filter: function, class, struct, enum, trait, ...
  limit         integer  Result cap (default 20)

RETURNS:
  data.results  array of {file, line, column, kind, name, context}

DIFFERENCE FROM Grep:
  Grep matches text (incl. comments and strings). search-code matches only
  declared symbol names parsed by tree-sitter. Use it when you want "where
  is X defined", not "where is X mentioned".

EXAMPLE:
  {"query": "rate_limit", "path": "kleos-server/src", "symbol_type": "function"}
"#;

const SKILL_SEARCH: &str = r#"skill-search -- Search Kleos for skills matching a query

REQUIRED:
  query   string   Free-form description to match against skill descriptions

OPTIONAL:
  limit   integer  Result cap

RETURNS:
  data.skills  array of skill records (id, description, score, ...)

EXAMPLE:
  {"query": "rust axum middleware rate limit", "limit": 10}
"#;

const SKILL_CAPTURE: &str = r#"skill-capture -- Register a new skill description in Kleos

REQUIRED:
  description   string  <= 2000 chars. The skill description to capture.

OPTIONAL:
  agent         string  Name of the capturing agent (e.g. "claude-session")

RETURNS:
  id       skill_id from Kleos
  data     Full Kleos response payload

EXAMPLE:
  {"description": "Run cargo test -p kleos-lib --features bundled-sqlite on Windows MSVC", "agent": "claude-session"}
"#;

const SKILL_RECORD_EXEC: &str = r#"skill-record-exec -- Record one execution attempt for a skill

REQUIRED:
  skill_id   integer   The skill ID returned by skill-capture
  success    boolean   Whether the execution succeeded

OPTIONAL:
  duration_ms     number   Wall-clock duration in milliseconds
  error_type      string   Short categorisation (e.g. "verify_failed", "timeout")
  error_message   string   Free-form error detail

EXAMPLE:
  {"skill_id": 42, "success": false, "duration_ms": 1500, "error_type": "verify_failed", "error_message": "exit 101"}
"#;

const SKILL_FIX: &str = r#"skill-fix -- Ask Kleos to derive a corrected version of an existing skill

REQUIRED:
  skill_id   integer   The skill to fix

OPTIONAL:
  hint       string    Free-text guidance on what to change

RETURNS:
  id       New skill_id of the corrected skill
  data     Full Kleos response

EXAMPLE:
  {"skill_id": 42, "hint": "the command needs --features bundled-sqlite on Windows"}
"#;

const SKILL_DERIVE: &str = r#"skill-derive -- Derive a new skill from one or more parent skills

REQUIRED:
  parent_ids   array<integer>  Minimum 1. Source skills to derive from.
  direction    string          <= 2000 chars. Prompt explaining the derivation.

OPTIONAL:
  agent        string          Name of the deriving agent

RETURNS:
  id       New skill_id
  data     Full Kleos response

EXAMPLE:
  {"parent_ids": [42, 47], "direction": "Combine these two cargo test patterns into one that handles both LXC and Windows"}
"#;

const SKILL_LINEAGE: &str = r#"skill-lineage -- Fetch ancestor/descendant lineage for a skill

REQUIRED:
  skill_id   integer   The skill whose lineage to fetch

RETURNS:
  data       Lineage graph (ancestors + descendants) for the skill

EXAMPLE:
  {"skill_id": 42}
"#;

const STATS: &str = r#"stats -- Aggregate counts and rates from the forge DB

OPTIONAL:
  days   integer  Time window in days (default 30)

RETURNS:
  data   Counts for specs / hypotheses / verifications / learnings / approaches
         / checkpoints over the window, plus derived rates (spec completion
         rate, hypothesis accuracy, verification pass rate, top error patterns).

EXAMPLE:
  {"days": 7}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_known_subcommand_has_help() {
        for name in KNOWN_SUBCOMMANDS {
            assert!(
                per_subcmd(name).is_some(),
                "KNOWN_SUBCOMMANDS lists '{}' but per_subcmd returns None",
                name
            );
        }
    }

    #[test]
    fn unknown_subcommand_returns_none() {
        assert!(per_subcmd("nope").is_none());
        assert!(per_subcmd("").is_none());
    }

    #[test]
    fn overview_mentions_workflow_and_discovery() {
        assert!(OVERVIEW.contains("WORKFLOW"));
        assert!(OVERVIEW.contains("DISCOVERY"));
        assert!(OVERVIEW.contains("agent-forge schema"));
    }
}
