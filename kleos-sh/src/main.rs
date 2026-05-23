mod exec;
mod gate;
mod observe;

use clap::Parser;
use std::process;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "kleos-sh", about = "Universal shell gate for AI agents")]
struct Cli {
    #[arg(short = 'c', help = "Command to execute")]
    command: Option<String>,

    #[arg(long, help = "Gate-only mode: check and exit, do not execute")]
    gate_only: bool,

    #[arg(long, default_value = "claude-code", help = "Agent identity")]
    agent: String,

    #[arg(long, help = "Tool name for gate check (e.g. Bash, Write)")]
    tool_name: Option<String>,

    /// Claude Code PreToolUse hook mode. Reads {tool_name, tool_input} JSON
    /// on stdin, emits {hookSpecificOutput:{...}} JSON on stdout, always
    /// exits 0 (the JSON carries deny/allow). Implies --gate-only.
    #[arg(long)]
    claude_hook: bool,
}

/// Build the Claude Code PreToolUse hookSpecificOutput envelope. Returns
/// None when the inputs would produce a meaningless body (silent allow),
/// in which case the caller emits nothing -- Claude treats absent stdout
/// as allow.
fn build_claude_decision(
    decision: &str,
    reason: Option<&str>,
    additional_context: Option<&str>,
) -> Option<serde_json::Value> {
    let mut hook_output = serde_json::Map::new();
    hook_output.insert(
        "hookEventName".to_string(),
        serde_json::Value::String("PreToolUse".to_string()),
    );
    if !decision.is_empty() {
        hook_output.insert(
            "permissionDecision".to_string(),
            serde_json::Value::String(decision.to_string()),
        );
        if let Some(r) = reason {
            hook_output.insert(
                "permissionDecisionReason".to_string(),
                serde_json::Value::String(r.to_string()),
            );
        }
    }
    if let Some(ctx) = additional_context {
        hook_output.insert(
            "additionalContext".to_string(),
            serde_json::Value::String(ctx.to_string()),
        );
    }
    if hook_output.len() == 1 {
        return None;
    }
    Some(serde_json::json!({ "hookSpecificOutput": hook_output }))
}

/// Emit a Claude Code PreToolUse hookSpecificOutput JSON line on stdout.
/// `decision` is "deny", "allow", or "" (silent allow). With an empty
/// decision and no additional_context, prints nothing.
fn emit_claude_decision(decision: &str, reason: Option<&str>, additional_context: Option<&str>) {
    if let Some(envelope) = build_claude_decision(decision, reason, additional_context) {
        println!("{}", envelope);
    }
}

/// Parse Claude Code's PreToolUse hook input from stdin. Expected shape:
/// {"tool_name": "Bash"|"Write"|"Edit"|"MultiEdit", "tool_input": {...}, ...}.
/// Returns (command, tool_name) or None if the payload does not parse or the
/// tool is not gateable. None is interpreted by the caller as a silent allow
/// (existing fail-open hook semantics).
fn parse_claude_hook_stdin() -> Option<(String, Option<String>)> {
    let mut buf = String::new();
    if std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf).is_err() {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(buf.trim()).ok()?;
    let tool_name = v
        .get("tool_name")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());
    let command = extract_command_for_tool(&v, tool_name.as_deref())?;
    Some((command, tool_name))
}

