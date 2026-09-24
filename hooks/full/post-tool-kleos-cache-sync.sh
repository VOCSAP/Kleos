#!/usr/bin/env bash
# PostToolUse hook (matcher Bash): after a successful `kleos-cli store`, asks
# kleos-cache for one sync so the new memory is searchable locally before the
# next periodic tick. Returns at once; the POST runs in a detached worker.
#
# Coalescing: every store sets a pending marker; a single worker (mkdir lock)
# loops POST /v1/sync while the marker exists. POST /v1/sync blocks until the
# pass ends and answers `skipped` when another pass is running, and that pass
# may have started before the store, so the worker waits and retries instead
# of dropping the event.
#
# The token reaches curl through a config read on stdin, never through argv.

set -u

LISTEN="${KLEOS_CACHE_LISTEN:-127.0.0.1:8765}"
STATE_DIR="${KLEOS_CACHE_HOOK_STATE_DIR:-${HOME:-.}/.claude/session-env/kleos-cache-sync}"
LOG_FILE="${KLEOS_CACHE_HOOK_LOG:-${HOME:-.}/.claude/logs/kleos-cache-sync.log}"
PENDING="$STATE_DIR/pending"
WORKER_LOCK="$STATE_DIR/worker.lock"
WORKER_LOCK_MAX_AGE_S=450
WORKER_DEADLINE_S="${KLEOS_CACHE_SYNC_HOOK_DEADLINE_S:-240}"
SYNC_MAX_TIME_S=180
RETRY_DELAY_S=3

log() {
  printf '%s %s\n' "$(date '+%Y-%m-%dT%H:%M:%S')" "$*" >>"$LOG_FILE" 2>/dev/null
}

post_sync() {
  printf 'header = "Authorization: Bearer %s"\n' "$KLEOS_CACHE_TOKEN" |
    curl --silent --show-error --noproxy '*' --config - \
      --connect-timeout 1 --max-time "$SYNC_MAX_TIME_S" \
      --write-out '\n%{http_code}' -X POST "http://$LISTEN/v1/sync" 2>/dev/null
}

run_worker() {
  local started now response code body
  started=$(date +%s)
  while :; do
    while [ -e "$PENDING" ]; do
      now=$(date +%s)
      if [ $((now - started)) -ge "$WORKER_DEADLINE_S" ]; then
        log "worker: deadline ${WORKER_DEADLINE_S}s reached, pending left for the periodic sync"
        rmdir "$WORKER_LOCK" 2>/dev/null
        return 0
      fi
      rm -f "$PENDING" 2>/dev/null
      response=$(post_sync)
      code="${response##*$'\n'}"
      body="${response%$'\n'*}"
      case "$code:$body" in
        200:*'"completed"'*)
          log "sync completed in $(($(date +%s) - now))s"
          ;;
        200:*'already in progress'*)
          log "sync already in progress, retry in ${RETRY_DELAY_S}s"
          touch "$PENDING" 2>/dev/null
          sleep "$RETRY_DELAY_S"
          ;;
        200:*'not configured'*)
          log "sync skipped: KLEOS_CACHE_SYNC_KEY is not configured for serve"
          rm -f "$PENDING" 2>/dev/null
          ;;
        000:*)
          log "sync not sent: kleos-cache is not answering on $LISTEN"
          rm -f "$PENDING" 2>/dev/null
          ;;
        *)
          log "sync failed: http $code"
          rm -f "$PENDING" 2>/dev/null
          ;;
      esac
    done
    rmdir "$WORKER_LOCK" 2>/dev/null
    # A store landing between the last marker check and the unlock would
    # otherwise wait for the periodic sync.
    [ -e "$PENDING" ] && mkdir "$WORKER_LOCK" 2>/dev/null && continue
    return 0
  done
}

if [ "${1:-}" = "--worker" ]; then
  run_worker
  exit 0
fi

INPUT=$(cat 2>/dev/null)
case "$INPUT" in
  *kleos-cli*store*"Stored memory #"*) ;;
  *) exit 0 ;;
esac

MATCH=$(python3 -c '
import json, re, sys
try:
    d = json.load(sys.stdin)
except Exception:
    sys.exit(0)
if d.get("tool_name") != "Bash":
    sys.exit(0)
cmd = str((d.get("tool_input") or {}).get("command", ""))
resp = d.get("tool_response")
out = resp.get("stdout", "") if isinstance(resp, dict) else str(resp or "")
if re.search(r"(^|[\s;&|(/\\])kleos-cli(\.exe)?\s+(-[^\s\"\x27]+\s+([^-\s\"\x27][^\s\"\x27]*\s+)?)*store(\s|$)", cmd) and re.search(r"Stored memory #\d+", str(out)):
    print("yes")
' <<<"$INPUT" 2>/dev/null)
[ "$MATCH" = "yes" ] || exit 0

mkdir -p "$STATE_DIR" "$(dirname "$LOG_FILE")" 2>/dev/null
if [ -z "${KLEOS_CACHE_TOKEN:-}" ]; then
  log "skip: KLEOS_CACHE_TOKEN is not set"
  exit 0
fi

touch "$PENDING" 2>/dev/null
if ! mkdir "$WORKER_LOCK" 2>/dev/null; then
  lock_age_s=$(($(date +%s) - $(stat -c %Y "$WORKER_LOCK" 2>/dev/null || echo 0)))
  if [ "$lock_age_s" -lt "$WORKER_LOCK_MAX_AGE_S" ]; then
    exit 0
  fi
  rm -rf "$WORKER_LOCK" 2>/dev/null
  mkdir "$WORKER_LOCK" 2>/dev/null || exit 0
fi

nohup bash "$0" --worker </dev/null >/dev/null 2>&1 &
disown 2>/dev/null
exit 0
