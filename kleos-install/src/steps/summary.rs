//! Summary and installation step for the Kleos installer wizard.
//!
//! Shows a read-only summary of all wizard choices, a file manifest of what
//! will be created, and an Install button. After confirmation, runs the
//! installation engine and reports progress inline. Post-install shows the
//! generated API key, server URL, and next-step instructions.

use crossterm::event::{KeyCode, KeyEvent};
use kleos_install_core::plan::{InstallProgress, InstallResult};
use kleos_install_core::InstallError;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::tui::{COLOR_ACTIVE, COLOR_COMPLETE, COLOR_DIM, COLOR_ERROR, COLOR_WARN};
use crate::types::StepResult;
use crate::wizard::WizardState;

/// Local UI state for the summary and installation step.
pub struct SummaryStepState {
    /// `true` once the user has pressed Enter to confirm installation.
    #[allow(dead_code)]
    pub confirmed: bool,
    /// `true` while the installation is running.
    pub installing: bool,
    /// Lines of progress output accumulated during installation.
    pub progress_lines: Vec<String>,
    /// Completed install result, set after a successful run.
    pub install_result: Option<InstallResult>,
    /// Installation error message, set on failure.
    pub install_error: Option<String>,
    /// Scroll offset for the summary / progress pane.
    pub scroll: u16,
}

/// State transitions for the summary/install step.
impl SummaryStepState {
    /// Create the default summary step state.
    pub fn new() -> Self {
        SummaryStepState {
            confirmed: false,
            installing: false,
            progress_lines: Vec::new(),
            install_result: None,
            install_error: None,
            scroll: 0,
        }
    }
}

/// Draw the summary step into `area`.
///
/// Before confirmation: shows the plan summary.
/// During install: shows progress lines.
/// After install: shows the result with API key and next steps.
pub fn draw_summary_step(
    f: &mut Frame,
    area: Rect,
    state: &WizardState,
    step_state: &SummaryStepState,
) {
    if step_state.install_result.is_some() {
        draw_post_install(f, area, step_state);
    } else if step_state.installing {
        draw_progress(f, area, step_state);
    } else {
        draw_plan_summary(f, area, state, step_state);
    }
}

