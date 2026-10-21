use clap::Parser;
use std::path::Path;
use tracing_subscriber::EnvFilter;

mod config;
mod extractor;
mod ledger;
mod one_shot;
mod summarizer;
mod tailer;
mod watcher;
mod writer;

#[derive(Parser)]
#[command(name = "kleos-ingest", about = "Transcript ingest daemon for Kleos")]
struct Cli {
    #[arg(long, help = "Print ledger stats and exit")]
    status: bool,

    #[arg(
        long,
        help = "Log what would be stored without actually posting to Kleos"
    )]
    dry_run: bool,

    #[arg(long, help = "Process existing files once then exit (no watcher loop)")]
    one_shot: bool,
}

fn open_ledger(config: &config::Config, dry_run: bool) -> ledger::Ledger {
    let path = if dry_run {
        Path::new(":memory:")
    } else {
        &config.ledger_path
    };
    ledger::Ledger::open(path).expect("failed to open ledger database")
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let config = config::Config::from_env();

    if cli.status {
        match ledger::Ledger::open(&config.ledger_path) {
            Ok(ledger) => ledger.print_stats(),
            Err(e) => {
                eprintln!("Failed to open ledger: {}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    tracing::info!(
        watch_dir = %config.watch_dir.display(),
        kleos_url = %config.kleos_url,
        dry_run = cli.dry_run,
        "kleos-ingest starting"
    );

    let ledger = open_ledger(&config, cli.dry_run);
    let writer = writer::KleosWriter::new(&config);

    if cli.one_shot {
        one_shot::run(config, ledger, writer, cli.dry_run).await;
    } else {
        watcher::run(config, ledger, writer, cli.dry_run).await;
    }
}
