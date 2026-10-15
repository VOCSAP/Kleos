#!/usr/bin/env bash
set -euo pipefail

: "${CLIENT_BIN:?set CLIENT_BIN to the new Linux cred binary}"

[[ $(uname -s) == Linux ]] || {
  printf 'broker client gate requires Linux Unix sockets\n' >&2
  exit 78
}
for command in node readlink; do
  command -v "$command" >/dev/null || {
    printf 'missing required command: %s\n' "$command" >&2
    exit 78
  }
done

root=$(mktemp -d)
socket="$root/cred.sock"
decoy_db="$root/unopened.db"
decoy_key="$root/unopened.key"
marker="$root/child-spawned"
trap 'kill "${server_pid:-}" 2>/dev/null || true; wait "${server_pid:-}" 2>/dev/null || true; rm -rf "$root"' EXIT

client() {
  env CRED_AUTH_MODE=broker CRED_BROKER_SOCKET="$socket" CRED_DB_PATH="$decoy_db" \
    "$CLIENT_BIN" "$@"
}

expect_exit() {
  local expected=$1
  local label=$2
  shift 2
  set +e
  "$@" >/dev/null 2>&1
  local status=$?
  set -e
  [[ $status == "$expected" ]] || {
    printf '%s returned %s, expected %s\n' "$label" "$status" "$expected" >&2
    exit 1
  }
}

expect_no_spawn() {
  local expected=$1
  local label=$2
  shift 2
  rm -f "$marker"
  expect_exit "$expected" "$label" "$@"
  [[ ! -e "$marker" ]] || {
    printf '%s spawned a child before returning an error\n' "$label" >&2
    exit 1
  }
}

expect_no_spawn 69 'socket down' \
  env CRED_AUTH_MODE=broker CRED_BROKER_SOCKET="$root/down.sock" CRED_DB_PATH="$decoy_db" \
  "$CLIENT_BIN" exec success api-key -- /bin/sh -c "touch '$marker'"

node "$(dirname "$0")/broker_fake_server.mjs" "$socket" >"$root/fake-broker.log" 2>&1 &
server_pid=$!
for _ in $(seq 1 50); do
  [[ -S "$socket" ]] && break
  sleep 0.1
done
[[ -S "$socket" ]] || {
  printf 'fake broker did not create its Unix socket\n' >&2
  exit 70
}

expect_no_spawn 65 'invalid JSON response' \
  client exec invalid-json api-key -- /bin/sh -c "touch '$marker'"
expect_no_spawn 65 'wrong protocol response' \
  client exec wrong-protocol api-key -- /bin/sh -c "touch '$marker'"
expect_no_spawn 65 'oversize response' \
  client exec oversize api-key -- /bin/sh -c "touch '$marker'"
expect_no_spawn 65 'redirect response' \
  client exec redirect api-key -- /bin/sh -c "touch '$marker'"
expect_no_spawn 66 'not found response' \
  client exec not-found api-key -- /bin/sh -c "touch '$marker'"
expect_no_spawn 70 'internal response' \
  client exec internal api-key -- /bin/sh -c "touch '$marker'"
expect_no_spawn 69 'timeout response' \
  client exec timeout api-key -- /bin/sh -c "touch '$marker'"
expect_no_spawn 78 'local keyfile conflict' \
  env CRED_AUTH_MODE=broker CRED_BROKER_SOCKET="$socket" CRED_DB_PATH="$decoy_db" \
  "$CLIENT_BIN" --auth-mode keyfile --keyfile "$decoy_key" exec success api-key -- /bin/sh -c "touch '$marker'"

expect_exit 0 'HTTP proxy ignored for Unix socket' \
  env HTTP_PROXY=http://127.0.0.1:9 HTTPS_PROXY=http://127.0.0.1:9 ALL_PROXY=http://127.0.0.1:9 \
  CRED_AUTH_MODE=broker CRED_BROKER_SOCKET="$socket" CRED_DB_PATH="$decoy_db" \
  "$CLIENT_BIN" exec success api-key -- /bin/true
expect_exit 0 'environment injection' \
  client exec success api-key --env TEST_CRED_VALUE -- /bin/sh -c \
  '[ "$TEST_CRED_VALUE" = "FAKE_API_VALUE_FOR_CONTRACT_TESTS" ]'
expect_exit 0 'stdin injection' \
  client exec success api-key --stdin -- /bin/sh -c \
  '[ "$(cat)" = "FAKE_API_VALUE_FOR_CONTRACT_TESTS" ]'
expect_exit 0 'field resolution' \
  client exec success api-key --field token -- /bin/true
expect_exit 0 'hashed resolution' \
  client exec success api-key --hash argon2 -- /bin/sh -c '[ -n "$CRED_VALUE" ]'
expect_exit 0 'entries response' client list
expect_exit 7 'child exit status' client exec success api-key -- /bin/sh -c 'exit 7'
expect_exit 1 'signalled child' client exec success api-key -- /bin/sh -c 'kill -TERM $$'
expect_exit 127 'missing child executable' \
  client exec success api-key -- "$root/missing-program"
printf '#!/bin/sh\nexit 0\n' > "$root/not-executable"
chmod 0644 "$root/not-executable"
expect_exit 126 'non-executable child' \
  client exec success api-key -- "$root/not-executable"
expect_exit 0 'no inherited socket descriptor' \
  client exec success api-key -- /bin/sh -c \
  'for fd in /proc/self/fd/*; do target=$(readlink "$fd") || continue; case "$target" in socket:*) exit 1;; esac; done'

[[ ! -e "$decoy_db" && ! -e "$decoy_key" ]] || {
  printf 'broker client accessed a local credential decoy\n' >&2
  exit 1
}
printf 'broker client contract gate passed\n'
