//! Upgrade detection and config migration for the Kleos installer.

use std::path::{Path, PathBuf};

use crate::config::{InstallerConfig, RerankerConfig, SecurityConfig, ServerConfig};
use crate::error::InstallError;

/// Information about an existing Kleos installation found on the system.
#[derive(Debug, Clone)]
pub struct ExistingInstall {
    /// Directory where the Kleos binaries are installed.
    pub install_dir: PathBuf,
    /// Path to the existing config file (`kleos.toml` or legacy `engram.toml`), if found.
    pub config_path: Option<PathBuf>,
    /// List of component binaries detected in `install_dir`.
    pub components: Vec<InstalledComponent>,
}

/// A single component binary found in an existing installation.
#[derive(Debug, Clone)]
pub struct InstalledComponent {
    /// The component's machine-readable ID (matches the component registry).
    pub id: String,
    /// The version string reported by `<binary> --version`, if available.
    pub version: Option<String>,
    /// Full path to the binary on disk.
    pub path: PathBuf,
}

/// Common locations to probe for an existing Kleos installation.
///
/// Checked in order; the first directory containing at least one known binary
/// is treated as the installation root.
const PROBE_DIRS: &[&str] = &["~/.local/bin", "/usr/local/bin", "/opt/kleos/bin"];

/// Known binary names to look for when probing an installation directory.
const KNOWN_BINARIES: &[&str] = &[
    "kleos-server",
    "kleos-cli",
    "kleos-mcp",
    "kleos-credd",
    "cred",
    "kleos-sidecar",
    "agent-forge",
    "eidolon-supervisor",
    "kleos-sh",
    "kleos-ingest",
];

/// Scan common install directories for an existing Kleos installation.
///
/// Returns `Some(ExistingInstall)` if at least one known Kleos binary is found,
/// or `None` if no installation is detected.
pub fn detect_existing_install() -> Option<ExistingInstall> {
    let home = dirs::home_dir()?;

    for &dir_str in PROBE_DIRS {
        let dir = if dir_str.starts_with('~') {
            home.join(&dir_str[2..])
        } else {
            PathBuf::from(dir_str)
        };

        if !dir.exists() {
            continue;
        }

        let mut components = Vec::new();
        for &name in KNOWN_BINARIES {
            let path = dir.join(name);
            let win_path = dir.join(format!("{name}.exe"));
            let found = if path.exists() {
                Some(path)
            } else if win_path.exists() {
                Some(win_path)
            } else {
                None
            };

            if let Some(bin_path) = found {
                let version = read_binary_version(&bin_path);
                components.push(InstalledComponent {
                    id: name.to_string(),
                    version,
                    path: bin_path,
                });
            }
        }

        if !components.is_empty() {
            let config_path = find_config_file(&home);
            return Some(ExistingInstall {
                install_dir: dir,
                config_path,
                components,
            });
        }
    }

    None
}

/// Parse an existing `kleos.toml` (or legacy `engram.toml`) into an
/// `InstallerConfig`.
///
/// Reads the file through the canonical [`kleos_config::Config`] loader -- the
/// same flat schema the server uses -- and maps the known fields back to the
/// installer's representation. Returns `InstallError::Upgrade` on a parse error.
///
/// Note: secrets and env-only settings (open access, CORS) are never stored in
/// the TOML, so `SecurityConfig` fields are empty and `cors_origins` is `None`;
/// callers regenerate secrets for upgrades.
pub fn read_existing_config(config_path: &Path) -> Result<InstallerConfig, InstallError> {
    let cfg = kleos_config::Config::from_file(config_path).map_err(|e| {
        InstallError::Upgrade(format!("failed to parse {}: {e}", config_path.display()))
    })?;

    let server = Some(ServerConfig {
        host: cfg.host,
        port: cfg.port,
        data_dir: PathBuf::from(cfg.data_dir),
        db_path: cfg.db_path,
        cors_origins: None,
    });

    let reranker = Some(if cfg.reranker_enabled {
        RerankerConfig::LocalOnnx
    } else {
        RerankerConfig::Disabled
    });

    // Secrets and open access are env-only, never in the TOML -- placeholders.
    let security = SecurityConfig {
        encryption_key: String::new(),
        api_key_pepper: String::new(),
        initial_api_key: String::new(),
        hmac_secret: String::new(),
        open_access: false,
    };

    Ok(InstallerConfig {
        server,
        embedding: None,
        reranker,
        security,
        overrides: crate::config::ConfigOverrides::default(),
    })
}

