//! Kleos visual theme for the GUI installer.
//!
//! Defines the brand colour palette and applies it to an [`egui::Context`] so
//! that every widget rendered afterwards uses consistent colours, spacing, and
//! corner rounding.

use eframe::egui;

/// Dark navy background used as the primary window / panel colour.
pub const COLOR_BG: egui::Color32 = egui::Color32::from_rgb(0x1a, 0x1a, 0x2e);

/// Teal accent colour used for active buttons, highlights, and progress
/// indicators.
pub const COLOR_ACCENT: egui::Color32 = egui::Color32::from_rgb(0x00, 0xd4, 0xaa);

/// Dimmed variant of the accent used for hovered interactive elements.
pub const COLOR_ACCENT_DIM: egui::Color32 = egui::Color32::from_rgb(0x00, 0xa8, 0x88);

/// Primary text colour -- pure white.
pub const COLOR_TEXT: egui::Color32 = egui::Color32::WHITE;

/// Secondary text colour -- light grey for descriptions and labels.
pub const COLOR_TEXT_DIM: egui::Color32 = egui::Color32::from_rgb(0xcc, 0xcc, 0xcc);

/// Warning colour -- amber yellow used for cautionary labels.
pub const COLOR_WARN: egui::Color32 = egui::Color32::from_rgb(0xff, 0xcc, 0x00);

/// Error colour -- red used for validation failures and danger labels.
pub const COLOR_ERROR: egui::Color32 = egui::Color32::from_rgb(0xff, 0x44, 0x44);

/// Slightly elevated surface colour for card-like frames.
pub const COLOR_SURFACE: egui::Color32 = egui::Color32::from_rgb(0x22, 0x22, 0x3a);

/// Corner rounding radius (u8) applied to buttons and frames in egui 0.31.
pub const ROUNDING: u8 = 4;

/// Apply the Kleos brand theme to the given egui context.
///
/// Sets background colours, widget colours, spacing, and corner rounding so
/// that every frame rendered within this context inherits the Kleos look.
pub fn apply_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    // -- Colours --
    style.visuals.window_fill = COLOR_BG;
    style.visuals.panel_fill = COLOR_BG;
    style.visuals.extreme_bg_color = egui::Color32::from_rgb(0x10, 0x10, 0x20);
    style.visuals.faint_bg_color = COLOR_SURFACE;
    style.visuals.override_text_color = Some(COLOR_TEXT);

    // Hyperlink colour reuses the teal accent.
    style.visuals.hyperlink_color = COLOR_ACCENT;

    // Selection highlight.
    style.visuals.selection.bg_fill = COLOR_ACCENT.gamma_multiply(0.3);
    style.visuals.selection.stroke = egui::Stroke::new(1.0, COLOR_ACCENT);

    // Corner radius type used by egui 0.31 WidgetVisuals.
    let cr = egui::CornerRadius::same(ROUNDING);

    // -- Widget normal state --
    style.visuals.widgets.noninteractive.bg_fill = COLOR_SURFACE;
    style.visuals.widgets.noninteractive.fg_stroke =
        egui::Stroke::new(1.0, COLOR_TEXT_DIM);
    style.visuals.widgets.noninteractive.corner_radius = cr;

    // -- Widget inactive --
    style.visuals.widgets.inactive.bg_fill = COLOR_SURFACE;
    style.visuals.widgets.inactive.fg_stroke =
        egui::Stroke::new(1.0, COLOR_TEXT);
    style.visuals.widgets.inactive.corner_radius = cr;

    // -- Widget hovered --
    style.visuals.widgets.hovered.bg_fill = COLOR_ACCENT_DIM;
    style.visuals.widgets.hovered.fg_stroke =
        egui::Stroke::new(1.5, COLOR_TEXT);
    style.visuals.widgets.hovered.corner_radius = cr;
    style.visuals.widgets.hovered.bg_stroke =
        egui::Stroke::new(1.0, COLOR_ACCENT);

    // -- Widget active (pressed) --
    style.visuals.widgets.active.bg_fill = COLOR_ACCENT;
    style.visuals.widgets.active.fg_stroke =
        egui::Stroke::new(1.5, egui::Color32::BLACK);
    style.visuals.widgets.active.corner_radius = cr;

    // -- Spacing --
    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    // egui 0.31: Margin::same takes i8
    style.spacing.window_margin = egui::Margin::same(16);

    ctx.set_style(style);
}
