//! Local stdio MCP entrypoint for the complete Agent-Forge workflow.

use agent_forge::db::Database;
use agent_forge::mcp;
use clap::Parser;
use std::path::PathBuf;

/// Command-line options for the local Agent-Forge MCP server.
#[derive(Debug, Parser)]
#[command(name = "agent-forge-mcp")]
struct Args {
    /// Path to the local Agent-Forge SQLite database.
    #[arg(long, default_value = "~/.agent-forge/forge.db")]
    db: String,
}

/// Expand a leading home-directory marker in a configured path.
fn expand_path(path: &str) -> PathBuf {
    path.strip_prefix("~/")
        .and_then(|rest| dirs::home_dir().map(|home| home.join(rest)))
        .unwrap_or_else(|| PathBuf::from(path))
}

/// Open the local database and serve MCP requests until stdin reaches EOF.
fn main() {
    let args = Args::parse();
    let db = Database::open(&expand_path(&args.db)).unwrap_or_else(|error| {
        eprintln!("agent-forge-mcp failed to open database: {error}");
        std::process::exit(1);
    });
    if let Err(error) = mcp::serve_stdio(&db) {
        eprintln!("agent-forge-mcp stdio transport failed: {error}");
        std::process::exit(1);
    }
}
