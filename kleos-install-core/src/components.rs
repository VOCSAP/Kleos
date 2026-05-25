//! Component registry and installation profiles for the Kleos installer.

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

impl Platform {
    /// Detect the current platform from compile-time OS and architecture constants.
    ///
    /// Returns the best matching `Platform` variant, or panics if the combination
    /// is not supported.
    pub fn detect() -> Platform {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => Platform::LinuxX64,
            ("linux", "aarch64") => Platform::LinuxArm64,
            ("macos", "x86_64") => Platform::DarwinX64,
            ("macos", "aarch64") => Platform::DarwinArm64,
            ("windows", "x86_64") => Platform::WindowsX64,
            (os, arch) => panic!("unsupported platform: {os}/{arch}"),
        }
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

/// All supported platforms (used for components that ship everywhere).
const ALL_PLATFORMS: &[Platform] = &[
    Platform::LinuxX64,
    Platform::LinuxArm64,
    Platform::DarwinX64,
    Platform::DarwinArm64,
    Platform::WindowsX64,
];

/// Unix-only platforms (Linux + macOS), used for shell-integration components.
const UNIX_PLATFORMS: &[Platform] = &[
    Platform::LinuxX64,
    Platform::LinuxArm64,
    Platform::DarwinX64,
    Platform::DarwinArm64,
];

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
