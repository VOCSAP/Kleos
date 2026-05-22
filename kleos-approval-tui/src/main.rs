use chrono::{DateTime, Utc};
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph},
};
use serde::{Deserialize, Serialize};
use std::io;
use std::time::{Duration, Instant};

/// Patch 20 (2026-05-22): default HTTP client timeout in seconds. Override
/// via `KLEOS_HTTP_LONGPOLL_TIMEOUT_SECS`. Pair this with the server-side
/// `KLEOS_SUPERVISOR_LONGPOLL_TIMEOUT_SECS` (default 30s) so that the
/// client laisse marge to the server to return its graceful empty-list
/// response before timing out on the network.
const DEFAULT_HTTP_LONGPOLL_TIMEOUT_SECS: u64 = 60;

fn http_longpoll_timeout() -> Duration {
    let secs = std::env::var("KLEOS_HTTP_LONGPOLL_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_HTTP_LONGPOLL_TIMEOUT_SECS);
    Duration::from_secs(secs)
}

/// Patch 20: parse the `Retry-After` header (seconds) from a 429 response.
/// Falls back to 60 seconds if the header is missing or unparseable so the
/// client never hot-loops on 429.
fn parse_retry_after(resp: &reqwest::Response) -> Duration {
    resp.headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(60))
}

#[derive(Parser)]
#[command(name = "kleos-approval-tui")]
#[command(about = "Terminal UI for human approval workflow")]
struct Args {
    /// Kleos server URL
    #[arg(
        short,
        long,
        env = "KLEOS_URL",
        default_value = "http://localhost:4200"
    )]
    url: String,

    /// API key for authentication. If not provided, resolved from credd daemon.
    #[arg(short = 'k', long)]
    api_key: Option<String>,

    /// Poll interval in milliseconds
    #[arg(short, long, default_value = "1000")]
    poll_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
