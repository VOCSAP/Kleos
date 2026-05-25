//! Entry point for the `kleos-install` TUI installer binary.
//!
//! Parses CLI arguments, then either runs the interactive TUI wizard or
//! falls through to non-interactive mode. On any panic during TUI operation
//! the raw terminal is restored before the process exits so the user's shell
//! is not left in an unusable state.

mod noninteractive;
mod steps;
mod tui;
mod types;
mod wizard;

use std::io;
use std::panic;

use clap::Parser;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tracing_subscriber::EnvFilter;

/// Interactive TUI installer for Kleos.
#[derive(Parser)]
#[command(name = "kleos-install", about = "Interactive installer for Kleos")]
struct Args {
    /// Run in non-interactive mode with defaults.
    #[arg(long)]
    non_interactive: bool,

    /// Kleos version to install (default: latest).
    #[arg(long)]
    version: Option<String>,

    /// Installation directory override.
    #[arg(long)]
    install_dir: Option<std::path::PathBuf>,

    /// Installation profile for non-interactive mode: server, agent-host, full.
    #[arg(long, default_value = "server")]
    profile: String,
}

/// Initialize the tracing subscriber for diagnostic output.
///
/// In non-interactive mode output goes to stderr. In TUI mode tracing is
/// suppressed so it does not corrupt the alternate screen.
fn init_tracing(tui_mode: bool) {
    if tui_mode {
        // Suppress tracing output so it does not bleed into the TUI.
        let _ = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new("off"))
            .try_init();
    } else {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .try_init();
    }
}

/// Install a panic hook that restores the terminal before printing the panic message.
///
/// Without this, a panic inside the TUI event loop leaves the terminal in raw
/// mode and the alternate screen active, making the shell unusable.
fn install_panic_hook() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        // Best-effort terminal restore -- ignore errors.
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        default_hook(info);
    }));
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    if args.non_interactive {
        init_tracing(false);
        noninteractive::run(
            args.version,
            args.install_dir,
            &args.profile,
        )
        .await
    } else {
        init_tracing(true);
        install_panic_hook();

        // Set up the crossterm alternate-screen terminal.
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = wizard::run_wizard(
            &mut terminal,
            args.version,
            args.install_dir,
        )
        .await;

        // Always restore the terminal even if the wizard returned an error.
        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;

        match result? {
            Some(result) => {
                println!("Installation complete.");
                if let Some(url) = &result.server_url {
                    println!("Server URL: {url}");
                }
                println!("API key: {}", result.api_key);
            }
            None => {
                println!("Installation cancelled.");
            }
        }

        Ok(())
    }
}
