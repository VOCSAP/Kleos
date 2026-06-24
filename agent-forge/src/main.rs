//! agent-forge CLI entrypoint. Each subcommand reads a JSON input file, runs
//! one tool from the `tools` module against the on-disk SQLite forge DB, and
//! writes a JSON result back. External hooks can enforce that agents call
//! these tools before and after editing code.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use agent_forge::db::Database;
use agent_forge::json_io::{read_input, write_output, Output};
use agent_forge::tools;

/// Top-level CLI: every invocation specifies a subcommand plus input/output JSON paths.
/// `--input` and `--output` are `Option`al at the parser level because the `Help`
/// (and Phase 2 `Schema`) subcommands print to stdout and need no file I/O. All
/// other subcommands require both flags and validate at dispatch time.
#[derive(Parser)]
#[command(name = "agent-forge")]
#[command(about = "Structured reasoning and code quality workflow")]
// Disable clap's auto-generated `help` subcommand so our `Commands::Help`
// variant can claim the name. The `--help` flag remains available.
#[command(disable_help_subcommand = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to input JSON file (required for every subcommand except `help`).
    #[arg(long)]
    input: Option<PathBuf>,

    /// Path to output JSON file (required for every subcommand except `help`).
    #[arg(long)]
    output: Option<PathBuf>,

    /// Path to database file
    #[arg(long, default_value = "~/.agent-forge/forge.db")]
    db: String,
}

/// One enum variant per agent-forge tool. Names map 1:1 to the agent-forge
/// tool reference.
#[derive(Subcommand, Debug)]
enum Commands {
    SpecTask,
    ConsiderApproaches,
    LogHypothesis,
    LogOutcome,
    RecallErrors,
    Verify,
    ChallengeCode,
    CommentCheck,
    Checkpoint,
    Rollback,
    SessionLearn,
    SessionRecall,
    SessionDiff,
    Think,
    DeclareUnknowns,
    UpdateSpec,
    ListSpecs,
    GetSpec,
    Stats,
    RepoMap,
    SearchCode,
    SkillSearch,
    SkillCapture,
    SkillRecordExec,
    SkillFix,
    SkillDerive,
    SkillLineage,
    /// Print structured documentation about agent-forge to stdout. With no
    /// argument, prints the overview + workflow + subcommand list. With a
    /// subcommand name (kebab-case, e.g. `spec-task`), prints that
    /// subcommand's input schema. Ignores `--input` and `--output`.
    Help {
        /// Name of the subcommand to detail. Omit for the overview.
        subcommand: Option<String>,
    },
    /// Print the machine-readable JSON Schema for the given subcommand's
    /// input. Derived via `schemars` from the actual `*Input` struct, so it
    /// stays in sync with the dispatch path. Ignores `--input` and `--output`.
    Schema {
        /// Name of the subcommand whose input schema to print (kebab-case).
        #[arg(long)]
        command: String,
    },
}

/// Expand a leading `~/` in a path string to the user's home directory.
fn expand_path(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(path)
}

/// Print the help output for `subcommand` to stdout and exit with the
/// appropriate code. Used by the `Help` variant before any DB / file I/O so
/// agents can discover the schema without side effects.
fn run_help(subcommand: Option<String>) -> ! {
    use tools::help::{per_subcmd, KNOWN_SUBCOMMANDS, OVERVIEW};
    match subcommand.as_deref() {
        None => {
            print!("{}", OVERVIEW);
            std::process::exit(0);
        }
        Some(name) => match per_subcmd(name) {
            Some(text) => {
                print!("{}", text);
                std::process::exit(0);
            }
            None => {
                eprintln!("agent-forge help: unknown subcommand '{}'", name);
                eprintln!("Valid subcommands:");
                for n in KNOWN_SUBCOMMANDS {
                    eprintln!("  {}", n);
                }
                std::process::exit(2);
            }
        },
    }
}