/// Patch 27 -- extract the gate-relevant command from tool_input based on
/// tool_name. Bash uses tool_input.command directly. Write/Edit/MultiEdit have
/// no command field; we synthesize a "<verb>:<file_path>" pseudo-command so
/// the server-side gate cascade (Patch 25 regex matcher + whitelist +
/// require_approval) can match file write/edit operations the same way it
/// matches shell commands. Unknown tool_name or missing required field returns
/// None -- the caller exits 0 = silent allow per existing fail-open semantics.
///
/// Splitting this out of `parse_claude_hook_stdin` keeps the JSON-extraction
/// branching unit-testable without spawning a subprocess to feed stdin.
fn extract_command_for_tool(
    v: &serde_json::Value,
    tool_name: Option<&str>,
) -> Option<String> {
    let tool_input = v.get("tool_input")?;
    match tool_name {
        // tool_name absent: preserve pre-Patch-26 behavior (assume Bash-shaped
        // payload, look for tool_input.command). This keeps any non-Claude
        // caller that omits tool_name working.
        Some("Bash") | None => tool_input
            .get("command")
            .and_then(|c| c.as_str())
            .map(|s| s.to_string()),
        Some("Write") => tool_input
            .get("file_path")
            .and_then(|p| p.as_str())
            .map(|s| format!("write:{}", s)),
        Some("Edit") => tool_input
            .get("file_path")
            .and_then(|p| p.as_str())
            .map(|s| format!("edit:{}", s)),
        Some("MultiEdit") => tool_input
            .get("file_path")
            .and_then(|p| p.as_str())
            .map(|s| format!("multiedit:{}", s)),
        _ => None,
    }
}

fn resolve_api_key() -> Option<String> {
    if let Ok(key) = std::env::var("KLEOS_API_KEY") {
        if !key.is_empty() {
            return Some(key);
        }
    }
    if let Ok(key) = std::env::var("EIDOLON_KEY") {
        if !key.is_empty() {
            return Some(key);
        }
    }
    // Primary path: ask credd for a bearer via its Unix socket. This is the
    // same flow as lib-eidolon.sh's _eidolon_key_via_credd().
    if let Some(key) = resolve_key_via_credd() {
        return Some(key);
    }
    // Fallback: standalone cred CLI (legacy, pre-Sparkling-Fairy).
    let slot = cred_slot();
    for attempt in 0..2 {
        let output = std::process::Command::new("cred")
            .args(["get", "kleos", &slot, "--raw"])
            .output()
            .ok()?;
        if output.status.success() {
            let key = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !key.is_empty() {
                return Some(key);
            }
        }
        if attempt == 0 {
            std::thread::sleep(Duration::from_millis(500));
        }
    }
    None
}

fn resolve_key_via_credd() -> Option<String> {
    #[cfg(unix)]
    return resolve_key_via_credd_socket();
    #[cfg(not(unix))]
    return resolve_key_via_credd_tcp();
}

/// Unix branch: contacts kleos-credd via a Unix domain socket (CREDD_SOCKET).
#[cfg(unix)]
fn resolve_key_via_credd_socket() -> Option<String> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;

    let socket_path = std::env::var("CREDD_SOCKET").ok()?;
    let agent_key = std::env::var("CREDD_AGENT_KEY").ok()?;
    if socket_path.is_empty() || agent_key.is_empty() {
        return None;
    }

    let slot = std::env::var("KLEOS_AGENT_SLOT")
        .unwrap_or_else(|_| "claude-code".into())
        .replace(['\r', '\n'], "");
    let agent_key = agent_key.replace(['\r', '\n'], "");
    let request = format!(
        "GET /bootstrap/kleos-bearer?agent={} HTTP/1.1\r\n\
         Host: localhost\r\n\
         Authorization: Bearer {}\r\n\
         Connection: close\r\n\
         \r\n",
        slot, agent_key
    );

    let mut stream = UnixStream::connect(&socket_path).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    stream.write_all(request.as_bytes()).ok()?;

    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;

    let body = response.split("\r\n\r\n").nth(1)?;
    let v: serde_json::Value = serde_json::from_str(body.trim()).ok()?;
    let key = v.get("key")?.as_str()?;
    if key.is_empty() {
        return None;
    }
    Some(key.to_string())
}

