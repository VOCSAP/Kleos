#!/usr/bin/env bash
set -euo pipefail

: "${CLIENT_BIN:?set CLIENT_BIN to the new Linux cred binary}"

[[ $(uname -s) == Linux ]] || {
  printf 'broker client acceptance gate requires Linux\n' >&2
  exit 78
}
for command in diff expect node openssl pgrep pkill ps setsid sha256sum sort strace timeout; do
  command -v "$command" >/dev/null || {
    printf 'missing required command: %s\n' "$command" >&2
    exit 78
  }
done

here=$(cd "$(dirname "$0")" && pwd)
root=$(mktemp -d)
traces="$root/traces"
home="$root/home"
config="$root/config"
lhome="$root/lhome"
lconfig="$lhome/.config"
socket="$root/cred.sock"
request_log="$root/broker-requests.log"
decoy_hits="$root/decoy-hits.log"
decoy_key="$root/decoy/master.key"
marker="$root/child-spawned"
run_path="$root/bin:/usr/local/bin:/usr/bin:/bin"
mkdir -p "$traces" "$home" "$config/cred" "$lconfig/cred" "$root/decoy" "$root/bin" "$root/cwd with space"
: > "$request_log"
: > "$decoy_hits"

cleanup() {
  kill "${server_pid:-}" "${decoy_pid:-}" 2>/dev/null || true
  wait "${server_pid:-}" "${decoy_pid:-}" 2>/dev/null || true
  pkill -f 'sleep 31[0-9][0-9]\.7' 2>/dev/null || true
  rm -rf "$root"
}
trap cleanup EXIT

fail() {
  printf '%s\n' "$*" >&2
  exit 1
}

printf '%s\n' "$(openssl rand -hex 32)" > "$decoy_key"
printf 'decoy-database-sentinel\n' > "$config/cred/cred.db"
printf 'none\n' > "$config/cred/encryption-mode"
printf 'decoy-salt-sentinel\n' > "$config/cred/kdf_salt"
printf 'decoy-challenge-sentinel\n' > "$config/cred/challenge"
(cd "$root" && find config decoy -type f -exec sha256sum {} + | sort) > "$root/decoys.before"

node "$here/broker_decoy_listener.mjs" "$root/decoy.port" "$decoy_hits" >/dev/null 2>&1 &
decoy_pid=$!
node "$here/broker_fake_server.mjs" "$socket" "$request_log" >"$root/fake-broker.log" 2>&1 &
server_pid=$!
for _ in $(seq 1 100); do
  [[ -S "$socket" && -s "$root/decoy.port" ]] && break
  sleep 0.1
done
[[ -S "$socket" ]] || fail 'fake broker did not create its Unix socket'
[[ -s "$root/decoy.port" ]] || fail 'decoy CREDD_URL listener did not start'
decoy_port=$(cat "$root/decoy.port")
decoy_url="http://127.0.0.1:$decoy_port"

exec 3<>"/dev/tcp/127.0.0.1/$decoy_port"
exec 3>&-
for _ in $(seq 1 50); do
  [[ -s "$decoy_hits" ]] && break
  sleep 0.1
done
[[ $(wc -l < "$decoy_hits") == 1 ]] || fail 'decoy listener control connection was not counted'
: > "$decoy_hits"

printf '#!/bin/sh\nexec /usr/bin/env -i PATH=%s HOME=%s XDG_CONFIG_HOME=%s CREDD_URL=%s "$@"\n' \
  "$run_path" "$home" "$config" "$decoy_url" > "$root/bin/clean_env"
printf '#!/bin/sh\nexec clean_env CRED_AUTH_MODE=broker CRED_BROKER_SOCKET=%s "$@"\n' \
  "$socket" > "$root/bin/broker_env"
printf '#!/bin/sh\nexec /usr/bin/env -i PATH=%s HOME=%s XDG_CONFIG_HOME=%s "$@"\n' \
  "$run_path" "$lhome" "$lconfig" > "$root/bin/local_env"
chmod 0755 "$root/bin/clean_env" "$root/bin/broker_env" "$root/bin/local_env"
export PATH="$root/bin:$PATH"

trace_count=0
traced() {
  trace_count=$((trace_count + 1))
  strace -f -qq -e trace=file,network -o "$traces/$trace_count.log" "$@"
}

