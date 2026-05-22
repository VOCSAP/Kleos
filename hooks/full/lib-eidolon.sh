#!/bin/bash
# Shared Eidolon helper for all hooks.
# Source this file: . "$HOME/.claude/hooks/lib-eidolon.sh"
#
# URL resolution cascade aligned on upstream kleos-sh/main.rs:310-313:
#   KLEOS_SERVER_URL -> KLEOS_URL -> ENGRAM_EIDOLON_URL -> EIDOLON_URL -> default
#
# Key resolution cascade (no upstream reference, VOCSAP-defined):
#   KLEOS_API_KEY -> EIDOLON_API_KEY -> cred get eidolon -> ~/.config/eidolon/kleos-api-key.txt
#
# The legacy default localhost:7700 is dropped (the standalone "Eidolon" service
# no longer exists; routes are on kleos-server). The new default points at the
# upstream-canonical local kleos-server (127.0.0.1:4200).

_HOOK_HOME="${HOME:-${USERPROFILE:-.}}"
_CRED_SESSION_ENV="$_HOOK_HOME/.claude/session-env/cred-get-session.env"

if [ -f "$_CRED_SESSION_ENV" ]; then
  # SessionStart writes an export line here so later hooks can use `cred get`
  # without reopening that escape hatch for agent tool commands.
  . "$_CRED_SESSION_ENV" 2>/dev/null || true
fi

# URL cascade resolved once at source time.
_EIDOLON_URL="${KLEOS_SERVER_URL:-${KLEOS_URL:-${ENGRAM_EIDOLON_URL:-${EIDOLON_URL:-http://127.0.0.1:4200}}}}"
_EIDOLON_KEY=""

# Patch 20 (2026-05-22): default HTTP client timeout. Override via
# KLEOS_HTTP_LONGPOLL_TIMEOUT_SECS. Pair with server-side
# KLEOS_SUPERVISOR_LONGPOLL_TIMEOUT_SECS (default 30s) so the curl
# --max-time leaves the server room to return its graceful response.
_EIDOLON_DEFAULT_TIMEOUT="${KLEOS_HTTP_LONGPOLL_TIMEOUT_SECS:-60}"

# Patch 20: rate-limit back-off cache file. When eidolon_call observes an
# HTTP 429, it writes the unix-seconds deadline here. Subsequent calls skip
# the network round-trip while the window is active so the rate-limit
# bucket has time to drain. Path resolution mirrors kleos-cli/hook.rs.
_EIDOLON_BACKOFF_FILE="${XDG_CACHE_HOME:-${HOME:-${USERPROFILE:-/tmp}}/.cache}/kleos/eidolon-backoff-until.unix"

# Returns 0 (true) if a previous 429 still applies. Side effect: removes the
# file when the deadline has passed so future calls go through normally.
_eidolon_in_backoff() {
  [ -f "$_EIDOLON_BACKOFF_FILE" ] || return 1
  local until
  until="$(cat "$_EIDOLON_BACKOFF_FILE" 2>/dev/null | tr -dc '0-9')"
  [ -n "$until" ] || { rm -f "$_EIDOLON_BACKOFF_FILE" 2>/dev/null; return 1; }
  local now
  now="$(date +%s)"
  if [ "$now" -lt "$until" ]; then
    return 0
  fi
  rm -f "$_EIDOLON_BACKOFF_FILE" 2>/dev/null
  return 1
}

# Records a back-off deadline. Argument is seconds from now (parsed from the
# Retry-After header, falling back to 60).
_eidolon_record_backoff() {
  local secs="${1:-60}"
  local now until
  now="$(date +%s)"
  until=$((now + secs))
  mkdir -p "$(dirname "$_EIDOLON_BACKOFF_FILE")" 2>/dev/null || true
  printf '%s' "$until" > "$_EIDOLON_BACKOFF_FILE" 2>/dev/null || true
}

