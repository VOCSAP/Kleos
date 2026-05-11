# Windows Port -- Source Changes Registry

**Date:** 2026-05-10
**Context:** Building Kleos client binaries for Windows (x86_64-pc-windows-msvc).
**Purpose:** Track every source modification made to enable Windows compilation,
so that after a future `git pull` from upstream (Ghost-Frame/Kleos) the changes
can be re-applied with awareness of intent vs. new upstream code.

---

## How to use this document after a pull

For each entry:
1. Check if upstream has already fixed the issue (intent matches new code).
2. If not, re-apply the fix as described.
3. If upstream has a different fix, evaluate whether it covers the same case.

The "merge conflict risk" column indicates how likely the upstream file is to
have changed in a way that conflicts.

---

## Change 1 -- agent-forge/Cargo.toml

**File:** `agent-forge/Cargo.toml`
**Merge conflict risk:** Low (Cargo.toml changes are usually additive)

### Problem

`agent-forge` uses `rusqlite = { workspace = true }` which resolves to
`rusqlite = "0.31"` (no features). On Linux, the system SQLite is found via
`pkg-config`. On Windows there is no system `sqlite3.lib`, so linking fails:

```
LINK : fatal error LNK1181: cannot open input file 'sqlite3.lib'
```

### Fix applied

Added a Windows-specific dependency block that enables the `bundled` feature,
which compiles SQLite from source and embeds it in the binary:

```toml
# Windows needs bundled SQLite since there's no system sqlite3.lib
[target.'cfg(windows)'.dependencies]
rusqlite = { version = "0.31", features = ["bundled"] }
```

### Why `bundled` and not `bundled-sqlcipher`

`agent-forge` stores structured reasoning logs (specs, hypotheses, verifications).
These are developer workflow data, not secrets. Plain SQLite is appropriate.
`bundled-sqlcipher` would add OpenSSL linking complexity with no security benefit
for this use case.

### Upstream PR note

The workspace `rusqlite` dep should gain a Windows target override. The fix
in `agent-forge/Cargo.toml` is correct as-is. Alternatively, upstream could
add a workspace-level `[target.'cfg(windows)'.dependencies]` for rusqlite.

---

## Change 2 -- kleos-sidecar/Cargo.toml

**File:** `kleos-sidecar/Cargo.toml`
**Merge conflict risk:** Low

### Problem

`kleos-sidecar` depends on `kleos-lib` without the `sqlcipher` feature. On
Windows, kleos-lib uses SQLCipher for database encryption, which requires
bundled compilation (no system `sqlite3.lib`). Without the feature flag,
linking fails with the same `sqlite3.lib` error as Change 1.

### Fix applied

Added a Windows-specific dependency block mirroring the pattern already used
in `kleos-cli`, `kleos-mcp`, and `kleos-cred`:

```toml
# Windows needs bundled SQLCipher since there's no system sqlite3.lib
[target.'cfg(windows)'.dependencies]
kleos-lib = { path = "../kleos-lib", version = "1.0.0", features = ["sqlcipher"] }
```

### Upstream PR note

This is an oversight in the original code. `kleos-sidecar` was the only
kleos-lib dependent that lacked the Windows SQLCipher override. Safe to
upstream as-is.

---

## Change 3 -- kleos-sh/src/main.rs

**File:** `kleos-sh/src/main.rs`
**Merge conflict risk:** Medium (main.rs may evolve with new features)

### Problem

Two functions used Unix-specific stdlib items that do not compile on Windows:

**A. `resolve_key_via_credd()`** -- used `std::os::unix::net::UnixStream` to
contact `kleos-credd` via a Unix domain socket. This is a compile-time error
on Windows even though the function would never be called at runtime (it is
guarded by `CREDD_SOCKET` env var which is never set on Windows).

**B. `read_hostname()`** -- reads `/proc/sys/kernel/hostname` (Linux-only path).
Compiles fine on Windows (file read returns an error), but the fallback chain
did not include `COMPUTERNAME`, the standard Windows hostname env var.

### Fix applied

**A.** Split `resolve_key_via_credd()` into two cfg-gated implementations:

```
resolve_key_via_credd()           -- dispatcher, selects at compile time
  #[cfg(unix)]     resolve_key_via_credd_socket()  -- original UnixStream code
  #[cfg(not(unix))] resolve_key_via_credd_tcp()    -- new TcpStream code
```

The TCP implementation connects to `CREDD_BIND` (default `127.0.0.1:4400`),
which is the existing HTTP listener that kleos-credd already exposes on all
platforms. The HTTP request structure is **identical** to the Unix socket
branch -- only the connection type changes (`UnixStream` -> `TcpStream`).
No new dependencies. `std::net::TcpStream` is stdlib.

```
Unix:    CREDD_SOCKET env var    -> Unix domain socket
Windows: CREDD_BIND env var      -> TCP 127.0.0.1:4400 (default)
```

**B.** `read_hostname()`: the `/proc/sys/kernel/hostname` read is now behind
`#[cfg(unix)]`. Added `COMPUTERNAME` as a Windows-specific fallback:

```rust
#[cfg(unix)]
if let Ok(h) = std::fs::read_to_string("/proc/sys/kernel/hostname") { ... }
// HOSTNAME already present (works on some Windows setups)
#[cfg(not(unix))]
if let Ok(h) = std::env::var("COMPUTERNAME") { ... }  // Windows standard
```