expect_exit() {
  local expected=$1
  local label=$2
  shift 2
  set +e
  "$@" >"$root/last.stdout" 2>"$root/last.stderr" </dev/null
  local status=$?
  set -e
  [[ $status == "$expected" ]] || fail "$label returned $status, expected $expected: $(cat "$root/last.stderr")"
}

expect_clean_refusal() {
  local expected=$1
  local label=$2
  shift 2
  rm -f "$marker"
  expect_exit "$expected" "$label" traced "$@"
  [[ ! -e "$marker" ]] || fail "$label spawned a child before returning an error"
  if grep -q 'unlocking' "$root/last.stderr"; then
    fail "$label started an unlock sequence: $(cat "$root/last.stderr")"
  fi
}

child_marker() {
  printf 'touch %s' "$marker"
}

forbidden_commands=(
  'get smtp api-key'
  'get --raw smtp api-key'
  'export'
  "export --output $root/export.json"
  "export-key --out $root/exported.key"
  'store smtp api-key'
  'delete smtp api-key --yes'
  'import'
  'import --dry-run'
  'init'
  'tui'
  'session start'
  'session end'
  'agent-key list'
  'agent-key generate synthetic-agent'
  'agent-key revoke synthetic-agent --yes'
  'recover'
  'kdf-salt-init'
  "bootstrap wrap smtp api-key --out $root/bootstrap.enc"
  'bootstrap unwrap'
  'piv status'
  "ssh-ca sign -I synthetic $root/agent.pub"
)
for line in "${forbidden_commands[@]}"; do
  read -r -a words <<<"$line"
  expect_clean_refusal 77 "forbidden command '$line'" broker_env "$CLIENT_BIN" "${words[@]}"
done
for artefact in export.json exported.key bootstrap.enc; do
  [[ ! -e "$root/$artefact" ]] || fail "a forbidden command wrote $artefact"
done

expect_clean_refusal 78 'broker mode without a socket variable' \
  clean_env CRED_AUTH_MODE=broker "$CLIENT_BIN" exec success api-key -- /bin/sh -c "$(child_marker)"
expect_clean_refusal 78 '--auth-mode broker without a socket variable' \
  clean_env "$CLIENT_BIN" --auth-mode broker exec success api-key -- /bin/sh -c "$(child_marker)"
for socket_value in '' 'relative/cred.sock' './cred.sock' 'cred.sock'; do
  expect_clean_refusal 78 "socket '$socket_value' in broker mode" \
    clean_env CRED_AUTH_MODE=broker CRED_BROKER_SOCKET="$socket_value" \
    "$CLIENT_BIN" exec success api-key -- /bin/sh -c "$(child_marker)"
  expect_clean_refusal 78 "socket '$socket_value' alone" \
    clean_env CRED_BROKER_SOCKET="$socket_value" \
    "$CLIENT_BIN" exec success api-key -- /bin/sh -c "$(child_marker)"
done

conflicts=(
  "--keyfile $decoy_key"
  '--master-password synthetic-password'
  '--auth-mode keyfile'
  '--auth-mode password'
  '--auth-mode yubikey'
)
for options in "${conflicts[@]}"; do
  read -r -a words <<<"$options"
  expect_clean_refusal 78 "broker with conflicting '$options'" \
    broker_env "$CLIENT_BIN" "${words[@]}" exec success api-key -- /bin/sh -c "$(child_marker)"
  expect_clean_refusal 78 "socket variable with conflicting '$options'" \
    clean_env CRED_BROKER_SOCKET="$socket" \
    "$CLIENT_BIN" "${words[@]}" exec success api-key -- /bin/sh -c "$(child_marker)"
done
expect_clean_refusal 78 'broker with CRED_KEYFILE' \
  broker_env CRED_KEYFILE="$decoy_key" "$CLIENT_BIN" exec success api-key -- /bin/sh -c "$(child_marker)"
expect_clean_refusal 78 'broker with CRED_MASTER_PASSWORD' \
  broker_env CRED_MASTER_PASSWORD=synthetic-password \
  "$CLIENT_BIN" exec success api-key -- /bin/sh -c "$(child_marker)"
