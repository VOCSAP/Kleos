#!/bin/bash
# Claude Code SessionStart hook: bootstrap context via Eidolon daemon.
# Calls Eidolon /prompt/generate for brain-aware context, /activity for registration.
# Falls back to direct kleos-cli if Eidolon unreachable.

set -euo pipefail

# Skip for subagents -- only main CLI sessions get full context bootstrap.
if [ -n "${CLAUDE_CODE_ENTRYPOINT:-}" ] && [ "$CLAUDE_CODE_ENTRYPOINT" != "cli" ]; then
  exit 0
fi

resolve_home() {
  if [ -n "${HOME:-}" ]; then printf '%s\n' "$HOME"; return; fi
  if command -v cygpath >/dev/null 2>&1 && [ -n "${USERPROFILE:-}" ]; then
    cygpath -u "$USERPROFILE"; return
  fi
  printf '%s\n' "${USERPROFILE:-.}"
}

HOME_DIR="$(resolve_home)"
LOG_DIR="$HOME_DIR/.claude/logs"
STATE_DIR="$HOME_DIR/.claude/session-env"
SESSION_KEY="${PPID:-$$}"
STAMP_FILE="$STATE_DIR/engram-ready-${SESSION_KEY}"
CRED_SESSION_ENV="$STATE_DIR/cred-get-session.env"
mkdir -p "$LOG_DIR" "$STATE_DIR" 2>/dev/null || true

# --- Write bootstrap stamps IMMEDIATELY (before any fallible call). ---
# Rationale: the rest of this script makes network calls to Eidolon and
# kleos-cli under `set -euo pipefail`. Any sporadic failure there used to
# silently kill the script before the stamp was written, leaving the
# pre-bash-guardrail blocking every subsequent command. The stamp represents
# "bootstrap started" not "bootstrap succeeded" -- the guardrail fallback
# just needs any kleos-ready-* file to exist. We rewrite STAMP_FILE with
# the success marker at the end of the script.
_NOW="$(date -u '+%Y-%m-%dT%H:%M:%SZ' 2>/dev/null || echo unknown)"
printf 'bootstrapping\t%s\t%s\n' "$_NOW" "early" > "$STATE_DIR/engram-ready-${PPID:-$$}" 2>/dev/null || true
printf 'global\t%s\n' "$_NOW" > "$STATE_DIR/engram-ready-global" 2>/dev/null || true

log() {
  echo "[$(date -u '+%Y-%m-%dT%H:%M:%SZ')] $*" >> "$LOG_DIR/session-start.log" 2>/dev/null || true
}

log "Wrote early bootstrap stamps (PPID=${PPID:-$$})"

if command -v cred >/dev/null 2>&1; then
  CRED_SESSION_TMP="${CRED_SESSION_ENV}.tmp.$$"
  if cred session start --shell > "$CRED_SESSION_TMP" 2>/dev/null; then
    mv "$CRED_SESSION_TMP" "$CRED_SESSION_ENV"
    chmod 600 "$CRED_SESSION_ENV" 2>/dev/null || true
    . "$CRED_SESSION_ENV" 2>/dev/null || true
    log "Minted cred get session grant"
  else
    rm -f "$CRED_SESSION_TMP" 2>/dev/null || true
    log "cred session start failed"
  fi
fi

# Source shared Eidolon helper
. "$HOME_DIR/.claude/hooks/lib-eidolon.sh"

# --- Clear engram-search enforcement stamp from previous session ---
rm -f "$STATE_DIR/engram-searched" 2>/dev/null || true
log "Cleared engram-searched stamp"

