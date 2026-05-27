//! Summary and installation wizard step.
//!
//! Displays a read-only review of all user selections, then drives the actual
//! installation when the "Install" button in the nav bar is clicked. Progress
//! messages streamed from the background thread are shown in a scrollable log.
//! After completion, the API key and server URL are displayed prominently.

use eframe::egui;
use kleos_install_core::system::SystemIntegration;

use crate::theme;
use crate::wizard::{InstallResult, InstallerApp};

/// Draw the summary and installation step.
///
/// Shows all selections from previous steps. During the install a scrollable
/// progress log is rendered. After completion the success panel is shown.
pub fn draw_summary(ui: &mut egui::Ui, app: &mut InstallerApp) {
    ui.heading("Installation Summary");
    ui.add_space(4.0);
    ui.colored_label(
        theme::COLOR_TEXT_DIM,
        "Review your selections. Click Install (bottom right) to begin.",
    );
    ui.add_space(12.0);

    // -- Post-install result display --
    if let Some(result) = &app.install_result.clone() {
        match result {
            Ok(r) => draw_success_panel(ui, r),
            Err(e) => {
                ui.colored_label(theme::COLOR_ERROR, format!("Installation failed: {}", e));
            }
        }
        return;
    }

    // -- Progress log (during install) --
    if app.install_running {
        // Check for completion or error sentinels from the background thread.
        if let Some(done_line) = app
            .install_progress
            .iter()
            .find(|l| l.starts_with("INSTALL_DONE:"))
        {
            let parts: Vec<&str> = done_line.splitn(3, ':').collect();
            let api_key = parts.get(1).unwrap_or(&"").to_string();
            let server_url = parts.get(2).unwrap_or(&"").to_string();
            let result = InstallResult {
                summary: format!(
                    "Installed {} components to {}",
                    app.selected_components.len(),
                    app.install_dir.display()
                ),
                api_key,
                server_url,
            };
            app.install_running = false;
            app.install_result = Some(Ok(result));
            return;
        }

        if let Some(err_line) = app
            .install_progress
            .iter()
            .find(|l| l.starts_with("INSTALL_ERROR:"))
        {
            let err_msg = err_line
                .strip_prefix("INSTALL_ERROR:")
                .unwrap_or("unknown error");
            app.install_running = false;
            app.install_result = Some(Err(err_msg.to_string()));
            return;
        }

        ui.label("Installing...");
        ui.add_space(4.0);

        egui::ScrollArea::vertical()
            .max_height(300.0)
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &app.install_progress {
                    if !line.starts_with("INSTALL_DONE:") && !line.starts_with("INSTALL_ERROR:") {
                        ui.monospace(line);
                    }
                }
            });
        return;
    }

    // -- Pre-install summary panels --
    draw_component_summary(ui, app);
    ui.add_space(8.0);

    draw_server_summary(ui, app);
    ui.add_space(8.0);

    draw_embedding_summary(ui, app);
    ui.add_space(8.0);

    draw_security_summary(ui, app);
    ui.add_space(8.0);

    draw_system_summary(ui, app);
}

/// Render the success panel shown after a successful installation.
///
/// Displays the generated API key, the server URL, and next-step instructions.
fn draw_success_panel(ui: &mut egui::Ui, result: &InstallResult) {
    ui.colored_label(theme::COLOR_ACCENT, "Installation complete!");
    ui.add_space(12.0);

    ui.label("Server URL:");
    let mut url = result.server_url.clone();
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut url)
                .font(egui::TextStyle::Monospace)
                .interactive(false)
                .desired_width(300.0),
        );
        if ui.button("Copy").clicked() {
            ui.ctx().copy_text(result.server_url.clone());
        }
    });

    ui.add_space(8.0);
    ui.label("API key (save this now -- it cannot be recovered):");
    let mut key = result.api_key.clone();
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut key)
                .font(egui::TextStyle::Monospace)
                .interactive(false)
                .desired_width(400.0),
        );
        if ui.button("Copy").clicked() {
            ui.ctx().copy_text(result.api_key.clone());
        }
    });

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);

    ui.heading("Next steps");
    ui.add_space(4.0);
    ui.label("1. Start the server: kleos-server");
    ui.label("2. Configure your agents with the API key above.");
    ui.label("3. Run: kleos-cli status   to verify the server is up.");
    ui.add_space(8.0);
    ui.colored_label(theme::COLOR_TEXT_DIM, &result.summary);
}

