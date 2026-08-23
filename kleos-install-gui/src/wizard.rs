//! Main application state and eframe integration for the Kleos installer wizard.
//!
//! The wizard is a linear 6-step flow. Each step is rendered by a dedicated
//! function in the `steps` sub-modules. Navigation is driven by "Back" and
//! "Next" buttons rendered in the bottom panel.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use eframe::egui;
use kleos_install_core::system::SystemIntegration;
use kleos_install_core::{
    profile_components, EmbeddingConfig, ExistingInstall, InstallPlan, InstallerConfig,
    PlatformInfo, PreservedSecrets, Profile, RerankerConfig, SecurityConfig, ServerConfig,
};

use crate::steps;
use crate::theme;

// ---------------------------------------------------------------------------
// Step enum
// ---------------------------------------------------------------------------

/// Ordered enumeration of all wizard steps.
///
/// The steps are presented in sequence; the user navigates forward and back
/// using the bottom-panel buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WizardStep {
    /// Component and profile selection.
    Components,
    /// Server bind address, port, and data directory.
    ServerConfig,
    /// Embedding and reranker provider selection.
    Embeddings,
    /// Security key generation and access control.
    Security,
    /// System service integration (systemd / launchd / none).
    SystemIntegration,
    /// Advanced (expert) server toggles.
    Advanced,
    /// Plan summary and installation trigger.
    Summary,
}

/// Step-ordering and display helpers for the GUI wizard flow.
impl WizardStep {
    /// Human-readable label shown in the step indicator bar.
    pub fn label(self) -> &'static str {
        match self {
            WizardStep::Components => "Components",
            WizardStep::ServerConfig => "Server",
            WizardStep::Embeddings => "Embeddings",
            WizardStep::Security => "Security",
            WizardStep::SystemIntegration => "System",
            WizardStep::Advanced => "Advanced",
            WizardStep::Summary => "Install",
        }
    }

    /// Ordered list of all wizard steps.
    pub fn all() -> Vec<WizardStep> {
        vec![
            WizardStep::Components,
            WizardStep::ServerConfig,
            WizardStep::Embeddings,
            WizardStep::Security,
            WizardStep::SystemIntegration,
            WizardStep::Advanced,
            WizardStep::Summary,
        ]
    }
}

// ---------------------------------------------------------------------------
// Install result
// ---------------------------------------------------------------------------

/// Outcome of a completed installation run.
#[derive(Debug, Clone)]
pub struct InstallResult {
    /// Human-readable summary of what was installed.
    pub summary: String,
    /// The initial API key generated for this installation.
    pub api_key: String,
    /// The base URL the server will listen on.
    pub server_url: String,
}

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

/// Main application state shared across all wizard steps.
///
/// Implements [`eframe::App`] and drives the immediate-mode render loop.
pub struct InstallerApp {
    // -- Wizard navigation --
    /// The step currently being displayed.
    pub current_step: WizardStep,
    /// Ordered list of all steps (determines progress-bar rendering).
    pub steps: Vec<WizardStep>,

    // -- User selections --
    /// Component IDs the user has selected for installation.
    pub selected_components: Vec<String>,
    /// The profile that seeded the component selection, if any.
    pub selected_profile: Option<Profile>,
    /// Security keys and access-control settings.
    pub security_config: SecurityConfig,
    /// Secrets read from an existing install's `.env`, if any, so the
    /// Security step can indicate which fields were preserved rather than
    /// freshly generated.
    pub preserved_secrets: PreservedSecrets,
    /// System service integration choice.
    pub system_integration: SystemIntegration,
    /// Directory where binaries will be placed.
    pub install_dir: PathBuf,
    /// Directory where configuration files will be written.
    pub config_dir: PathBuf,
    /// Target Kleos version string.
    pub version: String,

    // -- Detected environment --
    /// Platform detection results. Only meaningful when `platform_error` is
    /// `None`; otherwise this holds an inert placeholder that must not be
    /// rendered or acted on.
    pub platform_info: PlatformInfo,
    /// Set when platform detection failed at startup (unrecognized OS/arch,
    /// or a recognized platform with no published release). When `Some`, the
    /// wizard shows a dedicated error screen instead of the normal steps.
    pub platform_error: Option<String>,
    /// An existing Kleos installation detected on this machine, if any.
    pub existing_install: Option<ExistingInstall>,

