//! forge -- agent tool runtime.
//!
//! Skill-driven dispatch for Kleos-authenticated tools. Agents call
//! `forge exec <skill> [--params]` instead of raw HTTP, keeping auth
//! credentials out of tool output.

mod client;
mod dispatch;
mod error;
mod exec;
mod output;

use clap::Parser;

/// Agent tool runtime -- skill-driven dispatch for Kleos-authenticated tools.
#[derive(Parser)]
#[command(name = "forge", version, about)]
struct Cli {
    /// Kleos server URL.
    #[arg(
        short = 's',
        long = "server",
        env = "KLEOS_URL",
        default_value = "http://10.50.0.1:4200"
    )]
    server: String,

    #[command(subcommand)]
    command: Commands,
}

/// Top-level commands.
#[derive(clap::Subcommand)]
enum Commands {
    /// Execute a skill against the Kleos server.
    Exec {
        /// List available skills instead of executing one.
        #[arg(short, long)]
        list: bool,

        /// Output format: auto, json, human, agent.
        #[arg(short, long, default_value = "auto")]
        format: String,

        /// Skill name to execute.
        skill: Option<String>,

        /// Skill-specific arguments (--key value pairs).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

/// Entry point: initialise tracing, parse args, and run the CLI.
#[tokio::main]
async fn main() {
    let _guard = kleos_lib::observability::init_tracing("forge", "warn");
    let cli = Cli::parse();

    if let Err(e) = run(cli).await {
        eprintln!("error: {e}");
        std::process::exit(error::exit_code(&e));
    }
}

/// Run the CLI command.
async fn run(cli: Cli) -> error::Result<()> {
    match cli.command {
        Commands::Exec {
            list,
            format,
            skill,
            args,
        } => {
            let client = client::KleosClient::from_env(&cli.server).await?;

            if list {
                return exec::list_skills(&client).await;
            }

            let skill_name = skill.ok_or_else(|| {
                error::ForgeError::InvalidParam(
                    "skill".into(),
                    "skill name is required (or use --list)".into(),
                )
            })?;

            // Check for --help in the skill args
            let wants_help = args.iter().any(|a| a == "--help" || a == "-h");

            let config = exec::fetch_config(&client, &skill_name).await?;

            if wants_help {
                exec::print_skill_help(&config);
                return Ok(());
            }

            let fmt = output::Format::parse(&format).ok_or_else(|| {
                error::ForgeError::InvalidParam(
                    "format".into(),
                    format!("unknown format '{format}' -- use auto, json, human, or agent"),
                )
            })?;

            exec::execute_skill(&client, &config, &args, fmt).await
        }
    }
}