/// Non-Unix branch: contacts kleos-credd via TCP HTTP (CREDD_BIND).
/// kleos-credd exposes the same HTTP API on its TCP listener (default
/// 127.0.0.1:4400) as on its Unix socket, so the request is identical.
#[cfg(not(unix))]
fn resolve_key_via_credd_tcp() -> Option<String> {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let bind_addr = std::env::var("CREDD_BIND")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "127.0.0.1:4400".to_string());
    let agent_key = std::env::var("CREDD_AGENT_KEY").ok()?;
    if agent_key.is_empty() {
        return None;
    }

    let slot = std::env::var("KLEOS_AGENT_SLOT")
        .unwrap_or_else(|_| "claude-code-wsl".into())
        .replace(['\r', '\n'], "");
    let agent_key = agent_key.replace(['\r', '\n'], "");
    let request = format!(
        "GET /bootstrap/kleos-bearer?agent={} HTTP/1.1\r\n\
         Host: localhost\r\n\
         Authorization: Bearer {}\r\n\
         Connection: close\r\n\
         \r\n",
        slot, agent_key
    );

    let mut stream = TcpStream::connect(&bind_addr).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    stream.write_all(request.as_bytes()).ok()?;

    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;

    let body = response.split("\r\n\r\n").nth(1)?;
    let v: serde_json::Value = serde_json::from_str(body.trim()).ok()?;
    let key = v.get("key")?.as_str()?;
    if key.is_empty() {
        return None;
    }
    Some(key.to_string())
}

/// Fail-open policy. In --claude-hook mode the default is OPEN (matching
/// the legacy bash hook that always failed open with local safety blocks).
/// In exec mode the default is CLOSED. Override with KLEOS_SH_FAIL_OPEN.
fn fail_open_allowed(claude_hook: bool) -> bool {
    match std::env::var("KLEOS_SH_FAIL_OPEN").as_deref() {
        Ok("0") | Ok("false") => false,
        Ok("1") | Ok("true") => true,
        _ => claude_hook,
    }
}

/// Best-effort alert to Eidolon when the gate degrades. Fire-and-forget; we
/// never block the shell on the alert and we never propagate its errors.
fn alert_gate_degraded(
    client: &reqwest::Client,
    server_url: &str,
    api_key: Option<&str>,
    severity: &str,
    summary: &str,
) {
    let url = format!("{}/activity", server_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "agent": "kleos-sh",
        "action": if severity == "P0" { "error.raised" } else { "task.blocked" },
        "summary": summary,
        "project": "Kleos",
        "severity": severity,
    });
    let mut req = client.post(&url).json(&body);
    if let Some(key) = api_key {
        req = req.header("Authorization", format!("Bearer {}", key));
    }
    tokio::spawn(async move {
        let _ = req.send().await;
    });
}

/// Slot identifier used to look up this agent's credential. Defaults to
/// `claude-code-{user}-{host}`. Override with `KLEOS_CRED_KEY` (preferred)
/// or `KLEOS_AGENT_SLOT` to pin to a specific slot.
fn cred_slot() -> String {
    if let Ok(slot) = std::env::var("KLEOS_CRED_KEY") {
        if !slot.is_empty() {
            return slot;
        }
    }
    if let Ok(slot) = std::env::var("KLEOS_AGENT_SLOT") {
        if !slot.is_empty() {
            return slot;
        }
    }
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string());
    format!("claude-code-{}-{}", user, read_hostname())
}

