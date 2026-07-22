> **Review priority:** spec verified -- a verification run for this spec passed; this individual decision was not separately proved. The criteria were exercised, so read the decisions below for judgment rather than for correctness.

# Record: Prevent local filesystem home paths from entering public Agent-Forge Fluency records

- **spec:** `spec_478a6dfa`
- **type:** bugfix

## Acceptance criteria

- The mechanical leak scan flags Linux, macOS, and Windows user-home paths
- Relative repository paths remain publishable
- review(write=true) refuses a record containing a local home path before writing record.md
- All Agent-Forge tests remain green

## Edge cases

- A Linux path begins with /home/<user>/
- A macOS path begins with /Users/<user>/
- A Windows path uses C:\\Users\\<user>\\
- A repository-relative path contains docs/agent-forge without a home prefix
- A public URL contains the word users in its route

## Interface contract

```text
scan_for_leaks returns an absolute-home-path finding for user-specific filesystem paths, and guard_no_leaks makes every emitting caller fail closed before persistence.
```

## Decision: Reject absolute user-home paths in the shared leak scanner

- **why:** Extend scan_for_leaks with cross-platform home-path markers and verify review refuses before persistence.
- **alternative:** Redact paths during rendering -- rejected: Mutates recorded evidence; Can conceal missed sensitive context; Makes commands non-reproducible
- **trust:** spec verified -- a verification run for this spec passed; this individual decision was not separately proved

## Verification evidence

- `rg -n 'emit::gatekeeper::tests::flags_concrete_home_paths ... ok' .forge/path-guard-tests-final.txt` -- passed
- `rg -n 'emit::gatekeeper::tests::ignores_portable_path_examples ... ok' .forge/path-guard-tests-final.txt` -- passed
- `jq -e 'select(.id==72) | .result.isError == true and (.result.content[0].text | contains("absolute home path"))' .forge/path-guard-live-probe-output.ndjson` -- passed
- `rg -n 'test result: ok. 65 passed; 0 failed' .forge/path-guard-tests-final.txt` -- passed
