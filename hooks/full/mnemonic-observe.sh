#!/bin/bash
# PostToolUse hook: notify kleos-sidecar (Rust) of tool use (fire-and-forget)
# Reads tool_name + tool_input from stdin JSON, sends to kleos-sidecar /observe.
# Returns empty (no hookSpecificOutput) to avoid context noise.
#
# The Rust kleos-sidecar replaces the legacy Node.js mnemonic sidecar.
# It handles observe, recall, compress, and auto-capture in one binary.
#
# NOTE: Use absolute paths, not $HOME -- PostToolUse hooks don't expand $HOME on Windows.

# URL cascade aligned with VOCSAP convention (kleos-sidecar Rust binary).
# KLEOS_SIDECAR_URL is the canonical project var; ENGRAM_SIDECAR_URL kept as
# legacy fallback for compat with upstream Ghost-Frame deployments.
SIDECAR_URL="${KLEOS_SIDECAR_URL:-${ENGRAM_SIDECAR_URL:-http://127.0.0.1:7711}}"
SIDECAR_TOKEN="${KLEOS_SIDECAR_TOKEN:-${ENGRAM_SIDECAR_TOKEN:-}}"

# Check if sidecar is running; do NOT auto-start here (sidecar should be
# launched by session-start hook or systemd/launchd/process manager).
if ! curl -sf --max-time 1 "$SIDECAR_URL/health" >/dev/null 2>&1; then
  exit 0
fi

# Single python3 invocation: read stdin directly, extract fields, fire curl.
# Never capture stdin in a shell variable -- tool_response can be megabytes.
SIDECAR_URL="$SIDECAR_URL" SIDECAR_TOKEN="$SIDECAR_TOKEN" python3 -c "
import os, sys, json, subprocess

try:
    data = json.load(sys.stdin)
except:
    sys.exit(0)

tool = data.get('tool_name', 'unknown')
inp = data.get('tool_input', {})

if isinstance(inp, str):
    summary = inp[:200]
elif isinstance(inp, dict):
    summary = str(
        inp.get('command',
        inp.get('file_path',
        inp.get('filePath',
        inp.get('description',
        inp.get('prompt', '')))))
    )[:200]
else:
    summary = ''

# session_id propagation: read from the PostToolUse event stdin and inject so
# the sidecar routes /observe to the correct Claude session bucket instead of
# its `default_session_id`. SessionStart hook (sidecar-session-start.sh) declares
# this session_id to the sidecar via POST /session/start beforehand.
sid = data.get('session_id', data.get('sessionId', '')) or ''

payload_dict = {'tool': tool, 'tool_name': tool, 'summary': summary, 'content': summary}
if sid:
    payload_dict['session_id'] = sid
payload = json.dumps(payload_dict)

sidecar_url = os.environ.get('SIDECAR_URL', 'http://127.0.0.1:7711')
sidecar_token = os.environ.get('SIDECAR_TOKEN', '')
curl_args = ['curl', '-sf', '--max-time', '2', sidecar_url + '/observe',
             '-X', 'POST', '-H', 'Content-Type: application/json']
if sidecar_token:
    curl_args += ['-H', 'Authorization: Bearer ' + sidecar_token]
curl_args += ['-d', payload]

try:
    subprocess.Popen(
        curl_args,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
    )
except:
    pass
" 2>/dev/null

exit 0