    // -- GUI-specific embedding/reranker state --
    /// When `true` the embedding provider is local ONNX; when `false` it is remote.
    pub embedding_provider_local: bool,
    /// Reranker mode: 0 = local ONNX, 1 = remote, 2 = disabled.
    pub reranker_mode: u8,

    // -- Remote embedding text buffers --
    /// URL for a remote embedding endpoint.
    pub remote_embed_url: String,
    /// API key for a remote embedding endpoint.
    pub remote_embed_api_key: String,
    /// Model name for a remote embedding endpoint.
    pub remote_embed_model: String,
    /// Output dimension for a remote embedding endpoint.
    pub remote_embed_dimension: String,

    // -- Remote reranker text buffers --
    /// URL for a remote reranker endpoint.
    pub remote_reranker_url: String,
    /// API key for a remote reranker endpoint.
    pub remote_reranker_api_key: String,
    /// Model name for a remote reranker endpoint.
    pub remote_reranker_model: String,

    // -- Server config text buffers (String so egui can edit them) --
    /// Text buffer for the server host field.
    pub server_host_buf: String,
    /// Text buffer for the server port field.
    pub server_port_buf: String,
    /// Text buffer for the server data directory field.
    pub server_data_dir_buf: String,
    /// Text buffer for the database filename field.
    pub server_db_path_buf: String,
    /// Text buffer for the CORS origins field.
    pub server_cors_buf: String,

    // -- Install execution state --
    /// Whether the background install thread is currently running.
    pub install_running: bool,
    /// Progress log lines collected from the background thread.
    pub install_progress: Vec<String>,
    /// Shared progress channel written by the background thread.
    pub install_progress_channel: Arc<Mutex<Vec<String>>>,
    /// Final result of the installation (set when the thread finishes).
    pub install_result: Option<Result<InstallResult, String>>,

    // -- UI transient state --
    /// Whether the "are you sure you want to quit?" confirmation is visible.
    pub show_quit_confirm: bool,
    /// Whether the "installation in progress, cannot close" warning is
    /// visible. Set when the native window-close button is clicked while
    /// `install_running` is `true`.
    pub show_install_warning: bool,

    // -- Advanced toggles --
    /// Expert toggles collected on the Advanced step, folded into the plan.
    pub advanced: steps::advanced::AdvancedToggles,
}

/// Construction, per-step rendering, and plan assembly for the GUI app.
impl InstallerApp {
    /// Construct a new [`InstallerApp`] with sensible defaults.
    ///
    /// Detects the current platform, generates initial security keys (or
    /// reuses ones preserved from an existing install), and applies the
    /// Kleos visual theme to the egui context.
    ///
    /// Platform detection can fail (unrecognized OS/arch, or a recognized
    /// platform with no published release). Rather than propagating that as
    /// an error out of `eframe`'s app-creation closure -- which would leave a
    /// GUI user staring at a window that flashed and vanished with no
    /// message -- the error is stored in `platform_error` and a placeholder
    /// `PlatformInfo` is used so construction can still complete; `update`
    /// renders a dedicated error screen whenever `platform_error` is `Some`.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::apply_theme(&cc.egui_ctx);

        let (platform_info, platform_error) = match PlatformInfo::detect() {
            Ok(info) => (info, None),
            Err(e) => (fallback_platform_info(), Some(e.to_string())),
        };
        let install_dir = platform_info.default_install_dir.clone();
        let config_dir = platform_info.default_config_dir.clone();

        let existing_install = kleos_install_core::upgrade::detect_existing_install();
        let preserved_secrets = existing_install
            .as_ref()
            .map(kleos_install_core::upgrade::read_preserved_secrets)
            .unwrap_or_default();

        let security_config = generate_security_config(&preserved_secrets);

        let selected_components = profile_components(Profile::Server)
            .into_iter()
            .map(|s| s.to_string())
            .collect();

        let server_config = ServerConfig::default();
        let server_host_buf = server_config.host.clone();
        let server_port_buf = server_config.port.to_string();
        let server_data_dir_buf = server_config.data_dir.display().to_string();
        let server_db_path_buf = server_config.db_path.clone();
        let server_cors_buf = server_config.cors_origins.clone().unwrap_or_default();

