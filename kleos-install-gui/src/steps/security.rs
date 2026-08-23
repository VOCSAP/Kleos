//! Security configuration wizard step.
//!
//! Displays the generated security keys in read-only monospace text fields,
//! with "Regenerate" and "Copy" buttons beside each one. An open-access
//! toggle for development mode is shown with a prominent warning label.

use eframe::egui;

use crate::theme;
use crate::wizard::InstallerApp;

/// Draw the security configuration step.
///
/// Renders the encryption key, API key pepper, initial API key, and HMAC
/// secret in read-only monospace fields. Each field has Regenerate and Copy
/// buttons. An open-access development toggle is shown at the bottom.
pub fn draw_security(ui: &mut egui::Ui, app: &mut InstallerApp) {
    ui.heading("Security Keys");
    ui.add_space(4.0);
    ui.colored_label(
        theme::COLOR_WARN,
        "Save these keys -- you will need them to access Kleos after installation.",
    );
    ui.add_space(12.0);

    // -- Initial API key --
    ui.label("Initial API key:");
    {
        let current = app.security_config.initial_api_key.clone();
        let mut display = current.clone();
        if draw_key_row(ui, "initial_api_key", &mut display, &current) {
            app.security_config.initial_api_key = kleos_install_core::security::generate_api_key();
        }
    }
    ui.add_space(8.0);

    // -- Encryption key --
    ui.label("Database encryption key:");
    {
        let current = app.security_config.encryption_key.clone();
        let mut display = current.clone();
        if draw_key_row(ui, "encryption_key", &mut display, &current) {
            app.security_config.encryption_key =
                kleos_install_core::security::generate_encryption_key();
        }
        if is_preserved(&app.preserved_secrets.encryption_key, &current) {
            draw_preserved_note(ui);
        }
    }
    ui.add_space(8.0);

    // -- API key pepper --
    ui.label("API key pepper:");
    {
        let current = app.security_config.api_key_pepper.clone();
        let mut display = current.clone();
        if draw_key_row(ui, "api_key_pepper", &mut display, &current) {
            app.security_config.api_key_pepper =
                kleos_install_core::security::generate_api_key_pepper();
        }
        if is_preserved(&app.preserved_secrets.api_key_pepper, &current) {
            draw_preserved_note(ui);
        }
    }
    ui.add_space(8.0);

    // -- HMAC secret --
    ui.label("HMAC signing secret:");
    {
        let current = app.security_config.hmac_secret.clone();
        let mut display = current.clone();
        if draw_key_row(ui, "hmac_secret", &mut display, &current) {
            app.security_config.hmac_secret = kleos_install_core::security::generate_hmac_secret();
        }
        if is_preserved(&app.preserved_secrets.hmac_secret, &current) {
            draw_preserved_note(ui);
        }
    }
    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);

    // -- Open access toggle --
    ui.heading("Access Control");
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.checkbox(
            &mut app.security_config.open_access,
            "Enable open access (development mode)",
        );
    });

    if app.security_config.open_access {
        ui.add_space(4.0);
        ui.colored_label(
            theme::COLOR_ERROR,
            "WARNING: Open access disables API-key authentication. \
             Do not use in production environments.",
        );
    } else {
        ui.colored_label(
            theme::COLOR_TEXT_DIM,
            "All API requests will require the initial API key above.",
        );
    }
}

/// Return `true` if `current` still matches the secret preserved from an
/// existing install's `.env` (i.e. the user has not clicked Regenerate since
/// startup), so the row can show a note explaining why it must not be
/// casually regenerated.
fn is_preserved(preserved: &Option<String>, current: &str) -> bool {
    preserved.as_deref() == Some(current)
}

/// Render the note shown beneath a security field whose value was reused
/// from the existing installation's `.env` rather than freshly generated.
/// Regenerating a preserved value would make the existing encrypted database
/// unreadable or invalidate every previously issued API key/token.
fn draw_preserved_note(ui: &mut egui::Ui) {
    ui.colored_label(
        theme::COLOR_TEXT_DIM,
        "    Preserved from the existing install -- regenerating this will require \
         re-encrypting data or invalidate existing keys/tokens on next start.",
    );
}

/// Render a single key row: read-only monospace field + Regenerate button + Copy button.
///
/// Returns `true` if the Regenerate button was clicked (the caller should
/// then update the corresponding key field).
fn draw_key_row(ui: &mut egui::Ui, id: &str, display: &mut String, copy_value: &str) -> bool {
    let mut regenerate = false;
    ui.horizontal(|ui| {
        let avail = ui.available_width() - 190.0;
        ui.add(
            egui::TextEdit::singleline(display)
                .font(egui::TextStyle::Monospace)
                .interactive(false)
                .desired_width(avail.max(100.0))
                .id(egui::Id::new(id)),
        );

        if ui.button("Regenerate").clicked() {
            regenerate = true;
        }

        if ui.button("Copy").clicked() {
            ui.ctx().copy_text(copy_value.to_string());
        }
    });
    regenerate
}
