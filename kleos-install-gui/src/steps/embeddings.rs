//! Embedding and reranker configuration wizard step.
//!
//! Presents radio buttons to switch between local ONNX and remote HTTP
//! embedding providers. Conditional fields for URL, API key, model name, and
//! dimension appear when the remote option is selected. A separate reranker
//! section below mirrors the same pattern with an additional "Disabled" option.

use eframe::egui;

use crate::theme;
use crate::wizard::InstallerApp;

/// Validation error for the remote embedding output-dimension field, if any.
///
/// Must parse as a positive integer -- `0` is not a valid embedding dimension.
fn dimension_error(dimension: &str) -> Option<&'static str> {
    match dimension.parse::<u32>() {
        Ok(d) if d > 0 => None,
        _ => Some("Dimension must be a positive integer."),
    }
}

/// Return `true` if every field on the embeddings/reranker step is valid.
///
/// Local providers have no validatable input. The remote embedding provider
/// requires a non-empty endpoint URL and a positive-integer output dimension;
/// the remote reranker requires a non-empty endpoint URL.
pub fn is_valid(app: &InstallerApp) -> bool {
    let embed_ok = app.embedding_provider_local
        || (!app.remote_embed_url.is_empty()
            && dimension_error(&app.remote_embed_dimension).is_none());

    let reranker_ok = app.reranker_mode != 1 || !app.remote_reranker_url.is_empty();

    embed_ok && reranker_ok
}

/// Draw the embeddings and reranker configuration step.
///
/// The embedding section uses a radio pair (Local ONNX / Remote). The
/// reranker section uses a radio triple (Local ONNX / Remote / Disabled).
/// Selecting "Remote" in either section reveals additional URL/key fields.
pub fn draw_embeddings(ui: &mut egui::Ui, app: &mut InstallerApp) {
    ui.heading("Embedding Provider");
    ui.add_space(4.0);
    ui.colored_label(
        theme::COLOR_TEXT_DIM,
        "Kleos uses embeddings to convert text into vectors for semantic search.",
    );
    ui.add_space(12.0);

    // -- Embedding provider radio --
    ui.horizontal(|ui| {
        ui.radio_value(&mut app.embedding_provider_local, true, "Local ONNX");
        ui.radio_value(
            &mut app.embedding_provider_local,
            false,
            "Remote (OpenAI-compatible)",
        );
    });
    ui.add_space(8.0);

    if app.embedding_provider_local {
        ui.colored_label(
            theme::COLOR_TEXT_DIM,
            "The installer will download a local ONNX embedding model (BAAI/bge-m3).",
        );
    } else {
        draw_remote_embedding_fields(ui, app);
    }

    ui.add_space(16.0);
    ui.separator();
    ui.add_space(12.0);

    // -- Reranker section --
    ui.heading("Reranker");
    ui.add_space(4.0);
    ui.colored_label(
        theme::COLOR_TEXT_DIM,
        "The optional reranker improves retrieval precision by scoring candidate results.",
    );
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        ui.radio_value(&mut app.reranker_mode, 0u8, "Local ONNX");
        ui.radio_value(&mut app.reranker_mode, 1u8, "Remote");
        ui.radio_value(&mut app.reranker_mode, 2u8, "Disabled");
    });
    ui.add_space(8.0);

    match app.reranker_mode {
        0 => {
            ui.colored_label(
                theme::COLOR_TEXT_DIM,
                "A local cross-encoder reranker model will be used.",
            );
        }
        1 => {
            draw_remote_reranker_fields(ui, app);
        }
        _ => {
            ui.colored_label(
                theme::COLOR_TEXT_DIM,
                "Reranking is disabled. Retrieved results will not be reordered.",
            );
        }
    }
}

/// Render the text-edit fields for a remote embedding endpoint.
///
/// Shows URL, API key (optional), model name, and output dimension fields.
fn draw_remote_embedding_fields(ui: &mut egui::Ui, app: &mut InstallerApp) {
    ui.label("Endpoint URL:");
    ui.add(
        egui::TextEdit::singleline(&mut app.remote_embed_url)
            .hint_text("https://api.openai.com/v1"),
    );
    if app.remote_embed_url.is_empty() {
        ui.colored_label(theme::COLOR_ERROR, "Endpoint URL is required.");
    }
    ui.add_space(6.0);

    ui.label("API key (leave empty if not required):");
    ui.add(
        egui::TextEdit::singleline(&mut app.remote_embed_api_key)
            .hint_text("sk-...")
            .password(true),
    );
    ui.add_space(6.0);

    ui.label("Model name:");
    ui.add(
        egui::TextEdit::singleline(&mut app.remote_embed_model).hint_text("text-embedding-3-small"),
    );
    ui.add_space(6.0);

    ui.label("Output dimension:");
    ui.add(egui::TextEdit::singleline(&mut app.remote_embed_dimension).hint_text("1024"));
    if let Some(err) = dimension_error(&app.remote_embed_dimension) {
        ui.colored_label(theme::COLOR_ERROR, err);
    }
}

/// Render the text-edit fields for a remote reranker endpoint.
///
/// Shows URL, API key (optional), and model name fields.
fn draw_remote_reranker_fields(ui: &mut egui::Ui, app: &mut InstallerApp) {
    ui.label("Reranker endpoint URL:");
    ui.add(
        egui::TextEdit::singleline(&mut app.remote_reranker_url)
            .hint_text("https://reranker.example.com"),
    );
    if app.remote_reranker_url.is_empty() {
        ui.colored_label(theme::COLOR_ERROR, "Reranker URL is required.");
    }
    ui.add_space(6.0);

    ui.label("API key (leave empty if not required):");
    ui.add(
        egui::TextEdit::singleline(&mut app.remote_reranker_api_key)
            .hint_text("sk-...")
            .password(true),
    );
    ui.add_space(6.0);

    ui.label("Model name:");
    ui.add(
        egui::TextEdit::singleline(&mut app.remote_reranker_model)
            .hint_text("cross-encoder/ms-marco-MiniLM-L-6-v2"),
    );
}
