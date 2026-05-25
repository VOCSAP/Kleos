//! Server configuration wizard step.
//!
//! Presents labeled text-edit fields for the server bind address, port,
//! data directory, database path, and CORS origins. Validation errors are
//! shown as red labels beneath the offending field. A "Browse" button opens a
//! native directory picker for the data directory.

use eframe::egui;

use crate::theme;
use crate::wizard::InstallerApp;

/// Draw the server configuration step.
///
/// Renders a form with fields for host, port, data directory, database path,
/// and CORS origins. Inline validation feedback is shown for invalid values.
/// The browse button opens a native folder picker via [`rfd`].
pub fn draw_server_config(ui: &mut egui::Ui, app: &mut InstallerApp) {
    ui.heading("Server Configuration");
    ui.add_space(4.0);
    ui.colored_label(
        theme::COLOR_TEXT_DIM,
        "Configure the Kleos server bind address, storage paths, and access policy.",
    );
    ui.add_space(12.0);

    // -- Host --
    ui.label("Bind host:");
    ui.add(
        egui::TextEdit::singleline(&mut app.server_host_buf)
            .hint_text("127.0.0.1"),
    );
    if app.server_host_buf.is_empty() {
        ui.colored_label(theme::COLOR_ERROR, "Host cannot be empty.");
    }
    ui.add_space(8.0);

    // -- Port --
    ui.label("Port:");
    ui.add(
        egui::TextEdit::singleline(&mut app.server_port_buf)
            .hint_text("4200"),
    );
    let port_valid = app.server_port_buf.parse::<u16>().is_ok();
    if !port_valid {
        ui.colored_label(theme::COLOR_ERROR, "Port must be a number between 1 and 65535.");
    }
    ui.add_space(8.0);

    // -- Data directory --
    ui.label("Data directory:");
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut app.server_data_dir_buf)
                .hint_text("./data")
                .desired_width(ui.available_width() - 80.0),
        );
        if ui.button("Browse...").clicked() {
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                app.server_data_dir_buf = dir.display().to_string();
            }
        }
    });
    if app.server_data_dir_buf.is_empty() {
        ui.colored_label(theme::COLOR_ERROR, "Data directory cannot be empty.");
    }
    ui.add_space(8.0);

    // -- Database path --
    ui.label("Database file name:");
    ui.add(
        egui::TextEdit::singleline(&mut app.server_db_path_buf)
            .hint_text("kleos.db"),
    );
    if app.server_db_path_buf.is_empty() {
        ui.colored_label(theme::COLOR_ERROR, "Database path cannot be empty.");
    }
    ui.add_space(8.0);

    // -- CORS origins --
    ui.label("CORS allowed origins (comma-separated, leave empty for default policy):");
    ui.add(
        egui::TextEdit::singleline(&mut app.server_cors_buf)
            .hint_text("https://app.example.com,http://localhost:3000"),
    );
    ui.colored_label(
        theme::COLOR_TEXT_DIM,
        "Leave blank to allow only same-origin requests.",
    );
    ui.add_space(12.0);

    ui.separator();
    ui.add_space(8.0);

    // -- Summary of current values --
    ui.heading("Current values");
    ui.add_space(4.0);
    let host = if app.server_host_buf.is_empty() {
        "127.0.0.1"
    } else {
        &app.server_host_buf
    };
    let port = app.server_port_buf.parse::<u16>().unwrap_or(4200);
    ui.colored_label(
        theme::COLOR_ACCENT,
        format!("Server will listen on http://{}:{}", host, port),
    );
}
