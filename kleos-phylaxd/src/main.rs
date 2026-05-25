//! Phylax credential authority daemon.
//!
//! Composes credd's base router with Phylax agent-native extensions.
//! If no policies are configured, behaves identically to plain credd.

use std::net::SocketAddr;

use clap::Parser;
use tracing::info;

/// Phylax daemon CLI arguments.
#[derive(Parser)]
#[command(name = "phylaxd", about = "Phylax agent-native credential authority")]
struct Args {
    /// Address to bind to.
    #[arg(long, default_value = "127.0.0.1:3100")]
    bind: String,

    /// Path to the credential database.
    #[arg(long, env = "CREDD_DB_PATH")]
    db_path: Option<String>,
}

/// Entry point for the phylaxd daemon.
///
/// Starts up with the same master key derivation and bootstrap sequence
/// as credd, then layers Phylax policy and approval routing on top.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,kleos_phylax=debug,kleos_credd=debug".into()),
        )
        .init();

    let args = Args::parse();

    info!("phylaxd starting");

    // Build credd's AppState using the same startup sequence as credd.
    // For now, delegate to a simplified builder. The full credd startup
    // (master key derivation, YubiKey, bootstrap loading) will be wired
    // in when phylaxd replaces credd as the production binary.
    //
    // TODO: Wire up the full credd startup sequence. For now, phylaxd
    // requires CREDD_MASTER_KEY_HEX environment variable.
    let db_path = args
        .db_path
        .unwrap_or_else(|| {
            dirs::config_dir()
                .map(|p| p.join("cred").join("credentials.db").to_string_lossy().into_owned())
                .unwrap_or_else(|| "credentials.db".to_string())
        });

    info!(db_path = %db_path, "opening database");

    // Placeholder: in production, this would use credd's full startup
    // to build AppState (master key, bootstrap, agent keys, PIV keys).
    // For the initial port, we'll refine this in integration testing.
    info!("phylaxd placeholder -- full startup integration pending");
    info!("bind address: {}", args.bind);

    Ok(())
}
