#!/usr/bin/env bash
set -euo pipefail

readonly CRED_1_8_SHA256=85af63d5a56b27b0469a38ac12b8f941c88ad9a4d3128e791e505b33b458c266

: "${DEPLOYED_CRED_1_8:?set DEPLOYED_CRED_1_8 to the deployed Linux cred 1.8.0 binary}"
: "${OUT_DIR:?set OUT_DIR to Local-Firewall/config/openclaw/openclaw-config/bin}"

for command in cargo file ldd node python3 realpath sha256sum; do
  command -v "$command" >/dev/null || {
    printf 'missing required command: %s\n' "$command" >&2
    exit 78
  }
done

# Submodules are excluded: none of them is compiled into cred or kleos-credd.
tree_status() {
  git status --porcelain --ignore-submodules=all
}

repo_root=$(git rev-parse --show-toplevel) && status=$(tree_status) || {
  printf 'git status failed; run this script from the root of a Kleos worktree\n' >&2
  exit 78
}
[[ -z "$status" ]] || {
  printf 'working tree has uncommitted or untracked changes; build from a clean worktree:\n' >&2
  printf '  git worktree add --detach /tmp/kleos-clean <commit> && cd /tmp/kleos-clean\n' >&2
  exit 78
}
for config in "$repo_root/.cargo/config.toml" "$repo_root/.cargo/config"; do
  [[ ! -e "$config" ]] || {
    printf '%s can change the build outside git; remove it or build from a fresh worktree\n' "$config" >&2
    exit 78
  }
done

mkdir -p "$OUT_DIR"
out_dir=$(realpath -e "$OUT_DIR")
deployed_cred=$(realpath -e "$DEPLOYED_CRED_1_8") || {
  printf 'deployed cred 1.8.0 does not exist\n' >&2
  exit 78
}
out_cred=$(realpath -m "$out_dir/cred")
[[ "$deployed_cred" != "$out_cred" ]] || {
  printf 'deployed cred 1.8.0 must not be the delivery target\n' >&2
  exit 78
}

deployed_hash=$(sha256sum "$deployed_cred")
[[ ${deployed_hash%% *} == "$CRED_1_8_SHA256" ]] || {
  printf 'deployed cred 1.8.0 checksum does not match the pinned release\n' >&2
  exit 78
}

validate_binary() {
  local binary=$1
  file "$binary" | grep -q 'ELF 64-bit.*x86-64' || {
    printf '%s is not Linux x86_64\n' "$binary" >&2
    return 78
  }
}

ldd_dependencies() {
  local binary=$1
  local dependencies
  dependencies=$(ldd "$binary") || {
    printf 'ldd failed for %s\n' "$binary" >&2
    return 78
  }
  [[ "$dependencies" != *'not found'* ]] || {
    printf 'ldd reported a missing dependency for %s\n' "$binary" >&2
    return 78
  }
  [[ "$dependencies" == *'libc.so.6'* ]] || {
    printf '%s is not a glibc-linked binary\n' "$binary" >&2
    return 78
  }
  printf '%s' "$dependencies" | tr '\n' ';'
}

validate_binary "$deployed_cred"
ldd_dependencies "$deployed_cred" >/dev/null

metadata=$(cargo metadata --no-deps --format-version 1)
target_dir=$(printf '%s' "$metadata" | \
  python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')

cargo build --locked --release --target x86_64-unknown-linux-gnu \
  -p kleos-cred --bin cred
# Separate invocation so feature unification cannot re-enable piv.
cargo build --locked --release --target x86_64-unknown-linux-gnu \
  -p kleos-credd --bin kleos-credd --no-default-features

stage_dir=$(mktemp -d "$out_dir/.cred-broker-stage.XXXXXX")
trap 'rm -rf "$stage_dir"' EXIT
source_dir=$target_dir/x86_64-unknown-linux-gnu/release
install -m 0755 "$source_dir/cred" "$stage_dir/cred"
install -m 0755 "$source_dir/kleos-credd" "$stage_dir/kleos-credd"

validate_binary "$stage_dir/cred"
validate_binary "$stage_dir/kleos-credd"
cred_dependencies=$(ldd_dependencies "$stage_dir/cred")
credd_dependencies=$(ldd_dependencies "$stage_dir/kleos-credd")
[[ "$credd_dependencies" != *'libpcsclite'* ]] || {
  printf 'kleos-credd links libpcsclite; the broker image has no PC/SC library\n' >&2
  exit 78
}
cred_hash=$(sha256sum "$stage_dir/cred")
credd_hash=$(sha256sum "$stage_dir/kleos-credd")

commit=$(git rev-parse HEAD)
if [[ -z "$(tree_status)" ]]; then
  dirty=false
else
  dirty=true
fi
version=$(printf '%s' "$metadata" | \
  python3 -c 'import json, sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "kleos-credd"))')
{
  printf 'git_commit=%s\n' "$commit"
  printf 'git_dirty=%s\n' "$dirty"
  printf 'product_version=%s\n' "$version"
  printf 'broker_protocol=1\n'
  printf 'builder=%s\n' "$(uname -srmo)"
  printf 'toolchain=%s\n' "$(rustc --version)"
  printf 'target=x86_64-unknown-linux-gnu\n'
  printf 'dynamic_dependencies=cred:%s;kleos-credd:%s\n' "$cred_dependencies" "$credd_dependencies"
  printf 'sha256 %s  cred\n' "${cred_hash%% *}"
  printf 'sha256 %s  kleos-credd\n' "${credd_hash%% *}"
} > "$stage_dir/cred-broker-manifest.txt"

verifier="$(dirname "$out_dir")/verify-cred-broker-delivery.mjs"
[[ -f "$verifier" ]] || {
  printf 'missing delivery verifier: %s\n' "$verifier" >&2
  exit 78
}
node "$verifier" "$stage_dir/cred-broker-manifest.txt"

mv -f "$stage_dir/cred" "$out_dir/cred"
mv -f "$stage_dir/kleos-credd" "$out_dir/kleos-credd"
mv -f "$stage_dir/cred-broker-manifest.txt" "$out_dir/cred-broker-manifest.txt"
rmdir "$stage_dir"
trap - EXIT
printf 'wrote %s\n' "$out_dir/cred-broker-manifest.txt"
