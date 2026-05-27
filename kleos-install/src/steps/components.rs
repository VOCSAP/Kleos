//! Component selection step for the Kleos installer wizard.
//!
//! This step lets the user choose an installation profile (Server, Agent Host,
//! Full, Custom) and then fine-tune the component list with individual
//! checkboxes. Required components cannot be deselected.

use crossterm::event::{KeyCode, KeyEvent};
use kleos_install_core::{all_components, profile_components, resolve_dependencies, Profile};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::tui::{COLOR_ACTIVE, COLOR_COMPLETE, COLOR_DIM};
use crate::types::StepResult;
use crate::wizard::WizardState;

/// Number of profile preset buttons at the top of the step.
const PROFILE_COUNT: usize = 4;

/// Local UI state for the component selection step.
pub struct ComponentsStepState {
    /// Index of the focused profile preset button (0-3), or `None` if
    /// focus is on the component list.
    pub profile_focus: Option<usize>,
    /// Index of the focused item in the component checkbox list.
    pub focused_index: usize,
}

impl ComponentsStepState {
    /// Build the initial step state from the current wizard state.
    ///
    /// Focus starts on the Server profile button.
    pub fn new(_state: &WizardState) -> Self {
        ComponentsStepState {
            profile_focus: Some(0),
            focused_index: 0,
        }
    }

    /// Return `true` if focus is currently on the profile buttons row.
    fn on_profiles(&self) -> bool {
        self.profile_focus.is_some()
    }
}

/// All available profiles in display order.
static PROFILES: &[(Profile, &str)] = &[
    (Profile::Server, "Server"),
    (Profile::AgentHost, "Agent Host"),
    (Profile::Full, "Full"),
    (Profile::Custom, "Custom"),
];

/// Draw the component selection step into `area`.
///
/// Renders four profile preset buttons at the top and a scrollable checkbox
/// list of all components below. Required components show a locked indicator.
pub fn draw_components_step(
    f: &mut Frame,
    area: Rect,
    state: &WizardState,
    step_state: &ComponentsStepState,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // profile buttons
            Constraint::Min(5),    // component list
            Constraint::Length(2), // status bar
        ])
        .split(area);

    draw_profile_buttons(f, chunks[0], state, step_state);
    draw_component_list(f, chunks[1], state, step_state);
    draw_selection_status(f, chunks[2], state);
}

/// Render the profile preset button row.
fn draw_profile_buttons(
    f: &mut Frame,
    area: Rect,
    state: &WizardState,
    step_state: &ComponentsStepState,
) {
    let active_profile = state.selected_profile;
    let button_width = area.width / PROFILE_COUNT as u16;

    let constraints: Vec<Constraint> = (0..PROFILE_COUNT)
        .map(|_| Constraint::Ratio(1, PROFILE_COUNT as u32))
        .collect();

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    for (i, (profile, label)) in PROFILES.iter().enumerate() {
        let is_active = active_profile == Some(*profile);
        let is_focused = step_state.profile_focus == Some(i);

        let style = if is_active {
            Style::default()
                .fg(Color::Black)
                .bg(COLOR_ACTIVE)
                .add_modifier(Modifier::BOLD)
        } else if is_focused {
            Style::default()
                .fg(COLOR_ACTIVE)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(COLOR_DIM)
        };

        let border_style = if is_focused {
            Style::default().fg(COLOR_ACTIVE)
        } else {
            Style::default().fg(COLOR_DIM)
        };

        let btn = Paragraph::new(Line::from(vec![Span::styled(
            format!(" {} ", label),
            style,
        )]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style),
        );
        f.render_widget(btn, cols[i]);
        let _ = button_width; // suppress unused warning
    }
}

/// Render the scrollable component checkbox list.
fn draw_component_list(
    f: &mut Frame,
    area: Rect,
    state: &WizardState,
    step_state: &ComponentsStepState,
) {
    let platform = state.platform_info.platform;
    let components = all_components();

    let items: Vec<ListItem> = components
        .iter()
        .filter(|c| c.platforms.contains(&platform))
        .enumerate()
        .map(|(i, comp)| {
            let is_checked = state.selected_components.iter().any(|s| s == comp.id);
            let is_focused = !step_state.on_profiles() && step_state.focused_index == i;

            let checkbox = if comp.required {
                "[*]"
            } else if is_checked {
                "[x]"
            } else {
                "[ ]"
            };

            let checkbox_style = if comp.required {
                Style::default().fg(COLOR_COMPLETE)
            } else if is_checked {
                Style::default().fg(COLOR_ACTIVE)
            } else {
                Style::default().fg(COLOR_DIM)
            };

            let name_style = if is_focused {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let desc_style = Style::default().fg(COLOR_DIM);

            ListItem::new(vec![
                Line::from(vec![
                    Span::raw(if is_focused { "> " } else { "  " }),
                    Span::styled(checkbox, checkbox_style),
                    Span::raw(" "),
                    Span::styled(comp.display_name, name_style),
                ]),
                Line::from(vec![
                    Span::raw("        "),
                    Span::styled(comp.description, desc_style),
                ]),
            ])
        })
        .collect();

    let title = if step_state.on_profiles() {
        " Components (Tab to enter list) "
    } else {
        " Components (Tab to return to profiles) "
    };

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if !step_state.on_profiles() {
                COLOR_ACTIVE
            } else {
                COLOR_DIM
            }))
            .title(title),
    );
    f.render_widget(list, area);
}