struct Approval {
    id: String,
    action: String,
    context: Option<String>,
    requester: String,
    status: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    seconds_remaining: i64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct PendingResponse {
    approvals: Vec<Approval>,
    count: usize,
    expired_count: u64,
}

#[derive(Debug, Serialize)]
struct DecideRequest {
    decision: String,
    decided_by: Option<String>,
    reason: Option<String>,
}

struct App {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    approvals: Vec<Approval>,
    selected: usize,
    list_state: ListState,
    last_error: Option<String>,
    detail_mode: bool,
    /// Patch 20: when set, the server has issued a 429 and the client must
    /// not retry HTTP calls until this instant. Each tick the UI updates
    /// `last_error` with a countdown so the operator sees what is happening.
    last_429_until: Option<Instant>,
}

impl App {
    fn new(url: String, api_key: String) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        // Patch 20: configure the reqwest client with the long-poll timeout
        // so a single GET can sit on the wire while the server holds the
        // connection open in long-poll mode.
        let client = reqwest::Client::builder()
            .timeout(http_longpoll_timeout())
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            client,
            base_url: url,
            api_key,
            approvals: Vec::new(),
            selected: 0,
            list_state,
            last_error: None,
            detail_mode: false,
            last_429_until: None,
        }
    }

    /// Patch 20: if we're inside a Retry-After window, refuse to issue any
    /// HTTP call. Returns true if the call should be skipped. The UI
    /// status line gets a human-readable countdown.
    fn rate_limited_skip(&mut self) -> bool {
        if let Some(until) = self.last_429_until {
            let now = Instant::now();
            if now < until {
                let wait = (until - now).as_secs() + 1;
                self.last_error = Some(format!("Rate-limited by server, retry in {}s", wait));
                return true;
            }
            // Window expired -- clear and let the call go through.
            self.last_429_until = None;
        }
        false
    }

    async fn fetch_pending(&mut self) {
        if self.rate_limited_skip() {
            return;
        }
        let url = format!("{}/approvals/pending", self.base_url);
        match self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    match resp.json::<PendingResponse>().await {
                        Ok(data) => {
                            self.approvals = data.approvals;
                            self.last_error = None;
                            // Clamp selection to valid range
                            if !self.approvals.is_empty() && self.selected >= self.approvals.len() {
                                self.selected = self.approvals.len() - 1;
                            }
                            self.list_state.select(if self.approvals.is_empty() {
                                None
                            } else {
                                Some(self.selected)
                            });
                        }
                        Err(e) => {
                            self.last_error = Some(format!("Parse error: {}", e));
                        }
                    }
                } else if status.as_u16() == 429 {
                    // Patch 20: honour Retry-After. Refusing to retry until
                    // the window expires prevents the client from saturating
                    // the per-key rate-limit bucket and forcing the server
                    // to keep returning 429.
                    let wait = parse_retry_after(&resp);
                    self.last_429_until = Some(Instant::now() + wait);
                    self.last_error = Some(format!(
                        "Rate-limited by server. Backing off {}s.",
                        wait.as_secs()
                    ));
                } else {
                    self.last_error = Some(format!("HTTP {}", status));
                }
            }
            Err(e) => {
                self.last_error = Some(format!("Connection error: {}", e));
            }
        }
    }

    async fn decide(&mut self, approved: bool) {
        if self.approvals.is_empty() {
            return;
        }
        if self.rate_limited_skip() {
            return;
        }

        let approval = &self.approvals[self.selected];
        let url = format!("{}/approvals/{}/decide", self.base_url, approval.id);
        let decision = if approved { "approved" } else { "denied" };

        let req = DecideRequest {
            decision: decision.to_string(),
            decided_by: Some("tui-operator".to_string()),
            reason: None,
        };

        match self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&req)
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    self.last_error = None;
                    // Refresh list
                    self.fetch_pending().await;
                } else if status.as_u16() == 429 {
                    let wait = parse_retry_after(&resp);
                    self.last_429_until = Some(Instant::now() + wait);
                    self.last_error = Some(format!(
                        "Rate-limited by server. Backing off {}s.",
                        wait.as_secs()
                    ));
                } else {
                    let body = resp.text().await.unwrap_or_default();
                    self.last_error = Some(format!("HTTP {}: {}", status, body));
                }
            }
            Err(e) => {
                self.last_error = Some(format!("Request error: {}", e));
            }
        }
    }

    fn select_next(&mut self) {
        if self.approvals.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.approvals.len();
        self.list_state.select(Some(self.selected));
    }

    fn select_prev(&mut self) {
        if self.approvals.is_empty() {
            return;
        }
        self.selected = if self.selected == 0 {
            self.approvals.len() - 1
        } else {
            self.selected - 1
        };
        self.list_state.select(Some(self.selected));
    }
}

fn ui(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Min(10),   // List
            Constraint::Length(3), // Help
        ])
        .split(frame.area());

    // Title bar
    let title = Paragraph::new(format!(
        " ENGRAM APPROVAL CONSOLE                                    Pending: {}",
        app.approvals.len()
    ))
    .style(Style::default().fg(Color::White).bg(Color::Blue).bold())
    .block(Block::default());
    frame.render_widget(title, chunks[0]);

    // Main content area
    if app.detail_mode && !app.approvals.is_empty() {
        // Detail view
        render_detail(frame, app, chunks[1]);
    } else {
        // List view
        render_list(frame, app, chunks[1]);
    }

    // Help bar
    let help_text = if app.detail_mode {
        " [a] Approve  [d] Deny  [Esc] Back  [q] Quit"
    } else {
        " [↑/↓] Select  [a] Approve  [d] Deny  [Enter] Details  [q] Quit"
    };
    let error_text = app
        .last_error
        .as_ref()
        .map(|e| format!("  ERROR: {}", e))
        .unwrap_or_default();

    let help = Paragraph::new(format!("{}{}", help_text, error_text))
        .style(if app.last_error.is_some() {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::Gray)
        })
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(help, chunks[2]);
}

