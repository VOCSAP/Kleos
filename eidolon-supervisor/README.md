# eidolon-supervisor

Async drift supervisor for AI agents. Watches Claude Code session logs, detects
violations of behavioral rules, and forwards alerts to a `kleos-server` instance
on three channels: persistent inbox (audit), event bus (monitoring), and
session-scoped injection queue (inline correction).

This README is a quick reference for VOCSAP operators. For deeper coverage
(architecture, env vars, deployment plan), see
`docs/dev-notes/eidolon-supervisor-usage-guide-todo.md` (gitignored).

---

## What it does

1. Watches `~/.claude/projects/**/*.jsonl` (or `$CLAUDE_SESSIONS_DIR`) for new
   tool invocations recorded by Claude Code.
2. For each new `tool_use` JSONL entry, runs configurable rules:
   - **rule_match** -- regex applied to the aggregated text (Bash command,
     Write/Edit content, assistant output, commit message).
   - **retry_loop** -- detects 3+ identical Bash commands in a row.
3. On violation: POSTs to `kleos-server` on three endpoints (see below). A
   per-rule cooldown prevents alert spam.

It is a **passive observer**: violations are reported but not blocked
in-process. Blocking the agent requires a Claude Code hook that drains
`GET /supervisor/pending` and decides what to do with the violations.

---

## Environment variables

| Variable | Default | Role |
|---|---|---|
| `CLAUDE_SESSIONS_DIR` | `$HOME/.claude/projects` | Root directory watched recursively |
| `KLEOS_SERVER_URL` | `http://127.0.0.1:4200` | Target `kleos-server` for alerts |
| `KLEOS_API_KEY` | (none) | Bearer token; required by `/supervisor/inject` |
| `EIDOLON_SUPERVISOR_CONFIG` | `$HOME/.config/eidolon/supervisor.json` | Override path for rules JSON |
| `EIDOLON_SUPERVISOR_MAX_TRACKED_FILES` | `2048` | LRU cap on read positions per file |
| `RUST_LOG` | `info` | Tracing verbosity |
| `HOME` | (none) | Required on Windows for default paths; binary does NOT read `USERPROFILE` |

Legacy aliases (kept for upstream compatibility): `ENGRAM_EIDOLON_URL`
(= `KLEOS_SERVER_URL`), `EIDOLON_KEY` (= `KLEOS_API_KEY`).

**Windows pitfall**: `HOME` is not set by default. Always export
`CLAUDE_SESSIONS_DIR` explicitly when running as a Scheduled Task.

---

## Default rules

Loaded from `~/.config/eidolon/supervisor.json` if present, otherwise the
hard-coded defaults below (see `src/checks/mod.rs:default_rules`).

| ID | Type | Severity | Cooldown | What it detects |
|---|---|---|---|---|
| `no-force-push` | rule_match | Critical | 300s | `git push ... --force` (any form) |
| `no-reboot` | rule_match | Critical | 600s | `reboot`, `shutdown`, `systemctl reboot/poweroff` |
| `retry-loop` | retry_loop | Warning | 120s | 3+ identical Bash commands consecutively |
| `em-dash-usage` | rule_match | Info | 60s | Em-dash character `—` in any output |

Severity mapping for the `/inbox` channel: `Info=3`, `Warning=6`, `Critical=9`
(used as the `importance` field on the stored memory).

### Custom rules (recommended VOCSAP set)

Drop this into `~/.config/eidolon/supervisor.json` to replace the defaults
with a broader, VOCSAP-aligned set:

```json
[
  {
    "id": "no-force-push",
    "check_type": "rule_match",
    "pattern": "git\\s+push\\s+.*--force(?!-with-lease)",
    "severity": "critical",
    "cooldown_secs": 300,
    "message": "Force push detected (use --force-with-lease)"
  },
  {
    "id": "no-reboot",
    "check_type": "rule_match",
    "pattern": "reboot|shutdown|systemctl\\s+(reboot|poweroff)",
    "severity": "critical",
    "cooldown_secs": 600,
    "message": "Reboot/shutdown command"
  },
  {
    "id": "no-skip-hooks",
    "check_type": "rule_match",
    "pattern": "--no-verify|--no-gpg-sign",
    "severity": "critical",
    "cooldown_secs": 300,
    "message": "Skipping git hooks/signing"
  },
  {
    "id": "em-dash-vocsap",
    "check_type": "rule_match",
    "pattern": "—",
    "severity": "warning",
    "cooldown_secs": 60,
    "message": "Em dash detected (VOCSAP rule: use -- instead)"
  },
  {
    "id": "retry-loop",
    "check_type": "retry_loop",
    "pattern": "",
    "severity": "warning",
    "cooldown_secs": 120,
    "message": "Retry loop (3+ identical commands)"
  },
  {
    "id": "no-rm-rf-home",
    "check_type": "rule_match",
    "pattern": "rm\\s+-rf\\s+[~$/]",
    "severity": "critical",
    "cooldown_secs": 600,
    "message": "rm -rf targeting home/root detected"
  },
  {
    "id": "no-git-reset-hard-main",
    "check_type": "rule_match",
    "pattern": "git\\s+reset\\s+--hard.*\\b(main|master)\\b",
    "severity": "critical",
    "cooldown_secs": 300,
    "message": "git reset --hard on main/master"
  },
  {
    "id": "no-uall",
    "check_type": "rule_match",
    "pattern": "git\\s+status\\s+.*-uall",
    "severity": "warning",
    "cooldown_secs": 300,
    "message": "git status -uall flag (forbidden, memory issues on large repos)"
  },
  {
    "id": "no-short-f-push",
    "check_type": "rule_match",
    "pattern": "git\\s+(push|rebase)\\s+.*\\s-f\\b",
    "severity": "warning",
    "cooldown_secs": 300,
    "message": "Short -f flag on push/rebase (use --force-with-lease)"
  }
]
```