# --- Ensure eidolon-supervisor binary is running (Windows-only) ---
# Checks the running process directly (not the Scheduled Task). If absent,
# launches the binary detached via PowerShell Start-Process. User-scope env
# vars (CLAUDE_SESSIONS_DIR, EIDOLON_SUPERVISOR_CONFIG) are inherited
# automatically; KLEOS_API_KEY is loaded from ~/.config/eidolon/kleos-api-key.txt
# and exported so PowerShell child inherits it. Non-blocking: any failure is
# logged and ignored.
ensure_eidolon_running() {
  # Only attempt on Windows -- need tasklist.exe
  if ! command -v tasklist.exe >/dev/null 2>&1; then
    return 0
  fi

  # Check if the process is alive
  if tasklist.exe //FI "IMAGENAME eq eidolon-supervisor.exe" 2>/dev/null \
       | grep -qi "eidolon-supervisor.exe"; then
    log "eidolon-supervisor: process already running"
    return 0
  fi

  # Locate the binary -- check ~/.cargo/bin first, then PATH
  local bin_path=""
  if [ -x "$HOME_DIR/.cargo/bin/eidolon-supervisor.exe" ]; then
    bin_path="$HOME_DIR/.cargo/bin/eidolon-supervisor.exe"
  elif command -v eidolon-supervisor.exe >/dev/null 2>&1; then
    bin_path="$(command -v eidolon-supervisor.exe)"
  else
    log "eidolon-supervisor: binary not found in ~/.cargo/bin or PATH (skip)"
    return 0
  fi

  # Locate PowerShell
  local ps_bin=""
  if command -v pwsh.exe >/dev/null 2>&1; then
    ps_bin="pwsh.exe"
  elif command -v powershell.exe >/dev/null 2>&1; then
    ps_bin="powershell.exe"
  else
    log "eidolon-supervisor: no powershell available to launch detached (skip)"
    return 0
  fi

  # Load KLEOS_API_KEY from file if not already in env (with EIDOLON_API_KEY legacy)
  if [ -z "${KLEOS_API_KEY:-}" ]; then
    if [ -n "${EIDOLON_API_KEY:-}" ]; then
      KLEOS_API_KEY="$EIDOLON_API_KEY"
    else
      local key_file="$HOME_DIR/.config/eidolon/kleos-api-key.txt"
      if [ -f "$key_file" ]; then
        KLEOS_API_KEY="$(tr -d '\r\n ' < "$key_file" 2>/dev/null)"
      else
        log "eidolon-supervisor: no KLEOS_API_KEY/EIDOLON_API_KEY in env and key file missing at $key_file (launch anyway, /supervisor/inject will fail)"
      fi
    fi
    export KLEOS_API_KEY
  fi

  # URL cascade aligned on kleos-sh/main.rs:310-313. The supervisor binary
  # reads KLEOS_SERVER_URL first per its source, so we resolve and export
  # under that exact name.
  export KLEOS_SERVER_URL="${KLEOS_SERVER_URL:-${KLEOS_URL:-${ENGRAM_EIDOLON_URL:-${EIDOLON_URL:-http://127.0.0.1:4200}}}}"
  # HOME hint for the binary (it reads HOME, not USERPROFILE)
  export HOME="${HOME:-$HOME_DIR}"

  # Convert paths to Windows form for PowerShell
  local win_bin win_stdout win_stderr
  if command -v cygpath >/dev/null 2>&1; then
    win_bin="$(cygpath -w "$bin_path")"
    win_stdout="$(cygpath -w "$LOG_DIR/eidolon-supervisor.out.log")"
    win_stderr="$(cygpath -w "$LOG_DIR/eidolon-supervisor.err.log")"
  else
    win_bin="$bin_path"
    win_stdout="$LOG_DIR/eidolon-supervisor.out.log"
    win_stderr="$LOG_DIR/eidolon-supervisor.err.log"
  fi

  # Launch detached. The child inherits the parent PowerShell env, which
  # itself inherits the env we exported above.
  "$ps_bin" -NoProfile -NonInteractive -Command \
    "Start-Process -FilePath '$win_bin' -WindowStyle Hidden -RedirectStandardOutput '$win_stdout' -RedirectStandardError '$win_stderr'" \
    >/dev/null 2>&1 || true

  log "eidolon-supervisor: launched (bin=$bin_path, server=$KLEOS_SERVER_URL)"
}
ensure_eidolon_running || log "eidolon-supervisor: ensure-running block raised an error (ignored)"