        InstallerApp {
            current_step: WizardStep::Components,
            steps: WizardStep::all(),
            selected_components,
            selected_profile: Some(Profile::Server),
            security_config,
            preserved_secrets,
            system_integration: SystemIntegration::None,
            install_dir,
            config_dir,
            version: "latest".to_string(),
            platform_info,
            platform_error,
            existing_install,
            embedding_provider_local: true,
            reranker_mode: 2,
            remote_embed_url: String::new(),
            remote_embed_api_key: String::new(),
            remote_embed_model: String::new(),
            remote_embed_dimension: "1024".to_string(),
            remote_reranker_url: String::new(),
            remote_reranker_api_key: String::new(),
            remote_reranker_model: String::new(),
            server_host_buf,
            server_port_buf,
            server_data_dir_buf,
            server_db_path_buf,
            server_cors_buf,
            install_running: false,
            install_progress: Vec::new(),
            install_progress_channel: Arc::new(Mutex::new(Vec::new())),
            install_result: None,
            show_quit_confirm: false,
            show_install_warning: false,
            advanced: steps::advanced::AdvancedToggles::default(),
        }
    }

    /// Move to the next wizard step, if one exists.
    pub fn go_next(&mut self) {
        let idx = self.steps.iter().position(|&s| s == self.current_step);
        if let Some(i) = idx {
            if i + 1 < self.steps.len() {
                self.current_step = self.steps[i + 1];
            }
        }
    }

    /// Move to the previous wizard step, if one exists.
    pub fn go_back(&mut self) {
        let idx = self.steps.iter().position(|&s| s == self.current_step);
        if let Some(i) = idx {
            if i > 0 {
                self.current_step = self.steps[i - 1];
            }
        }
    }

    /// Return `true` when the current step is the first one.
    pub fn is_first_step(&self) -> bool {
        self.current_step == *self.steps.first().unwrap_or(&WizardStep::Components)
    }

    /// Return `true` when the current step is the last one (Summary).
    pub fn is_last_step(&self) -> bool {
        self.current_step == *self.steps.last().unwrap_or(&WizardStep::Summary)
    }

    /// Build the embedding config from current GUI state.
    fn build_embedding_config(&self) -> EmbeddingConfig {
        if self.embedding_provider_local {
            EmbeddingConfig::LocalOnnx {
                model: "BAAI/bge-m3".to_string(),
                dimension: 1024,
                auto_download: true,
            }
        } else {
            EmbeddingConfig::Remote {
                url: self.remote_embed_url.clone(),
                api_key: self.remote_embed_api_key.clone(),
                model: if self.remote_embed_model.is_empty() {
                    None
                } else {
                    Some(self.remote_embed_model.clone())
                },
                dimension: self.remote_embed_dimension.parse().unwrap_or(1024),
            }
        }
    }

    /// Build the reranker config from current GUI state.
    fn build_reranker_config(&self) -> RerankerConfig {
        match self.reranker_mode {
            0 => RerankerConfig::LocalOnnx,
            1 => RerankerConfig::Remote {
                endpoint: self.remote_reranker_url.clone(),
                api_key: self.remote_reranker_api_key.clone(),
                model: self.remote_reranker_model.clone(),
            },
            _ => RerankerConfig::Disabled,
        }
    }

    /// Build the server config from current text buffers.
    fn build_server_config(&self) -> ServerConfig {
        ServerConfig {
            host: if self.server_host_buf.is_empty() {
                "127.0.0.1".to_string()
            } else {
                self.server_host_buf.clone()
            },
            port: self.server_port_buf.parse().unwrap_or(4200),
            data_dir: PathBuf::from(&self.server_data_dir_buf),
            db_path: self.server_db_path_buf.clone(),
            cors_origins: if self.server_cors_buf.is_empty() {
                None
            } else {
                Some(self.server_cors_buf.clone())
            },
        }
    }

    /// Build an [`InstallPlan`] from the current wizard state.
    ///
    /// Server, embedding, and reranker configs are only included when
    /// `kleos-server` is among the selected components.
    pub fn build_plan(&self) -> InstallPlan {
        let has_server = self.selected_components.iter().any(|c| c == "kleos-server");

        let installer_config = InstallerConfig {
            server: if has_server {
                Some(self.build_server_config())
            } else {
                None
            },
            embedding: if has_server {
                Some(self.build_embedding_config())
            } else {
                None
            },
            reranker: if has_server {
                Some(self.build_reranker_config())
            } else {
                None
            },
            security: self.security_config.clone(),
            overrides: self.advanced.to_overrides(),
        };

        InstallPlan {
            components: self.selected_components.clone(),
            install_dir: self.install_dir.clone(),
            config_dir: self.config_dir.clone(),
            version: self.version.clone(),
            config: installer_config,
            system_integration: self.system_integration.clone(),
            is_upgrade: self.existing_install.is_some(),
        }
    }

    /// Poll the shared progress channel and drain any new messages into
    /// [`InstallerApp::install_progress`].
    pub fn poll_progress(&mut self) {
        if let Ok(mut ch) = self.install_progress_channel.lock() {
            let new: Vec<String> = ch.drain(..).collect();
            self.install_progress.extend(new);
        }
    }
}

