//! Component selection wizard step.
//!
//! Renders profile quick-select buttons and a per-component checkbox list. A
//! required component is only rendered as disabled (locked, checked) while it
//! is actually part of the current profile's selection -- e.g. the Agent
//! Host profile does not include `kleos-server`, so `kleos-server`'s
//! `required` flag must not force it to render checked in that case. A
//! summary line at the bottom shows how many required/optional components
//! are actually selected.

use eframe::egui;
use kleos_install_core::{all_components, profile_components, resolve_dependencies, Profile};

use crate::theme;
use crate::wizard::InstallerApp;

/// Draw the component-selection step.
///
/// Shows four profile buttons at the top for quick selection, followed by a
/// scrollable list of individual component checkboxes. A required component
/// is checked and non-interactive only while it is actually part of the
/// current selection; otherwise it renders like any other unselected
/// component. A count summary is displayed at the bottom.
pub fn draw_components(ui: &mut egui::Ui, app: &mut InstallerApp) {
    ui.heading("Select Components");
    ui.add_space(4.0);
    ui.colored_label(
        theme::COLOR_TEXT_DIM,
        "Choose an installation profile or customise the component list below.",
    );
    ui.add_space(12.0);

    // -- Profile buttons --
    ui.label("Quick profiles:");
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        draw_profile_button(ui, app, Profile::Server, "Server");
        draw_profile_button(ui, app, Profile::AgentHost, "Agent Host");
        draw_profile_button(ui, app, Profile::Full, "Full");
        draw_profile_button(ui, app, Profile::Custom, "Custom");
    });

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(8.0);

    // -- Component list --
    let platform = app.platform_info.platform;
    let components: Vec<_> = all_components()
        .iter()
        .filter(|c| c.platforms.contains(&platform))
        .collect();

    for component in &components {
        let id = component.id.to_string();
        let is_member = app.selected_components.contains(&id);
        // A required component is locked (cannot be unchecked) only while it
        // is actually part of the current selection -- some profiles (e.g.
        // Agent Host) deliberately omit components that are marked required
        // for other profiles, so `component.required` alone must not force a
        // checked/locked render here.
        let is_locked = component.required && is_member;
        let mut selected = is_member;

        ui.horizontal(|ui| {
            if is_locked {
                ui.add_enabled(false, egui::Checkbox::new(&mut selected, ""));
                ui.colored_label(theme::COLOR_TEXT_DIM, component.display_name);
                ui.colored_label(theme::COLOR_WARN, "(required)");
            } else {
                let changed = ui.checkbox(&mut selected, component.display_name).changed();
                if changed {
                    if selected {
                        if !app.selected_components.contains(&id) {
                            app.selected_components.push(id.clone());
                        }
                        // Resolve and add dependencies.
                        let refs: Vec<&str> =
                            app.selected_components.iter().map(|s| s.as_str()).collect();
                        let resolved = resolve_dependencies(&refs);
                        app.selected_components =
                            resolved.into_iter().map(|s| s.to_string()).collect();
                        // Selecting anything manually switches to Custom.
                        app.selected_profile = Some(Profile::Custom);
                    } else {
                        app.selected_components.retain(|c| c != &id);
                        app.selected_profile = Some(Profile::Custom);
                    }
                }
            }
        });

        ui.colored_label(
            theme::COLOR_TEXT_DIM,
            format!("    {}", component.description),
        );
        ui.add_space(2.0);
    }

    ui.add_space(12.0);
    ui.separator();
    ui.add_space(4.0);

    // -- Summary --
    // Both counts reflect actual membership in `selected_components`, not
    // just the registry's static `required` flag -- otherwise a profile like
    // Agent Host (which omits `kleos-server`) would report it as selected
    // when it is not.
    let required_selected = app
        .selected_components
        .iter()
        .filter(|id| {
            components
                .iter()
                .find(|c| c.id == id.as_str())
                .map(|c| c.required)
                .unwrap_or(false)
        })
        .count();
    let optional_selected = app
        .selected_components
        .iter()
        .filter(|id| {
            components
                .iter()
                .find(|c| c.id == id.as_str())
                .map(|c| !c.required)
                .unwrap_or(false)
        })
        .count();

    ui.colored_label(
        theme::COLOR_ACCENT,
        format!(
            "{} required + {} optional component(s) selected",
            required_selected, optional_selected
        ),
    );
}

/// Render a single profile quick-select button.
///
/// Highlights the button with the accent colour when the given `profile`
/// matches the app's currently active profile. Clicking the button updates the
/// component selection to match the profile.
fn draw_profile_button(ui: &mut egui::Ui, app: &mut InstallerApp, profile: Profile, label: &str) {
    let is_active = app.selected_profile == Some(profile);
    let button = if is_active {
        egui::Button::new(egui::RichText::new(label).color(egui::Color32::BLACK))
            .fill(theme::COLOR_ACCENT)
    } else {
        egui::Button::new(label)
    };

    if ui.add(button).clicked() {
        app.selected_profile = Some(profile);
        if profile != Profile::Custom {
            let ids = profile_components(profile);
            let resolved = resolve_dependencies(&ids);
            app.selected_components = resolved.into_iter().map(|s| s.to_string()).collect();
        }
    }
}
