#!/usr/bin/env bash
# Patch 33 -- parity test bash vs Rust for the project-name resolution.
#
# Verifies that the bash helper at hooks/full/lib-kleos-space.sh and the
# Rust implementation at kleos-cli/src/space.rs agree on every test case.
# If they diverge, one was edited without the other -- treat as a CI bug
# and re-align before shipping.
#
# Requirements:
#   - `bash` and `kleos-cli` on PATH (or KLEOS_CLI env var pointing at the
#     release binary).
#   - GNU coreutils (`mktemp`, `rm`, `git`).
#
# Usage:
#   bash tests/space-resolution-parity.sh
#   KLEOS_CLI=target/release/kleos-cli.exe bash tests/space-resolution-parity.sh

set -uo pipefail

KLEOS_CLI="${KLEOS_CLI:-kleos-cli}"

# Source the repository helper unless KLEOS_SPACE_HELPER selects a deployed copy.
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LIB="${KLEOS_SPACE_HELPER:-$REPO_ROOT/hooks/full/lib-kleos-space.sh}"
if [ ! -f "$LIB" ]; then
    echo "FAIL: helper not found at $LIB" >&2
    exit 1
fi
# shellcheck disable=SC1090
. "$LIB"

if ! command -v "$KLEOS_CLI" >/dev/null 2>&1 && [ ! -x "$KLEOS_CLI" ]; then
    echo "SKIP: kleos-cli not on PATH and KLEOS_CLI not set to an executable" >&2
    exit 0
fi

PASS=0
FAIL=0
TMPROOT="$(mktemp -d -t kleos-parity.XXXXXX)"
trap 'rm -rf "$TMPROOT"' EXIT

assert_parity() {
    local label="$1"
    local dir="$2"
    local bash_out rust_out
    bash_out="$(cd "$dir" && resolve_project_name "$dir")"
    rust_out="$(cd "$dir" && "$KLEOS_CLI" space resolve 2>/dev/null)"
    if [ "$bash_out" = "$rust_out" ]; then
        PASS=$((PASS + 1))
        printf '  OK  %-50s bash=%-30s rust=%s\n' "$label" "$bash_out" "$rust_out"
    else
        FAIL=$((FAIL + 1))
        printf '  FAIL %-50s bash=%-30s rust=%s\n' "$label" "$bash_out" "$rust_out"
    fi
}

assert_space_setting() {
    local label="$1"
    local settings="$2"
    local expected="$3"
    local actual
    actual="$(jq -r '.env.KLEOS_SPACE // empty' "$settings")"
    if [ "$actual" = "$expected" ]; then
        PASS=$((PASS + 1))
        printf '  OK  %-50s settings=%s\n' "$label" "$settings"
    else
        FAIL=$((FAIL + 1))
        printf '  FAIL %-50s expected=%s actual=%s\n' "$label" "$expected" "${actual:-<empty>}"
    fi
}

echo "=== Patch 33 space-resolution parity (bash vs Rust) ==="

# Case 1 -- .git/ at the root, no marker.
mkdir -p "$TMPROOT/case1-git/.git"
assert_parity "git-basename" "$TMPROOT/case1-git"

# Case 2 -- .kleos-space at the root with a clean name.
mkdir -p "$TMPROOT/case2-marker"
printf 'kleos\n' > "$TMPROOT/case2-marker/.kleos-space"
assert_parity "marker-basic" "$TMPROOT/case2-marker"

# Case 3 -- no marker, no git -- basename fallback.
mkdir -p "$TMPROOT/case3-basename-only"
assert_parity "basename-fallback" "$TMPROOT/case3-basename-only"

# Case 4 -- marker with mixed case + spaces, must normalize.
mkdir -p "$TMPROOT/case4-marker-messy"
printf 'Kleos VOCSAP\n' > "$TMPROOT/case4-marker-messy/.kleos-space"
assert_parity "marker-normalize" "$TMPROOT/case4-marker-messy"

# Case 5 -- marker walk-up from a deep subdirectory.
mkdir -p "$TMPROOT/case5-walkup/sub/dir/deep"
printf 'walkup-test\n' > "$TMPROOT/case5-walkup/.kleos-space"
assert_parity "marker-walk-up" "$TMPROOT/case5-walkup/sub/dir/deep"

# Case 6 -- marker with leading comment + blank lines.
mkdir -p "$TMPROOT/case6-marker-comments"
{
    printf '# Project space for kleos\n'
    printf '\n'
    printf 'kleos\n'
} > "$TMPROOT/case6-marker-comments/.kleos-space"
assert_parity "marker-comments-and-blanks" "$TMPROOT/case6-marker-comments"

# Case 7 -- .git/ AND marker -- marker must win.
mkdir -p "$TMPROOT/case7-marker-wins/.git"
printf 'override\n' > "$TMPROOT/case7-marker-wins/.kleos-space"
assert_parity "marker-beats-git" "$TMPROOT/case7-marker-wins"

# Case 8 -- an existing local settings file receives the session space.
mkdir -p "$TMPROOT/case8-local-settings/.claude"
printf '{"permissions":{"allow":["Bash(git status)"]}}\n' \
    > "$TMPROOT/case8-local-settings/.claude/settings.local.json"
printf '{"env":{"KLEOS_SPACE":"tracked"}}\n' \
    > "$TMPROOT/case8-local-settings/.claude/settings.json"
write_kleos_space_to_settings "$TMPROOT/case8-local-settings" "local-space"
assert_space_setting "settings-local-preferred" \
    "$TMPROOT/case8-local-settings/.claude/settings.local.json" "local-space"
assert_space_setting "settings-json-preserved" \
    "$TMPROOT/case8-local-settings/.claude/settings.json" "tracked"

# Case 9 -- without a local settings file, the tracked settings file is used.
mkdir -p "$TMPROOT/case9-settings-fallback/.claude"
printf '{"permissions":{"allow":["Bash(git status)"]}}\n' \
    > "$TMPROOT/case9-settings-fallback/.claude/settings.json"
write_kleos_space_to_settings "$TMPROOT/case9-settings-fallback" "fallback-space"
assert_space_setting "settings-json-fallback" \
    "$TMPROOT/case9-settings-fallback/.claude/settings.json" "fallback-space"

echo
echo "=== Result: PASS=$PASS FAIL=$FAIL ==="
if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
exit 0
