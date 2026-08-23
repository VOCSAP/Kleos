//! Non-interactive installation mode for the Kleos installer.
//!
//! When the user passes `--non-interactive`, this module takes over. It selects
//! components from the given profile, applies all defaults, generates security
//! keys, and runs the installation engine with plain `println`-based progress
//! output -- no TUI required.

use std::path::PathBuf;

use kleos_install_core::config::{
    ConfigOverrides, EmbeddingConfig, InstallerConfig, RerankerConfig, SecurityConfig,
};
use kleos_install_core::plan::{InstallPlan, InstallProgress};
use kleos_install_core::security;
use kleos_install_core::system::SystemIntegration;
use kleos_install_core::{
    profile_components, resolve_dependencies, InstallError, PlatformInfo, Profile,
};

/// Full-coverage configuration knobs accepted on the non-interactive CLI.
///
/// These extend the curated profile defaults to the entire server config
/// surface so an unattended install can set anything the interactive wizard or
/// a hand-edited file could.
pub struct CliConfig {
    /// `field=value` overrides for any config field (dotted keys for nested).
    pub set: Vec<String>,
    /// Raw `KEY=VALUE` lines appended to `.env`.
    pub env: Vec<String>,
    /// Existing `engram.toml` to seed config from before applying overrides.
    pub config_file: Option<PathBuf>,
    /// Enable anonymous read-only access.
    pub open_access: bool,
    /// Comma-separated allowed CORS origins.
    pub cors: Option<String>,
    /// GUI password (enables the web GUI).
    pub gui_password: Option<String>,
    /// Force regeneration of all secrets even on an upgrade, restoring the
    /// pre-preservation always-regenerate behavior. Invalidates existing API
    /// keys and sessions -- see the `--regenerate-secrets` help text.
    pub regenerate_secrets: bool,
}

/// Split a `KEY=VALUE` CLI argument on the first `=`. Returns an error if no
/// `=` is present so a typo fails loudly instead of being silently ignored.
fn parse_kv(arg: &str) -> anyhow::Result<(String, String)> {
    let (key, val) = arg
        .split_once('=')
        .ok_or_else(|| anyhow::anyhow!("expected KEY=VALUE, got '{arg}'"))?;
    let key = key.trim();
    if key.is_empty() {
        anyhow::bail!("empty key in '{arg}'");
    }
    Ok((key.to_string(), val.to_string()))
}

/// Assemble [`ConfigOverrides`] from the parsed CLI flags.
fn build_overrides(cli: &CliConfig) -> anyhow::Result<ConfigOverrides> {
    let mut overrides = ConfigOverrides::default();

    if let Some(path) = &cli.config_file {
        let base = kleos_config::Config::from_file(path)
            .map_err(|e| anyhow::anyhow!("read --config-file {}: {e}", path.display()))?;
        overrides.base = Some(base);
    }

    for raw in &cli.set {
        overrides.toml_overrides.push(parse_kv(raw)?);
    }
    for raw in &cli.env {
        overrides.extra_env.push(parse_kv(raw)?);
    }
    if let Some(pw) = &cli.gui_password {
        overrides
            .extra_env
            .push(("KLEOS_GUI_PASSWORD".to_string(), pw.clone()));
    }

    Ok(overrides)
}