expect_clean_refusal 78 'socket variable with CRED_AUTH_MODE=keyfile' \
  clean_env CRED_BROKER_SOCKET="$socket" CRED_AUTH_MODE=keyfile CRED_KEYFILE="$decoy_key" \
  "$CLIENT_BIN" exec success api-key -- /bin/sh -c "$(child_marker)"
expect_clean_refusal 78 'socket variable with CRED_MASTER_PASSWORD' \
  clean_env CRED_BROKER_SOCKET="$socket" CRED_MASTER_PASSWORD=synthetic-password \
  "$CLIENT_BIN" exec success api-key -- /bin/sh -c "$(child_marker)"

for informational in '--help' '--version' 'exec --help' 'list --help'; do
  read -r -a words <<<"$informational"
  expect_exit 0 "informational '$informational' in broker mode" \
    traced broker_env "$CLIENT_BIN" "${words[@]}"
done

[[ ! -s "$request_log" ]] || fail "a refused invocation reached the broker: $(cat "$request_log")"
[[ ! -s "$decoy_hits" ]] || fail 'a refused invocation contacted the CREDD_URL decoy'

expect_exit 0 'broker mode served through CRED_AUTH_MODE=broker' \
  traced broker_env "$CLIENT_BIN" exec success api-key --env TEST_CRED_VALUE -- \
  /bin/sh -c '[ "$TEST_CRED_VALUE" = "FAKE_API_VALUE_FOR_CONTRACT_TESTS" ]'
requests_after_success=$(wc -l < "$request_log")
[[ $requests_after_success == 1 ]] || fail "expected one broker request, saw $requests_after_success"

expect_exit 0 'empty listing exits zero' traced broker_env "$CLIENT_BIN" list --service nomatch
grep -qx 'no secrets stored' "$root/last.stdout" || \
  fail "empty listing must say so on stdout: $(cat "$root/last.stdout")"

probe_script='pwd; printf "PATH=%s\n" "$PATH"; id -u; umask; printf "[%s]\n" "$@"; printf "stderr-line\n" >&2'
probe_args=(probe-name 'two words' '' '*' '--flag' 'quote"d' "ap'os" 'ü ñ')
cd "$root/cwd with space"
env -i PATH="$run_path" /bin/sh -c "$probe_script" "${probe_args[@]}" \
  >"$root/direct.stdout" 2>"$root/direct.stderr"
set +e
traced broker_env "$CLIENT_BIN" exec success api-key --env INJECTED -- \
  /bin/sh -c "$probe_script" "${probe_args[@]}" >"$root/via.stdout" 2>"$root/via.stderr" </dev/null
probe_status=$?
set -e
cd "$root"
[[ $probe_status == 0 ]] || fail "argv probe through the client failed with $probe_status: $(cat "$root/via.stderr")"
diff "$root/direct.stdout" "$root/via.stdout" >/dev/null || \
  fail "child cwd, PATH, uid, umask or argv changed: $(diff "$root/direct.stdout" "$root/via.stdout")"
diff "$root/direct.stderr" "$root/via.stderr" >/dev/null || \
  fail "child stderr changed: $(diff "$root/direct.stderr" "$root/via.stderr")"

broker_env "$CLIENT_BIN" exec success api-key --env INJECTED -- /usr/bin/env </dev/null 2>/dev/null \
  | sort > "$root/env.via"
clean_env CRED_AUTH_MODE=broker CRED_BROKER_SOCKET="$socket" /usr/bin/env | sort > "$root/env.inherited"
diff <(grep -v '^INJECTED=' "$root/env.via") "$root/env.inherited" >/dev/null || \
  fail "the child environment differs from the inherited one beyond the injected variable: $(diff <(grep -v '^INJECTED=' "$root/env.via") "$root/env.inherited")"
grep -qx 'INJECTED=FAKE_API_VALUE_FOR_CONTRACT_TESTS' "$root/env.via" || \
  fail 'the injected variable is missing from the child environment'

stdin_probe=$(printf 'forwarded-input' | broker_env "$CLIENT_BIN" exec success api-key --env INJECTED -- /bin/cat)
[[ $stdin_probe == forwarded-input ]] || fail 'stdin is not forwarded to the child when injecting through the environment'

