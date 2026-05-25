#!/bin/bash
# Patch 33 -- shared helpers for the spaces partitioning convention.
# Sourced by hooks/full/session-start-kleos.sh and any future hook that
# needs to resolve the current project's space name from the filesystem.
#
# The resolution rule is the SAME one implemented in Rust by
# `kleos-cli space resolve` (kleos-cli/src/space.rs). A parity test
# (tests/space-resolution-parity.sh) keeps the two implementations in
# lock-step.
#
# Resolution order:
#   1. walk up from cwd looking for a `.kleos-space` marker -> use its
#      first non-comment, non-blank line.
#   2. walk up looking for a `.git/` directory -> use basename of its
#      parent.
#   3. fall back to basename of cwd.
#
# Names are normalized via `normalize_space_name` to keep them
# wire-stable: lowercase, trimmed, only `[a-z0-9_-]` retained.
#
# The marker file is IMMUTABLE once created (CONST VIOLATION rule by
# the operator). Hooks NEVER rewrite an existing `.kleos-space`.

# Normalize a free-form space name to the canonical form. Output goes
# to stdout. No newline issues -- consumed via `$(...)` capture.
normalize_space_name() {
    printf '%s' "$1" \
        | tr '[:upper:]' '[:lower:]' \
        | tr -d ' \t' \
        | sed 's/[^a-z0-9_-]//g'
}

# Walk up from `$1` (default $PWD) looking for `$2` (default `.kleos-space`).
# Echoes the path of the first match found, or empty string.
# Caps at 50 levels to defend against fs loops.
_find_walk_up() {
    local start="${1:-$PWD}"
    local needle="${2:-.kleos-space}"
    local cur
    cur="$(cd "$start" 2>/dev/null && pwd)"
    [ -z "$cur" ] && return 0
    local i=0
    while [ "$i" -lt 50 ]; do
        if [ -e "$cur/$needle" ]; then
            printf '%s' "$cur/$needle"
            return 0
        fi
        # Stop at filesystem root.
        local parent
        parent="$(dirname "$cur")"
        if [ "$parent" = "$cur" ]; then
            return 0
        fi
        cur="$parent"
        i=$((i + 1))
    done
    return 0
}

# Resolve the project's space name for cwd `$1` (default $PWD).
# Echoes the normalized name on stdout (always non-empty -- worst case
# falls back to basename of cwd).
resolve_project_name() {
    local cwd="${1:-$PWD}"
    local marker
    marker="$(_find_walk_up "$cwd" .kleos-space)"
    if [ -n "$marker" ]; then
        # First non-comment, non-blank line, trimmed.
        local raw
        raw="$(grep -v '^[[:space:]]*#' "$marker" 2>/dev/null \
               | grep -v '^[[:space:]]*$' \
               | head -1)"
        if [ -n "$raw" ]; then
            normalize_space_name "$raw"
            return 0
        fi
    fi
    # Try the .git walk-up.
    local git_dir
    git_dir="$(_find_walk_up "$cwd" .git)"
    if [ -n "$git_dir" ]; then
        local repo_root
        repo_root="$(dirname "$git_dir")"
        normalize_space_name "$(basename "$repo_root")"
        return 0
    fi
    # Last resort: basename of cwd.
    normalize_space_name "$(basename "$cwd")"
}

# Ensure the `.kleos-space` marker exists at `$1` (default: git repo root
# of $PWD, falling back to $PWD). NEVER overwrites an existing file --
# the marker is immutable once written.
# Echoes the resolved name on stdout for caller convenience.
ensure_kleos_space_marker() {
    local cwd="${1:-$PWD}"
    local name
    name="$(resolve_project_name "$cwd")"
    [ -z "$name" ] && return 0

    # Find a sensible location for the marker: git repo root if any,
    # else $cwd itself.
    local repo_root
    repo_root="$(cd "$cwd" 2>/dev/null && git rev-parse --show-toplevel 2>/dev/null)"
    local target_dir="${repo_root:-$cwd}"
    local target="$target_dir/.kleos-space"
    if [ ! -f "$target" ]; then
        # Best-effort write; do not fail the caller on permission errors.
        printf '%s\n' "$name" > "$target" 2>/dev/null || true
    fi
    printf '%s' "$name"
}

# Merge KLEOS_SPACE into the project's .claude/settings.json `env` map.
# `$1` = project root (settings.json lives at $1/.claude/settings.json).
# `$2` = space name. Skips silently when jq is unavailable.
write_kleos_space_to_settings() {
    local project_root="$1"
    local space="$2"
    [ -z "$project_root" ] && return 0
    [ -z "$space" ] && return 0
    command -v jq >/dev/null 2>&1 || return 0

    local settings="$project_root/.claude/settings.json"
    mkdir -p "$(dirname "$settings")" 2>/dev/null || return 0
    if [ -f "$settings" ]; then
        local tmp="$settings.tmp.$$"
        if jq --arg s "$space" '.env = ((.env // {}) | .KLEOS_SPACE = $s)' \
               "$settings" > "$tmp" 2>/dev/null; then
            mv "$tmp" "$settings"
        else
            rm -f "$tmp" 2>/dev/null || true
        fi
    else
        # Create a minimal settings.json with just the env block.
        printf '{\n  "env": {\n    "KLEOS_SPACE": "%s"\n  }\n}\n' \
               "$space" > "$settings" 2>/dev/null || true
    fi
}

# Persist the resolved space under the per-session file consumed by
# kleos-mcp (so MCP tool calls without an explicit `space` payload can
# auto-inject it). Cheap, always re-written each session.
write_kleos_space_per_sid() {
    local space="$1"
    local sid="${CLAUDE_SESSION_ID:-${CLAUDE_CODE_SESSION_ID:-}}"
    [ -z "$space" ] && return 0
    [ -z "$sid" ] && return 0
    local dir="$HOME/.kleos/sessions/$sid"
    mkdir -p "$dir" 2>/dev/null || return 0
    printf '%s\n' "$space" > "$dir/space_name" 2>/dev/null || true
}