/// Run the installer in non-interactive mode.
///
/// Prints progress to stdout, exits with a non-zero status code on error.
pub async fn run(
    version: Option<String>,
    install_dir: Option<PathBuf>,
    profile: &str,
    cli: CliConfig,
) -> anyhow::Result<()> {
    let platform_info = PlatformInfo::detect()?;
    println!("Kleos Installer -- non-interactive mode");
    println!(
        "Platform: {} ({})",
        platform_info.os_name, platform_info.arch
    );

    let selected_profile =
        parse_profile(profile).map_err(|e| anyhow::anyhow!("invalid --profile: {e}"))?;
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

    // Assemble full-coverage overrides from the CLI before building the plan.
    let overrides = build_overrides(&cli)?;

    // Detect an existing install once -- reused both to decide whether the
    // plan should back up the existing config (`is_upgrade`) and to preserve
    // its secrets instead of silently regenerating them.
    let existing_install = kleos_install_core::upgrade::detect_existing_install();
    let is_upgrade = existing_install.is_some();

    if is_upgrade {
        println!(
            "Existing install detected -- secrets will be preserved and the existing config backed up."
        );
    }
    if cli.regenerate_secrets {
        println!(
            "--regenerate-secrets set: generating fresh secrets (this invalidates all existing API keys and sessions)."
        );
    }

    // Generate or preserve security keys. `--regenerate-secrets` forces fresh
    // secrets even on an upgrade, restoring the previous always-regenerate
    // behavior explicitly; otherwise any secret found in the existing
    // install's `.env` is reused, and only a field that comes back `None`
    // (missing/partial `.env`, or no existing install at all) falls back to
    // a freshly generated value.
    let preserved_secrets = if cli.regenerate_secrets {
        kleos_install_core::upgrade::PreservedSecrets::default()
    } else {
        existing_install
            .as_ref()
            .map(kleos_install_core::upgrade::read_preserved_secrets)
            .unwrap_or_default()
    };

    let security_config = SecurityConfig {
        encryption_key: preserved_secrets
            .encryption_key
            .unwrap_or_else(security::generate_encryption_key),
        api_key_pepper: preserved_secrets
            .api_key_pepper
            .unwrap_or_else(security::generate_api_key_pepper),
        initial_api_key: security::generate_api_key(),
        hmac_secret: preserved_secrets
            .hmac_secret
            .unwrap_or_else(security::generate_hmac_secret),
        open_access: cli.open_access,
    };

    // Build server config from defaults, applying any CORS origins from the CLI.
    let server_config = kleos_install_core::config::ServerConfig {
        data_dir: platform_info.default_data_dir.clone(),
        cors_origins: cli.cors.clone(),
        ..kleos_install_core::config::ServerConfig::default()
    };

    let has_server = components.iter().any(|c| c == "kleos-server");

    let installer_config = InstallerConfig {
        server: if has_server {
            Some(server_config)
        } else {
            None
        },
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
        overrides,
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
        is_upgrade,
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

/// Parse and validate a `--profile` value for non-interactive installs.
///
/// Accepts `server`, `agent-host` (also `agenthost`/`agent_host` for
/// backward compatibility), and `full`. Any other value is rejected with an
/// error naming the supported profiles instead of silently falling back to
/// `server` -- this covers both a typo'd profile name and `custom`, which has
/// no component selection outside the interactive wizard
/// (`profile_components(Profile::Custom)` returns an empty list, so a
/// non-interactive `--profile custom` install would otherwise select
/// nothing).
fn parse_profile(s: &str) -> Result<Profile, String> {
    match s {
        "server" => Ok(Profile::Server),
        "agent-host" | "agenthost" | "agent_host" => Ok(Profile::AgentHost),
        "full" => Ok(Profile::Full),
        _ => Err(format!(
            "unknown profile '{s}'; supported profiles for non-interactive installs are: server, agent-host, full"
        )),
    }
}

/// A simple `InstallProgress` implementation that prints to stdout.
struct PrintProgress;

/// Stdout-based progress reporting for the non-interactive installer.
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

/// Tests for `parse_profile`'s validation behavior.
#[cfg(test)]
mod tests {
    use super::*;

    // Recognised profile strings map to the correct `Profile` variant,
    // including the legacy agent-host aliases kept for backward compatibility.
    #[test]
    fn parse_profile_accepts_known_values() {
        assert_eq!(parse_profile("server"), Ok(Profile::Server));
        assert_eq!(parse_profile("agent-host"), Ok(Profile::AgentHost));
        assert_eq!(parse_profile("agenthost"), Ok(Profile::AgentHost));
        assert_eq!(parse_profile("agent_host"), Ok(Profile::AgentHost));
        assert_eq!(parse_profile("full"), Ok(Profile::Full));
    }

    // An unrecognised value must be rejected with an actionable message that
    // names the supported profiles -- never silently fall back to `server`
    // (the bug this replaces).
    #[test]
    fn parse_profile_rejects_unknown_value() {
        let err = parse_profile("srever").expect_err("typo must be rejected");
        assert!(err.contains("srever"));
        assert!(err.contains("server"));
    }

    // `custom` has no component selection outside the interactive wizard
    // (`profile_components(Profile::Custom)` is empty), so it must be
    // rejected rather than silently installing nothing.
    #[test]
    fn parse_profile_rejects_custom() {
        let err = parse_profile("custom").expect_err("custom must be rejected");
        assert!(err.contains("server"));
    }
}
