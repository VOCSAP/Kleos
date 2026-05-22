#!/usr/bin/env bash
# PostToolUse hook: detects Bash error -> success pattern and prompts Claude
# to store the root cause in Kleos if non-obvious.
# Stdout JSON is injected as a system-reminder into the tool result Claude sees.

set -uo pipefail

resolve_home() {
  if [ -n "${HOME:-}" ]; then printf '%s\n' "$HOME"; return; fi
  if command -v cygpath >/dev/null 2>&1 && [ -n "${USERPROFILE:-}" ]; then
    cygpath -u "$USERPROFILE"; return
  fi
  printf '%s\n' "${USERPROFILE:-.}"
}

HOME_DIR="$(resolve_home)"
STATE_DIR="${AGENT_FORGE_STATE_DIR:-/tmp/agent-forge-state}"
LAST_ERROR_FILE="$STATE_DIR/last-bash-error"
mkdir -p "$STATE_DIR" 2>/dev/null || true

INPUT=$(cat 2>/dev/null || echo "{}")

TOOL_NAME=$(python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    print(d.get('tool_name', ''))
except:
    print('')
" <<< "$INPUT" 2>/dev/null || echo "")

[ "$TOOL_NAME" = "Bash" ] || exit 0

EXIT_CODE=$(python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    resp = d.get('tool_response', {})
    # Claude Code puts exit code in different places depending on version
    code = resp.get('exit_code', resp.get('exitCode', None))
    if code is None:
        # Fallback: look for 'Exit code N' in content string
        content = str(resp.get('content', ''))
        import re
        m = re.search(r'Exit code[: ]+(\d+)', content)
        print(m.group(1) if m else '0')
    else:
        print(code)
except:
    print('0')
" <<< "$INPUT" 2>/dev/null || echo "0")

if [ "$EXIT_CODE" != "0" ] && [ "$EXIT_CODE" != "" ]; then
    # Failed Bash: save context for next successful call
    python3 -c "
import sys, json
try:
    d = json.load(sys.stdin)
    cmd = d.get('tool_input', {}).get('command', '')[:400]
    resp = str(d.get('tool_response', ''))[:400]
    print(cmd + '\n---\n' + resp)
except:
    print('unknown error')
" <<< "$INPUT" > "$LAST_ERROR_FILE" 2>/dev/null || true
    exit 0
fi

# Successful Bash: check if a prior error was pending
[ -f "$LAST_ERROR_FILE" ] || exit 0

FAILED_CMD=$(head -1 "$LAST_ERROR_FILE" 2>/dev/null || echo "")
rm -f "$LAST_ERROR_FILE" 2>/dev/null || true

# Skip trivial cases: permission fixes, cd, ls, pwd, echo
if echo "$FAILED_CMD" | grep -qE '^\s*(ls|cd|pwd|echo|cat |head |tail |mkdir |chmod |chown )'; then
    exit 0
fi

# Inject reminder into Claude's context via the documented hookSpecificOutput
# envelope (same shape used by session-start-kleos.sh:396 and user-prompt-lean.sh:173).
# The legacy {"type":"text","text":...} shape this hook used before was never a
# recognised PostToolUse output schema -> Claude silently dropped the prompt.
python3 -c "
import json, sys
msg = (
    '[KLEOS-PROMPT] A Bash error was just resolved. '
    'If the root cause was non-obvious (not findable in 30s from the code/docs), '
    'store it now: kleos-cli store \"<root cause, precise, session-agnostic>\" '
    '-c discovery -i 8 -t \"<relevant,tags>\" -s \"claude-session\"'
)
print(json.dumps({
    'hookSpecificOutput': {
        'hookEventName': 'PostToolUse',
        'additionalContext': msg,
    }
}))
" 2>/dev/null || true

exit 0
