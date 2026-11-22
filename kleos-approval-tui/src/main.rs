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
use tokio::task::JoinHandle;

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

    /// UI refresh tick in milliseconds. Patch 20b: lower-bounded short tick
    /// (default 100ms) keeps the event loop responsive while a long-poll
    /// network request runs in the background; we no longer block the loop
    /// on `fetch_pending().await`.
    #[arg(short, long, default_value = "100")]
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

impl Approval {
    /// Total approval window in seconds (`expires_at - created_at`), floored at
    /// 1 to avoid division by zero. BF-5: the timer gauges divide by this real
    /// window instead of a hardcoded 120s, so the bar stays accurate when the
    /// server's approval timeout differs from 120s.
    fn window_secs(&self) -> f64 {
        (self.expires_at - self.created_at).num_seconds().max(1) as f64
    }
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

/// Patch 20b: outcome of an async fetch task. The background task hands
/// this to the main loop, which then applies it to App state on its next
/// tick. The main loop never blocks on the HTTP call.
enum FetchOutcome {
    Ok(Vec<Approval>),
    RateLimited(Duration),
    HttpError(String),
    ParseError(String),
    ConnectionError(String),
}

/// Patch 20b: outcome of an async decide task. Same idea as FetchOutcome
/// but for the POST /approvals/{id}/decide call.
enum DecideOutcome {
    Ok,
    RateLimited(Duration),
    HttpError(String),
    ConnectionError(String),
}

struct App {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    /// Identity recorded in audit log for every approve/deny decision.
    /// Defaults to the $USER environment variable, falling back to "tui-operator".
    decided_by: String,
    approvals: Vec<Approval>,
    selected: usize,
    list_state: ListState,
    last_error: Option<String>,
    detail_mode: bool,
    /// Patch 20: when set, the server has issued a 429 and the client must
    /// not retry HTTP calls until this instant. Each tick the UI updates
    /// `last_error` with a countdown so the operator sees what is happening.
    last_429_until: Option<Instant>,
    /// Patch 20b: in-flight background fetch task. None means no fetch is
    /// active; `try_start_fetch` will start a new one as soon as the
    /// previous one completes (or is dropped).
    fetch_handle: Option<JoinHandle<FetchOutcome>>,
    /// Patch 20b: in-flight background decide task. The TUI accepts a key
    /// press while one is in flight (it just shows a status), the result
    /// is applied when ready.
    decide_handle: Option<JoinHandle<DecideOutcome>>,
    /// Patch 20b: instant the previous fetch task completed. Combined with
    /// `MIN_REFETCH_INTERVAL` this acts as a hard floor on the re-fetch
    /// rate so that an upstream error which returns instantly (DNS,
    /// connection refused, malformed response) cannot turn into a
    /// 600 req/min hot-loop against the server. In steady state with a
    /// long-poll that holds 30s the floor is irrelevant.
    last_fetch_completed_at: Option<Instant>,
}

/// Patch 20b: minimum delay between two consecutive fetch attempts when
/// the pending queue is empty. Acts as a cooldown so the TUI does not
/// hot-loop on transient failure (DNS, connection refused, malformed
/// response) while the server-side long-poll is otherwise expected to
/// hold the connection 30s. Override via `KLEOS_APPROVAL_TUI_REFETCH_BUSY_MS`.
const DEFAULT_REFETCH_BUSY_MS: u64 = 500;

/// Patch 23 (2026-05-22): minimum delay between two consecutive fetch
/// attempts when the pending queue is non-empty. The server-side long-poll
/// (Patch 20c) returns immediately when the list is non-empty -- that is
/// correct by design (the client must see the row now). But the TUI was
/// re-fetching every 500ms while the operator was reading/deciding, so a
/// single pending approval generated ~60-120 calls/min and blew through
/// the preauth IP rate-limit (default 20/min) in seconds. With a 5s
/// cooldown a typical 20-30s decision window costs 4-6 calls instead of
/// ~60. Override via `KLEOS_APPROVAL_TUI_REFETCH_PENDING_MS`.
const DEFAULT_REFETCH_PENDING_MS: u64 = 5000;

fn refetch_busy_interval() -> Duration {
    let ms = std::env::var("KLEOS_APPROVAL_TUI_REFETCH_BUSY_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_REFETCH_BUSY_MS);
    Duration::from_millis(ms)
}

fn refetch_pending_interval() -> Duration {
    let ms = std::env::var("KLEOS_APPROVAL_TUI_REFETCH_PENDING_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(DEFAULT_REFETCH_PENDING_MS);
    Duration::from_millis(ms)
}

impl App {
    fn new(url: String, api_key: String) -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        // Patch 20: configure the reqwest client with the long-poll timeout
        // so a single GET can sit on the wire while the server holds the
        // connection open in long-poll mode. 727d97fc merge: keep upstream's
        // (TUI-1) connect_timeout so connection establishment is bounded
        // separately from the long-poll request wait.
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(http_longpoll_timeout())
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        // Resolve operator identity at startup for audit attribution (TUI-1,
        // upstream #93): replaces the hardcoded "tui-operator" literal so the
        // approval audit records the real operator user.
        let decided_by = std::env::var("USER").unwrap_or_else(|_| "tui-operator".to_string());
        Self {
            client,
            base_url: url,
            api_key,
            decided_by,
            approvals: Vec::new(),
            selected: 0,
            list_state,
            last_error: None,
            detail_mode: false,
            last_429_until: None,
            fetch_handle: None,
            decide_handle: None,
            last_fetch_completed_at: None,
        }
    }

    /// Patch 20: returns true if a previous 429 still applies. Side effect:
    /// updates `last_error` with a live countdown so the operator can see
    /// how long the back-off has left to run.
    fn in_rate_limit_window(&mut self) -> bool {
        if let Some(until) = self.last_429_until {
            let now = Instant::now();
            if now < until {
                let wait = (until - now).as_secs() + 1;
                self.last_error = Some(format!("Rate-limited by server, retry in {}s", wait));
                return true;
            }
            self.last_429_until = None;
        }
        false
    }

    /// Patch 20b: start a non-blocking fetch in the background if no fetch
    /// is currently in flight, we're not inside a rate-limit window, and
    /// the minimum cooldown since the previous fetch has elapsed. The
    /// result is collected by `poll_handles` on a later tick.
    fn try_start_fetch(&mut self) {
        if self.fetch_handle.is_some() {
            return;
        }
        if self.in_rate_limit_window() {
            return;
        }
        if let Some(last) = self.last_fetch_completed_at {
            // Patch 23 (2026-05-22): adaptive cooldown. When the pending
            // queue is empty, the server-side long-poll holds the
            // connection so a 500ms floor is safe. When the queue is
            // non-empty, the server returns immediately (Patch 20c
            // contract) and the previous 500ms floor produced a poll
            // storm while the operator was reading the row. 5s default
            // is calibrated to the typical decision window (20-30s).
            let cooldown = if self.approvals.is_empty() {
                refetch_busy_interval()
            } else {
                refetch_pending_interval()
            };
            if Instant::now() < last + cooldown {
                return;
            }
        }
        let client = self.client.clone();
        let url = format!("{}/approvals/pending", self.base_url);
        let api_key = self.api_key.clone();
        let handle = tokio::spawn(async move {
            match client
                .get(&url)
                .header("Authorization", format!("Bearer {}", api_key))
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        match resp.json::<PendingResponse>().await {
                            Ok(data) => FetchOutcome::Ok(data.approvals),
                            Err(e) => FetchOutcome::ParseError(format!("{}", e)),
                        }
                    } else if status.as_u16() == 429 {
                        FetchOutcome::RateLimited(parse_retry_after(&resp))
                    } else {
                        FetchOutcome::HttpError(format!("HTTP {}", status))
                    }
                }
                Err(e) => FetchOutcome::ConnectionError(format!("{}", e)),
            }
        });
        self.fetch_handle = Some(handle);
    }

    /// Patch 20b: start a non-blocking decide POST. Only one in flight at
    /// a time; subsequent key presses are ignored while the task runs.
    fn try_start_decide(&mut self, approved: bool) {
        if self.approvals.is_empty() {
            return;
        }
        if self.decide_handle.is_some() {
            return;
        }
        if self.in_rate_limit_window() {
            return;
        }
        let approval = &self.approvals[self.selected];
        let url = format!("{}/approvals/{}/decide", self.base_url, approval.id);
        let api_key = self.api_key.clone();
        let client = self.client.clone();
        // TUI-1 (upstream #93): real operator identity resolved at startup
        // instead of a hardcoded "tui-operator" literal, for audit attribution.
        // Cloned into a local so the `async move` task can own it (cannot borrow
        // `self` across the spawn).
        let decided_by = self.decided_by.clone();
        let body = DecideRequest {
            decision: if approved { "approved" } else { "denied" }.to_string(),
            decided_by: Some(decided_by),
            reason: None,
        };
        let handle = tokio::spawn(async move {
            match client
                .post(&url)
                .header("Authorization", format!("Bearer {}", api_key))
                .json(&body)
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        DecideOutcome::Ok
                    } else if status.as_u16() == 429 {
                        DecideOutcome::RateLimited(parse_retry_after(&resp))
                    } else {
                        let body = resp.text().await.unwrap_or_default();
                        DecideOutcome::HttpError(format!("HTTP {}: {}", status, body))
                    }
                }
                Err(e) => DecideOutcome::ConnectionError(format!("{}", e)),
            }
        });
        self.decide_handle = Some(handle);
    }

    /// Patch 20b: collect any finished background task and apply its result
    /// to App state. Non-blocking: a task that is still running is left
    /// alone and re-checked on the next tick.
    async fn poll_handles(&mut self) {
        if let Some(handle) = self.fetch_handle.take() {
            if handle.is_finished() {
                match handle.await {
                    Ok(outcome) => self.apply_fetch_outcome(outcome),
                    Err(e) => {
                        self.last_error = Some(format!("fetch task aborted: {}", e));
                    }
                }
                // Patch 20b: arm the cooldown for the next try_start_fetch.
                self.last_fetch_completed_at = Some(Instant::now());
            } else {
                self.fetch_handle = Some(handle);
            }
        }
        if let Some(handle) = self.decide_handle.take() {
            if handle.is_finished() {
                match handle.await {
                    Ok(outcome) => {
                        self.apply_decide_outcome(outcome);
                    }
                    Err(e) => {
                        self.last_error = Some(format!("decide task aborted: {}", e));
                    }
                }
            } else {
                self.decide_handle = Some(handle);
            }
        }
    }

    fn apply_fetch_outcome(&mut self, outcome: FetchOutcome) {
        match outcome {
            FetchOutcome::Ok(approvals) => {
                self.approvals = approvals;
                self.last_error = None;
                if !self.approvals.is_empty() && self.selected >= self.approvals.len() {
                    self.selected = self.approvals.len() - 1;
                }
                self.list_state.select(if self.approvals.is_empty() {
                    None
                } else {
                    Some(self.selected)
                });
            }
            FetchOutcome::RateLimited(wait) => {
                self.last_429_until = Some(Instant::now() + wait);
                self.last_error = Some(format!(
                    "Rate-limited by server. Backing off {}s.",
                    wait.as_secs()
                ));
            }
            FetchOutcome::HttpError(msg) | FetchOutcome::ParseError(msg)
            | FetchOutcome::ConnectionError(msg) => {
                self.last_error = Some(msg);
            }
        }
    }

    fn apply_decide_outcome(&mut self, outcome: DecideOutcome) {
        match outcome {
            DecideOutcome::Ok => {
                self.last_error = None;
                // Force a refresh: drop any in-flight fetch so the next tick
                // immediately kicks off a new one. The drop sends a cancel
                // to the task; reqwest aborts cleanly.
                if let Some(handle) = self.fetch_handle.take() {
                    handle.abort();
                }
                // Patch 23 (2026-05-22): bypass the adaptive cooldown so the
                // next try_start_fetch kicks off without waiting the 5s
                // pending-window cooldown. Without this, the operator
                // would see a stale UI for up to 5s after their decision
                // even though the server is ready to push the next state.
                self.last_fetch_completed_at = None;
            }
            DecideOutcome::RateLimited(wait) => {
                self.last_429_until = Some(Instant::now() + wait);
                self.last_error = Some(format!(
                    "Rate-limited by server. Backing off {}s.",
                    wait.as_secs()
                ));
            }
            DecideOutcome::HttpError(msg) | DecideOutcome::ConnectionError(msg) => {
                self.last_error = Some(msg);
            }
        }
        // Always refresh the pending list regardless of outcome so stale items
        // do not remain in the display after a failed decide (TUI-2). 727d97fc
        // merge: upstream did this with a synchronous `fetch_pending().await`,
        // but in the Patch 20b non-blocking model the refresh is driven by the
        // event loop -- clearing the cooldown makes the next tick's
        // `try_start_fetch` fire immediately (the Ok arm above also aborts the
        // in-flight fetch for the same effect).
        self.last_fetch_completed_at = None;
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
        " KLEOS APPROVAL CONSOLE                                     Pending: {}",
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
        " [Up/Down] Select  [a] Approve  [d] Deny  [Enter] Details  [q] Quit"
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
            let time_bar = make_time_bar(a.seconds_remaining, a.window_secs());
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

    // Timer gauge (BF-5: denominator is the real approval window, not 120s).
    let ratio = (approval.seconds_remaining as f64 / approval.window_secs()).clamp(0.0, 1.0);
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
        approval.action, approval.requester, context_str,
        approval.created_at.format("%Y-%m-%d %H:%M:%S UTC")
    );

    let detail = Paragraph::new(detail_text)
        .block(Block::default().borders(Borders::ALL).title(" Details "))
        .wrap(ratatui::widgets::Wrap { trim: false });
    frame.render_widget(detail, chunks[1]);
}