/// eframe entry point: drives one render frame of the installer per update.
impl eframe::App for InstallerApp {
    /// Render the installer for one frame.
    ///
    /// Draws the step-indicator panel at the top, the current step's content
    /// in the central area, and the navigation controls in a bottom panel.
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Platform detection failed at startup -- show the error screen only
        // and skip every other panel; there is nothing safe to configure or
        // install without a valid platform.
        if let Some(message) = &self.platform_error {
            egui::CentralPanel::default().show(ctx, |ui| {
                draw_platform_error(ui, message);
            });
            return;
        }

        // Intercept the native window-close request while an install is
        // running: non-atomic file writes may be in flight, so cancel the
        // close and show a warning instead of letting the window disappear
        // mid-write.
        if self.install_running && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.show_install_warning = true;
        }

        // Drain progress messages from background thread every frame.
        if self.install_running {
            self.poll_progress();
            ctx.request_repaint();
        }

        // -- Top panel: step indicator --
        egui::TopBottomPanel::top("step_indicator").show(ctx, |ui| {
            draw_step_indicator(ui, self);
        });

        // -- Upgrade notice banner (only when an existing install was found) --
        if let Some(existing) = &self.existing_install {
            egui::TopBottomPanel::top("upgrade_banner").show(ctx, |ui| {
                draw_upgrade_banner(ui, existing);
            });
        }

        // -- Bottom panel: navigation buttons --
        egui::TopBottomPanel::bottom("nav_buttons").show(ctx, |ui| {
            draw_nav_buttons(ui, self);
        });

        // -- Central panel: current step content --
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| match self.current_step {
                WizardStep::Components => steps::components::draw_components(ui, self),
                WizardStep::ServerConfig => steps::server::draw_server_config(ui, self),
                WizardStep::Embeddings => steps::embeddings::draw_embeddings(ui, self),
                WizardStep::Security => steps::security::draw_security(ui, self),
                WizardStep::SystemIntegration => steps::system::draw_system_integration(ui, self),
                WizardStep::Advanced => steps::advanced::draw_advanced(ui, self),
                WizardStep::Summary => steps::summary::draw_summary(ui, self),
            });
        });

        // -- Install-in-progress warning modal (native close attempted mid-install) --
        if self.show_install_warning && self.install_running {
            egui::Window::new("Installation in Progress")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label(
                        "Kleos is still writing files to disk and cannot be safely \
                         interrupted. Please wait for the installation to finish \
                         before closing this window.",
                    );
                    ui.add_space(8.0);
                    if ui.button("OK").clicked() {
                        self.show_install_warning = false;
                    }
                });
        }

        // -- Quit confirmation modal (never shown while an install is running) --
        if self.show_quit_confirm && !self.install_running {
            egui::Window::new("Quit Installer?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.label("Installation is not complete. Are you sure you want to quit?");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_quit_confirm = false;
                        }
                    });
                });
        }
    }
}

// ---------------------------------------------------------------------------
// Internal render helpers
// ---------------------------------------------------------------------------

/// Draw the horizontal step-indicator bar at the top of the window.
///
/// Each step is shown as a labelled node. The active step is highlighted with
/// the accent colour; completed steps use a dimmed style.
fn draw_step_indicator(ui: &mut egui::Ui, app: &InstallerApp) {
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let active_idx = app
            .steps
            .iter()
            .position(|&s| s == app.current_step)
            .unwrap_or(0);

        for (i, &step) in app.steps.iter().enumerate() {
            let is_active = i == active_idx;
            let is_done = i < active_idx;

            let color = if is_active {
                theme::COLOR_ACCENT
            } else if is_done {
                theme::COLOR_ACCENT_DIM
            } else {
                theme::COLOR_TEXT_DIM
            };

            ui.colored_label(color, format!("{}", i + 1));
            ui.colored_label(color, step.label());

            if i + 1 < app.steps.len() {
                ui.colored_label(theme::COLOR_TEXT_DIM, " > ");
            }
        }
    });
    ui.add_space(4.0);
    ui.separator();
}