/// Render the selection status bar beneath the component list.
fn draw_selection_status(f: &mut Frame, area: Rect, state: &WizardState) {
    let count = state.selected_components.len();
    let text = format!(
        "  {} component(s) selected. Press Enter to continue.",
        count
    );
    let status = Paragraph::new(text).style(Style::default().fg(COLOR_DIM));
    f.render_widget(status, area);
}

/// Handle a key event for the component selection step.
///
/// Returns a `StepResult` indicating what the wizard should do next.
pub fn handle_components_input(
    key: KeyEvent,
    state: &mut WizardState,
    step_state: &mut ComponentsStepState,
) -> StepResult {
    let platform = state.platform_info.platform;
    let component_count = all_components()
        .iter()
        .filter(|c| c.platforms.contains(&platform))
        .count();

    match key.code {
        KeyCode::Enter => {
            if step_state.on_profiles() {
                // Move focus to component list.
                step_state.profile_focus = None;
                step_state.focused_index = 0;
            } else {
                // Advance wizard.
                return StepResult::Next;
            }
        }
        KeyCode::Esc | KeyCode::Backspace => {
            return StepResult::Back;
        }
        KeyCode::Char('q') => {
            return StepResult::Quit;
        }
        KeyCode::Tab => {
            // Toggle between profile row and component list.
            if step_state.on_profiles() {
                step_state.profile_focus = None;
                step_state.focused_index = 0;
            } else {
                step_state.profile_focus = Some(0);
            }
        }
        KeyCode::Left | KeyCode::Right if step_state.on_profiles() => {
            let current = step_state.profile_focus.unwrap_or(0);
            let next = if key.code == KeyCode::Right {
                (current + 1) % PROFILE_COUNT
            } else {
                current.wrapping_sub(1).min(PROFILE_COUNT - 1)
            };
            step_state.profile_focus = Some(next);
        }
        KeyCode::Up if !step_state.on_profiles() => {
            if step_state.focused_index == 0 {
                step_state.profile_focus = Some(0);
            } else {
                step_state.focused_index -= 1;
            }
        }
        KeyCode::Down => {
            if step_state.on_profiles() {
                step_state.profile_focus = None;
                step_state.focused_index = 0;
            } else if step_state.focused_index + 1 < component_count {
                step_state.focused_index += 1;
            }
        }
        KeyCode::Char(' ') => {
            if step_state.on_profiles() {
                // Apply the focused profile preset.
                let profile_idx = step_state.profile_focus.unwrap_or(0);
                apply_profile(state, PROFILES[profile_idx].0);
                step_state.profile_focus = Some(profile_idx);
            } else {
                // Toggle the focused component (if not required).
                toggle_component(state, step_state.focused_index);
            }
        }
        _ => {}
    }

    StepResult::Continue
}

/// Apply a profile preset to the component selection.
fn apply_profile(state: &mut WizardState, profile: Profile) {
    state.selected_profile = Some(profile);
    let ids = profile_components(profile);
    if profile == Profile::Custom {
        // Custom: don't change selection, just mark profile.
    } else {
        let resolved = resolve_dependencies(&ids);
        state.selected_components = resolved.into_iter().map(|s| s.to_string()).collect();
    }
}

/// Toggle the component at the given list index.
///
/// Required components cannot be deselected. Selecting a component also
/// resolves its dependencies. Deselecting removes only the component itself
/// (dependents are not automatically removed).
fn toggle_component(state: &mut WizardState, index: usize) {
    let platform = state.platform_info.platform;
    let components: Vec<_> = all_components()
        .iter()
        .filter(|c| c.platforms.contains(&platform))
        .collect();

    if index >= components.len() {
        return;
    }

    let comp = components[index];
    if comp.required {
        // Required components cannot be toggled.
        return;
    }

    if state.selected_components.iter().any(|s| s == comp.id) {
        // Deselect.
        state.selected_components.retain(|s| s != comp.id);
    } else {
        // Select and resolve dependencies.
        state.selected_components.push(comp.id.to_string());
        let ids: Vec<&str> = state
            .selected_components
            .iter()
            .map(|s| s.as_str())
            .collect();
        let resolved = resolve_dependencies(&ids);
        state.selected_components = resolved.into_iter().map(|s| s.to_string()).collect();
    }

    // Mark as Custom if the selection no longer matches any preset.
    state.selected_profile = Some(Profile::Custom);
}
