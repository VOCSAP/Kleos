//! Component registry and installation profiles for the Kleos installer.

use crate::error::InstallError;

/// Target platform for a binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Platform {
    /// 64-bit Linux on x86_64.
    LinuxX64,
    /// 64-bit Linux on ARM64 / AArch64.
    LinuxArm64,
    /// macOS on x86_64 (Intel).
    DarwinX64,
    /// macOS on ARM64 (Apple Silicon).
    DarwinArm64,
    /// Windows on x86_64.
    WindowsX64,
}

/// Detection and release-artifact naming for the current platform.
impl Platform {
    /// Detect the current platform from compile-time OS and architecture
    /// constants and confirm Kleos publishes release binaries for it.
    ///
    /// Returns `InstallError::Platform` if the OS/arch combination is not
    /// recognized at all, or if it is recognized but has no published release
    /// (see [`is_platform_published`]) -- e.g. macOS or Linux ARM64 today.
    /// Never panics: an installer is exactly the kind of code that must fail
    /// with an actionable message instead of aborting the process.
    pub fn detect() -> Result<Platform, InstallError> {
        let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => Platform::LinuxX64,
            ("linux", "aarch64") => Platform::LinuxArm64,
            ("macos", "x86_64") => Platform::DarwinX64,
            ("macos", "aarch64") => Platform::DarwinArm64,
            ("windows", "x86_64") => Platform::WindowsX64,
            (os, arch) => {
                return Err(InstallError::Platform(format!(
                    "unsupported platform: {os}/{arch}"
                )))
            }
        };
        check_platform_published(platform)?;
        Ok(platform)
    }

    /// Return the release-asset suffix string for this platform.
    ///
    /// This matches the suffix used when naming binaries in GitHub release assets,
    /// e.g. `kleos-server-linux-x64`.
    pub fn release_suffix(&self) -> &'static str {
        match self {
            Platform::LinuxX64 => "linux-x64",
            Platform::LinuxArm64 => "linux-arm64",
            Platform::DarwinX64 => "darwin-x64",
            Platform::DarwinArm64 => "darwin-arm64",
            Platform::WindowsX64 => "windows-x64",
        }
    }
}

/// Decide whether Kleos publishes release binaries for `platform`.
///
/// Only `linux-x64` and `windows-x64` are built and published today. The
/// other `Platform` variants remain in the enum so frontends can exhaustively
/// match on it, but this function is the single source of truth for which of
/// them are actually installable from a release -- both `Platform::detect`
/// and the component registry's platform lists key off it, so the advertised
/// set and the enforced set can't drift apart.
pub fn is_platform_published(platform: Platform) -> bool {
    matches!(platform, Platform::LinuxX64 | Platform::WindowsX64)
}

/// Return `Ok(())` if `platform` has published Kleos release binaries, or a
/// clear, actionable `InstallError::Platform` if not.
///
/// This is the decision point that keeps the installer from silently
/// attempting a release-asset download on a platform nothing was ever built
/// for (which would otherwise fail confusingly later, in `fetch_release` or
/// `download_component`, with a generic "asset not found" error).
pub fn check_platform_published(platform: Platform) -> Result<(), InstallError> {
    if is_platform_published(platform) {
        return Ok(());
    }
    Err(InstallError::Platform(format!(
        "Kleos v{} releases are not yet published for {}; build from source (see README) or use linux-x64/windows-x64",
        env!("CARGO_PKG_VERSION"),
        platform.release_suffix(),
    )))
}

/// An installable Kleos component (binary or tool).
#[derive(Debug, Clone)]
pub struct Component {
    /// Unique machine-readable identifier for this component.
    pub id: &'static str,
    /// Human-readable name shown in the installer UI.
    pub display_name: &'static str,
    /// Short description of what this component does.
    pub description: &'static str,
    /// Whether this component must be installed (cannot be deselected).
    pub required: bool,
    /// Set of platforms this component is available for.
    pub platforms: &'static [Platform],
    /// IDs of components that must also be installed alongside this one.
    pub depends_on: &'static [&'static str],
}

