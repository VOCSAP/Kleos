#!/bin/bash
# Claude Code PreToolUse hook: drain pending eidolon-supervisor violations.
#
# Reads the hook input JSON from stdin (Claude Code contract), extracts
# session_id, then calls GET /supervisor/pending on kleos-server. Any
# unclaimed injection is atomically marked claimed by the server and
# returned to this hook. We surface them as follows:
#
#   - severity=Critical -> exit 2 (block the upcoming tool call). The
#     concatenated messages are printed to stderr so Claude sees them.
#   - severity=Warning/Info -> exit 0 but emit the messages to stderr so
#     the operator notices them in the session transcript.
#
# Non-blocking on transport errors: if the server is unreachable or the
# response is malformed, we log and exit 0 (do not block the agent on
# infrastructure hiccups).
#
# Register in ~/.claude/settings.json under PreToolUse with matcher ".*"
# and a short timeout (3-5s).

set -uo pipefail

resolve_home() {
  if [ -n "${HOME:-}" ]; then printf '%s\n' "$HOME"; return; fi
  if command -v cygpath >/dev/null 2>&1 && [ -n "${USERPROFILE:-}" ]; then
    cygpath -u "$USERPROFILE"; return
  fi
  printf '%s\n' "${USERPROFILE:-.}"
}

HOME_DIR="$(resolve_home)"
LOG_DIR="$HOME_DIR/.claude/logs"
mkdir -p "$LOG_DIR" 2>/dev/null || true

log() {
  echo "[$(date -u '+%Y-%m-%dT%H:%M:%SZ')] $*" >> "$LOG_DIR/supervisor-drain.log" 2>/dev/null || true
}

# --- Read hook input from stdin ---
INPUT="$(cat 2>/dev/null || true)"
if [ -z "$INPUT" ]; then
  log "empty stdin, exit 0"
  exit 0
fi

SESSION_ID="$(printf '%s' "$INPUT" | python3 -c '
import sys, json, os
try:
    d = json.loads(sys.stdin.read())
    sid = d.get("session_id") or d.get("sessionId") or os.environ.get("CLAUDE_SESSION_ID", "")
    print(sid)
except Exception:
    print("")
' 2>/dev/null || true)"

if [ -z "$SESSION_ID" ]; then
  log "no session_id in input, exit 0"
  exit 0
fi

# --- Source the shared helper (provides eidolon_call) ---
LIB_EIDOLON="$HOME_DIR/.claude/hooks/lib-eidolon.sh"
if [ ! -f "$LIB_EIDOLON" ]; then
  log "lib-eidolon.sh missing at $LIB_EIDOLON, exit 0"
  exit 0
fi
. "$LIB_EIDOLON"

# --- Call /supervisor/pending ---
RESPONSE="$(eidolon_call GET "/supervisor/pending?session_id=$SESSION_ID" "" 3 2>/dev/null || echo "")"
if [ -z "$RESPONSE" ]; then
  log "no response from /supervisor/pending (server unreachable or auth failed)"
  exit 0
fi

# --- Parse response and decide ---
DECISION="$(printf '%s' "$RESPONSE" | python3 -c '
import sys, json
try:
    d = json.loads(sys.stdin.read())
    injs = d.get("injections", []) or []
    if not injs:
        print("none")
        sys.exit(0)
    has_critical = any(str(i.get("severity", "")).lower() == "critical" for i in injs)
    lines = []
    for i in injs:
        sev = i.get("severity", "?")
        rid = i.get("rule_id", "?")
        msg = i.get("message", "")
        lines.append(f"[supervisor:{sev}] {rid}: {msg}")
    body = "\n".join(lines)
    verdict = "block" if has_critical else "warn"
    print(f"{verdict}\t{body}")
except Exception as e:
    print(f"error\t{e}")
' 2>/dev/null || echo "error\tparse-failed")"

VERDICT="$(printf '%s' "$DECISION" | cut -f1)"
BODY="$(printf '%s' "$DECISION" | cut -f2-)"

case "$VERDICT" in
  none)
    exit 0
    ;;
  warn)
    log "warn: $BODY"
    printf '%s\n' "$BODY" >&2
    exit 0
    ;;
  block)
    log "block: $BODY"
    printf 'eidolon-supervisor blocked the tool call:\n%s\n' "$BODY" >&2
    exit 2
    ;;
  error|*)
    log "decision parse error: $DECISION"
    exit 0
    ;;
esac