/// Return `true` if every validatable field on the current step passes its
/// validator, mirroring the TUI's field validators. Steps with no
/// validatable input (Components, Security, SystemIntegration, Advanced,
/// Summary) are always valid -- the wizard is strictly linear, so by the
/// time the user reaches Summary every earlier step was valid at the moment
/// they last advanced past it.
fn current_step_is_valid(app: &InstallerApp) -> bool {
    match app.current_step {
        WizardStep::ServerConfig => steps::server::is_valid(app),
        WizardStep::Embeddings => steps::embeddings::is_valid(app),
        _ => true,
    }
}

/// Draw the navigation bar (Back / Next / Cancel) at the bottom of the window.
///
/// The "Back" button is hidden on the first step. The "Next" button becomes
/// "Install" when on the final Summary step and is disabled while the current
/// step has an active validation error. Cancel shows the quit confirmation
/// unless the install is already complete, and is disabled entirely while an
/// install is running -- there is no safe way to cancel mid-write.
fn draw_nav_buttons(ui: &mut egui::Ui, app: &mut InstallerApp) {
    ui.separator();
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        // Cancel / Close button on the left.
        let close_label = if app.install_result.is_some() {
            "Close"
        } else {
            "Cancel"
        };
        ui.add_enabled_ui(!app.install_running, |ui| {
            if ui.button(close_label).clicked() {
                if app.install_result.is_some() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                } else {
                    app.show_quit_confirm = true;
                }
            }
        });

        // Push Back / Next to the right.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Only show Next/Install when not in the middle of installing.
            if !app.install_running && app.install_result.is_none() {
                let next_label = if app.is_last_step() {
                    "Install"
                } else {
                    "Next >"
                };

                let valid = current_step_is_valid(app);
                let clicked = ui
                    .add_enabled_ui(valid, |ui| ui.button(next_label).clicked())
                    .inner;
                if clicked {
                    if app.is_last_step() {
                        app.install_running = true;
                        start_install(app);
                    } else {
                        app.go_next();
                    }
                }
            }

            if !app.is_first_step()
                && !app.install_running
                && app.install_result.is_none()
                && ui.button("< Back").clicked()
            {
                app.go_back();
            }
        });
    });
    ui.add_space(4.0);
}

/// Spawn a background thread to execute the install plan.
///
/// Progress messages are pushed into the shared channel so the UI thread can
/// drain them each frame. The completion sentinel "INSTALL_DONE" or
/// "INSTALL_ERROR:..." is used by the summary step to detect completion.
fn start_install(app: &mut InstallerApp) {
    let plan = app.build_plan();
    let channel = Arc::clone(&app.install_progress_channel);

    let _ = std::thread::spawn(move || {
        let ch = Arc::clone(&channel);
        let progress = ChannelProgress { channel: ch };

        match plan.execute(&progress, "") {
            Ok(result) => {
                let push = |msg: &str| {
                    if let Ok(mut ch) = channel.lock() {
                        ch.push(msg.to_string());
                    }
                };
                push(&format!(
                    "INSTALL_DONE:{}:{}",
                    result.api_key,
                    result.server_url.unwrap_or_default()
                ));
            }
            Err(e) => {
                if let Ok(mut ch) = channel.lock() {
                    ch.push(format!("INSTALL_ERROR:{e}"));
                }
            }
        }
    });
}

/// Adapter that forwards [`InstallProgress`] callbacks to the shared channel.
struct ChannelProgress {
    /// Shared channel written by the background thread, read by the UI.
    channel: Arc<Mutex<Vec<String>>>,
}

/// Forwards installer progress events into the shared log channel the GUI polls.
impl kleos_install_core::InstallProgress for ChannelProgress {
    /// Record the start of an installation phase.
    fn on_phase(&self, phase: &str, detail: &str) {
        if let Ok(mut ch) = self.channel.lock() {
            ch.push(format!("[{phase}] {detail}"));
        }
    }

