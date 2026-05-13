# kleos-sh -- Windows Port Analysis

**Date:** 2026-05-10 (initial)
**Updated:** 2026-05-13 (gate.rs subprocess curl fix, see local-patches.md Patch 9A)
**Author:** Claude (session analysis)
**Status:** Fix implemented, awaiting upstream PR

> **Scope :** This document covers only the original Unix-socket-vs-TCP split for
> `resolve_key_via_credd()` (Patches 3/4). The later gate.rs port (Patch 9A --
> reqwest replaced by curl subprocess, root cause traced to OPNSense + curl
> Windows `--local-port` behavior) is documented in
> `docs/dev-notes/local-patches.md` (Patch 9, section A). The full attempt log
> lives in the source comment at the top of `kleos-sh/src/gate.rs`.

---

## Problem

`kleos-sh` fails to compile on Windows (x86_64-pc-windows-msvc) with:

```
error[E0433]: failed to resolve: could not find `unix` in `os`
   --> kleos-sh\src\main.rs:137:18
    |
137 |     use std::os::unix::net::UnixStream;
    |                  ^^^^ could not find `unix` in `os`
```

The binary is otherwise fully cross-platform. The Unix socket is used in one
single function: `resolve_key_via_credd()`.

---

## Root Cause

`resolve_key_via_credd()` contacts `kleos-credd` via a Unix socket to bootstrap
a Kleos bearer token without needing the bearer stored in an env var. The Unix
socket path comes from `CREDD_SOCKET`.

The function is naturally guarded -- it returns `None` if `CREDD_SOCKET` is
not set -- so on Windows it would never be called at runtime even if it compiled.
The blocker is compile-time only.

A secondary minor issue: `read_hostname()` reads `/proc/sys/kernel/hostname`
(Linux-only path) before falling back to the `HOSTNAME` env var. This compiles
fine on Windows (it is just a file read that returns an error) but the fallback
chain does not include `COMPUTERNAME` (the Windows standard env var for
hostname).

---

## kleos-credd Transport Model

kleos-credd exposes two interfaces:
- **Unix socket** (Linux/macOS): path from `CREDD_SOCKET` env var
- **TCP HTTP** (all platforms): address from `CREDD_BIND` env var (default `127.0.0.1:4400`)

Both serve the same HTTP API. The Windows fix uses the TCP path, which is the
correct transport on non-Unix systems. The request structure (raw HTTP/1.1
over the stream) is identical in both branches -- only the connection type
differs (`UnixStream` vs `TcpStream`).

---

## Fix Applied

### Strategy: `#[cfg(unix)]` / `#[cfg(not(unix))]` branch

The original `resolve_key_via_credd()` function is split into:
- `resolve_key_via_credd_socket()` -- Unix only, unchanged logic
- `resolve_key_via_credd_tcp()` -- non-Unix (Windows/macOS without Unix sockets)

A dispatcher function `resolve_key_via_credd()` selects the right branch at
compile time. This keeps identical semantics on Unix (no regression) while
enabling compilation on Windows.

`read_hostname()` gains a `COMPUTERNAME` fallback for non-Unix targets.

### Env var mapping

| Platform | Connection type | Env var for address |
|----------|----------------|---------------------|
| Linux/macOS | Unix socket | `CREDD_SOCKET` |
| Windows | TCP | `CREDD_BIND` (default: `127.0.0.1:4400`) |

No changes to Cargo.toml -- no new dependencies. `std::net::TcpStream` is
used directly, mirroring how the Unix branch uses `std::os::unix::net::UnixStream`.

### Files modified

- `kleos-sh/src/main.rs` -- split `resolve_key_via_credd()` + `read_hostname()` fix

---

## Upstream PR Notes

This fix is safe to upstream to Ghost-Frame/Kleos. The rationale to include:

1. Zero behavioral change on Linux -- the Unix socket path is fully preserved
   under `#[cfg(unix)]`, byte-for-byte identical to the original.
2. Windows / macOS support: `CREDD_BIND` maps to the existing TCP listener
   that kleos-credd already exposes (`--listen` flag, default `127.0.0.1:4400`).
3. No new dependencies. `TcpStream` is stdlib.
4. The fallback chain in `resolve_key_via_credd()` is unchanged: env var
   `KLEOS_API_KEY` -> `EIDOLON_KEY` -> credd (socket or TCP) -> `cred` CLI.

Suggested PR title: `fix(kleos-sh): compile on Windows -- replace unix socket with cfg-gated TcpStream`

---

## Build Verification

```
# Windows (MSVC)
cargo build --release -p kleos-sh

# Linux (cross-check)
# Build via WSL or CI to confirm Unix branch still compiles
cargo build --release -p kleos-sh
```

Expected: both succeed, no warnings related to this change.

---

## What kleos-sh Does (context for future reference)

`kleos-sh` is a PreToolUse hook for Claude Code that gates shell commands
before execution. Flow:

1. Receives `{tool_name, tool_input.command}` on stdin (Claude Code JSON)
2. Resolves an API key (env > credd socket/TCP > cred CLI)
3. POSTs to kleos-server `/gate/check` endpoint
4. Emits `{hookSpecificOutput: {permissionDecision: "deny"|"allow", ...}}` on stdout
5. Exits 0 in hook mode (decision is in the JSON, not the exit code)

On Windows, step 2 will use TCP to contact kleos-credd if `CREDD_BIND` and
`CREDD_AGENT_KEY` are set. If not, it falls back to env vars directly.