# --- Ensure engram-approval-tui binary is running (Windows-only) ---
# Patch 19b: la cascade require_approval_patterns declenche un long-poll
# cote serveur qui attend une decision humaine via la TUI. Si la TUI n'est
# pas lancee, toutes les commandes matchant un pattern require_approval
# timeoutent apres APPROVAL_TIMEOUT_SECS (=120s) et sont denied.
#
# La TUI est une fullscreen ratatui interactive, pas un daemon silencieux:
# on la lance dans une fenetre console MINIMIZED (l'utilisateur la
# maximise depuis la barre des taches quand un approval arrive). Pattern
# calque sur ensure_eidolon_running mais sans WindowStyle Hidden (la TUI
# a besoin d'un host console pour son terminal alternate-screen).
ensure_approval_tui_running() {
  if ! command -v tasklist.exe >/dev/null 2>&1; then
    return 0
  fi

  if tasklist.exe //FI "IMAGENAME eq engram-approval-tui.exe" 2>/dev/null \
       | grep -qi "engram-approval-tui.exe"; then
    log "engram-approval-tui: process already running"
    return 0
  fi

  local tui_bin=""
  if [ -x "$HOME_DIR/.cargo/bin/engram-approval-tui.exe" ]; then
    tui_bin="$HOME_DIR/.cargo/bin/engram-approval-tui.exe"
  elif command -v engram-approval-tui.exe >/dev/null 2>&1; then
    tui_bin="$(command -v engram-approval-tui.exe)"
  else
    log "engram-approval-tui: binary not found in ~/.cargo/bin or PATH (skip)"
    return 0
  fi

  local ps_bin=""
  if command -v pwsh.exe >/dev/null 2>&1; then
    ps_bin="pwsh.exe"
  elif command -v powershell.exe >/dev/null 2>&1; then
    ps_bin="powershell.exe"
  else
    log "engram-approval-tui: no powershell available to launch (skip)"
    return 0
  fi

  # Reuse the KLEOS_API_KEY + KLEOS_SERVER_URL exported by
  # ensure_eidolon_running just above -- same precondition.
  if [ -z "${KLEOS_API_KEY:-}" ]; then
    log "engram-approval-tui: KLEOS_API_KEY missing -- launch skipped (TUI would fail auth)"
    return 0
  fi

  local win_tui
  if command -v cygpath >/dev/null 2>&1; then
    win_tui="$(cygpath -w "$tui_bin")"
  else
    win_tui="$tui_bin"
  fi

  # Note: pas de WindowStyle Hidden -- la TUI ratatui s'attend a un host
  # console. Minimized evite de voler le focus, l'utilisateur ouvre la
  # fenetre quand un approval arrive (notify systray pas encore implemente).
  "$ps_bin" -NoProfile -NonInteractive -Command \
    "Start-Process -FilePath '$win_tui' -WindowStyle Minimized -ArgumentList '--url','$KLEOS_SERVER_URL','--api-key','$KLEOS_API_KEY'" \
    >/dev/null 2>&1 || true

  log "engram-approval-tui: launched minimized (bin=$tui_bin, server=$KLEOS_SERVER_URL)"
}
ensure_approval_tui_running || log "engram-approval-tui: ensure-running block raised an error (ignored)"

log "SessionStart fired. HOME_DIR=$HOME_DIR"

