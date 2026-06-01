//! Shared rendering utilities for the Kleos installer TUI.
//!
//! All functions in this module write directly into a ratatui `Frame`. They
//! are stateless -- all data they need is passed as arguments so they can be
//! called from any step renderer without holding references to wizard state.

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::wizard::WizardStep;

/// Colour used for active/focused elements throughout the TUI.
pub const COLOR_ACTIVE: Color = Color::Cyan;
/// Colour used to indicate a completed wizard step.
pub const COLOR_COMPLETE: Color = Color::Green;
/// Colour used for warning messages and advisory text.
pub const COLOR_WARN: Color = Color::Yellow;
/// Colour used for error messages and invalid input feedback.
pub const COLOR_ERROR: Color = Color::Red;
/// Colour used for secondary / dimmed text.
pub const COLOR_DIM: Color = Color::DarkGray;

/// Draw the "Kleos Installer" title header into `area`.
///
/// Renders a full-width bold cyan header bar with the application name and
/// version. Intended to occupy a fixed-height `Constraint::Length(3)` row at
/// the very top of the layout.
pub fn draw_title(f: &mut Frame, area: Rect) {
    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            "  Kleos Installer  ",
            Style::default()
                .fg(Color::Black)
                .bg(COLOR_ACTIVE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "  v1.2.1",
            Style::default().fg(Color::White).bg(Color::DarkGray),
        ),
    ]))
    .block(Block::default().borders(Borders::NONE))
    .alignment(Alignment::Left);
    f.render_widget(title, area);
}

/// Draw the horizontal step indicator bar into `area`.
///
/// Each step is shown as a labelled box. The current step is highlighted in
/// cyan. Completed steps (those before the current step in the ordered list)
/// are shown in green with a checkmark prefix. Future steps are shown dimmed.
pub fn draw_step_indicator(f: &mut Frame, area: Rect, steps: &[WizardStep], current: WizardStep) {
    let current_idx = steps.iter().position(|s| *s == current).unwrap_or(0);

    let mut spans: Vec<Span> = Vec::new();
    for (i, step) in steps.iter().enumerate() {
        let label = step.label();
        if i == current_idx {
            spans.push(Span::styled(
                format!(" [{label}] "),
                Style::default()
                    .fg(Color::Black)
                    .bg(COLOR_ACTIVE)
                    .add_modifier(Modifier::BOLD),
            ));
        } else if i < current_idx {
            spans.push(Span::styled(
                format!(" \u{2714}{label} "),
                Style::default().fg(COLOR_COMPLETE),
            ));
        } else {
            spans.push(Span::styled(
                format!(" {label} "),
                Style::default().fg(COLOR_DIM),
            ));
        }
        if i + 1 < steps.len() {
            spans.push(Span::styled(" > ", Style::default().fg(COLOR_DIM)));
        }
    }

    let bar = Paragraph::new(Line::from(spans))
        .block(Block::default().borders(Borders::BOTTOM))
        .alignment(Alignment::Left);
    f.render_widget(bar, area);
}

/// Draw the navigation hint bar at the bottom of the screen.
///
/// Shows standard keybindings. When `can_go_back` is false the Back hint is
/// omitted. This should occupy a fixed-height `Constraint::Length(3)` row at
/// the very bottom.
pub fn draw_navigation_bar(f: &mut Frame, area: Rect, can_go_back: bool) {
    let mut hints: Vec<Span> = Vec::new();

    hints.push(Span::styled(" [Enter] ", Style::default().fg(COLOR_ACTIVE)));
    hints.push(Span::raw("Next  "));

    if can_go_back {
        hints.push(Span::styled(
            " [Esc/Backspace] ",
            Style::default().fg(COLOR_ACTIVE),
        ));
        hints.push(Span::raw("Back  "));
    }

    hints.push(Span::styled(
        " [Tab/Arrow] ",
        Style::default().fg(COLOR_ACTIVE),
    ));
    hints.push(Span::raw("Navigate  "));

    hints.push(Span::styled(" [Space] ", Style::default().fg(COLOR_ACTIVE)));
    hints.push(Span::raw("Toggle  "));

    hints.push(Span::styled(" [q] ", Style::default().fg(COLOR_WARN)));
    hints.push(Span::raw("Quit"));

    let bar = Paragraph::new(Line::from(hints))
        .block(Block::default().borders(Borders::TOP))
        .alignment(Alignment::Left);
    f.render_widget(bar, area);
}

/// Draw a "Really quit? [y/n]" confirmation popup centred on `area`.
///
/// The popup is rendered over a `Clear` widget so the content beneath is
/// erased. It should be drawn last so it sits above all other widgets.
pub fn draw_quit_confirm(f: &mut Frame, area: Rect) {
    // Centre a small popup in the provided area.
    let popup_width = 36u16;
    let popup_height = 5u16;
    let x = area.x + area.width.saturating_sub(popup_width) / 2;
    let y = area.y + area.height.saturating_sub(popup_height) / 2;
    let popup_rect = Rect::new(
        x,
        y,
        popup_width.min(area.width),
        popup_height.min(area.height),
    );

    f.render_widget(Clear, popup_rect);

    let popup = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("  Really quit? "),
            Span::styled(
                "[y]",
                Style::default()
                    .fg(COLOR_ERROR)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Yes  "),
            Span::styled(
                "[n]",
                Style::default()
                    .fg(COLOR_COMPLETE)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" No"),
        ]),
        Line::from(""),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(COLOR_WARN))
            .title(" Quit Installer "),
    )
    .alignment(Alignment::Left);

    f.render_widget(popup, popup_rect);
}

/// Render a single labelled text input field.
///
/// Draws the field label, the current value (or placeholder in dim style if
/// empty), and an error message beneath if `error` is `Some`. The active field
/// is highlighted with a cyan border; inactive fields use the default border.
pub fn draw_input_field(
    f: &mut Frame,
    area: Rect,
    label: &str,
    value: &str,
    placeholder: &str,
    is_focused: bool,
    error: Option<&str>,
) {
    let border_style = if is_focused {
        Style::default().fg(COLOR_ACTIVE)
    } else {
        Style::default().fg(COLOR_DIM)
    };

    let display_value = if value.is_empty() {
        Span::styled(placeholder, Style::default().fg(COLOR_DIM))
    } else {
        Span::styled(value, Style::default().fg(Color::White))
    };

    let content_line = Line::from(vec![display_value]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(format!(" {label} "));

    let paragraph = Paragraph::new(vec![content_line]).block(block);
    f.render_widget(paragraph, area);

    // Draw error message below the field if present.
    if let Some(err) = error {
        if area.y + area.height + 1 < f.area().height {
            let err_area = Rect::new(
                area.x + 2,
                area.y + area.height,
                area.width.saturating_sub(2),
                1,
            );
            let err_widget = Paragraph::new(err).style(Style::default().fg(COLOR_ERROR));
            f.render_widget(err_widget, err_area);
        }
    }
}
