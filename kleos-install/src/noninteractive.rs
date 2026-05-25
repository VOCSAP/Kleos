//! Non-interactive installation mode for the Kleos installer.
//!
//! When the user passes `--non-interactive`, this module takes over. It selects
//! components from the given profile, applies all defaults, generates security
//! keys, and runs the installation engine with plain `println`-based progress
//! output -- no TUI required.

use std::path::PathBuf;

use kleos_install_core::config::{EmbeddingConfig, InstallerConfig, RerankerConfig, SecurityConfig};
use kleos_install_core::plan::{InstallPlan, InstallProgress};
use kleos_install_core::security;
use kleos_install_core::system::SystemIntegration;
use kleos_install_core::{profile_components, resolve_dependencies, InstallError, PlatformInfo, Profile};

/// Run the installer in non-interactive mode.
///
/// Prints progress to stdout, exits with a non-zero status code on error.
pub async fn run(
    version: Option<String>,
    install_dir: Option<PathBuf>,
    profile: &str,
) -> anyhow::Result<()> {
    let platform_info = PlatformInfo::detect();
    println!("Kleos Installer -- non-interactive mode");
    println!("Platform: {} ({})", platform_info.os_name, platform_info.arch);

    let selected_profile = parse_profile(profile);
    println!("Profile: {profile}");

    let install_dir = install_dir.unwrap_or_else(|| platform_info.default_install_dir.clone());
    let config_dir = platform_info.default_config_dir.clone();
    let version = version.unwrap_or_else(|| "latest".to_string());

    println!("Install dir: {}", install_dir.display());
    println!("Config dir: {}", config_dir.display());
    println!("Version: {version}");

    // Resolve components.
    let profile_ids = profile_components(selected_profile);
    let component_ids = resolve_dependencies(&profile_ids);
    let components: Vec<String> = component_ids.iter().map(|s| s.to_string()).collect();
    println!("Components: {}", components.join(", "));

    // Generate security keys.
    let security_config = SecurityConfig {
        encryption_key: security::generate_encryption_key(),
        api_key_pepper: security::generate_api_key_pepper(),
        initial_api_key: security::generate_api_key(),
        hmac_secret: security::generate_hmac_secret(),
        open_access: false,
    };

    // Build server config from defaults.
    let server_config = kleos_install_core::config::ServerConfig {
        data_dir: platform_info.default_data_dir.clone(),
        ..kleos_install_core::config::ServerConfig::default()
    };

    let has_server = components.iter().any(|c| c == "kleos-server");

    let installer_config = InstallerConfig {
        server: if has_server { Some(server_config) } else { None },
        embedding: if has_server {
            Some(EmbeddingConfig::LocalOnnx {
                model: "BAAI/bge-m3".to_string(),
                dimension: 1024,
                auto_download: true,
            })
        } else {
            None
        },
        reranker: if has_server {
            Some(RerankerConfig::Disabled)
        } else {
            None
        },
        security: security_config,
    };

    // Choose system integration.
    let system_integration = if platform_info.has_systemd && has_server {
        SystemIntegration::Systemd { auto_start: true }
    } else if platform_info.has_launchd && has_server {
        SystemIntegration::Launchd { auto_start: true }
    } else {
        SystemIntegration::None
    };

    let plan = InstallPlan {
        components,
        install_dir,
        config_dir,
        version,
        config: installer_config,
        system_integration,
        is_upgrade: kleos_install_core::upgrade::detect_existing_install().is_some(),
    };

    println!("\nStarting installation...\n");

    let progress = PrintProgress;
    match tokio::task::spawn_blocking(move || plan.execute(&progress, "")).await? {
        Ok(result) => {
            println!("\nInstallation complete.");
            if let Some(url) = &result.server_url {
                println!("Server URL: {url}");
            }
            println!("API key: {}", result.api_key);
            println!("Config: {}", result.config_path.display());
            Ok(())
        }
        Err(e) => {
            eprintln!("\nInstallation failed: {e}");
            std::process::exit(1);
        }
    }
}

/// Parse a profile string into a `Profile` variant.
///
/// Defaults to `Profile::Server` for unrecognised strings.
fn parse_profile(s: &str) -> Profile {
    match s {
        "agent-host" | "agenthost" | "agent_host" => Profile::AgentHost,
        "full" => Profile::Full,
        "custom" => Profile::Custom,
        _ => Profile::Server,
    }
}

/// A simple `InstallProgress` implementation that prints to stdout.
struct PrintProgress;

impl InstallProgress for PrintProgress {
    /// Print the phase and detail to stdout.
    fn on_phase(&self, phase: &str, detail: &str) {
        println!("[{phase}] {detail}");
    }

    /// Print download progress as a percentage.
    fn on_download_progress(&self, component: &str, bytes: u64, total: u64) {
        let pct = (bytes * 100).checked_div(total).unwrap_or(0);
        print!("\r  {component}: {pct}%    ");
        use std::io::Write;
        let _ = std::io::stdout().flush();
    }

    /// Print a component-installed confirmation.
    fn on_component_installed(&self, component: &str) {
        println!("\n  installed: {component}");
    }

    /// Print a completion message.
    fn on_complete(&self) {
        println!("All components installed.");
    }

    /// Print an error message to stderr.
    fn on_error(&self, error: &InstallError) {
        eprintln!("ERROR: {error}");
    }
}
