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
eidolon_call() {
  local method="$1" path="$2" body="${3:-}" timeout="${4:-5}"
  local key
  key="$(eidolon_key)"
  if [ -z "$key" ]; then
    echo '{"error":"no eidolon key"}'
    return 1
  fi
  local args=(
    -sf --max-time "$timeout"
    -X "$method"
    -H "Authorization: Bearer $key"
    -H "Content-Type: application/json"
  )
  if [ -n "$body" ]; then
    args+=(-d "$body")
  fi
  curl "${args[@]}" "${_EIDOLON_URL}${path}" 2>/dev/null
}

# JSON-escape a string for embedding in JSON payloads
json_escape() {
  python3 -c "import sys,json; print(json.dumps(sys.argv[1]))" "$1" 2>/dev/null || printf '"%s"' "$1"
}