/// Render a summary of the selected components.
fn draw_component_summary(ui: &mut egui::Ui, app: &InstallerApp) {
    ui.strong("Components");
    ui.separator();
    ui.add_space(4.0);

    let profile_label = match app.selected_profile {
        Some(kleos_install_core::Profile::Server) => "Server",
        Some(kleos_install_core::Profile::AgentHost) => "Agent Host",
        Some(kleos_install_core::Profile::Full) => "Full",
        Some(kleos_install_core::Profile::Custom) | None => "Custom",
    };
    ui.label(format!(
        "Profile: {}   ({} components)",
        profile_label,
        app.selected_components.len()
    ));
    ui.label(format!("Install directory: {}", app.install_dir.display()));
    ui.label(format!("Config directory: {}", app.config_dir.display()));
}

/// Render a summary of the server configuration.
fn draw_server_summary(ui: &mut egui::Ui, app: &InstallerApp) {
    ui.strong("Server");
    ui.separator();
    ui.add_space(4.0);

    let host = if app.server_host_buf.is_empty() {
        "127.0.0.1"
    } else {
        &app.server_host_buf
    };
    let port = app.server_port_buf.parse::<u16>().unwrap_or(4200);
    ui.label(format!("Listen: http://{}:{}", host, port));
    ui.label(format!("Data directory: {}", app.server_data_dir_buf));
    ui.label(format!("Database: {}", app.server_db_path_buf));
}

/// Render a summary of the embedding and reranker configuration.
fn draw_embedding_summary(ui: &mut egui::Ui, app: &InstallerApp) {
    ui.strong("Embeddings");
    ui.separator();
    ui.add_space(4.0);

    let embed_label = if app.embedding_provider_local {
        "Local ONNX (BAAI/bge-m3)".to_string()
    } else {
        format!(
            "Remote: {} (model: {})",
            app.remote_embed_url, app.remote_embed_model
        )
    };
    ui.label(format!("Provider: {}", embed_label));

    let reranker_label = match app.reranker_mode {
        0 => "Local ONNX".to_string(),
        1 => format!("Remote: {}", app.remote_reranker_url),
        _ => "Disabled".to_string(),
    };
    ui.label(format!("Reranker: {}", reranker_label));
}

/// Render a summary of the security configuration.
fn draw_security_summary(ui: &mut egui::Ui, app: &InstallerApp) {
    ui.strong("Security");
    ui.separator();
    ui.add_space(4.0);

    let key_preview: String = app
        .security_config
        .initial_api_key
        .chars()
        .take(20)
        .collect();
    ui.label(format!("API key: {}...", key_preview));
    ui.label(format!(
        "Open access: {}",
        if app.security_config.open_access {
            "YES (dev mode)"
        } else {
            "No"
        }
    ));
    if app.security_config.open_access {
        ui.colored_label(theme::COLOR_WARN, "WARNING: open access is enabled.");
    }
}

/// Render a summary of the system integration settings.
fn draw_system_summary(ui: &mut egui::Ui, app: &InstallerApp) {
    ui.strong("System Integration");
    ui.separator();
    ui.add_space(4.0);

    match &app.system_integration {
        SystemIntegration::Systemd { auto_start } => {
            ui.label("Service manager: systemd user service");
            ui.label(format!(
                "Auto-start on login: {}",
                if *auto_start { "Yes" } else { "No" }
            ));
        }
        SystemIntegration::Launchd { auto_start } => {
            ui.label("Service manager: launchd user agent");
            ui.label(format!(
                "Auto-start on login: {}",
                if *auto_start { "Yes" } else { "No" }
            ));
        }
        SystemIntegration::WindowsService => {
            ui.label("Service manager: Windows Service");
        }
        SystemIntegration::None => {
            ui.label("Service manager: None (manual start)");
        }
    }
}