/// Create a timestamped backup of a config file by copying it to `<path>.bak.<timestamp>`.
///
/// Returns the path of the newly created backup file.
pub fn backup_config(config_path: &Path) -> Result<PathBuf, InstallError> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let backup_path = config_path.with_extension(format!("bak.{timestamp}"));
    std::fs::copy(config_path, &backup_path)?;
    Ok(backup_path)
}

/// Secrets read from an existing installation's `.env` file, so an upgrade
/// can reuse them instead of regenerating fresh secrets.
///
/// Regenerating these on upgrade would make the existing SQLCipher database
/// undecryptable (`encryption_key`), invalidate every previously issued API
/// key (`api_key_pepper`), and invalidate every outstanding signed token
/// (`hmac_secret`). Field names mirror the `.env` keys written by
/// [`crate::config::InstallerConfig::generate_env`]: `KLEOS_DB_KEY`,
/// `KLEOS_API_KEY_PEPPER`, `KLEOS_HMAC_SECRET`.
///
/// Each field is `None` when the corresponding env var was not found (missing
/// file, partial or hand-edited `.env`, or a pre-secrets install). Callers
/// must generate a fresh value for any `None` field exactly as they would for
/// a brand-new install.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreservedSecrets {
    /// `KLEOS_DB_KEY` -- the SQLCipher database encryption key.
    pub encryption_key: Option<String>,
    /// `KLEOS_API_KEY_PEPPER` -- the pepper mixed into API key hashes.
    pub api_key_pepper: Option<String>,
    /// `KLEOS_HMAC_SECRET` -- the HMAC signing secret.
    pub hmac_secret: Option<String>,
}

/// Read the secret values out of an existing installation's `.env` file so an
/// upgrade can preserve them instead of regenerating secrets that would
/// invalidate the existing encrypted database, issued API keys, and signed
/// tokens.
///
/// The `.env` file is expected alongside `existing.config_path` -- the same
/// directory `kleos.toml` and `.env` are written into together by
/// [`crate::config::InstallerConfig::write_config`]. This function never
/// fails the whole install: if `config_path` is `None`, the `.env` file is
/// missing or unreadable, or a line is malformed, the corresponding field(s)
/// are simply left `None` in the returned struct. Callers should fall back to
/// generating a fresh secret for any field that comes back `None`.
pub fn read_preserved_secrets(existing: &ExistingInstall) -> PreservedSecrets {
    let mut secrets = PreservedSecrets::default();

    let Some(config_path) = &existing.config_path else {
        return secrets;
    };
    let Some(dir) = config_path.parent() else {
        return secrets;
    };

    let Ok(contents) = std::fs::read_to_string(dir.join(".env")) else {
        return secrets;
    };

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }

        match key.trim() {
            "KLEOS_DB_KEY" => secrets.encryption_key = Some(value.to_string()),
            "KLEOS_API_KEY_PEPPER" => secrets.api_key_pepper = Some(value.to_string()),
            "KLEOS_HMAC_SECRET" => secrets.hmac_secret = Some(value.to_string()),
            _ => {}
        }
    }

    secrets
}

/// Path of the version marker written alongside an installed binary
/// (`<binary>.version`). Reading the recorded version from this file avoids
/// executing the binary to learn its version: the probe dirs include
/// user-writable locations (e.g. `~/.local/bin`), so executing a discovered
/// binary would run attacker-planted code -- a privilege-escalation vector when
/// the installer runs as root.
pub(crate) fn version_marker_path(bin: &Path) -> PathBuf {
    let mut s = bin.as_os_str().to_owned();
    s.push(".version");
    PathBuf::from(s)
}

/// Read the recorded version of an installed binary from its `<binary>.version`
/// marker. Returns `None` if no marker exists. The binary is never executed.
fn read_binary_version(path: &Path) -> Option<String> {
    std::fs::read_to_string(version_marker_path(path))
        .ok()
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()))
        .filter(|s| !s.is_empty())
}

/// Search common config locations for an existing Kleos config file.
///
/// The current `kleos`-named locations are checked first, then the legacy
/// `engram` names so a pre-rename install is still detected. Returns the first
/// path found, or `None` if none exist.
fn find_config_file(home: &Path) -> Option<PathBuf> {
    let candidates = [
        home.join(".config").join("kleos").join("kleos.toml"),
        home.join(".config").join("kleos").join("config.toml"),
        home.join(".kleos").join("kleos.toml"),
        PathBuf::from("/etc/kleos/kleos.toml"),
        // Legacy (pre engram->kleos rename).
        home.join(".config").join("engram").join("engram.toml"),
        home.join(".kleos").join("engram.toml"),
        PathBuf::from("/etc/kleos/engram.toml"),
    ];

    candidates.into_iter().find(|p| p.exists())
}

