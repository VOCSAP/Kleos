//! Embedding and reranker configuration step for the Kleos installer wizard.
//!
//! Presents radio-button selection for embedding provider (Local ONNX or Remote)
//! and reranker provider (Local ONNX, Remote, or Disabled), with conditional
//! text fields that appear based on the selection.

use crossterm::event::{KeyCode, KeyEvent};
use kleos_install_core::config::{EmbeddingConfig, RerankerConfig};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::tui::{draw_input_field, COLOR_ACTIVE, COLOR_DIM};
use crate::types::{InputField, StepResult};
use crate::wizard::WizardState;

/// Which section has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingsFocus {
    /// Focus is on the embedding provider radio buttons.
    EmbeddingProvider,
    /// Focus is on one of the embedding remote URL/key/model fields.
    EmbeddingFields,
    /// Focus is on the reranker provider radio buttons.
    RerankerProvider,
    /// Focus is on one of the reranker remote URL/key fields.
    RerankerFields,
}

/// Which embedding provider variant is selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingProviderSelection {
    /// Use a local ONNX model.
    LocalOnnx,
    /// Use a remote HTTP endpoint.
    Remote,
}

/// Which reranker provider variant is selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RerankerProviderSelection {
    /// Use a local ONNX reranker.
    LocalOnnx,
    /// Use a remote HTTP reranker endpoint.
    Remote,
    /// Disable reranking.
    Disabled,
}

/// Local UI state for the embeddings configuration step.
pub struct EmbeddingsStepState {
    /// Which UI section is currently focused.
    pub focus: EmbeddingsFocus,
    /// Currently selected embedding provider.
    pub provider_selection: EmbeddingProviderSelection,
    /// Currently selected reranker provider.
    pub reranker_selection: RerankerProviderSelection,
    /// Index within the focused field group.
    pub focused_field: usize,
    /// Remote embedding URL field.
    pub embed_url: InputField,
    /// Remote embedding API key field.
    pub embed_api_key: InputField,
    /// Remote embedding model name field.
    pub embed_model: InputField,
    /// Remote reranker URL field.
    pub reranker_url: InputField,
    /// Remote reranker API key field.
    pub reranker_api_key: InputField,
    /// Remote reranker model name field.
    pub reranker_model: InputField,
}

impl EmbeddingsStepState {
    /// Build the default step state.
    ///
    /// Defaults to local ONNX for both embedding and reranker.
    pub fn new() -> Self {
        EmbeddingsStepState {
            focus: EmbeddingsFocus::EmbeddingProvider,
            provider_selection: EmbeddingProviderSelection::LocalOnnx,
            reranker_selection: RerankerProviderSelection::Disabled,
            focused_field: 0,
            embed_url: InputField::new("Embedding URL", "http://localhost:11434/v1"),
            embed_api_key: InputField::new("API key (optional)", ""),
            embed_model: InputField::new("Model name", "BAAI/bge-m3"),
            reranker_url: InputField::new("Reranker URL", "http://localhost:8080"),
            reranker_api_key: InputField::new("API key (optional)", ""),
            reranker_model: InputField::new("Model name", "cross-encoder"),
        }
    }

    /// Return the number of remote fields for the embedding provider.
    fn embed_field_count(&self) -> usize {
        if self.provider_selection == EmbeddingProviderSelection::Remote {
            3
        } else {
            0
        }
    }

    /// Return the number of remote fields for the reranker.
    fn reranker_field_count(&self) -> usize {
        if self.reranker_selection == RerankerProviderSelection::Remote {
            3
        } else {
            0
        }
    }
}

