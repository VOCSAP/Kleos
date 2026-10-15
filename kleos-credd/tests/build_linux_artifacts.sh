#!/usr/bin/env bash
set -euo pipefail

: "${DEPLOYED_CRED_1_8:?set DEPLOYED_CRED_1_8 to the deployed Linux cred 1.8.0 binary}"
: "${OUT_DIR:?set OUT_DIR to Local-Firewall/config/openclaw/openclaw-config/bin}"

command -v cargo >/dev/null || { printf 'missing cargo\n' >&2; exit 78; }
command -v file >/dev/null || { printf 'missing file\n' >&2; exit 78; }
command -v ldd >/dev/null || { printf 'missing ldd\n' >&2; exit 78; }
command -v sha256sum >/dev/null || { printf 'missing sha256sum\n' >&2; exit 78; }

file "$DEPLOYED_CRED_1_8" | grep -q 'ELF 64-bit.*x86-64' || {
  printf 'deployed cred 1.8.0 is not Linux x86_64\n' >&2
  exit 78
}
ldd "$DEPLOYED_CRED_1_8" | grep -q 'libc\.so\.6' || {
  printf 'deployed cred 1.8.0 is not a glibc-linked binary\n' >&2
  exit 78
}

cargo build --release --target x86_64-unknown-linux-gnu \
  -p kleos-cred --bin cred \
  -p kleos-credd --bin kleos-credd

source_dir=target/x86_64-unknown-linux-gnu/release
mkdir -p "$OUT_DIR"
install -m 0755 "$source_dir/cred" "$OUT_DIR/cred"
install -m 0755 "$source_dir/kleos-credd" "$OUT_DIR/kleos-credd"

commit=$(git rev-parse HEAD)
if git diff --quiet && git diff --cached --quiet; then
  dirty=false
else
  dirty=true
fi
version=$(cargo metadata --no-deps --format-version 1 | \
  python3 -c 'import json, sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "kleos-credd"))')
{
  printf 'git_commit=%s\n' "$commit"
  printf 'git_dirty=%s\n' "$dirty"
  printf 'product_version=%s\n' "$version"
  printf 'broker_protocol=1\n'
  printf 'builder=%s\n' "$(uname -srmo)"
  printf 'toolchain=%s\n' "$(rustc --version)"
  printf 'target=x86_64-unknown-linux-gnu\n'
  printf 'compatibility=glibc Debian bookworm, aligned after ldd DEPLOYED_CRED_1_8 check\n'
  printf 'dynamic_dependencies_cred:\n'
  ldd "$OUT_DIR/cred"
  printf 'dynamic_dependencies_kleos-credd:\n'
  ldd "$OUT_DIR/kleos-credd"
  printf 'sha256:\n'
  sha256sum "$OUT_DIR/cred" "$OUT_DIR/kleos-credd"
} > "$OUT_DIR/cred-broker-manifest.txt"

printf 'wrote %s\n' "$OUT_DIR/cred-broker-manifest.txt"