/// Print the JSON Schema for `command_name` to stdout and exit. Mirrors
/// `run_help` for the machine-readable surface.
fn run_schema(command_name: &str) -> ! {
    use tools::help::KNOWN_SUBCOMMANDS;
    use tools::schema::for_command;
    match for_command(command_name) {
        Some(json) => {
            println!("{}", json);
            std::process::exit(0);
        }
        None => {
            eprintln!(
                "agent-forge schema: no input schema for '{}'",
                command_name
            );
            eprintln!("Valid subcommands:");
            for n in KNOWN_SUBCOMMANDS {
                eprintln!("  {}", n);
            }
            std::process::exit(2);
        }
    }
}

/// Extract the `(input, output)` paths from the CLI args, exiting with a
/// descriptive error if either is missing. Used by every subcommand except
/// `Help`, which prints to stdout instead.
fn require_io(cli: &Cli) -> (PathBuf, PathBuf) {
    let input = cli.input.clone().unwrap_or_else(|| {
        eprintln!("agent-forge: --input <FILE> is required for this subcommand");
        std::process::exit(2);
    });
    let output = cli.output.clone().unwrap_or_else(|| {
        eprintln!("agent-forge: --output <FILE> is required for this subcommand");
        std::process::exit(2);
    });
    (input, output)
}