/// Draw the embeddings step into `area`.
pub fn draw_embeddings_step(
    f: &mut Frame,
    area: Rect,
    _state: &WizardState,
    step_state: &EmbeddingsStepState,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(5),  // embedding provider section
            Constraint::Length(10), // embedding remote fields (conditional)
            Constraint::Length(5),  // reranker provider section
            Constraint::Length(8),  // reranker remote fields (conditional)
            Constraint::Min(0),     // spacer
        ])
        .split(area);

    draw_embedding_provider(f, chunks[0], step_state);
    if step_state.provider_selection == EmbeddingProviderSelection::Remote {
        draw_embedding_fields(f, chunks[1], step_state);
    }

    draw_reranker_provider(f, chunks[2], step_state);
    if step_state.reranker_selection == RerankerProviderSelection::Remote {
        draw_reranker_fields(f, chunks[3], step_state);
    }
}

/// Render the embedding provider radio buttons.
fn draw_embedding_provider(f: &mut Frame, area: Rect, step_state: &EmbeddingsStepState) {
    let focused = step_state.focus == EmbeddingsFocus::EmbeddingProvider;
    let border_style = if focused {
        Style::default().fg(COLOR_ACTIVE)
    } else {
        Style::default().fg(COLOR_DIM)
    };

    let options = vec![
        Line::from(vec![
            Span::raw(if step_state.provider_selection == EmbeddingProviderSelection::LocalOnnx {
                " (o) "
            } else {
                " ( ) "
            }),
            Span::raw("Local ONNX  "),
            Span::raw(if step_state.provider_selection == EmbeddingProviderSelection::Remote {
                " (o) "
            } else {
                " ( ) "
            }),
            Span::raw("Remote (OpenAI-compatible)"),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Embedding Provider ");
    let p = Paragraph::new(options).block(block);
    f.render_widget(p, area);
}

/// Render the embedding remote configuration fields.
fn draw_embedding_fields(f: &mut Frame, area: Rect, step_state: &EmbeddingsStepState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(3), Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let focused_field = if step_state.focus == EmbeddingsFocus::EmbeddingFields {
        Some(step_state.focused_field)
    } else {
        None
    };

    draw_input_field(
        f, chunks[0], &step_state.embed_url.label, &step_state.embed_url.value,
        &step_state.embed_url.placeholder, focused_field == Some(0), step_state.embed_url.error.as_deref(),
    );
    draw_input_field(
        f, chunks[1], &step_state.embed_api_key.label, &step_state.embed_api_key.value,
        &step_state.embed_api_key.placeholder, focused_field == Some(1), None,
    );
    draw_input_field(
        f, chunks[2], &step_state.embed_model.label, &step_state.embed_model.value,
        &step_state.embed_model.placeholder, focused_field == Some(2), None,
    );
}

/// Render the reranker provider radio buttons.
fn draw_reranker_provider(f: &mut Frame, area: Rect, step_state: &EmbeddingsStepState) {
    let focused = step_state.focus == EmbeddingsFocus::RerankerProvider;
    let border_style = if focused {
        Style::default().fg(COLOR_ACTIVE)
    } else {
        Style::default().fg(COLOR_DIM)
    };

    let sel = &step_state.reranker_selection;
    let line = Line::from(vec![
        Span::raw(if *sel == RerankerProviderSelection::LocalOnnx { " (o) " } else { " ( ) " }),
        Span::raw("Local ONNX  "),
        Span::raw(if *sel == RerankerProviderSelection::Remote { " (o) " } else { " ( ) " }),
        Span::raw("Remote  "),
        Span::raw(if *sel == RerankerProviderSelection::Disabled { " (o) " } else { " ( ) " }),
        Span::raw("Disabled"),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(" Reranker ");
    let p = Paragraph::new(vec![line]).block(block);
    f.render_widget(p, area);
}

/// Render the reranker remote configuration fields.
fn draw_reranker_fields(f: &mut Frame, area: Rect, step_state: &EmbeddingsStepState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Length(3), Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    let focused_field = if step_state.focus == EmbeddingsFocus::RerankerFields {
        Some(step_state.focused_field)
    } else {
        None
    };

    draw_input_field(
        f, chunks[0], &step_state.reranker_url.label, &step_state.reranker_url.value,
        &step_state.reranker_url.placeholder, focused_field == Some(0), None,
    );
    draw_input_field(
        f, chunks[1], &step_state.reranker_api_key.label, &step_state.reranker_api_key.value,
        &step_state.reranker_api_key.placeholder, focused_field == Some(1), None,
    );
    draw_input_field(
        f, chunks[2], &step_state.reranker_model.label, &step_state.reranker_model.value,
        &step_state.reranker_model.placeholder, focused_field == Some(2), None,
    );
}

/// Handle a key event for the embeddings step.
///
/// Navigates between provider selectors and input fields. Updates wizard state
/// when advancing to the next step.
pub fn handle_embeddings_input(
    key: KeyEvent,
    state: &mut WizardState,
    step_state: &mut EmbeddingsStepState,
) -> StepResult {
    match key.code {
        KeyCode::Esc => {
            if step_state.focus == EmbeddingsFocus::EmbeddingProvider {
                return StepResult::Back;
            }
            // Move focus backwards through sections.
            step_state.focus = match step_state.focus {
                EmbeddingsFocus::EmbeddingFields => EmbeddingsFocus::EmbeddingProvider,
                EmbeddingsFocus::RerankerProvider => {
                    if step_state.provider_selection == EmbeddingProviderSelection::Remote {
                        EmbeddingsFocus::EmbeddingFields
                    } else {
                        EmbeddingsFocus::EmbeddingProvider
                    }
                }
                EmbeddingsFocus::RerankerFields => EmbeddingsFocus::RerankerProvider,
                EmbeddingsFocus::EmbeddingProvider => EmbeddingsFocus::EmbeddingProvider,
            };
            step_state.focused_field = 0;
        }
        KeyCode::Enter | KeyCode::Tab | KeyCode::Down => {
            match step_state.focus {
                EmbeddingsFocus::EmbeddingProvider => {
                    if step_state.provider_selection == EmbeddingProviderSelection::Remote {
                        step_state.focus = EmbeddingsFocus::EmbeddingFields;
                        step_state.focused_field = 0;
                    } else {
                        step_state.focus = EmbeddingsFocus::RerankerProvider;
                    }
                }
                EmbeddingsFocus::EmbeddingFields => {
                    if step_state.focused_field + 1 < step_state.embed_field_count() {
                        step_state.focused_field += 1;
                    } else {
                        step_state.focus = EmbeddingsFocus::RerankerProvider;
                        step_state.focused_field = 0;
                    }
                }
                EmbeddingsFocus::RerankerProvider => {
                    if step_state.reranker_selection == RerankerProviderSelection::Remote {
                        step_state.focus = EmbeddingsFocus::RerankerFields;
                        step_state.focused_field = 0;
                    } else if key.code == KeyCode::Enter {
                        apply_to_state(state, step_state);
                        return StepResult::Next;
                    }
                }
                EmbeddingsFocus::RerankerFields => {
                    if step_state.focused_field + 1 < step_state.reranker_field_count() {
                        step_state.focused_field += 1;
                    } else if key.code == KeyCode::Enter {
                        apply_to_state(state, step_state);
                        return StepResult::Next;
                    }
                }
            }
        }
        KeyCode::Up | KeyCode::BackTab => {
            match step_state.focus {
                EmbeddingsFocus::EmbeddingFields if step_state.focused_field > 0 => {
                    step_state.focused_field -= 1;
                }
                EmbeddingsFocus::RerankerFields if step_state.focused_field > 0 => {
                    step_state.focused_field -= 1;
                }
                _ => {}
            }
        }
        KeyCode::Left => {
            match step_state.focus {
                EmbeddingsFocus::EmbeddingProvider => {
                    step_state.provider_selection = EmbeddingProviderSelection::LocalOnnx;
                }
                EmbeddingsFocus::RerankerProvider => {
                    step_state.reranker_selection = match step_state.reranker_selection {
                        RerankerProviderSelection::Remote => RerankerProviderSelection::LocalOnnx,
                        RerankerProviderSelection::Disabled => RerankerProviderSelection::Remote,
                        RerankerProviderSelection::LocalOnnx => RerankerProviderSelection::LocalOnnx,
                    };
                }
                EmbeddingsFocus::EmbeddingFields => {
                    focused_field_mut(step_state).move_left();
                }
                EmbeddingsFocus::RerankerFields => {
                    focused_reranker_field_mut(step_state).move_left();
                }
            }
        }
        KeyCode::Right => {
            match step_state.focus {
                EmbeddingsFocus::EmbeddingProvider => {
                    step_state.provider_selection = EmbeddingProviderSelection::Remote;
                }
                EmbeddingsFocus::RerankerProvider => {
                    step_state.reranker_selection = match step_state.reranker_selection {
                        RerankerProviderSelection::LocalOnnx => RerankerProviderSelection::Remote,
                        RerankerProviderSelection::Remote => RerankerProviderSelection::Disabled,
                        RerankerProviderSelection::Disabled => RerankerProviderSelection::Disabled,
                    };
                }
                EmbeddingsFocus::EmbeddingFields => {
                    focused_field_mut(step_state).move_right();
                }
                EmbeddingsFocus::RerankerFields => {
                    focused_reranker_field_mut(step_state).move_right();
                }
            }
        }
        KeyCode::Backspace => {
            match step_state.focus {
                EmbeddingsFocus::EmbeddingFields => {
                    focused_field_mut(step_state).delete_char_before();
                }
                EmbeddingsFocus::RerankerFields => {
                    focused_reranker_field_mut(step_state).delete_char_before();
                }
                _ => {}
            }
        }
        KeyCode::Char(c) => {
            match step_state.focus {
                EmbeddingsFocus::EmbeddingFields => {
                    focused_field_mut(step_state).insert_char(c);
                }
                EmbeddingsFocus::RerankerFields => {
                    focused_reranker_field_mut(step_state).insert_char(c);
                }
                _ => {}
            }
        }
        _ => {}
    }

    StepResult::Continue
}

/// Return a mutable reference to the currently focused embedding remote field.
fn focused_field_mut(step_state: &mut EmbeddingsStepState) -> &mut InputField {
    match step_state.focused_field {
        0 => &mut step_state.embed_url,
        1 => &mut step_state.embed_api_key,
        _ => &mut step_state.embed_model,
    }
}

/// Return a mutable reference to the currently focused reranker remote field.
fn focused_reranker_field_mut(step_state: &mut EmbeddingsStepState) -> &mut InputField {
    match step_state.focused_field {
        0 => &mut step_state.reranker_url,
        1 => &mut step_state.reranker_api_key,
        _ => &mut step_state.reranker_model,
    }
}

/// Apply the embeddings step state back into `WizardState`.
fn apply_to_state(state: &mut WizardState, step_state: &EmbeddingsStepState) {
    state.embedding_config = Some(match step_state.provider_selection {
        EmbeddingProviderSelection::LocalOnnx => EmbeddingConfig::LocalOnnx {
            model: "BAAI/bge-m3".to_string(),
            dimension: 1024,
            auto_download: true,
        },
        EmbeddingProviderSelection::Remote => EmbeddingConfig::Remote {
            url: step_state.embed_url.effective_value().to_string(),
            api_key: step_state.embed_api_key.effective_value().to_string(),
            model: Some(step_state.embed_model.effective_value().to_string()),
            dimension: 1024,
        },
    });

    state.reranker_config = Some(match step_state.reranker_selection {
        RerankerProviderSelection::LocalOnnx => RerankerConfig::LocalOnnx,
        RerankerProviderSelection::Remote => RerankerConfig::Remote {
            endpoint: step_state.reranker_url.effective_value().to_string(),
            api_key: step_state.reranker_api_key.effective_value().to_string(),
            model: step_state.reranker_model.effective_value().to_string(),
        },
        RerankerProviderSelection::Disabled => RerankerConfig::Disabled,
    });
}