nested_check='[ "$OUTER" = ECHO_VALUE_outer ] && [ "$INNER" = ECHO_VALUE_inner ]'
expect_exit 0 'nested cred resolves a second reference with inherited broker settings' \
  traced broker_env OUTER_CLIENT="$CLIENT_BIN" NESTED_CHECK="$nested_check" \
  "$CLIENT_BIN" exec echo-outer api-key --env OUTER -- \
  /bin/sh -c '"$OUTER_CLIENT" exec echo-inner api-key --env INNER -- /bin/sh -c "$NESTED_CHECK"'
expect_exit 0 'nested stdin injection after an environment injection' \
  traced broker_env OUTER_CLIENT="$CLIENT_BIN" \
  "$CLIENT_BIN" exec echo-outer api-key --env OUTER -- \
  /bin/sh -c '"$OUTER_CLIENT" exec echo-inner api-key --stdin -- /bin/sh -c "[ \"\$OUTER\" = ECHO_VALUE_outer ] && [ \"\$(cat)\" = ECHO_VALUE_inner ]"'

expect_exit 255 'child exit status 255 is preserved' \
  traced broker_env "$CLIENT_BIN" exec success api-key -- /bin/sh -c 'exit 255'
expect_exit 0 'child exit status 0 is preserved' \
  traced broker_env "$CLIENT_BIN" exec success api-key -- /bin/sh -c 'exit 0'

broker_env "$CLIENT_BIN" exec success api-key -- /bin/sleep 3100.7 </dev/null >/dev/null 2>&1 &
client_pid=$!
child_pid=
for _ in $(seq 1 50); do
  child_pid=$(pgrep -P "$client_pid" -f 'sleep 3100.7' || true)
  [[ -n $child_pid ]] && break
  sleep 0.1
done
[[ -n $child_pid ]] || fail 'the child process was not found under the client process'
client_group=$(ps -o pgid= -p "$client_pid" | tr -d ' ')
child_group=$(ps -o pgid= -p "$child_pid" | tr -d ' ')
client_session=$(ps -o sid= -p "$client_pid" | tr -d ' ')
child_session=$(ps -o sid= -p "$child_pid" | tr -d ' ')
[[ $client_group == "$child_group" ]] || fail "child left the client process group ($client_group vs $child_group)"
[[ $client_session == "$child_session" ]] || fail "child left the client session ($client_session vs $child_session)"
kill "$child_pid" 2>/dev/null || true
wait "$client_pid" 2>/dev/null || true

gone() {
  local pattern=$1
  for _ in $(seq 1 50); do
    pgrep -f "$pattern" >/dev/null || return 0
    sleep 0.1
  done
  return 1
}

set +e
timeout --signal=TERM 2 broker_env "$CLIENT_BIN" exec success api-key -- /bin/sh -c 'sleep 3101.7 & wait' \
  </dev/null >/dev/null 2>&1
group_status=$?
set -e
[[ $group_status == 124 ]] || fail "group timeout returned $group_status, expected 124"
gone 'sleep 3101.7' || fail 'group timeout left an orphaned grandchild'

setsid -f /bin/sleep 3102.7 </dev/null >/dev/null 2>&1
sleep 0.3
pgrep -f 'sleep 3102.7' >/dev/null || fail 'orphan detector control did not observe a detached process'
pkill -f 'sleep 3102.7' || true

expect_exit 0 'broker mode served through CRED_BROKER_SOCKET alone' \
  traced clean_env CRED_BROKER_SOCKET="$socket" "$CLIENT_BIN" exec success api-key --env TEST_CRED_VALUE -- \
  /bin/sh -c '[ "$TEST_CRED_VALUE" = "FAKE_API_VALUE_FOR_CONTRACT_TESTS" ]'
expect_exit 0 'listing through CRED_BROKER_SOCKET alone' \
  traced clean_env CRED_BROKER_SOCKET="$socket" "$CLIENT_BIN" list
grep -q '^success  *api-key' "$root/last.stdout" || \
  fail "listing through CRED_BROKER_SOCKET alone lacks the served entry: $(cat "$root/last.stdout")"
expect_exit 0 'nested cred through CRED_BROKER_SOCKET alone' \
  traced clean_env CRED_BROKER_SOCKET="$socket" OUTER_CLIENT="$CLIENT_BIN" NESTED_CHECK="$nested_check" \
  "$CLIENT_BIN" exec echo-outer api-key --env OUTER -- \
  /bin/sh -c '"$OUTER_CLIENT" exec echo-inner api-key --env INNER -- /bin/sh -c "$NESTED_CHECK"'
