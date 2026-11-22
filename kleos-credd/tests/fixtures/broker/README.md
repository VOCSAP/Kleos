# Broker fixtures

`policy.toml` covers the eight references retained at the OpenClaw cutover.
`policy-extended.toml` adds `memory-hub/api-key` as the ninth synthetic entry.
`policy-field.toml` authorises single fields only (`account-demo/login@username`,
`ssh-demo/test-host@private_key`, `endpoint-demo/api-key@endpoint`) plus the bare
value of `account-demo/login`; any other field of those secrets is a forbidden-field case.

`golden/` holds the exact request and response bytes of the real router for the
synthetic vault seeded by `broker_router_contract.rs`: `FAKE_API_VALUE_FOR_CONTRACT_TESTS`
behind `smtp/api-key`, an unauthorised `github/token` and an unlisted `stray/api-key`.
Trailing newlines are ignored when comparing.

Linux gate scripts generate random synthetic values at runtime. They never print
those values, the generated keyfile, the SQLite database, or decrypted responses.