# --- 0. Ensure kleos-sidecar Rust binary is running (Windows-only) ---
# Replaces the legacy Node.js "Mnemonic sidecar" path (~/.local/lib/mnemonic/index.ts)
# which never existed on VOCSAP hosts. The Rust binary kleos-sidecar.exe bind
# defaults to 127.0.0.1:7711 and reads KLEOS_URL / KLEOS_API_KEY / KLEOS_SIDECAR_TOKEN
# from env (clap-derived). Same detached launch pattern as ensure_eidolon_running.
ensure_kleos_sidecar_running() {
  if ! command -v tasklist.exe >/dev/null 2>&1; then
    return 0
  fi

  if tasklist.exe //FI "IMAGENAME eq kleos-sidecar.exe" 2>/dev/null \
       | grep -qi "kleos-sidecar.exe"; then
    log "kleos-sidecar: process already running"
    return 0
  fi

  local bin_path=""
  if [ -x "$HOME_DIR/.cargo/bin/kleos-sidecar.exe" ]; then
    bin_path="$HOME_DIR/.cargo/bin/kleos-sidecar.exe"
  elif command -v kleos-sidecar.exe >/dev/null 2>&1; then
    bin_path="$(command -v kleos-sidecar.exe)"
  else
    log "kleos-sidecar: binary not found in ~/.cargo/bin or PATH (skip)"
    return 0
  fi

  local ps_bin=""
  if command -v pwsh.exe >/dev/null 2>&1; then
    ps_bin="pwsh.exe"
  elif command -v powershell.exe >/dev/null 2>&1; then
    ps_bin="powershell.exe"
  else
    log "kleos-sidecar: no powershell available to launch detached (skip)"
    return 0
  fi

  # Resolve KLEOS_API_KEY same way as eidolon-supervisor so the child inherits it.
  if [ -z "${KLEOS_API_KEY:-}" ]; then
    if [ -n "${EIDOLON_API_KEY:-}" ]; then
      KLEOS_API_KEY="$EIDOLON_API_KEY"
    else
      local key_file="$HOME_DIR/.config/eidolon/kleos-api-key.txt"
      if [ -f "$key_file" ]; then
        KLEOS_API_KEY="$(tr -d '\r\n ' < "$key_file" 2>/dev/null)"
      fi
    fi
    export KLEOS_API_KEY
  fi

  # URL cascade: kleos-sidecar reads KLEOS_URL (clap env, kleos-sidecar/src/main.rs:111).
  # Mirror the same cascade used by lib-eidolon.sh / ensure_eidolon_running so the
  # operator only has to set KLEOS_SERVER_URL once.
  export KLEOS_URL="${KLEOS_URL:-${KLEOS_SERVER_URL:-${ENGRAM_EIDOLON_URL:-${EIDOLON_URL:-http://127.0.0.1:4200}}}}"
  export HOME="${HOME:-$HOME_DIR}"

  local win_bin win_stdout win_stderr
  if command -v cygpath >/dev/null 2>&1; then
    win_bin="$(cygpath -w "$bin_path")"
    win_stdout="$(cygpath -w "$LOG_DIR/kleos-sidecar.out.log")"
    win_stderr="$(cygpath -w "$LOG_DIR/kleos-sidecar.err.log")"
  else
    win_bin="$bin_path"
    win_stdout="$LOG_DIR/kleos-sidecar.out.log"
    win_stderr="$LOG_DIR/kleos-sidecar.err.log"
  fi

  "$ps_bin" -NoProfile -NonInteractive -Command \
    "Start-Process -FilePath '$win_bin' -WindowStyle Hidden -RedirectStandardOutput '$win_stdout' -RedirectStandardError '$win_stderr'" \
    >/dev/null 2>&1 || true

  log "kleos-sidecar: launched (bin=$bin_path, server=$KLEOS_URL)"

  # Best-effort short wait so /observe POSTs in the same SessionStart phase
  # see a bound listener. Aligned with the prior Node.js sidecar sleep 0.5.
  sleep 0.5
}
ensure_kleos_sidecar_running || log "kleos-sidecar: ensure-running block raised an error (ignored)"

# --- 1. Call Eidolon /prompt/generate for brain-aware context ---
PROMPT_RESULT=""
PROMPT_RESPONSE=$(eidolon_call POST "/prompt/generate" \
  '{"task":"session-bootstrap agent-rules infrastructure active-tasks recent-decisions","agent":"claude-code"}' \
  8 || echo "")

