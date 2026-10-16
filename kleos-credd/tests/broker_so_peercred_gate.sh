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

listing=$(CRED_AUTH_MODE=broker CRED_BROKER_SOCKET="$socket" \
  setpriv --reuid 10001 --regid 10001 --clear-groups \
  "$CLIENT_BIN" list)
if [[ -n ${EXPECT_LISTED:-} ]]; then
  listed=$(printf '%s\n' "$listing" | awk 'NR > 2 { print $1 "/" $2 }' | sort)
  expected=$(printf '%s\n' $EXPECT_LISTED | sort)
  [[ $listed == "$expected" ]] || {
    printf 'authorised UID listed [%s], expected [%s]\n' "$listed" "$expected" >&2
    exit 1
  }
fi

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

if [[ -n ${EXPECT_RESOLVE:-} ]]; then
  read -r -a resolve_reference <<<"$EXPECT_RESOLVE"
  expect_exit 0 'authorised UID resolve' \
    env CRED_AUTH_MODE=broker CRED_BROKER_SOCKET="$socket" \
    setpriv --reuid 10001 --regid 10001 --clear-groups \
    "$CLIENT_BIN" exec "${resolve_reference[@]}" --stdin -- sh -c 'test -n "$(cat)"'
fi

expect_exit 77 'unauthorised UID' \
  env CRED_AUTH_MODE=broker CRED_BROKER_SOCKET="$socket" \
  setpriv --reuid 10003 --regid 10003 --groups 10001 \
  "$CLIENT_BIN" list

expect_exit 77 'unauthorised UID resolve' \
  env CRED_AUTH_MODE=broker CRED_BROKER_SOCKET="$socket" \
  setpriv --reuid 10003 --regid 10003 --groups 10001 \
  "$CLIENT_BIN" exec smtp api-key -- true

expect_exit 77 'broker UID entries' \
  env CRED_AUTH_MODE=broker CRED_BROKER_SOCKET="$socket" \
  setpriv --reuid 10002 --regid 10002 --groups 10001 \
  "$CLIENT_BIN" list

expect_exit 77 'broker UID resolve' \
  env CRED_AUTH_MODE=broker CRED_BROKER_SOCKET="$socket" \
  setpriv --reuid 10002 --regid 10002 --groups 10001 \
  "$CLIENT_BIN" exec smtp api-key -- true

expect_exit 69 'UID outside socket group' \
  env CRED_AUTH_MODE=broker CRED_BROKER_SOCKET="$socket" \
  setpriv --reuid 10003 --regid 10003 --clear-groups \
  "$CLIENT_BIN" list

printf 'SO_PEERCRED cross-UID gate passed\n'