    /// Record a download progress percentage for a component.
    fn on_download_progress(&self, component: &str, bytes: u64, total: u64) {
        if let Some(pct) = (bytes * 100).checked_div(total) {
            if let Ok(mut ch) = self.channel.lock() {
                ch.push(format!("  {component}: {pct}%"));
            }
        }
    }

    /// Record that a component finished installing.
    fn on_component_installed(&self, component: &str) {
        if let Ok(mut ch) = self.channel.lock() {
            ch.push(format!("  Installed {component}"));
        }
    }

    /// Record successful completion of the whole install.
    fn on_complete(&self) {
        if let Ok(mut ch) = self.channel.lock() {
            ch.push("Installation complete.".to_string());
        }
    }

    /// Record a fatal installation error.
    fn on_error(&self, error: &kleos_install_core::InstallError) {
        if let Ok(mut ch) = self.channel.lock() {
            ch.push(format!("ERROR: {error}"));
        }
    }
}

// ---------------------------------------------------------------------------
// Upgrade / platform-error UI
// ---------------------------------------------------------------------------

/// Draw a persistent banner shown on every step once an existing Kleos
/// installation has been detected, so the upgrade behavior is visible well
/// before the user reaches the Install button: an existing install was
/// found, already-configured secrets are being reused rather than
/// regenerated, and the current config will be backed up before being
/// overwritten (see `InstallPlan::execute`'s upgrade backup step).
fn draw_upgrade_banner(ui: &mut egui::Ui, existing: &ExistingInstall) {
    ui.add_space(4.0);
    ui.colored_label(
        theme::COLOR_WARN,
        format!(
            "Existing Kleos installation detected at {} ({} component(s)) -- this is an \
             upgrade. Preserved secrets will be reused (any missing ones generated fresh) \
             and the current kleos.toml/.env will be backed up before being overwritten.",
            existing.install_dir.display(),
            existing.components.len(),
        ),
    );
    ui.add_space(4.0);
}

/// Render a full-window error screen shown when platform detection failed at
/// startup (unrecognized OS/arch, or a recognized platform with no published
/// Kleos release). The wizard cannot proceed without a valid platform, so
/// this replaces the normal step flow entirely rather than letting the
/// window flash and vanish with no visible message.
fn draw_platform_error(ui: &mut egui::Ui, message: &str) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        ui.heading("Unable to Start Installer");
        ui.add_space(12.0);
        ui.colored_label(theme::COLOR_ERROR, message);
        ui.add_space(20.0);
        if ui.button("Quit").clicked() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    });
}

/// Build an inert placeholder [`PlatformInfo`] used only when platform
/// detection fails, so [`InstallerApp::new`] can still finish constructing a
/// valid `InstallerApp`. Every field is a harmless default; none of it is
/// ever read because `update` shows the platform-error screen instead of the
/// normal wizard whenever `platform_error` is `Some`.
fn fallback_platform_info() -> PlatformInfo {
    PlatformInfo {
        platform: kleos_install_core::Platform::LinuxX64,
        os_name: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        has_systemd: false,
        has_launchd: false,
        default_install_dir: PathBuf::new(),
        default_config_dir: PathBuf::new(),
        default_data_dir: PathBuf::new(),
    }
}

// ---------------------------------------------------------------------------
// Key generation
// ---------------------------------------------------------------------------

/// Generate a new [`SecurityConfig`], reusing any secrets preserved from an
/// existing installation's `.env` and generating a fresh value (via the OS
/// CSPRNG through the `rand` crate, delegated to `kleos_install_core::security`)
/// for any field that came back `None` -- no existing install, or a field
/// missing/unreadable in the existing `.env`. `initial_api_key` is never
/// preserved (it is not part of `PreservedSecrets`): a fresh one is always
/// issued.
fn generate_security_config(preserved: &PreservedSecrets) -> SecurityConfig {
    use kleos_install_core::security;
    SecurityConfig {
        encryption_key: preserved
            .encryption_key
            .clone()
            .unwrap_or_else(security::generate_encryption_key),
        api_key_pepper: preserved
            .api_key_pepper
            .clone()
            .unwrap_or_else(security::generate_api_key_pepper),
        initial_api_key: security::generate_api_key(),
        hmac_secret: preserved
            .hmac_secret
            .clone()
            .unwrap_or_else(security::generate_hmac_secret),
        open_access: false,
    }
}