/// Tests for [`read_preserved_secrets`] against synthetic `.env` files on
/// disk, covering the fallback-to-`None` behavior the whole function exists
/// to guarantee.
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Build a minimal `ExistingInstall` pointing at `config_path`, with no
    /// detected component binaries -- enough to exercise
    /// `read_preserved_secrets`, which only cares about `config_path`.
    fn existing_with_config(config_path: PathBuf) -> ExistingInstall {
        ExistingInstall {
            install_dir: PathBuf::from("/unused"),
            config_path: Some(config_path),
            components: Vec::new(),
        }
    }

    // All three secrets are read back from a well-formed .env sitting beside
    // the config file, ignoring comments and unrelated keys.
    #[test]
    fn reads_all_three_secrets_from_env() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("kleos.toml");
        std::fs::write(&config_path, "host = \"127.0.0.1\"\n").expect("write toml");

        let env_path = dir.path().join(".env");
        let mut f = std::fs::File::create(&env_path).expect("create env");
        writeln!(f, "# comment line, ignored").unwrap();
        writeln!(f, "KLEOS_CONFIG_FILE={}", config_path.display()).unwrap();
        writeln!(f, "KLEOS_DB_KEY=deadbeefcafe").unwrap();
        writeln!(f, "KLEOS_API_KEY_PEPPER=feedface1234").unwrap();
        writeln!(f, "KLEOS_HMAC_SECRET=0123456789abcdef").unwrap();
        drop(f);

        let existing = existing_with_config(config_path);
        let secrets = read_preserved_secrets(&existing);

        assert_eq!(secrets.encryption_key.as_deref(), Some("deadbeefcafe"));
        assert_eq!(secrets.api_key_pepper.as_deref(), Some("feedface1234"));
        assert_eq!(secrets.hmac_secret.as_deref(), Some("0123456789abcdef"));
    }

    // A missing .env file must not error the caller -- everything comes back
    // None so the caller can fall back to generating fresh secrets.
    #[test]
    fn missing_env_file_yields_all_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("kleos.toml");
        std::fs::write(&config_path, "host = \"127.0.0.1\"\n").expect("write toml");
        // Deliberately no .env written alongside it.

        let existing = existing_with_config(config_path);
        let secrets = read_preserved_secrets(&existing);

        assert_eq!(secrets, PreservedSecrets::default());
    }

    // No detected config path at all (binaries found but no config file
    // located) must also yield an empty struct rather than panicking.
    #[test]
    fn no_config_path_yields_all_none() {
        let existing = ExistingInstall {
            install_dir: PathBuf::from("/unused"),
            config_path: None,
            components: Vec::new(),
        };
        let secrets = read_preserved_secrets(&existing);
        assert_eq!(secrets, PreservedSecrets::default());
    }

    // A partial .env (only one secret present, e.g. an older install from
    // before HMAC secrets existed) leaves the other fields None instead of
    // erroring or defaulting to empty strings.
    #[test]
    fn partial_env_leaves_missing_fields_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("kleos.toml");
        std::fs::write(&config_path, "host = \"127.0.0.1\"\n").expect("write toml");

        let env_path = dir.path().join(".env");
        std::fs::write(&env_path, "KLEOS_DB_KEY=onlythisone\n").expect("write env");

        let existing = existing_with_config(config_path);
        let secrets = read_preserved_secrets(&existing);

        assert_eq!(secrets.encryption_key.as_deref(), Some("onlythisone"));
        assert_eq!(secrets.api_key_pepper, None);
        assert_eq!(secrets.hmac_secret, None);
    }

    // An explicitly empty value (KEY=) must not be treated as a preserved
    // secret -- an empty string is not a usable encryption key.
    #[test]
    fn empty_value_is_treated_as_absent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let config_path = dir.path().join("kleos.toml");
        std::fs::write(&config_path, "host = \"127.0.0.1\"\n").expect("write toml");

        let env_path = dir.path().join(".env");
        std::fs::write(&env_path, "KLEOS_DB_KEY=\n").expect("write env");

        let existing = existing_with_config(config_path);
        let secrets = read_preserved_secrets(&existing);

        assert_eq!(secrets.encryption_key, None);
    }
}