fn render_list(frame: &mut Frame, app: &App, area: Rect) {
    if app.approvals.is_empty() {
        let empty =
            Paragraph::new("\n\n  No pending approvals.\n\n  Waiting for approval requests...")
                .style(Style::default().fg(Color::Gray))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Pending Approvals "),
                );
        frame.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = app
        .approvals
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let marker = if i == app.selected { ">" } else { " " };
            let time_bar = make_time_bar(a.seconds_remaining);
            ListItem::new(vec![
                Line::from(vec![
                    Span::raw(format!("{} [{}] ", marker, i + 1)),
                    Span::styled(&a.action, Style::default().fg(Color::Cyan)),
                    Span::raw(format!(" {}", time_bar)),
                    Span::styled(
                        format!(" {:>3}s", a.seconds_remaining),
                        if a.seconds_remaining < 30 {
                            Style::default().fg(Color::Red).bold()
                        } else if a.seconds_remaining < 60 {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default().fg(Color::Green)
                        },
                    ),
                ]),
                Line::from(vec![
                    Span::raw("     Requester: "),
                    Span::styled(&a.requester, Style::default().fg(Color::Gray)),
                ]),
            ])
            .style(if i == app.selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            })
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Pending Approvals "),
        )
        .highlight_style(Style::default());

    frame.render_stateful_widget(list, area, &mut app.list_state.clone());
}

fn render_detail(frame: &mut Frame, app: &App, area: Rect) {
    let approval = &app.approvals[app.selected];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Timer
            Constraint::Min(5),    // Details
        ])
        .split(area);

    // Timer gauge
    let ratio = (approval.seconds_remaining as f64 / 120.0).clamp(0.0, 1.0);
    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Time Remaining "),
        )
        .gauge_style(if approval.seconds_remaining < 30 {
            Style::default().fg(Color::Red)
        } else if approval.seconds_remaining < 60 {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::Green)
        })
        .ratio(ratio)
        .label(format!("{}s", approval.seconds_remaining));
    frame.render_widget(gauge, chunks[0]);

    // Details
    let context_str = approval
        .context
        .as_ref()
        .map(|c| {
            // Pretty print JSON if possible
            serde_json::from_str::<serde_json::Value>(c)
                .map(|v| serde_json::to_string_pretty(&v).unwrap_or_else(|_| c.clone()))
                .unwrap_or_else(|_| c.clone())
        })
        .unwrap_or_else(|| "(none)".to_string());

    let detail_text = format!(
        "Action: {}\n\nRequester: {}\n\nContext:\n{}\n\nCreated: {}",
        approval.action,
        approval.requester,
        context_str,
        approval.created_at.format("%Y-%m-%d %H:%M:%S UTC")
    );

    let detail = Paragraph::new(detail_text)
        .block(Block::default().borders(Borders::ALL).title(" Details "))
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(detail, chunks[1]);
}

fn make_time_bar(seconds: i64) -> String {
    let filled = ((seconds as f64 / 120.0) * 6.0).ceil() as usize;
    let filled = filled.min(6);
    let empty = 6 - filled;
    format!("[{}{}]", "█".repeat(filled), "░".repeat(empty))
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let args = Args::parse();

    // Resolve API key: CLI arg > credd daemon.
    let api_key = if let Some(k) = args.api_key {
        k
    } else {
        let slot = kleos_lib::cred::bootstrap::current_agent_slot();
        match kleos_lib::cred::bootstrap::resolve_api_key(&slot).await {
            Ok(k) => k,
            Err(e) => {
                eprintln!("error: could not resolve API key: {}", e);
                eprintln!("hint: set CREDD_AGENT_KEY and CREDD_SOCKET, or pass --api-key");
                std::process::exit(1);
            }
        }
    };

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(args.url, api_key);
    let poll_duration = Duration::from_millis(args.poll_ms);

    // Initial fetch
    app.fetch_pending().await;

    loop {
        terminal.draw(|f| ui(f, &app))?;

        // Poll for events with timeout
        if event::poll(poll_duration)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('a') => {
                            app.decide(true).await;
                        }
                        KeyCode::Char('d') => {
                            app.decide(false).await;
                        }
                        KeyCode::Up | KeyCode::Char('k') => app.select_prev(),
                        KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                        KeyCode::Enter if !app.approvals.is_empty() => {
                            app.detail_mode = true;
                        }
                        KeyCode::Esc => {
                            app.detail_mode = false;
                        }
                        _ => {}
                    }
                }
            }
        } else {
            // Timeout - refresh data
            app.fetch_pending().await;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
