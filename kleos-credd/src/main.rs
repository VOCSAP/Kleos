//! Kleos credential management daemon.

use std::path::PathBuf;

use clap::{
    error::ErrorKind, parser::ValueSource, CommandFactory, FromArgMatches, Parser, ValueEnum,
};
use kleos_credd::{broker, server};
use tracing::info;

#[derive(Clone, Debug, ValueEnum)]
enum Mode {
    Server,
    Broker,
}

#[derive(Parser)]
#[command(name = "kleos-credd")]
#[command(about = "Kleos credential management daemon")]
struct Args {
    /// Server mode. Broker is the isolated Unix socket credential resolver.
    #[arg(long, value_enum, default_value_t = Mode::Server)]
    mode: Mode,

    /// Listen address for the historical server mode.
    #[arg(long, default_value = "127.0.0.1:4400", env = "CREDD_LISTEN")]
    listen: String,

    /// Database path.
    #[arg(long, default_value = "kleos.db", env = "CREDD_DB_PATH")]
    db_path: String,

    /// Master password (only consulted when --auth-mode=password).
    #[arg(long, env = "CREDD_MASTER_PASSWORD")]
    master_password: Option<String>,

    /// How credd derives its master key.
    #[arg(long, default_value = "yubikey", env = "CREDD_AUTH_MODE")]
    auth_mode: String,

    /// Path to keyfile containing hex-encoded 32-byte master key.
    #[arg(long, env = "CREDD_KEYFILE")]
    keyfile: Option<PathBuf>,

    /// Unix socket used by broker mode.
    #[arg(long)]
    broker_socket: Option<PathBuf>,

    /// TOML policy used by broker mode.
    #[arg(long)]
    broker_policy: Option<PathBuf>,

    /// Unix UID allowed to resolve broker references.
    #[arg(long, default_value_t = 10001)]
    broker_client_uid: u32,

    /// Validate a broker policy without opening a vault or socket.
    #[arg(
        long,
        conflicts_with_all = [
            "healthcheck",
            "listen",
            "db_path",
            "master_password",
            "auth_mode",
            "keyfile",
            "broker_socket",
            "broker_client_uid"
        ]
    )]
    check_config: bool,

    /// Probe an already-running broker without opening a vault.
    #[arg(
        long,
        conflicts_with_all = [
            "check_config",
            "listen",
            "db_path",
            "master_password",
            "auth_mode",
            "keyfile",
            "broker_policy",
            "broker_client_uid"
        ]
    )]
    healthcheck: bool,
}

#[tokio::main]
async fn main() {
    let command = Args::command();
    let matches = match command.try_get_matches() {
        Ok(matches) => matches,
        Err(error) if error.kind() == ErrorKind::ArgumentConflict => {
            eprintln!("kleos-credd: {}", error);
            std::process::exit(78);
        }
        Err(error) => error.exit(),
    };
    let listen_explicit = matches.value_source("listen") != Some(ValueSource::DefaultValue);
    let args = match Args::from_arg_matches(&matches) {
        Ok(args) => args,
        Err(error) => error.exit(),
    };

    if let Err(error) = run(args, listen_explicit).await {
        eprintln!("kleos-credd: {}", error.message);
        std::process::exit(error.code);
    }
}

struct StartError {
    code: i32,
    message: String,
}

impl StartError {
    fn config(message: impl Into<String>) -> Self {
        Self {
            code: 78,
            message: message.into(),
        }
    }

    #[cfg(unix)]
    fn unavailable(message: impl Into<String>) -> Self {
        Self {
            code: 69,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            code: 70,
            message: message.into(),
        }
    }
}

async fn run(args: Args, listen_explicit: bool) -> Result<(), StartError> {
    match args.mode {
        Mode::Server => {
            if args.check_config
                || args.healthcheck
                || args.broker_socket.is_some()
                || args.broker_policy.is_some()
            {
                return Err(StartError::config("broker options require --mode broker"));
            }
            kleos_lib::config::migrate_env_prefix();
            let _otel_guard = kleos_lib::observability::init_tracing(
                "engram-credd",
                "kleos_credd=info,tower_http=debug",
            );
            info!("starting credd on {}", args.listen);
            server::run_with_env(
                &args.listen,
                &args.db_path,
                args.auth_mode.as_str(),
                args.master_password,
                args.keyfile,
                kleos_credd::build_router,
            )
            .await
            .map_err(|error| StartError::internal(error.to_string()))
        }
        Mode::Broker => run_broker(args, listen_explicit).await,
    }
}

async fn run_broker(args: Args, listen_explicit: bool) -> Result<(), StartError> {
    if args.check_config {
        let policy_path = args
            .broker_policy
            .ok_or_else(|| StartError::config("broker mode requires --broker-policy"))?;
        broker::Policy::from_file(&policy_path)
            .map_err(|error| StartError::config(error.to_string()))?;
        return Ok(());
    }

    if args.healthcheck {
        let socket_path = args
            .broker_socket
            .ok_or_else(|| StartError::config("broker mode requires --broker-socket"))?;
        #[cfg(unix)]
        {
            return broker::healthcheck(&socket_path)
                .await
                .map_err(|error| StartError::unavailable(error.to_string()));
        }
        #[cfg(not(unix))]
        {
            let _ = socket_path;
            return Err(StartError::config(
                "broker mode is unavailable on this platform",
            ));
        }
    }

    if listen_explicit || std::env::var_os("CREDD_BIND").is_some() {
        return Err(StartError::config(
            "broker mode does not allow TCP configuration",
        ));
    }
    if args.master_password.is_some() || std::env::var_os("CREDD_MASTER_PASSWORD").is_some() {
        return Err(StartError::config(
            "broker mode does not allow a master password",
        ));
    }
    let socket_path = args
        .broker_socket
        .ok_or_else(|| StartError::config("broker mode requires --broker-socket"))?;
    let policy_path = args
        .broker_policy
        .ok_or_else(|| StartError::config("broker mode requires --broker-policy"))?;

    #[cfg(unix)]
    {
        server::run_broker(
            &socket_path,
            &policy_path,
            args.broker_client_uid,
            &args.auth_mode,
            args.keyfile,
            &args.db_path,
        )
        .await
        .map_err(|error| match error {
            server::BrokerStartError::Config(error) => StartError::config(error.to_string()),
            server::BrokerStartError::Initialization => StartError::internal(error.to_string()),
        })
    }
    #[cfg(not(unix))]
    {
        let _ = (socket_path, policy_path);
        Err(StartError::config(
            "broker mode is unavailable on this platform",
        ))
    }
}
