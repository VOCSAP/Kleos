#!/usr/bin/env bash
set -euo pipefail

: "${BROKER_BIN:?set BROKER_BIN to the new Linux kleos-credd binary}"
: "${CLIENT_BIN:?set CLIENT_BIN to the new Linux cred binary}"
: "${KEYFILE:?set KEYFILE to a synthetic-vault keyfile}"
: "${DB_PATH:?set DB_PATH to a synthetic-vault database}"
: "${POLICY:?set POLICY to a valid broker policy}"

[[ $(id -u) == 0 ]] || {
  printf 'run this gate as root to create the two numeric identities\n' >&2
  exit 78
}
command -v setpriv >/dev/null || {
  printf 'missing required command: setpriv\n' >&2
  exit 78
}

root=$(mktemp -d)
trap 'kill "${broker_pid:-}" 2>/dev/null || true; rm -rf "$root"' EXIT
chmod 711 "$root"
socket_dir="$root/run"
socket="$socket_dir/cred.sock"
vault_dir="$root/vault"
install -d -o 10002 -g 10001 -m 2750 "$socket_dir"
install -d -o 10002 -g 10002 -m 700 "$vault_dir"
install -o 10002 -g 10002 -m 600 "$KEYFILE" "$vault_dir/master.key"
install -o 10002 -g 10002 -m 600 "$DB_PATH" "$vault_dir/cred.db"
keyfile="$vault_dir/master.key"
db_path="$vault_dir/cred.db"

setpriv --reuid 10002 --regid 10002 --groups 10001 \
  "$BROKER_BIN" --mode broker \
  --broker-socket "$socket" \
  --broker-policy "$POLICY" \
  --broker-client-uid 10001 \
  --auth-mode keyfile \
  --keyfile "$keyfile" \
  --db-path "$db_path" >"$root/broker.log" 2>&1 &
broker_pid=$!

for _ in $(seq 1 50); do
  if setpriv --reuid 10002 --regid 10002 --groups 10001 \
    "$BROKER_BIN" --mode broker --healthcheck --broker-socket "$socket" >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done
kill -0 "$broker_pid" 2>/dev/null || {
  printf 'broker exited before healthcheck\n' >&2
  exit 70
}

stat -c '%u:%g %a' "$socket" | grep -qx '10002:10001 660' || {
  printf 'unexpected broker socket owner or mode\n' >&2
  exit 1
}

CRED_AUTH_MODE=broker CRED_BROKER_SOCKET="$socket" \
  setpriv --reuid 10001 --regid 10001 --clear-groups \
  "$CLIENT_BIN" list >/dev/null

set +e
CRED_AUTH_MODE=broker CRED_BROKER_SOCKET="$socket" \
  setpriv --reuid 10003 --regid 10003 --clear-groups \
  "$CLIENT_BIN" list >/dev/null 2>&1
status=$?
set -e
[[ $status == 77 ]] || {
  printf 'unauthorised UID returned %s, expected 77\n' "$status" >&2
  exit 1
}
printf 'SO_PEERCRED cross-UID gate passed\n'