/// Pre-defined installation profiles that select a curated component set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// Minimal server installation: just the server binary and CLI.
    Server,
    /// Agent host: CLI tools, shell helpers, agent workflow tooling, and credential daemon.
    AgentHost,
    /// All available components.
    Full,
    /// User-defined component selection.
    Custom,
}

/// All currently published platforms (used for components that ship on every
/// platform Kleos actually builds for).
///
/// Kept narrow on purpose: releases are only built for linux-x64 and
/// windows-x64 today (see [`is_platform_published`]). macOS (Intel/ARM) and
/// Linux ARM64 are real `Platform` variants -- the installer can run on those
/// machines and detect them -- but there is nothing to download, so they are
/// deliberately excluded here rather than advertised as installable.
const ALL_PLATFORMS: &[Platform] = &[Platform::LinuxX64, Platform::WindowsX64];

/// Unix-only platforms among the currently published set, used for
/// shell-integration components (e.g. `kr`/`kw`/`ke` helpers). Only
/// linux-x64 ships today; see [`ALL_PLATFORMS`] for why macOS is absent.
const UNIX_PLATFORMS: &[Platform] = &[Platform::LinuxX64];

/// Static registry of all installable Kleos components.
static ALL_COMPONENTS: &[Component] = &[
    Component {
        id: "kleos-server",
        display_name: "Kleos Server",
        description: "Core memory and intelligence server",
        required: true,
        platforms: ALL_PLATFORMS,
        depends_on: &[],
    },
    Component {
        id: "kleos-cli",
        display_name: "Kleos CLI",
        description: "Command-line interface for Kleos",
        required: true,
        platforms: ALL_PLATFORMS,
        depends_on: &[],
    },
    Component {
        id: "kleos-mcp",
        display_name: "Kleos MCP",
        description: "MCP protocol bridge",
        required: false,
        platforms: ALL_PLATFORMS,
        depends_on: &[],
    },
    Component {
        id: "kleos-credd",
        display_name: "Credential Daemon",
        description: "Credential daemon",
        required: false,
        platforms: ALL_PLATFORMS,
        depends_on: &[],
    },
    Component {
        id: "cred",
        display_name: "Credential CLI",
        description: "Credential CLI",
        required: false,
        platforms: ALL_PLATFORMS,
        depends_on: &["kleos-credd"],
    },
    Component {
        id: "kleos-sidecar",
        display_name: "Kleos Sidecar",
        description: "Sidecar proxy",
        required: false,
        platforms: ALL_PLATFORMS,
        depends_on: &[],
    },
    Component {
        id: "agent-forge",
        display_name: "Agent Forge",
        description: "Agent workflow tool",
        required: false,
        platforms: ALL_PLATFORMS,
        depends_on: &[],
    },
    Component {
        id: "eidolon-supervisor",
        display_name: "Eidolon Supervisor",
        description: "Agent supervisor",
        required: false,
        platforms: ALL_PLATFORMS,
        depends_on: &[],
    },
    Component {
        id: "kleos-sh",
        display_name: "Shell Helpers",
        description: "Shell helpers (kr/kw/ke)",
        required: false,
        platforms: UNIX_PLATFORMS,
        depends_on: &[],
    },
    Component {
        id: "kleos-ingest",
        display_name: "Kleos Ingest",
        description: "Document ingestion tool",
        required: false,
        platforms: ALL_PLATFORMS,
        depends_on: &[],
    },
];

/// Return the full static component registry.
///
/// Callers should filter by platform availability before presenting choices to
/// the user.
pub fn all_components() -> &'static [Component] {
    ALL_COMPONENTS
}