expect_clean_refusal 78 'a socket variable with an explicit local auth mode is a configuration error' \
  clean_env CRED_BROKER_SOCKET="$socket" "$CLIENT_BIN" --auth-mode keyfile --keyfile "$decoy_key" list

(cd "$root" && find config decoy -type f -exec sha256sum {} + | sort) > "$root/decoys.after"
diff "$root/decoys.before" "$root/decoys.after" >/dev/null || fail 'a decoy file was modified'
touched=$(grep -rhE "$decoy_key|$config/cred" "$traces" | grep -v 'execve(' || true)
[[ -z $touched ]] || fail "a broker-mode invocation touched the decoy keyfile or the credential directory: $touched"
sockets=$(grep -rhE 'AF_INET6?' "$traces" || true)
[[ -z $sockets ]] || fail "a broker-mode invocation opened a TCP/UDP socket: $sockets"
terminals=$(grep -rh '/dev/tty' "$traces" || true)
[[ -z $terminals ]] || fail "a broker-mode invocation opened the controlling terminal: $terminals"
[[ ! -s "$decoy_hits" ]] || fail 'the CREDD_URL decoy received a connection'
rm -f "$traces"/*.log

openssl rand -hex 32 > "$root/local.key"
chmod 0600 "$root/local.key"
local_value=$(openssl rand -hex 16)
LOCAL_VALUE=$local_value CLIENT_BIN=$CLIENT_BIN LOCAL_KEY="$root/local.key" LHOME=$lhome LCONFIG=$lconfig RUN_PATH=$run_path \
  expect <<'EXPECT' >/dev/null
set timeout 30
log_user 0
spawn env -i PATH=$env(RUN_PATH) HOME=$env(LHOME) XDG_CONFIG_HOME=$env(LCONFIG) $env(CLIENT_BIN) --auth-mode keyfile --keyfile $env(LOCAL_KEY) store localsvc localkey
expect "api key:"
send -- "$env(LOCAL_VALUE)\r"
expect "endpoint (optional):"
send -- "\r"
expect eof
EXPECT

lkey="$root/local.key"
expect_exit 0 'local mode injects through the environment' \
  local_env "$CLIENT_BIN" --auth-mode keyfile --keyfile "$lkey" exec localsvc localkey --env LOCAL_PROBE -- \
  /bin/sh -c "[ \"\$LOCAL_PROBE\" = '$local_value' ]"
expect_exit 0 'local mode injects through stdin' \
  local_env "$CLIENT_BIN" --auth-mode keyfile --keyfile "$lkey" exec localsvc localkey --stdin -- \
  /bin/sh -c "[ \"\$(cat)\" = '$local_value' ]"
expect_exit 0 'local mode honours CRED_AUTH_MODE and CRED_KEYFILE' \
  local_env CRED_AUTH_MODE=keyfile CRED_KEYFILE="$lkey" "$CLIENT_BIN" exec localsvc localkey --env LOCAL_PROBE -- \
  /bin/sh -c "[ \"\$LOCAL_PROBE\" = '$local_value' ]"
expect_exit 7 'local mode preserves the child exit status' \
  local_env "$CLIENT_BIN" --auth-mode keyfile --keyfile "$lkey" exec localsvc localkey -- /bin/sh -c 'exit 7'
expect_exit 0 'local mode lists its secrets' \
  local_env "$CLIENT_BIN" --auth-mode keyfile --keyfile "$lkey" list
grep -q 'localsvc' "$root/last.stdout" || fail "local listing lacks the stored service: $(cat "$root/last.stdout")"
expect_exit 0 'local mode still prints its version' local_env "$CLIENT_BIN" --version

set +e
traced local_env "$CLIENT_BIN" --auth-mode keyfile --keyfile "$decoy_key" list >/dev/null 2>&1 </dev/null
set -e
opened=$(grep -h "$decoy_key" "$traces/$trace_count.log" | grep -v 'execve(' || true)
[[ -n $opened ]] || fail 'trace control did not observe a local-mode run opening the keyfile it was given'

printf 'broker client acceptance gate passed\n'
