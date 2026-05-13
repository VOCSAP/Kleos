use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct GateCheckRequest {
    pub command: String,
    pub agent: String,
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GateCheckResult {
    pub allowed: bool,
    pub reason: Option<String>,
    pub resolved_command: Option<String>,
    pub gate_id: i64,
    #[allow(dead_code)]
    pub requires_approval: bool,
    pub enrichment: Option<String>,
}

pub enum GateOutcome {
    Allow {
        command: String,
        enrichment: Option<String>,
        gate_id: i64,
    },
    Deny {
        reason: String,
        #[allow(dead_code)]
        gate_id: i64,
    },
}

pub async fn check_remote(
    client: &reqwest::Client,
    server_url: &str,
    api_key: &str,
    req: &GateCheckRequest,
) -> Result<GateOutcome, String> {
    let url = format!("{}/gate/check", server_url.trim_end_matches('/'));

    let result = send_request(client, &url, api_key, req).await?;

    if result.allowed {
        let command = result
            .resolved_command
            .unwrap_or_else(|| req.command.clone());
        Ok(GateOutcome::Allow {
            command,
            enrichment: result.enrichment,
            gate_id: result.gate_id,
        })
    } else {
        let reason = result
            .reason
            .unwrap_or_else(|| "denied by gate (no reason given)".to_string());
        Ok(GateOutcome::Deny {
            reason,
            gate_id: result.gate_id,
        })
    }
}

// Windows gate check -- diagnostic history (do not re-try invalidated approaches)
//
// ATTEMPT 1: reqwest async (tokio) -- INVALIDATED
//   Error: "error sending request" / "operation timed out" (source chain)
//   Root cause: tokio IOCP on this Windows 11 machine delivers completion
//   notifications late (confirmed: TCP handshake succeeds at OS level, IOCP
//   notification never arrives within connect_timeout). Raising
//   KLEOS_SH_CONNECT_TIMEOUT_SECS had no effect on the root cause.
//   Status: permanently broken for this host, do not retry.
//
// ATTEMPT 2: spawn_blocking + std::net::TcpStream + raw HTTP/1.1 (BufReader)
//   Error: "read status: os error 10060" (WSAETIMEDOUT on read_line)
//   Observation: connect OK, write_all OK (267 bytes confirmed by debug print),
//   but read_line blocks until the 10s timeout -- server sends no response.
//   Tested: curl.exe to same endpoint with malformed JSON => 400 immediately.
//   Conclusion: server responds to curl but NOT to our raw TcpStream read.
//   Root cause unknown (WFP / server-side hang -- see PENDING TEST below).
//
// ATTEMPT 3: raw HTTP/1.1 fixes (Host header port, BufReader -> read_to_string,
//            read_to_string -> read_line+Content-Length) -- INVALIDATED
//   All three sub-variants timed out on the read side. The request format was
//   confirmed correct by debug print. The issue is not HTTP formatting.
//
// ATTEMPT 4: reqwest::blocking -- INVALIDATED
//   Error: "error sending request" (same as attempt 1)
//   Root cause: reqwest::blocking internally creates a tokio runtime, which
//   uses the same IOCP driver -- same failure mode as async reqwest.
//
// PENDING TEST: send valid JSON via curl --data-binary @file (avoiding
//   PowerShell quote mangling). If curl ALSO hangs with valid JSON, the
//   root cause is server-side (gate handler blocks on DB/LLM call).
//   If curl responds, root cause is client-side OS filtering (WFP/Defender).
//
// RESULT OF PENDING TEST (2026-05-12): curl standalone from PowerShell: 201 in <1s.
// curl spawned via spawn_blocking: exit 28 (CURLE_OPERATION_TIMEDOUT).
// Add-MpPreference -ExclusionProcess "kleos-sh.exe" did NOT fix the subprocess timeout.
//
// ATTEMPT 5B: tokio::process::Command (async, no spawn_blocking).
// Result: curl trace shows source port 1434 (abnormal -- ephemeral range is 49152-65535).
// TCP connect OK, write_all OK (79 bytes), 0 bytes received in 12006ms.
// Root cause hypothesis: source port 1434 is in a "registered" range; router or Proxmox
// bridge drops return packets destined to ports below 49152 (asymmetric routing artifact).
//
// ATTEMPT 6: force ephemeral source port with --local-port 49152-65535.
// Result: curl_exit=7 (CURLE_COULDNT_CONNECT), port 49152 "Address already in use".
// curl tried the first port in the range (49152), found it bound, and did NOT retry
// the next port -- immediate failure instead of scanning the range. Progress: confirms
// curl was previously using port 1434 (non-ephemeral). Next port in range was not tried.
// NOTE: OPNSense is the firewall -- investigate stateful filtering rules for LXC traffic.
//
// ATTEMPT 7 (2026-05-13): drop --local-port entirely -- FIX CONFIRMED.
// Diagnostic results that closed the chapter:
//   * `curl --local-port 49152` (single) on /health = HTTP 200 from src 49152.
//   * `curl --local-port 49152-65535` (range) = "Address already in use" on
//     49152, no scan, no retry. Same port, same context, opposite results.
//   * `curl --local-port 55000-65535` first call OK from src 55000. Second
//     call 1ms later = "Address already in use" on 55000 (TIME_WAIT from the
//     previous call). curl Windows takes ONLY the first port of the range
//     and never increments. ANY fixed range will degrade after a single use
//     until TIME_WAIT clears (~60-120s).
//   * `curl` without --local-port: three consecutive calls succeed from
//     src 48900 / 48902 / 48905 -- the OS picks a fresh ephemeral port each
//     time from the configured dynamic range (this host has it reset to
//     1024-65535 via `netsh int ipv4 set dynamicport tcp start=1024
//     num=64511`).
// Why ATTEMPT 5B failed without --local-port: OPNSense was dropping the
// reply packets back to the abnormally-low source port (1434). OPNSense
// was cleared on 2026-05-12 (subnet_admin rule seq=25, quick=1) -- the
// router no longer drops low-port return traffic from 192.168.10.100, so
// we can let the OS choose the source port freely.
#[cfg(not(unix))]
async fn send_request(
    _client: &reqwest::Client,
    url: &str,
    api_key: &str,
    req: &GateCheckRequest,
) -> Result<GateCheckResult, String> {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    use tokio::process::Command;

    let body = serde_json::to_string(req)
        .map_err(|e| format!("failed to serialize request: {}", e))?;

    // Curl --max-time. Default 12s matches the legacy hardcoded value and
    // is enough for the fast path (non-approval tools resolve in <1s).
    // Tools in kleos-lib TOOLS_REQUIRING_APPROVAL ("Bash", "Write", "Edit",
    // "WebFetch", "WebSearch") trigger a server-side wait of up to
    // APPROVAL_TIMEOUT_SECS (=120s) for a human to approve via the Kleos
    // GUI. To exec-mode a Bash command and actually wait for that approval,
    // set KLEOS_SH_APPROVAL_TIMEOUT_SECS=121 (or higher) in the calling
    // shell. Hook mode (--claude-hook) fails open on short timeouts, so a
    // higher value there mostly just delays Claude Code's PreToolUse path.
    let max_time = std::env::var("KLEOS_SH_APPROVAL_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(12)
        .to_string();

    let mut child = Command::new("curl")
        .args([
            "--silent",
            "-v",
            "--max-time",
            &max_time,
            url,
            "-H",
            &format!("Authorization: Bearer {}", api_key),
            "-H",
            "Content-Type: application/json",
            "--data-binary",
            "@-",
            "-w",
            "\n%{http_code}",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("gate check failed: curl not available: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(body.as_bytes())
            .await
            .map_err(|e| format!("gate check failed: write stdin: {}", e))?;
    }

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("gate check failed: curl wait: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let curl_exit = output.status.code().unwrap_or(-1);

    let mut parts = stdout.rsplitn(2, '\n');
    let status_str = parts.next().unwrap_or("").trim();
    let response_body = parts.next().unwrap_or("").trim().to_string();

    let status: u16 = status_str.parse().unwrap_or(0);
    if status != 200 && status != 201 {
        return Err(format!(
            "gate check returned http={} curl_exit={} body={:?} curl_trace={:?}",
            status,
            curl_exit,
            response_body,
            stderr.trim()
        ));
    }

    serde_json::from_str::<GateCheckResult>(&response_body)
        .map_err(|e| format!("failed to parse gate response: {}", e))
}

#[cfg(unix)]
async fn send_request(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    req: &GateCheckRequest,
) -> Result<GateCheckResult, String> {
    let resp = client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(req)
        .send()
        .await
        .map_err(|e| format!("gate check request failed: {}", e))?;

    let status = resp.status();
    if !status.is_success() && status.as_u16() != 201 {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("gate check returned {}: {}", status, body));
    }

    resp.json::<GateCheckResult>()
        .await
        .map_err(|e| format!("failed to parse gate response: {}", e))
}