/// Return the component IDs that belong to a given installation profile.
///
/// For `Profile::Custom` an empty slice is returned -- callers are responsible
/// for building the selection themselves.
pub fn profile_components(profile: Profile) -> Vec<&'static str> {
    match profile {
        Profile::Server => vec!["kleos-server", "kleos-cli"],
        Profile::AgentHost => vec![
            "kleos-cli",
            "kleos-sh",
            "agent-forge",
            "eidolon-supervisor",
            "cred",
            "kleos-credd",
        ],
        Profile::Full => ALL_COMPONENTS.iter().map(|c| c.id).collect(),
        Profile::Custom => vec![],
    }
}

/// Expand a component selection by adding any transitively required dependencies.
///
/// Iterates the global registry and appends any `depends_on` IDs that are not
/// already present in `selected`. Returns the augmented list (no duplicates,
/// preserving insertion order of new additions).
pub fn resolve_dependencies(selected: &[&str]) -> Vec<&'static str> {
    let mut result: Vec<&'static str> = selected
        .iter()
        .filter_map(|&id| ALL_COMPONENTS.iter().find(|c| c.id == id).map(|c| c.id))
        .collect();

    let mut i = 0;
    while i < result.len() {
        let current_id = result[i];
        if let Some(component) = ALL_COMPONENTS.iter().find(|c| c.id == current_id) {
            for &dep in component.depends_on {
                if !result.contains(&dep) {
                    result.push(dep);
                }
            }
        }
        i += 1;
    }

    result
}

/// Tests for the platform-narrowing decision functions ([`is_platform_published`]
/// / [`check_platform_published`]) and the registry lists that must stay in
/// sync with them.
#[cfg(test)]
mod tests {
    use super::*;

    // Only the two platforms Kleos actually builds releases for are reported
    // published; every other enum variant must be excluded.
    #[test]
    fn only_linux_x64_and_windows_x64_are_published() {
        assert!(is_platform_published(Platform::LinuxX64));
        assert!(is_platform_published(Platform::WindowsX64));
        assert!(!is_platform_published(Platform::LinuxArm64));
        assert!(!is_platform_published(Platform::DarwinX64));
        assert!(!is_platform_published(Platform::DarwinArm64));
    }

    // A published platform passes the check with no error.
    #[test]
    fn check_platform_published_ok_for_published_platforms() {
        assert!(check_platform_published(Platform::LinuxX64).is_ok());
        assert!(check_platform_published(Platform::WindowsX64).is_ok());
    }

    // An unpublished platform gets a clear, actionable error naming the
    // release suffix and the supported alternatives -- not a generic
    // "unsupported" message a user can't act on.
    #[test]
    fn check_platform_published_errors_with_actionable_message() {
        let err = check_platform_published(Platform::DarwinArm64)
            .expect_err("darwin-arm64 must not be published");
        let msg = err.to_string();
        assert!(msg.contains("darwin-arm64"), "message was: {msg}");
        assert!(msg.contains("linux-x64/windows-x64"), "message was: {msg}");
        assert!(msg.contains("build from source"), "message was: {msg}");
    }

    // The advertised platform lists must never claim support for a platform
    // the decision function says isn't published -- this is the guard against
    // the registry and the enforcement drifting apart.
    #[test]
    fn advertised_platform_lists_match_published_set() {
        for &p in ALL_PLATFORMS {
            assert!(
                is_platform_published(p),
                "{p:?} advertised but not published"
            );
        }
        for &p in UNIX_PLATFORMS {
            assert!(
                is_platform_published(p),
                "{p:?} advertised but not published"
            );
        }
    }

    // Every component in the static registry must only list published
    // platforms -- otherwise the installer would advertise a component on a
    // platform with nothing to download.
    #[test]
    fn no_component_advertises_an_unpublished_platform() {
        for component in ALL_COMPONENTS {
            for &p in component.platforms {
                assert!(
                    is_platform_published(p),
                    "component '{}' advertises unpublished platform {p:?}",
                    component.id
                );
            }
        }
    }
}
