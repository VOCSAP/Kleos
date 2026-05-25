//! Server configuration step for the Kleos installer wizard.
//!
//! Collects host, port, data directory, database filename, and CORS origins
//! from the user using text input fields with inline validation.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

use crate::tui::draw_input_field;
use crate::types::{InputField, StepResult};
use crate::wizard::WizardState;

/// Index constants for each field in the fields vector.
const FIELD_HOST: usize = 0;
const FIELD_PORT: usize = 1;
const FIELD_DATA_DIR: usize = 2;
const FIELD_DB_PATH: usize = 3;
const FIELD_CORS: usize = 4;

/// Local UI state for the server configuration step.
pub struct ServerStepState {
    /// Index of the currently focused input field.
    pub focused_field: usize,
    /// All editable input fields for this step.
    pub fields: Vec<InputField>,
}

impl ServerStepState {
    /// Build the initial step state from the current wizard state.
    ///
    /// Pre-populates all fields from the existing `server_config` defaults.
    pub fn new(state: &WizardState) -> Self {
        let sc = &state.server_config;

        let mut fields = vec![
            InputField::with_value("Host", sc.host.clone(), "127.0.0.1").with_validator(|v| {
                if v.is_empty() || v.contains(' ') {
                    Some("Host must not be empty or contain spaces".to_string())
                } else {
                    None
                }
            }),
            InputField::with_value("Port", sc.port.to_string(), "4200").with_validator(|v| {
                match v.parse::<u16>() {
                    Ok(p) if p > 0 => None,
                    _ => Some("Port must be a number between 1 and 65535".to_string()),
                }
            }),
            InputField::with_value(
                "Data directory",
                sc.data_dir.to_string_lossy().to_string(),
                "./data",
            ),
            InputField::with_value("Database filename", sc.db_path.clone(), "kleos.db"),
            InputField::with_value(
                "CORS origins (optional)",
                sc.cors_origins.clone().unwrap_or_default(),
                "https://example.com",
            ),
        ];

        // Run initial validation.
        for field in &mut fields {
            field.validate();
        }

        ServerStepState {
            focused_field: 0,
            fields,
        }
    }

    /// Return `true` if all required fields pass validation.
    pub fn is_valid(&self) -> bool {
        self.fields[..FIELD_CORS]
            .iter()
            .all(|f| f.error.is_none())
    }
}

/// Draw the server configuration step into `area`.
pub fn draw_server_step(
    f: &mut Frame,
    area: Rect,
    _state: &WizardState,
    step_state: &ServerStepState,
) {
    let field_count = step_state.fields.len();
    // Each field gets 3 rows (border top + content + border bottom = 3)
    // plus 1 error row beneath, plus 1 row gap. Approximate layout:
    let mut constraints: Vec<Constraint> = Vec::new();
    for _ in 0..field_count {
        constraints.push(Constraint::Length(3)); // field box
        constraints.push(Constraint::Length(1)); // error / spacer
    }
    constraints.push(Constraint::Min(0)); // fill remainder

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(constraints)
        .split(area);

    for (i, field) in step_state.fields.iter().enumerate() {
        let is_focused = step_state.focused_field == i;
        draw_input_field(
            f,
            chunks[i * 2],
            &field.label,
            &field.value,
            &field.placeholder,
            is_focused,
            field.error.as_deref(),
        );
    }
}

/// Handle a key event for the server configuration step.
///
/// Routes character input to the focused field and handles navigation between
/// fields and between steps.
pub fn handle_server_input(
    key: KeyEvent,
    state: &mut WizardState,
    step_state: &mut ServerStepState,
) -> StepResult {
    let field_count = step_state.fields.len();

    match key.code {
        KeyCode::Enter | KeyCode::Tab | KeyCode::Down => {
            // Tab/Down: advance to next field or step.
            if step_state.focused_field + 1 < field_count {
                step_state.focused_field += 1;
            } else if key.code == KeyCode::Enter
                && step_state.is_valid() {
                    apply_to_state(state, step_state);
                    return StepResult::Next;
                }
        }
        KeyCode::BackTab | KeyCode::Up
            if step_state.focused_field > 0 => {
                step_state.focused_field -= 1;
            }
        KeyCode::Esc | KeyCode::Backspace if step_state.focused_field == 0 => {
            return StepResult::Back;
        }
        KeyCode::Backspace => {
            step_state.fields[step_state.focused_field].delete_char_before();
        }
        KeyCode::Left => {
            step_state.fields[step_state.focused_field].move_left();
        }
        KeyCode::Right => {
            step_state.fields[step_state.focused_field].move_right();
        }
        KeyCode::Char('q') if step_state.focused_field == 0 => {
            return StepResult::Quit;
        }
        KeyCode::Char(c) => {
            step_state.fields[step_state.focused_field].insert_char(c);
        }
        _ => {}
    }

    StepResult::Continue
}

/// Apply the step state fields back into `WizardState.server_config`.
fn apply_to_state(state: &mut WizardState, step_state: &ServerStepState) {
    let sc = &mut state.server_config;

    sc.host = step_state.fields[FIELD_HOST].effective_value().to_string();
    sc.port = step_state.fields[FIELD_PORT]
        .effective_value()
        .parse()
        .unwrap_or(4200);
    sc.data_dir = PathBuf::from(step_state.fields[FIELD_DATA_DIR].effective_value());
    sc.db_path = step_state.fields[FIELD_DB_PATH].effective_value().to_string();

    let cors = step_state.fields[FIELD_CORS].effective_value();
    sc.cors_origins = if cors.is_empty() {
        None
    } else {
        Some(cors.to_string())
    };
}