Notes:

- Rules are read **once at startup**. Edit the JSON then restart the daemon.
- If the JSON is malformed, the supervisor logs a warning and falls back to
  the hard-coded defaults (no crash).
- The aggregated text checked by `rule_match` includes Bash commands,
  Write/Edit content, assistant text, and commit messages -- so a pattern
  like `em-dash-vocsap` catches em-dashes regardless of where they appear.
- `retry_loop` ignores the `pattern` field; it tracks Bash command
  repetition through a fixed-size in-memory queue (10 slots).

### Severity guide

| Severity | When to use | Effect |
|---|---|---|
| `info` | Style/policy nudges with no operational risk (em-dash, cosmetic) | Importance 3 in `/inbox`; mostly for audit. |
| `warning` | Behavioral patterns that may indicate drift or inefficiency (retry loop, short flag) | Importance 6 in `/inbox`; worth reviewing. |
| `critical` | Actions with hard-to-reverse or destructive consequences (force push, reboot, rm -rf) | Importance 9 in `/inbox`; should trigger inline correction once the drain hook is in place. |

---

## How alerts reach `kleos-server`

On each violation past cooldown, the supervisor POSTs to three endpoints
**in sequence** (not in parallel). All three are best-effort: a failure on
one does not stop the others.

1. `POST /supervisor/inject` -- only if `sessionId` was present in the JSONL
   line. Persists in the `supervisor_injections` table, keyed by
   `(user_id, session_id)`, and waits to be drained by the agent via
   `GET /supervisor/pending`.
2. `POST /inbox` -- persists a memory of category `alert` with the
   `eidolon-supervisor` source tag. Always sent.
3. `POST /axon/publish` -- fire-and-forget event on topic
   `eidolon:supervisor`, event type `violation`. Always sent.

All requests use Bearer auth if `KLEOS_API_KEY` is set. Read timeout 10s,
connect timeout 5s.

---

## Deployment

### Linux (systemd)

```bash
cargo install --path eidolon-supervisor
cp dist/eidolon-supervisor.service ~/.config/systemd/user/
mkdir -p ~/.config/eidolon
cat > ~/.config/eidolon/supervisor.env <<EOF
KLEOS_SERVER_URL=http://192.168.10.21:4200
KLEOS_API_KEY=<your kleos_* key>
RUST_LOG=info
EOF
chmod 600 ~/.config/eidolon/supervisor.env
systemctl --user daemon-reload
systemctl --user enable --now eidolon-supervisor.service
```

### Windows (Scheduled Task)

A registration script lives outside the repo at
`C:\Users\Olivier\workspace\claude-experiment\register-eidolon-task.ps1` for
the VOCSAP operator. Pre-requisites:

- Binary at `%USERPROFILE%\.cargo\bin\eidolon-supervisor.exe` (build via
  `cargo install --path eidolon-supervisor`).
- API key on a single line at
  `%USERPROFILE%\.config\eidolon\kleos-api-key.txt`.
- User-scope env vars `CLAUDE_SESSIONS_DIR` and
  `EIDOLON_SUPERVISOR_CONFIG` set via System Properties or `setx`.

Run the script once (no admin required for a user-scope AtLogon task). The
SessionStart hook `hooks/full/session-start-kleos.sh` will then ensure the
task is running each time a Claude Code session starts.

### Local interactive run (debugging)

```bash
RUST_LOG=debug \
KLEOS_SERVER_URL=http://192.168.10.21:4200 \
KLEOS_API_KEY=<key> \
CLAUDE_SESSIONS_DIR="$HOME/.claude/projects" \
./target/release/eidolon-supervisor
```

---

## Operational checks

```bash
# See how many violations have been injected and how many are still unclaimed
ssh root@192.168.10.21 \
  "sqlite3 /var/lib/kleos/tenants/2/kleos.db \
   'SELECT count(*) AS total, sum(claimed_at IS NULL) AS unclaimed FROM supervisor_injections;'"

# Recent alerts in the inbox channel
kleos-cli search "eidolon-supervisor" --limit 10

# Subscribe to the live event bus (axon)
curl -sN -H "Authorization: Bearer $KLEOS_API_KEY" \
  "http://192.168.10.21:4200/axon/events?topic=eidolon:supervisor&limit=20"
```

---

## What is **not** implemented

These checks have placeholder stubs marked `#[allow(dead_code)]` and are no-op:

- `drift::check_promise_action` -- promise vs action drift detection (would
  need two-turn assistant-text tracking).
- `scope::check_file_scope` -- file path allow-list for Write/Edit
  (logic exists, just not wired into `watch.rs`).

Hook integration:

- The drain hook for `/supervisor/pending` is
  `hooks/full/eidolon-supervisor-drain-pending.sh`. It must be registered
  as a PreToolUse hook in `~/.claude/settings.json` to close the
  supervisor-to-agent feedback loop.

---

## Related files in this repo

- Source: `eidolon-supervisor/src/{main,watch,alert,checks/*}.rs` (~760 LOC total).
- Server-side handlers: `kleos-server/src/routes/supervisor/mod.rs`.
- SessionStart hook ensuring daemon liveness: `hooks/full/session-start-kleos.sh`.
- PreToolUse drain hook: `hooks/full/eidolon-supervisor-drain-pending.sh`.
- Systemd unit: `dist/eidolon-supervisor.service`.
- Windows task XML (reference, the operator uses the PS1 script instead):
  `dist/eidolon-supervisor.task.xml`.
- Full reference guide (gitignored): `docs/dev-notes/eidolon-supervisor-usage-guide-todo.md`.