# Lazy-resolve Eidolon key (only calls cred once per hook invocation).
eidolon_key() {
  if [ -z "$_EIDOLON_KEY" ]; then
    if [ -n "${KLEOS_API_KEY:-}" ]; then
      _EIDOLON_KEY="$KLEOS_API_KEY"
    elif [ -n "${EIDOLON_API_KEY:-}" ]; then
      _EIDOLON_KEY="$EIDOLON_API_KEY"
    else
      _EIDOLON_KEY="$(cred get eidolon "${EIDOLON_CRED_KEY:-default}" --raw 2>/dev/null || echo '')"
      if [ -z "$_EIDOLON_KEY" ] && [ -f "$_HOOK_HOME/.config/eidolon/kleos-api-key.txt" ]; then
        _EIDOLON_KEY="$(tr -d '\r\n ' < "$_HOOK_HOME/.config/eidolon/kleos-api-key.txt" 2>/dev/null || echo '')"
      fi
    fi
  fi
  printf '%s' "$_EIDOLON_KEY"
}

# Call an Eidolon endpoint. Args: METHOD PATH JSON_BODY [TIMEOUT_SECS]
# Prints response body. Returns curl exit code.
#
# Patch 20 (2026-05-22):
#   - Default timeout switches from 5s to KLEOS_HTTP_LONGPOLL_TIMEOUT_SECS
#     (60s) so callers can use the server-side long-poll without aborting
#     the connection prematurely. Pass an explicit 4th argument to override.
#   - HTTP 429 responses are detected via -w "%{http_code}" and any
#     "retry-after:" header in the response. The deadline is persisted to
#     $_EIDOLON_BACKOFF_FILE and subsequent calls return early without
#     hitting the network until the window expires. Callers see an empty
#     body and exit status 1 in that case, same as before for a failed
#     curl, so existing hooks remain fail-open.
eidolon_call() {
  local method="$1" path="$2" body="${3:-}" timeout="${4:-$_EIDOLON_DEFAULT_TIMEOUT}"
  local key
  key="$(eidolon_key)"
  if [ -z "$key" ]; then
    echo '{"error":"no eidolon key"}'
    return 1
  fi
  if _eidolon_in_backoff; then
    # Caller-visible behavior: empty body + non-zero exit. Hooks already
    # treat a failed eidolon_call as a no-op so this is fail-open.
    return 1
  fi
  local headers_file
  headers_file="$(mktemp 2>/dev/null || echo "/tmp/eidolon-headers-$$")"
  local args=(
    -s --max-time "$timeout"
    -D "$headers_file"
    -w "%{http_code}"
    -o /dev/stdout
    -X "$method"
    -H "Authorization: Bearer $key"
    -H "Content-Type: application/json"
  )
  if [ -n "$body" ]; then
    args+=(-d "$body")
  fi
  local combined http_code rc
  combined="$(curl "${args[@]}" "${_EIDOLON_URL}${path}" 2>/dev/null)"
  rc=$?
  http_code="${combined: -3}"
  local body_out="${combined:0:${#combined}-3}"
  if [ "$http_code" = "429" ]; then
    local retry_after
    retry_after="$(grep -i '^retry-after:' "$headers_file" 2>/dev/null | tr -dc '0-9')"
    _eidolon_record_backoff "${retry_after:-60}"
    rm -f "$headers_file" 2>/dev/null
    return 1
  fi
  rm -f "$headers_file" 2>/dev/null
  # Match the original -sf semantics: success on 2xx, otherwise return 1.
  if [ "$rc" -ne 0 ]; then
    return "$rc"
  fi
  if [ "${http_code:0:1}" = "2" ]; then
    printf '%s' "$body_out"
    return 0
  fi
  return 1
}

# JSON-escape a string for embedding in JSON payloads
json_escape() {
  python3 -c "import sys,json; print(json.dumps(sys.argv[1]))" "$1" 2>/dev/null || printf '"%s"' "$1"
}
