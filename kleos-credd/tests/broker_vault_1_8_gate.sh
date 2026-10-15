#!/usr/bin/env bash
set -euo pipefail

: "${CRED_1_8_BIN:?set CRED_1_8_BIN to the Linux cred v1.8.0 binary}"
: "${NEW_CRED_BIN:?set NEW_CRED_BIN to the new Linux cred binary}"
: "${NEW_CREDD_BIN:?set NEW_CREDD_BIN to the new Linux kleos-credd binary}"

readonly CRED_1_8_SHA256=85af63d5a56b27b0469a38ac12b8f941c88ad9a4d3128e791e505b33b458c266

for command in expect openssl sha256sum sqlite3; do
  command -v "$command" >/dev/null || {
    printf 'missing required command: %s\n' "$command" >&2
    exit 78
  }
done

if [[ $(id -u) != $(id -g) ]]; then
  printf 'this gate requires an equal UID and GID for the single-user fixture\n' >&2
  exit 78
fi

root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT
export TEST_HOME="$root/home"
export TEST_CONFIG="$TEST_HOME/.config"
export KEYFILE="$root/master.key"
export SOCKET="$root/cred.sock"
export DB_PATH="$TEST_CONFIG/cred/cred.db"
export POLICY="$(dirname "$0")/fixtures/broker/policy.toml"
export EXTENDED_POLICY="$(dirname "$0")/fixtures/broker/policy-extended.toml"
mkdir -p "$TEST_CONFIG"
umask 077
openssl rand -hex 32 > "$KEYFILE"
export RECOVERY_PASSPHRASE
RECOVERY_PASSPHRASE=$(openssl rand -hex 24)

cred_1_8_hash=$(sha256sum "$CRED_1_8_BIN")
[[ ${cred_1_8_hash%% *} == "$CRED_1_8_SHA256" ]] || {
  printf 'cred 1.8.0 checksum does not match the pinned release\n' >&2
  exit 78
}
[[ ! "$CRED_1_8_BIN" -ef "$NEW_CRED_BIN" ]] || {
  printf 'cred 1.8.0 binary must not share an inode with the new cred binary\n' >&2
  exit 78
}

expect <<'EXPECT'
set timeout 30
log_user 0
spawn env HOME=$env(TEST_HOME) XDG_CONFIG_HOME=$env(TEST_CONFIG) $env(CRED_1_8_BIN) init
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

store_api_key() {
  SERVICE=$1 ENTRY_KEY=$2 VALUE=$(openssl rand -hex 24) expect <<'EXPECT'
set timeout 20
spawn env HOME=$env(TEST_HOME) XDG_CONFIG_HOME=$env(TEST_CONFIG) $env(CRED_1_8_BIN) --auth-mode keyfile --keyfile $env(KEYFILE) store $env(SERVICE) $env(ENTRY_KEY)
expect "api key:"
send -- "$env(VALUE)\r"
expect "endpoint (optional):"
send -- "\r"
expect eof
EXPECT
}

store_api_key smtp api-key
store_api_key resend api-key
store_api_key kleos api-key
store_api_key grafana api-key
store_api_key opnsense-key api-key
store_api_key opnsense-secret api-key
store_api_key github api-key
store_api_key docker-inspect api-key
store_api_key github token

schema_before=$(sqlite3 "$DB_PATH" '.schema')
users_before=$(sqlite3 "$DB_PATH" 'SELECT group_concat(DISTINCT user_id) FROM cred_secrets ORDER BY user_id;')
secret_rows_digest() {
  sqlite3 "$DB_PATH" "
    SELECT category, name, secret_type, quote(encrypted_data), quote(nonce), user_id
    FROM cred_secrets
    WHERE category != 'memory-hub'
    ORDER BY category, name
  " | sha256sum
}
secrets_before=$(secret_rows_digest)

start_broker() {
  "$NEW_CREDD_BIN" --mode broker \
    --broker-socket "$SOCKET" \
    --broker-policy "$1" \
    --broker-client-uid "$(id -u)" \
    --auth-mode keyfile \
    --keyfile "$KEYFILE" \
    --db-path "$DB_PATH" >"$root/broker.log" 2>&1 &
  broker_pid=$!
  for _ in $(seq 1 50); do
    if "$NEW_CREDD_BIN" --mode broker --healthcheck --broker-socket "$SOCKET" >/dev/null 2>&1; then
      return
    fi
    sleep 0.1
  done
  wait "$broker_pid" || true
  printf 'broker did not become healthy\n' >&2
  exit 70
}

stop_broker() {
  kill "$broker_pid"
  wait "$broker_pid" || true
}

start_broker "$POLICY"
CRED_AUTH_MODE=broker CRED_BROKER_SOCKET="$SOCKET" \
  "$NEW_CRED_BIN" exec smtp api-key --stdin -- sh -c 'test -n "$(cat)"'

set +e
CRED_AUTH_MODE=broker CRED_BROKER_SOCKET="$SOCKET" \
  "$NEW_CRED_BIN" exec github token --stdin -- sh -c 'exit 99' >/dev/null 2>&1
status=$?
set -e
[[ $status == 77 ]] || {
  printf 'unauthorised reference returned %s, expected 77\n' "$status" >&2
  exit 1
}
stop_broker

store_api_key memory-hub api-key
start_broker "$EXTENDED_POLICY"
CRED_AUTH_MODE=broker CRED_BROKER_SOCKET="$SOCKET" \
  "$NEW_CRED_BIN" exec memory-hub api-key --stdin -- sh -c 'test -n "$(cat)"'
stop_broker

schema_after=$(sqlite3 "$DB_PATH" '.schema')
users_after=$(sqlite3 "$DB_PATH" 'SELECT group_concat(DISTINCT user_id) FROM cred_secrets ORDER BY user_id;')
secrets_after=$(secret_rows_digest)
[[ $schema_before == "$schema_after" ]] || {
  printf 'broker changed the SQLite schema\n' >&2
  exit 1
}
[[ $users_before == "$users_after" ]] || {
  printf 'broker changed secret user IDs\n' >&2
  exit 1
}
[[ $secrets_before == "$secrets_after" ]] || {
  printf 'broker changed pre-existing encrypted secret rows\n' >&2
  exit 1
}
printf 'cred 1.8 synthetic-vault compatibility gate passed\n'