fn make_time_bar(seconds: i64, window_secs: f64) -> String {
    // BF-5: scale against the real approval window rather than a hardcoded 120s.
    let window = window_secs.max(1.0);
    let filled = ((seconds as f64 / window) * 6.0).ceil() as usize;
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

    // Restore the terminal on panic: a crash mid-render would otherwise leave
    // the user's shell in raw mode and the alternate screen.
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        original_hook(info);
    }));

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(args.url, api_key);
    let tick = Duration::from_millis(args.poll_ms);

    // Patch 20b: kick off the very first fetch in the background so the
    // first UI frame already shows an empty list with the long-poll in
    // flight rather than a blank screen. The main loop's poll_handles
    // collects the result a few ticks later.
    app.try_start_fetch();

    loop {
        terminal.draw(|f| ui(f, &app))?;

        // Patch 20b: collect finished background tasks (fetch + decide)
        // without blocking. JoinHandle::is_finished is a synchronous check
        // and the await on a finished handle returns immediately.
        app.poll_handles().await;

        // Patch 20b: keep a fetch in flight at all times so the server's
        // long-poll can wake us up the moment a new approval arrives. If
        // a fetch is already running or we're inside a 429 back-off, this
        // is a no-op.
        app.try_start_fetch();

        // Patch 20b: short event::poll timeout (default 100ms) keeps the
        // UI responsive while the long-poll runs in the background. The
        // operator's key presses are handled on the next tick at worst.
        if event::poll(tick)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('a') => {
                            app.try_start_decide(true);
                        }
                        KeyCode::Char('d') => {
                            app.try_start_decide(false);
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