/// Parse args, open the forge DB, dispatch to the requested tool, and write
/// the JSON result to `--output`. Any error becomes an `Output::error` payload.
/// The `Help` variant short-circuits before any DB or file I/O.
fn main() {
    let cli = Cli::parse();

    // Short-circuit help: prints to stdout, no DB, no --input/--output needed.
    if let Commands::Help { subcommand } = cli.command {
        run_help(subcommand);
    }

    // Short-circuit schema: same contract as help (stdout, no DB, no files).
    if let Commands::Schema { command } = &cli.command {
        run_schema(command);
    }

    // Every other subcommand requires --input and --output.
    let (input_path, output_path) = require_io(&cli);

    let db_path = expand_path(&cli.db);
    let db = match Database::open(&db_path) {
        Ok(db) => db,
        Err(e) => {
            let output = Output::error(format!("Database error: {}", e));
            write_output(&output_path, &output).ok();
            std::process::exit(1);
        }
    };

    let result = match cli.command {
        Commands::Help { .. } => unreachable!("Help is handled above"),
        Commands::Schema { .. } => unreachable!("Schema is handled above"),
        Commands::SpecTask => read_input(&input_path)
            .map_err(|e| e.to_string())
            .and_then(|input| tools::spec::spec_task(&db, input).map_err(|e| e.to_string())),
        Commands::LogHypothesis => {
            read_input(&input_path)
                .map_err(|e| e.to_string())
                .and_then(|input| {
                    tools::hypothesis::log_hypothesis(&db, input).map_err(|e| e.to_string())
                })
        }
        Commands::LogOutcome => {
            read_input(&input_path)
                .map_err(|e| e.to_string())
                .and_then(|input| {
                    tools::hypothesis::log_outcome(&db, input).map_err(|e| e.to_string())
                })
        }
        Commands::RecallErrors => {
            read_input(&input_path)
                .map_err(|e| e.to_string())
                .and_then(|input| {
                    tools::hypothesis::recall_errors(&db, input).map_err(|e| e.to_string())
                })
        }
        Commands::Verify => read_input(&input_path)
            .map_err(|e| e.to_string())
            .and_then(|input| tools::verify::verify(&db, input).map_err(|e| e.to_string())),
        Commands::ChallengeCode => read_input(&input_path)
            .map_err(|e| e.to_string())
            .and_then(|input| tools::verify::challenge_code(&db, input).map_err(|e| e.to_string())),
        Commands::CommentCheck => {
            read_input(&input_path)
                .map_err(|e| e.to_string())
                .and_then(|input| {
                    tools::comments::comment_check(&db, input).map_err(|e| e.to_string())
                })
        }
        Commands::SessionDiff => read_input(&input_path)
            .map_err(|e| e.to_string())
            .and_then(|input| tools::verify::session_diff(&db, input).map_err(|e| e.to_string())),
        Commands::Checkpoint => read_input(&input_path)
            .map_err(|e| e.to_string())
            .and_then(|input| tools::session::checkpoint(&db, input).map_err(|e| e.to_string())),
        Commands::Rollback => read_input(&input_path)
            .map_err(|e| e.to_string())
            .and_then(|input| tools::session::rollback(&db, input).map_err(|e| e.to_string())),
        Commands::SessionLearn => read_input(&input_path)
            .map_err(|e| e.to_string())
            .and_then(|input| tools::session::session_learn(&db, input).map_err(|e| e.to_string())),
        Commands::SessionRecall => {
            read_input(&input_path)
                .map_err(|e| e.to_string())
                .and_then(|input| {
                    tools::session::session_recall(&db, input).map_err(|e| e.to_string())
                })
        }
        Commands::Think => read_input(&input_path)
            .map_err(|e| e.to_string())
            .and_then(|input| tools::think::think(&db, input).map_err(|e| e.to_string())),
        Commands::DeclareUnknowns => {
            read_input(&input_path)
                .map_err(|e| e.to_string())
                .and_then(|input| {
                    tools::think::declare_unknowns(&db, input).map_err(|e| e.to_string())
                })
        }
        Commands::ConsiderApproaches => {
            read_input(&input_path)
                .map_err(|e| e.to_string())
                .and_then(|input| {
                    tools::approaches::consider_approaches(&db, input).map_err(|e| e.to_string())
                })
        }
        Commands::UpdateSpec => read_input(&input_path)
            .map_err(|e| e.to_string())
            .and_then(|input| tools::spec::update_spec(&db, input).map_err(|e| e.to_string())),
        Commands::ListSpecs => read_input(&input_path)
            .map_err(|e| e.to_string())
            .and_then(|input| tools::spec::list_specs(&db, input).map_err(|e| e.to_string())),
        Commands::GetSpec => read_input(&input_path)
            .map_err(|e| e.to_string())
            .and_then(|input| tools::spec::get_spec(&db, input).map_err(|e| e.to_string())),
        Commands::RepoMap => read_input(&input_path)
            .map_err(|e| e.to_string())
            .and_then(|input| {
                tools::ast::repo_map::repo_map(&db, input).map_err(|e| e.to_string())
            }),
        Commands::SearchCode => {
            read_input(&input_path)
                .map_err(|e| e.to_string())
                .and_then(|input| {
                    tools::ast::search::search_code(&db, input).map_err(|e| e.to_string())
                })
        }
        Commands::SkillSearch => read_input(&input_path)
            .map_err(|e| e.to_string())
            .and_then(|input| tools::skills::skill_search(input).map_err(|e| e.to_string())),
        Commands::SkillCapture => read_input(&input_path)
            .map_err(|e| e.to_string())
            .and_then(|input| tools::skills::skill_capture(input).map_err(|e| e.to_string())),
        Commands::SkillRecordExec => read_input(&input_path)
            .map_err(|e| e.to_string())
            .and_then(|input| tools::skills::skill_record_exec(input).map_err(|e| e.to_string())),
        Commands::SkillFix => read_input(&input_path)
            .map_err(|e| e.to_string())
            .and_then(|input| tools::skills::skill_fix(input).map_err(|e| e.to_string())),
        Commands::SkillDerive => read_input(&input_path)
            .map_err(|e| e.to_string())
            .and_then(|input| tools::skills::skill_derive(input).map_err(|e| e.to_string())),
        Commands::SkillLineage => read_input(&input_path)
            .map_err(|e| e.to_string())
            .and_then(|input| tools::skills::skill_lineage(input).map_err(|e| e.to_string())),
        Commands::Stats => read_input(&input_path)
            .map_err(|e| e.to_string())
            .and_then(|input| tools::stats::stats(&db, input).map_err(|e| e.to_string())),
    };

    let output = match result {
        Ok(out) => out,
        Err(e) => Output::error(e),
    };

    if let Err(e) = write_output(&output_path, &output) {
        eprintln!("Failed to write output: {}", e);
        std::process::exit(1);
    }
}