fn read_hostname() -> String {
    #[cfg(unix)]
    if let Ok(h) = std::fs::read_to_string("/proc/sys/kernel/hostname") {
        let trimmed = h.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
    if let Ok(h) = std::env::var("HOSTNAME") {
        if !h.is_empty() {
            return h;
        }
    }
    // COMPUTERNAME is the standard hostname env var on Windows
    #[cfg(not(unix))]
    if let Ok(h) = std::env::var("COMPUTERNAME") {
        if !h.is_empty() {
            return h;
        }
    }
    "unknown-host".to_string()
}

fn server_url() -> String {
    std::env::var("KLEOS_SERVER_URL")
        .or_else(|_| std::env::var("KLEOS_URL"))
        .or_else(|_| std::env::var("ENGRAM_EIDOLON_URL"))
        .or_else(|_| std::env::var("EIDOLON_URL"))
        .unwrap_or_else(|_| "http://127.0.0.1:4200".to_string())
}

fn sidecar_url() -> String {
    std::env::var("KLEOS_SIDECAR_URL").unwrap_or_else(|_| "http://127.0.0.1:4201".to_string())
}

fn build_client() -> reqwest::Client {
    let timeout_secs: u64 = std::env::var("KLEOS_SH_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    // On Windows, tokio IOCP completion notifications can arrive after 2s even
    // when the TCP handshake succeeds quickly. Default raised to 5s; override
    // with KLEOS_SH_CONNECT_TIMEOUT_SECS if needed.
    let connect_timeout_secs: u64 = std::env::var("KLEOS_SH_CONNECT_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    // Ensure overall timeout is long enough to outlive the connect phase.
    let effective_timeout = timeout_secs.max(connect_timeout_secs + 2);
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(connect_timeout_secs))
        .timeout(Duration::from_secs(effective_timeout))
        .redirect(reqwest::redirect::Policy::limited(1))
        .build()
        .expect("failed to build HTTP client")
}

/// In claude-hook mode, emit a deny JSON envelope and exit 0. In legacy
/// mode, print to stderr and exit 2. Either way, this never returns.
fn deny_and_exit(claude_hook: bool, reason: &str) -> ! {
    if claude_hook {
        emit_claude_decision("deny", Some(reason), None);
        process::exit(0);
    }
    eprintln!("EIDOLON GATE DENIED: {}", reason);
    process::exit(2);
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // claude_hook implies gate_only: a hook decides, it does not execute.
    let gate_only = cli.gate_only || cli.claude_hook;

    let (command, effective_tool_name) = if cli.claude_hook {
        match parse_claude_hook_stdin() {
            Some((cmd, tn)) => (cmd, tn.or_else(|| cli.tool_name.clone())),
            None => {
                // Bad payload: emit a silent allow so we do not block the
                // tool over a parse hiccup. Stderr would mix with stdout JSON.
                process::exit(0);
            }
        }
    } else {
        let cmd = match &cli.command {
            Some(cmd) => cmd.clone(),
            None => {
                let mut input = String::new();
                if std::io::Read::read_to_string(&mut std::io::stdin(), &mut input).is_err()
                    || input.is_empty()
                {
                    process::exit(0);
                }
                input
            }
        };
        (cmd, cli.tool_name.clone())
    };

    if command.trim().is_empty() {
        process::exit(0);
    }

    let api_key = resolve_api_key();
    let server = server_url();
    let sidecar = sidecar_url();
    let client = build_client();

    let req = gate::GateCheckRequest {
        command: command.clone(),
        agent: cli.agent.clone(),
        context: None,
        tool_name: effective_tool_name
            .clone()
            .or_else(|| Some("Bash".to_string())),
    };

    // In hook mode: single 4s attempt, fail open on timeout (matching bash).
    // Non-hook (exec) mode: 4 retries with exponential backoff, fail closed.
    let max_attempts: usize = if cli.claude_hook { 1 } else { 4 };
    let outcome = match &api_key {
        Some(key) => {
            let mut last_err: Option<String> = None;
            let mut delay_ms = 250u64;
            let mut got: Option<gate::GateOutcome> = None;
            for attempt in 0..max_attempts {
                match gate::check_remote(&client, &server, key, &req).await {
                    Ok(outcome) => {
                        got = Some(outcome);
                        break;
                    }
                    Err(err) => {
                        last_err = Some(err);
                        if attempt + 1 < max_attempts {
                            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                            delay_ms *= 2;
                        }
                    }
                }
            }
            match got {
                Some(o) => o,
                None => {
                    let err_msg = last_err.unwrap_or_else(|| "unknown error".to_string());
                    if fail_open_allowed(cli.claude_hook) {
                        if !cli.claude_hook {
                            eprintln!(
                                "kleos-sh: gate unreachable after retries ({}), failing OPEN per KLEOS_SH_FAIL_OPEN",
                                err_msg
                            );
                        }
                        alert_gate_degraded(
                            &client,
                            &server,
                            Some(key),
                            "P1",
                            &format!("kleos-sh gate unreachable, fail-open opt-in: {}", err_msg),
                        );
                        gate::GateOutcome::Allow {
                            command: command.clone(),
                            enrichment: None,
                            gate_id: 0,
                        }
                    } else {
                        alert_gate_degraded(
                            &client,
                            &server,
                            Some(key),
                            "P0",
                            &format!("kleos-sh gate unreachable, fail-closed: {}", err_msg),
                        );
                        deny_and_exit(
                            cli.claude_hook,
                            &format!(
                                "kleos-sh: gate unreachable after retries ({}); failing CLOSED. Set KLEOS_SH_FAIL_OPEN=1 to override.",
                                err_msg
                            ),
                        );
                    }
                }
            }
        }
        None => {
            if fail_open_allowed(cli.claude_hook) {
                if !cli.claude_hook {
                    eprintln!(
                        "kleos-sh: no API key available, failing OPEN per KLEOS_SH_FAIL_OPEN"
                    );
                }
                alert_gate_degraded(
                    &client,
                    &server,
                    None,
                    "P1",
                    "kleos-sh missing API key, fail-open opt-in",
                );
                gate::GateOutcome::Allow {
                    command: command.clone(),
                    enrichment: None,
                    gate_id: 0,
                }
            } else {
                alert_gate_degraded(
                    &client,
                    &server,
                    None,
                    "P0",
                    "kleos-sh missing API key, fail-closed",
                );
                deny_and_exit(
                    cli.claude_hook,
                    "kleos-sh: no API key available; failing CLOSED. Set KLEOS_SH_FAIL_OPEN=1 to override (development only).",
                );
            }
        }
    };

    match outcome {
        gate::GateOutcome::Deny { reason, .. } => {
            deny_and_exit(cli.claude_hook, &reason);
        }
        gate::GateOutcome::Allow {
            command: resolved_cmd,
            enrichment,
            gate_id,
        } => {
            if let Some(ctx) = &enrichment {
                if cli.claude_hook {
                    emit_claude_decision("", None, Some(ctx));
                } else {
                    eprintln!("[kleos enrichment] {}", ctx);
                }
            }

            if gate_only {
                process::exit(0);
            }

            let result = exec::run_command(&resolved_cmd).await;

            match result {
                Ok(res) => {
                    if let Some(key) = &api_key {
                        observe::fire_and_forget(
                            &client,
                            &sidecar,
                            key,
                            &cli.agent,
                            &resolved_cmd,
                            gate_id,
                            res.exit_code,
                        );
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                    process::exit(res.exit_code);
                }
                Err(err) => {
                    eprintln!("kleos-sh: exec failed: {}", err);
                    process::exit(1);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_decision_deny_with_reason() {
        let v = build_claude_decision("deny", Some("nope"), None).expect("non-empty body");
        let h = &v["hookSpecificOutput"];
        assert_eq!(h["hookEventName"], "PreToolUse");
        assert_eq!(h["permissionDecision"], "deny");
        assert_eq!(h["permissionDecisionReason"], "nope");
        assert!(h.get("additionalContext").is_none());
    }

    #[test]
    fn build_decision_allow_with_enrichment() {
        let v = build_claude_decision("", None, Some("context here")).expect("non-empty body");
        let h = &v["hookSpecificOutput"];
        assert_eq!(h["hookEventName"], "PreToolUse");
        assert!(h.get("permissionDecision").is_none());
        assert_eq!(h["additionalContext"], "context here");
    }

    #[test]
    fn build_decision_silent_allow_returns_none() {
        assert!(build_claude_decision("", None, None).is_none());
    }

    #[test]
    fn build_decision_explicit_allow_no_enrichment() {
        // An explicit allow without context still emits something so the
        // caller can confirm the hook ran.
        let v = build_claude_decision("allow", None, None).expect("non-empty body");
        let h = &v["hookSpecificOutput"];
        assert_eq!(h["permissionDecision"], "allow");
    }

    // ---- Patch 27 tests : extract_command_for_tool ------------------------

    fn payload(tool: &str, input: serde_json::Value) -> serde_json::Value {
        serde_json::json!({ "tool_name": tool, "tool_input": input })
    }

    #[test]
    fn patch27_extract_bash_returns_raw_command() {
        let v = payload("Bash", serde_json::json!({"command": "ls -la"}));
        assert_eq!(
            extract_command_for_tool(&v, Some("Bash")),
            Some("ls -la".to_string()),
            "Bash payload must pass tool_input.command through unchanged"
        );
    }

    #[test]
    fn patch27_extract_write_synthesizes_pseudo_command() {
        let v = payload("Write", serde_json::json!({"file_path": "/etc/foo.env", "content": "x"}));
        assert_eq!(
            extract_command_for_tool(&v, Some("Write")),
            Some("write:/etc/foo.env".to_string()),
            "Write payload must yield write:<file_path>"
        );
    }

    #[test]
    fn patch27_extract_edit_synthesizes_pseudo_command() {
        let v = payload(
            "Edit",
            serde_json::json!({"file_path": "C:/Users/op/.ssh/config", "old_string": "a", "new_string": "b"}),
        );
        assert_eq!(
            extract_command_for_tool(&v, Some("Edit")),
            Some("edit:C:/Users/op/.ssh/config".to_string()),
            "Edit payload must yield edit:<file_path>"
        );
    }

    #[test]
    fn patch27_extract_multiedit_synthesizes_pseudo_command() {
        let v = payload(
            "MultiEdit",
            serde_json::json!({"file_path": "/var/lib/kleos/gate/blocked_patterns.txt", "edits": []}),
        );
        assert_eq!(
            extract_command_for_tool(&v, Some("MultiEdit")),
            Some("multiedit:/var/lib/kleos/gate/blocked_patterns.txt".to_string()),
            "MultiEdit payload must yield multiedit:<file_path>, edits array ignored"
        );
    }

    #[test]
    fn patch27_extract_unknown_tool_returns_none() {
        let v = payload("Read", serde_json::json!({"file_path": "/etc/passwd"}));
        assert!(
            extract_command_for_tool(&v, Some("Read")).is_none(),
            "Unknown tool must return None so caller falls through to silent allow"
        );
        let v = payload(
            "NotebookEdit",
            serde_json::json!({"notebook_path": "/x.ipynb", "cell_id": "c1"}),
        );
        assert!(extract_command_for_tool(&v, Some("NotebookEdit")).is_none());
    }

    #[test]
    fn patch27_extract_missing_required_field_returns_none() {
        // Write without file_path
        let v = payload("Write", serde_json::json!({"content": "no path"}));
        assert!(extract_command_for_tool(&v, Some("Write")).is_none());
        // Edit with file_path of wrong type (number instead of string)
        let v = payload("Edit", serde_json::json!({"file_path": 42}));
        assert!(extract_command_for_tool(&v, Some("Edit")).is_none());
        // Bash without command
        let v = payload("Bash", serde_json::json!({"timeout": 5}));
        assert!(extract_command_for_tool(&v, Some("Bash")).is_none());
        // No tool_input at all
        let v = serde_json::json!({"tool_name": "Bash"});
        assert!(extract_command_for_tool(&v, Some("Bash")).is_none());
        // tool_name absent treated as Bash, no command -> None
        let v = serde_json::json!({"tool_input": {"foo": "bar"}});
        assert!(extract_command_for_tool(&v, None).is_none());
    }
}
