# Broker fixtures

`policy.toml` covers the eight references retained at the OpenClaw cutover.
`policy-extended.toml` adds `memory-hub/api-key` as the ninth synthetic entry.

Linux gate scripts generate random synthetic values at runtime. They never print
those values, the generated keyfile, the SQLite database, or decrypted responses.