### Upstream PR note

This change is safe and complete for upstreaming to Ghost-Frame/Kleos:
- Zero behavioral change on Linux (Unix socket path byte-for-byte preserved)
- Windows gains a functional credd integration via TCP
- No new dependencies
- `COMPUTERNAME` is a well-known Windows standard, not a workaround

Suggested PR title: `fix(kleos-sh): compile on Windows -- cfg-gate Unix socket, add TCP fallback for credd`

Full analysis: `docs/dev-notes/kleos-sh-windows-port.md`

---

## Change 4 -- kleos-cred/src/bin/derive-db-key.rs

**File:** `kleos-cred/src/bin/derive-db-key.rs`
**Merge conflict risk:** Low (small utility binary, unlikely to change often)

### Problem

`derive-db-key` is a utility that generates a SQLCipher database key from a
YubiKey HMAC-SHA1 response and writes it to a file. It uses two Unix-specific
items at the top level (not gated), causing compile errors on Windows:

1. `use std::os::unix::fs::OpenOptionsExt;` -- top-level import
2. `.mode(0o600)` -- called on `OpenOptions` to create the file owner-only

All other `PermissionsExt`/`OpenOptionsExt` usages in kleos-cred are ALREADY
correctly gated behind `#[cfg(unix)]` blocks. This file is the only exception.

### Fix applied

Moved the `use` statement inside a `#[cfg(unix)]` block, and wrapped the
`OpenOptions` construction so `.mode(0o600)` is only applied on Unix.
On Windows, the file is created without a mode flag (inherits ACL from the
parent directory, which is the user's profile -- already private).

```rust
// Before: top-level import (compile error on Windows)
use std::os::unix::fs::OpenOptionsExt;

// After: build OpenOptions, conditionally set mode
let mut opts = std::fs::OpenOptions::new();
opts.write(true).create(true).truncate(true);
#[cfg(unix)]
{
    use std::os::unix::fs::OpenOptionsExt;
    opts.mode(0o600);
}
let mut f = opts.open(path).unwrap_or_else(|e| { ... });
```

### Why only this file needed fixing

The other Unix permission calls in kleos-cred were already gated:
- `agent_keys_file.rs` lines 110-115: `#[cfg(unix)]` block present
- `agent_keys_file.rs` lines 377-398: `#[cfg(unix)] #[test]` present
- `yubikey.rs` lines 299-304: `#[cfg(unix)]` block present
- `bin/cred.rs` line 501-505: `#[cfg(unix)]` block present
- `bin/cred.rs` lines 765-769: `#[cfg(unix)]` block present
- `bin/cred.rs` lines 867-871: `#[cfg(unix)]` block present
- `bin/cred.rs` lines 2476-2486: `#[cfg(unix)]` + `#[cfg(not(unix))]` present
- `bin/cred.rs` lines 2767-2772: `#[cfg(unix)]` block present

### Windows security note

On Windows, file-level access control uses ACLs, not Unix mode bits. Files
created in `%APPDATA%\kleos\` are already inaccessible to other users by
default (Windows ACL inheritance from APPDATA). The 0o600 behavior is
therefore replicated at the OS level without any additional code.

### Upstream PR note

Small, isolated fix. Safe to upstream. The pattern is consistent with how
the rest of kleos-cred already handles this (all other sites are gated).

---

## Summary table

| File | Type | Problem | Fix |
|------|------|---------|-----|
| `agent-forge/Cargo.toml` | Config | No system sqlite3.lib on Windows | `[target.cfg(windows)] rusqlite bundled` |
| `kleos-sidecar/Cargo.toml` | Config | Missing sqlcipher feature for Windows | `[target.cfg(windows)] kleos-lib sqlcipher` |
| `kleos-sh/src/main.rs` | Code | `UnixStream` compile error + missing COMPUTERNAME | cfg-gated socket/TCP split + hostname fix |
| `kleos-cred/src/bin/derive-db-key.rs` | Code | Top-level `OpenOptionsExt` import ungated | Move use + `.mode()` inside `#[cfg(unix)]` |

---

## Binaries NOT ported to Windows

| Binary | Reason |
|--------|--------|
| `kleos-mcp` | Deployed server-side (Debian LXC) next to the SQLCipher DB |
| `kleos-sh` | Ported (see Change 3) |
| `kleos-credd` | Server-side daemon; kleos-cred (client CLI) is the Windows counterpart |

## Binaries confirmed Linux-only (not attempted)

| Binary | Reason |
|--------|--------|
| `kleos-ingest` | Heavy PIV + file-watching; server-side use case |
| `kleos-migrate` | Admin tool, run once on server |
| `kleos-cleanup` | Admin tool, run on server |

---

## Notes on upstream ECDH dead code warnings

`kleos-lib/src/cred/bootstrap.rs` emits 10 `dead_code` warnings for ECDH-P256
bootstrap functions (`resolve_via_ecdh`, `unix_post`, `tcp_post`, etc.).
These implement a planned credential bootstrapping protocol via PIV smartcard,
but no CLI command or API endpoint currently calls them. They are dormant code,
not a bug. Binary size impact: negligible. Runtime impact: zero.
Upstream tracking: consider filing an issue or adding `#[allow(dead_code)]`
with a comment pointing to the planned integration ticket.