/// Render the pre-install plan summary.
fn draw_plan_summary(
    f: &mut Frame,
    area: Rect,
    state: &WizardState,
    step_state: &SummaryStepState,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Min(10),   // summary text
            Constraint::Length(3), // install button
        ])
        .split(area);

    let mut lines: Vec<Line> = Vec::new();

    // Upgrade notice: shown persistently at the top of the summary so the
    // user knows secrets are being preserved and the existing config will be
    // backed up rather than silently overwritten.
    if state.is_upgrade {
        if let Some(existing) = &state.existing_install {
            lines.push(Line::from(vec![Span::styled(
                "Upgrade detected:",
                Style::default().fg(COLOR_WARN).add_modifier(Modifier::BOLD),
            )]));
            lines.push(Line::from(vec![
                Span::raw("  Existing install detected at "),
                Span::styled(
                    existing.install_dir.to_string_lossy().to_string(),
                    Style::default().fg(Color::White),
                ),
            ]));
            lines.push(Line::from(Span::styled(
                "  Secrets will be preserved; existing config will be backed up before overwrite.",
                Style::default().fg(COLOR_DIM),
            )));
            lines.push(Line::from(""));
        }
    }

    // Failure banner: rendered persistently until the user retries (Enter,
    // which clears it before starting a new attempt) or navigates back
    // (Esc/Backspace, which also clears it) -- see `handle_summary_input`. A
    // failed install must never look identical to the plain pre-install
    // summary, which previously let the process report "cancelled" and exit
    // 0 for what was actually a failure.
    if let Some(err) = &step_state.install_error {
        lines.push(Line::from(vec![Span::styled(
            " INSTALLATION FAILED ",
            Style::default()
                .fg(Color::White)
                .bg(COLOR_ERROR)
                .add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(vec![Span::styled(
            err.as_str(),
            Style::default().fg(COLOR_ERROR),
        )]));
        lines.push(Line::from(vec![Span::styled(
            "Press Enter to retry, or Esc to go back and adjust settings.",
            Style::default().fg(COLOR_DIM),
        )]));
        lines.push(Line::from(""));
    }

    // Components section.
    lines.push(Line::from(vec![Span::styled(
        "Components to install:",
        Style::default()
            .fg(COLOR_ACTIVE)
            .add_modifier(Modifier::BOLD),
    )]));
    for comp in &state.selected_components {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("- {comp}"), Style::default().fg(Color::White)),
        ]));
    }
    lines.push(Line::from(""));

    // Install paths section.
    lines.push(Line::from(vec![Span::styled(
        "Installation paths:",
        Style::default()
            .fg(COLOR_ACTIVE)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(vec![
        Span::raw("  Binaries:  "),
        Span::styled(
            state.install_dir.to_string_lossy().to_string(),
            Style::default().fg(Color::White),
        ),
    ]));
    lines.push(Line::from(vec![
        Span::raw("  Config:    "),
        Span::styled(
            state.config_dir.to_string_lossy().to_string(),
            Style::default().fg(Color::White),
        ),
    ]));
    lines.push(Line::from(""));

    // Server config section (if server selected).
    if state
        .selected_components
        .iter()
        .any(|c| c == "kleos-server")
    {
        let sc = &state.server_config;
        lines.push(Line::from(vec![Span::styled(
            "Server configuration:",
            Style::default()
                .fg(COLOR_ACTIVE)
                .add_modifier(Modifier::BOLD),
        )]));
        lines.push(Line::from(vec![
            Span::raw("  Listen: "),
            Span::styled(
                format!("{}:{}", sc.host, sc.port),
                Style::default().fg(Color::White),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::raw("  Data:   "),
            Span::styled(
                sc.data_dir.to_string_lossy().to_string(),
                Style::default().fg(Color::White),
            ),
        ]));
        lines.push(Line::from(""));
    }

    // Security section.
    lines.push(Line::from(vec![Span::styled(
        "Security:",
        Style::default()
            .fg(COLOR_ACTIVE)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(vec![
        Span::raw("  Open access: "),
        Span::styled(
            if state.security_config.open_access {
                "YES (insecure)"
            } else {
                "No (recommended)"
            },
            if state.security_config.open_access {
                Style::default().fg(COLOR_ERROR)
            } else {
                Style::default().fg(COLOR_COMPLETE)
            },
        ),
    ]));
    lines.push(Line::from(""));

    // System integration.
    let integration_str = match &state.system_integration {
        kleos_install_core::system::SystemIntegration::Systemd { auto_start } => {
            format!("systemd (auto-start: {auto_start})")
        }
        kleos_install_core::system::SystemIntegration::Launchd { auto_start } => {
            format!("launchd (auto-start: {auto_start})")
        }
        kleos_install_core::system::SystemIntegration::WindowsService => {
            "Windows Service".to_string()
        }
        kleos_install_core::system::SystemIntegration::None => "none".to_string(),
    };
    lines.push(Line::from(vec![Span::styled(
        "System integration:",
        Style::default()
            .fg(COLOR_ACTIVE)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(vec![
        Span::raw("  "),
        Span::styled(integration_str, Style::default().fg(Color::White)),
    ]));

    let summary = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Installation Summary ")
                .border_style(Style::default().fg(COLOR_DIM)),
        )
        .scroll((step_state.scroll, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(summary, chunks[0]);

    // Install button.
    let btn_style = Style::default()
        .fg(Color::Black)
        .bg(COLOR_ACTIVE)
        .add_modifier(Modifier::BOLD);
    let btn = Paragraph::new(Line::from(vec![Span::styled(
        "  [ Press Enter to Install ]  ",
        btn_style,
    )]))
    .block(Block::default().borders(Borders::NONE));
    f.render_widget(btn, chunks[1]);
}

/// Render the installation progress view.
fn draw_progress(f: &mut Frame, area: Rect, step_state: &SummaryStepState) {
    let lines: Vec<Line> = step_state
        .progress_lines
        .iter()
        .map(|s| Line::from(Span::styled(s.as_str(), Style::default().fg(Color::White))))
        .collect();

    let p = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Installing... ")
                .border_style(Style::default().fg(COLOR_WARN)),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

/// Render the post-installation result screen.
fn draw_post_install(f: &mut Frame, area: Rect, step_state: &SummaryStepState) {
    let result = step_state.install_result.as_ref().unwrap();

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![Span::styled(
        "Installation complete!",
        Style::default()
            .fg(COLOR_COMPLETE)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(""));

    if let Some(url) = &result.server_url {
        lines.push(Line::from(vec![
            Span::raw("  Server URL:  "),
            Span::styled(url.as_str(), Style::default().fg(COLOR_ACTIVE)),
        ]));
    }

    lines.push(Line::from(vec![
        Span::raw("  API key:     "),
        Span::styled(result.api_key.as_str(), Style::default().fg(COLOR_WARN)),
    ]));
    lines.push(Line::from(vec![
        Span::raw("  Config dir:  "),
        Span::styled(
            result.config_path.to_string_lossy().to_string(),
            Style::default().fg(Color::White),
        ),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "Next steps:",
        Style::default()
            .fg(COLOR_ACTIVE)
            .add_modifier(Modifier::BOLD),
    )]));
    lines.push(Line::from(
        "  1. Save the API key shown above in a secure location.",
    ));
    lines.push(Line::from(
        "  2. Run `kleos-cli status` to verify the server is running.",
    ));
    lines.push(Line::from(
        "  3. See the docs for MCP and agent configuration.",
    ));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "  Press q or Esc to exit the installer.",
        Style::default().fg(COLOR_DIM),
    )]));

    let p = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Done ")
                .border_style(Style::default().fg(COLOR_COMPLETE)),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

/// A simple `InstallProgress` implementation that accumulates progress strings.
///
/// Used by the wizard to receive progress callbacks from `InstallPlan::execute`.
/// The collected lines are stored in a `Arc<Mutex<Vec<String>>>` so they can be
/// read back by the draw function. Because `execute` is synchronous (blocking),
/// this is only used in the non-interactive runner. The TUI runner runs execute
/// in a blocking thread.
struct CollectingProgress {
    /// Accumulated progress lines, shared with the draw function.
    lines: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

// Buffers progress callbacks so the TUI thread can drain them per frame.
impl InstallProgress for CollectingProgress {
    /// Append a "phase: detail" line.
    fn on_phase(&self, phase: &str, detail: &str) {
        if let Ok(mut v) = self.lines.lock() {
            v.push(format!("[{phase}] {detail}"));
        }
    }

    /// Append a download progress line.
    fn on_download_progress(&self, component: &str, bytes: u64, total: u64) {
        if let Ok(mut v) = self.lines.lock() {
            let pct = (bytes * 100).checked_div(total).unwrap_or(0);
            // Update the last line rather than spamming.
            let line = format!("  downloading {component}: {pct}%");
            if let Some(last) = v.last_mut() {
                if last.starts_with("  downloading") {
                    *last = line;
                    return;
                }
            }
            v.push(line);
        }
    }

    /// Append a component installed confirmation.
    fn on_component_installed(&self, component: &str) {
        if let Ok(mut v) = self.lines.lock() {
            v.push(format!("  installed: {component}"));
        }
    }

    /// Append a completion line.
    fn on_complete(&self) {
        if let Ok(mut v) = self.lines.lock() {
            v.push("Installation complete.".to_string());
        }
    }

    /// Append an error line.
    fn on_error(&self, error: &InstallError) {
        if let Ok(mut v) = self.lines.lock() {
            v.push(format!("ERROR: {error}"));
        }
    }
}

/// Handle a key event for the summary step.
///
/// Pressing Enter on the pre-install screen triggers the install. After
/// installation the user can only quit.
pub async fn handle_summary_input(
    key: KeyEvent,
    state: &mut WizardState,
    step_state: &mut SummaryStepState,
) -> StepResult {
    // Post-install: only allow quitting.
    if step_state.install_result.is_some() {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return StepResult::Quit,
            _ => {}
        }
        return StepResult::Continue;
    }

    // During install: ignore all input.
    if step_state.installing {
        return StepResult::Continue;
    }

    match key.code {
        KeyCode::Enter => {
            // Run installation. Clear any error from a previous failed
            // attempt -- this is the "retry" acknowledgement of the failure
            // banner above.
            step_state.installing = true;
            step_state.install_error = None;
            step_state
                .progress_lines
                .push("Starting installation...".to_string());

            let plan = state.build_plan();
            let lines_arc = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
            let progress = CollectingProgress {
                lines: lines_arc.clone(),
            };

            // Execute in a blocking thread to avoid freezing the async runtime.
            let result = tokio::task::spawn_blocking(move || plan.execute(&progress, "")).await;

            match result {
                Ok(Ok(install_result)) => {
                    if let Ok(lines) = lines_arc.lock() {
                        step_state.progress_lines.extend(lines.clone());
                    }
                    step_state.install_result = Some(install_result);
                    step_state.installing = false;
                }
                Ok(Err(e)) => {
                    step_state.install_error = Some(e.to_string());
                    step_state.progress_lines.push(format!("ERROR: {e}"));
                    step_state.installing = false;
                }
                Err(e) => {
                    step_state.install_error = Some(e.to_string());
                    step_state.progress_lines.push(format!("PANIC: {e}"));
                    step_state.installing = false;
                }
            }

            // If we have a result, signal Next so the wizard can return it.
            if step_state.install_result.is_some() {
                return StepResult::Next;
            }
        }
        KeyCode::Esc | KeyCode::Backspace => {
            // Navigating away also acknowledges a failed attempt's error --
            // it should not reappear stale once the user returns having
            // changed nothing, only to look identical to a fresh retry.
            step_state.install_error = None;
            return StepResult::Back;
        }
        KeyCode::Char('q') => {
            return StepResult::Quit;
        }
        KeyCode::PageDown | KeyCode::Down => {
            step_state.scroll = step_state.scroll.saturating_add(3);
        }
        KeyCode::PageUp | KeyCode::Up => {
            step_state.scroll = step_state.scroll.saturating_sub(3);
        }
        _ => {}
    }

    StepResult::Continue
}
