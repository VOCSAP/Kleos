#!/usr/bin/env bash
# Claude Code SessionStart hook: declare the Claude session_id to kleos-sidecar.
#
# Without this hook, every PostToolUse observation lands in the sidecar's
# `default_session_id` (a fresh UUID generated at sidecar boot) instead of the
# Claude Code session_id. Result: observations from N successive Claude sessions
# get bucketed together. Hard to query, hard to flush per-session.
#
# This hook reads `session_id` from the SessionStart event stdin JSON and POSTs
# /session/start to the sidecar so subsequent /observe and /end calls (with the
# same session_id) route to the correct session bucket.
#
# Idempotent by design: 201 Created and 409 Conflict are both fine
# (409 means the session already exists, which is the desired end state).
#
# Fail-open: any failure (sidecar absent, network, parse) -> exit 0 silently.
# This hook MUST run after session-start-kleos.sh which spawns the sidecar
# binary; settings.json orders them in the same hooks array.

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
  echo "[$(date -u '+%Y-%m-%dT%H:%M:%SZ')] $*" >> "$LOG_DIR/sidecar-session-start.log" 2>/dev/null || true
}

# URL + token cascade (canonical KLEOS_SIDECAR_*, legacy ENGRAM_SIDECAR_* fallback).
SIDECAR_URL="${KLEOS_SIDECAR_URL:-${ENGRAM_SIDECAR_URL:-http://127.0.0.1:7711}}"
SIDECAR_TOKEN="${KLEOS_SIDECAR_TOKEN:-${ENGRAM_SIDECAR_TOKEN:-}}"

# Read SessionStart event JSON.
INPUT=$(cat 2>/dev/null || echo "{}")
SID=$(printf '%s' "$INPUT" | python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    print(d.get('session_id', d.get('sessionId', '')))
except Exception:
    print('')
" 2>/dev/null || echo "")

if [ -z "$SID" ]; then
  log "no session_id in stdin payload, exit"
  exit 0
fi

# Health-gate with short retry: the sibling hook session-start-kleos.sh launches
# kleos-sidecar.exe detached, the binary needs ~200-500ms to bind 7711. Both
# SessionStart hooks may run in parallel so we poll briefly instead of giving up
# on the first miss. ~3s total max.
SIDECAR_READY=0
for _i in 1 2 3 4 5 6; do
  if curl -sf --max-time 1 "$SIDECAR_URL/health" \
       ${SIDECAR_TOKEN:+-H "Authorization: Bearer $SIDECAR_TOKEN"} \
       >/dev/null 2>&1; then
    SIDECAR_READY=1
    break
  fi
  sleep 0.5 2>/dev/null || true
done

if [ "$SIDECAR_READY" -ne 1 ]; then
  log "sidecar /health unreachable after ~3s, exit (session_id=$SID)"
  exit 0
fi

# Build JSON payload via python to avoid quoting headaches on Windows Git Bash.
PAYLOAD=$(python3 -c "import json,sys; print(json.dumps({'session_id': sys.argv[1]}))" "$SID" 2>/dev/null || echo '{}')

# POST /session/start. Idempotent: 201 (Created) and 409 (Conflict) both OK.
HTTP_CODE=$(curl -sf --max-time 2 "$SIDECAR_URL/session/start" \
  -X POST \
  ${SIDECAR_TOKEN:+-H "Authorization: Bearer $SIDECAR_TOKEN"} \
  -H "Content-Type: application/json" \
  -d "$PAYLOAD" \
  -o /dev/null -w "%{http_code}" 2>/dev/null || echo "000")

case "$HTTP_CODE" in
  201) log "session declared OK (session_id=$SID, http=201)" ;;
  409) log "session already exists, fine (session_id=$SID, http=409)" ;;
  000) log "POST /session/start failed (network/timeout, session_id=$SID)" ;;
  *)   log "POST /session/start unexpected http=$HTTP_CODE (session_id=$SID)" ;;
esac

exit 0