if [ -n "$PROMPT_RESPONSE" ]; then
  PROMPT_RESULT=$(python3 -c "
import sys, json
try:
    d = json.loads(sys.argv[1])
    prompt = d.get('prompt', '')
    if prompt:
        print(prompt)
except:
    pass
" "$PROMPT_RESPONSE" 2>/dev/null || echo "")
fi

log "Eidolon /prompt/generate returned ${#PROMPT_RESULT} chars"

# --- 2. Register session via Eidolon /activity (fans out to Chiasm, Axon, Broca, Brain) ---
ACTIVITY_RESPONSE=$(eidolon_call POST "/activity" \
  '{"agent":"claude-code","action":"task.started","summary":"Claude Code session started","project":"unknown"}' \
  5 || echo "")

# Extract Chiasm task ID from fanout response
if [ -n "$ACTIVITY_RESPONSE" ]; then
  TASK_ID=$(python3 -c "
import sys, json
try:
    d = json.loads(sys.argv[1])
    chiasm = d.get('fanout', {}).get('chiasm', {})
    tid = chiasm.get('created', chiasm.get('id', chiasm.get('auto_created', '')))
    if tid: print(str(tid))
except: pass
" "$ACTIVITY_RESPONSE" 2>/dev/null || echo "")
  if [ -n "$TASK_ID" ]; then
    echo "$TASK_ID" > /tmp/chiasm-claude-task-id
    log "Chiasm task via Eidolon: $TASK_ID"
  fi
fi

# --- 3. Recent memories via kleos-cli (lightweight, local) ---
# Resolves kleos-cli, falling back to the legacy engram-cli alias for installs
# that haven't reinstalled since the rename.
resolve_kleos_cli() {
  if [ -n "${KLEOS_CLI:-}" ]; then printf '%s\n' "$KLEOS_CLI"; return; fi
  if [ -n "${ENGRAM_CLI:-}" ]; then printf '%s\n' "$ENGRAM_CLI"; return; fi
  if command -v kleos-cli >/dev/null 2>&1; then command -v kleos-cli; return; fi
  if command -v engram-cli >/dev/null 2>&1; then command -v engram-cli; return; fi
  printf '%s/.local/bin/kleos-cli\n' "$HOME_DIR"
}

KLEOS_CLI="$(resolve_kleos_cli)"
RECENT_MEMORIES=""
# kleos-cli list returns plain text (one line per memory: "#ID [score] content...").
# The --json/--quiet flags do not exist on current kleos-cli (verified via --help,
# only --limit and --offset are accepted). We forward the raw text as-is.
if [ -f "$KLEOS_CLI" ] || command -v "$KLEOS_CLI" >/dev/null 2>&1; then
  RECENT_MEMORIES=$("$KLEOS_CLI" list --limit 5 2>/dev/null || echo "")
fi

# --- 4. Fallback: if Eidolon unreachable, query kleos-cli directly ---
if [ -z "$PROMPT_RESULT" ]; then
  log "Eidolon unreachable, falling back to direct kleos-cli"
  _resolve_kleos_key() {
    if [ -n "${KLEOS_API_KEY:-}" ]; then printf '%s\n' "$KLEOS_API_KEY"; return; fi
    if [ -n "${ENGRAM_API_KEY:-}" ]; then printf '%s\n' "$ENGRAM_API_KEY"; return; fi
    if command -v cred >/dev/null 2>&1; then
      local resolved
      resolved="$(cred get kleos api-key-claude --raw 2>/dev/null || cred get engram api-key-claude --raw 2>/dev/null || true)"
      if [ -n "$resolved" ]; then printf '%s\n' "$resolved"; return; fi
    fi
    printf '\n'
  }
  KLEOS_API_KEY="$(_resolve_kleos_key)"
  # kleos-cli context accepts only --limit on current code (verified via --help).
  # --budget and --quiet do not exist. The output is JSON (per KLEOS.md), so we
  # parse it locally to extract memory contents and join them as plain text.
  if { [ -f "$KLEOS_CLI" ] || command -v "$KLEOS_CLI" >/dev/null 2>&1; } && [ -n "$KLEOS_API_KEY" ]; then
    LOCAL_CTX_RAW=$("$KLEOS_CLI" context "agent-rules critical infrastructure active-tasks recent-decisions personality" --limit 8 2>/dev/null || echo "")
    if [ -n "$LOCAL_CTX_RAW" ]; then
      PROMPT_RESULT=$(python3 -c "
import sys, json
try:
    d = json.loads(sys.argv[1])
    items = d.get('memories', []) if isinstance(d, dict) else []
    lines = []
    for it in items:
        cat = it.get('category', 'unknown')
        content = str(it.get('content', ''))[:400]
        lines.append(f'[{cat}] {content}')
    print('\n'.join(lines))
except Exception:
    pass
" "$LOCAL_CTX_RAW" 2>/dev/null || echo "")
    fi
  fi
fi

# --- Handle total failure ---
if [ -z "$PROMPT_RESULT" ] && [ -z "$RECENT_MEMORIES" ]; then
  log "No context from Eidolon or Kleos"
  echo "EIDOLON AND KLEOS UNREACHABLE. Do NOT proceed with infrastructure work until context is confirmed."
  echo "MANDATORY: Use kleos-cli to check connectivity."
  exit 0
fi

# --- Build context block ---
CONTEXT_BLOCK=""

if [ -n "$PROMPT_RESULT" ]; then
  CONTEXT_BLOCK+="=== EIDOLON LIVING CONTEXT ===
$PROMPT_RESULT"
fi

if [ -n "$RECENT_MEMORIES" ]; then
  CONTEXT_BLOCK+="

=== RECENT MEMORIES ===
$RECENT_MEMORIES"
fi

CONTEXT_BLOCK+="

=== MANDATORY RULES ===
Use the Kleos skill and local kleos-cli via Git Bash (Bash tool) for ALL Kleos operations (search, store, list, context). Never use curl for Kleos. Do NOT use WSL.
Use OpenSpace MCP tools for all OpenSpace operations.
Search Kleos BEFORE asking the operator any question about servers, credentials, or past decisions.
Store outcomes to Kleos AFTER completing any task. Do not batch. Do not wait.
If Chiasm task ID exists at /tmp/chiasm-claude-task-id, update task status on changes.
EIDOLON: Before ANY destructive/irreversible action, the pre-bash-guardrail hook handles gate checks automatically. If gate returns deny, STOP and ask the operator. No exceptions."

# ── Growth materialization ─────────────────────────────────────────────
# DISABLED 2026-05-20 (spec_fe726101): the original GET /growth/materialize call
# does not match the real kleos-server route, which is POST with body
# {observation_id: i64} (kleos-server/src/routes/growth/mod.rs:24,50-60). The
# server materializes ONE observation per call; there is no aggregated markdown
# export route. Re-enabling requires either (a) creating a new server route
# (e.g. GET /growth/digest) or (b) replacing this with GET /growth/observations
# + client-side markdown formatting. Tracked in docs/dev-notes/agent-forge-guide.md
# section 10. Until then GROWTH.md is not refreshed by SessionStart.
GROWTH_MD=""
# GROWTH_RESULT=$(eidolon_call GET "/growth/materialize?service=claude-code&limit=30&max_bytes=16000" 2>/dev/null || true)
# if [ -n "$GROWTH_RESULT" ] && [ "$GROWTH_RESULT" != "null" ]; then
#   echo "$GROWTH_RESULT" > "$HOME_DIR/.claude/GROWTH.md"
#   GROWTH_MD="$GROWTH_RESULT"
#   log "Materialized GROWTH.md ($(echo "$GROWTH_RESULT" | wc -c) bytes)"
# fi

# Write stamp
printf 'ready\t%s\t%s\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" "eidolon" > "$STAMP_FILE"
log "Wrote session bootstrap stamp"

# Output structured JSON
python3 -c "
import json, sys
context = sys.argv[1]
growth = sys.argv[2] if len(sys.argv) > 2 else ''
if growth:
    context = context + '\n\n## Growth & Learnings\n' + growth
output = {
    'hookSpecificOutput': {
        'hookEventName': 'SessionStart',
        'additionalContext': context
    }
}
print(json.dumps(output))
" "$CONTEXT_BLOCK" "$GROWTH_MD"

log "SessionStart completed"
exit 0
