//! Entry point for the Kleos native GUI installer.
//!
//! Initialises tracing, configures the eframe window, and launches the
//! installer application.

mod steps;
mod theme;
mod wizard;

use eframe::egui;
use eframe::NativeOptions;
use tracing_subscriber::EnvFilter;
use wizard::InstallerApp;

/// Minimum allowed window width in logical pixels.
const MIN_WIDTH: f32 = 700.0;

/// Minimum allowed window height in logical pixels.
const MIN_HEIGHT: f32 = 500.0;

/// Default initial window width in logical pixels.
const DEFAULT_WIDTH: f32 = 850.0;

/// Default initial window height in logical pixels.
const DEFAULT_HEIGHT: f32 = 650.0;

fn main() -> eframe::Result<()> {
    // Initialise structured tracing; fall back to INFO if RUST_LOG is unset.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!("kleos-install-gui starting");

    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Kleos Installer")
            .with_inner_size([DEFAULT_WIDTH, DEFAULT_HEIGHT])
            .with_min_inner_size([MIN_WIDTH, MIN_HEIGHT])
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "Kleos Installer",
        options,
        Box::new(|cc| Ok(Box::new(InstallerApp::new(cc)))),
    )
}
