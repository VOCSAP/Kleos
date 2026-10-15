#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage: run_operator_gates.sh <new-bin-dir> <cred-1.8.0-bin>

Prerequisites on Debian or Ubuntu: bash, coreutils, util-linux, expect, openssl,
sqlite3, nodejs, and sudo. Node is required by broker_client_gate.sh.
USAGE
}

if [[ $# != 2 ]]; then
  usage
  exit 78
fi

for command in env expect install mktemp node openssl readlink realpath setpriv sha256sum sqlite3 stat sudo; do
  command -v "$command" >/dev/null || {
    printf 'missing required command: %s\n' "$command" >&2
    usage
    exit 78
  }
done

bin_dir=$(realpath -e "$1") || {
  printf 'new binary directory does not exist\n' >&2
  exit 78
}
cred_1_8_bin=$(realpath -e "$2") || {
  printf 'cred 1.8.0 binary does not exist\n' >&2
  exit 78
}
new_cred_bin="$bin_dir/cred"
new_credd_bin="$bin_dir/kleos-credd"
[[ -d "$bin_dir" && -x "$new_cred_bin" && -x "$new_credd_bin" && -x "$cred_1_8_bin" ]] || {
  printf 'all binary paths must be executable\n' >&2
  exit 78
}
[[ ! "$cred_1_8_bin" -ef "$new_cred_bin" ]] || {
  printf 'cred 1.8.0 binary must not share an inode with the new cred binary\n' >&2
  exit 78
}

script_dir=$(dirname "$(realpath -e "$0")")
root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT
chmod 711 "$root"
readonly root_path=/usr/sbin:/usr/bin:/sbin:/bin
readonly root_env=/usr/bin/env
readonly root_bash=/usr/bin/bash
readonly root_expect=/usr/bin/expect
readonly root_node=/usr/bin/node
readonly root_sudo=/usr/bin/sudo
[[ -x "$root_env" && -x "$root_bash" && -x "$root_expect" && -x "$root_node" && -x "$root_sudo" ]] || {
  printf 'required system binaries are unavailable\n' >&2
  exit 78
}
node_path=$(PATH="$root_path" command -v node)
[[ "$node_path" == "$root_node" ]] || {
  printf 'node does not resolve to %s under the system PATH\n' "$root_node" >&2
  exit 78
}
umask 077
isolated_home="$root/home"
isolated_config="$root/config"
install -d -m 755 "$isolated_home" "$isolated_config"

run_gate() {
  local name=$1
  local status
  shift
  if "$@"; then
    return 0
  else
    status=$?
  fi
  printf 'GATE FAILED: %s\n' "$name" >&2
  exit "$status"
}

run_gate 'vault-1.8' "$root_env" -i PATH="$root_path" \
  HOME="$isolated_home" \
  XDG_CONFIG_HOME="$isolated_config" \
  CRED_1_8_BIN="$cred_1_8_bin" \
  NEW_CRED_BIN="$new_cred_bin" \
  NEW_CREDD_BIN="$new_credd_bin" \
  "$root_bash" "$script_dir/broker_vault_1_8_gate.sh"

fixture_home="$root/home"
fixture_config="$fixture_home/.config"
keyfile="$root/master.key"
db_path="$fixture_config/cred/cred.db"
policy="$root/policy.toml"
mkdir -p "$fixture_config"
openssl rand -hex 32 > "$keyfile"
recovery_passphrase=$(openssl rand -hex 24)

if ! "$root_env" -i PATH="$root_path" TEST_HOME="$fixture_home" TEST_CONFIG="$fixture_config" \
  CRED_1_8_BIN="$cred_1_8_bin" RECOVERY_PASSPHRASE="$recovery_passphrase" \
  "$root_expect" <<'EXPECT'
set timeout 30
log_user 0
spawn /usr/bin/env -i PATH=$env(PATH) HOME=$env(TEST_HOME) XDG_CONFIG_HOME=$env(TEST_CONFIG) $env(CRED_1_8_BIN) init
expect {
  "Program slot 2" {
    send -- "n\r"
    exp_continue
  }
  "recovery passphrase:" {
    send -- "$env(RECOVERY_PASSPHRASE)\r"
  }
}
expect "confirm passphrase:"
send -- "$env(RECOVERY_PASSPHRASE)\r"
expect eof
EXPECT
then
  printf 'GATE FAILED: SO_PEERCRED\n' >&2
  exit 70
fi

[[ -f "$db_path" ]] || {
  printf 'GATE FAILED: SO_PEERCRED\n' >&2
  exit 70
}
cat > "$policy" <<'POLICY'
version = 1

[tools]
mail = ["smtp/api-key"]
POLICY
chmod 0644 "$policy"

printf 'running SO_PEERCRED gate via sudo\n'
run_gate 'SO_PEERCRED' "$root_sudo" "$root_env" -i PATH="$root_path" \
  HOME="$isolated_home" \
  XDG_CONFIG_HOME="$isolated_config" \
  BROKER_BIN="$new_credd_bin" \
  CLIENT_BIN="$new_cred_bin" \
  KEYFILE="$keyfile" \
  DB_PATH="$db_path" \
  POLICY="$policy" \
  "$root_bash" "$script_dir/broker_so_peercred_gate.sh"

run_gate 'client' "$root_env" -i PATH="$root_path" \
  HOME="$isolated_home" \
  XDG_CONFIG_HOME="$isolated_config" \
  CLIENT_BIN="$new_cred_bin" \
  "$root_bash" "$script_dir/broker_client_gate.sh"

printf 'ALL GATES PASSED\n'
