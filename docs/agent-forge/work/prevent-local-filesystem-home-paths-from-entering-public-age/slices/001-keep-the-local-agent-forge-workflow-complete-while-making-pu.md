# Slice 001: Keep the local Agent-Forge workflow complete while making public evidence portable and fail closed.

- **spec:** `spec_478a6dfa`

## Components

- The local MCP registry exposes session_learn and session_recall through the shared typed handlers.
- The shared leak scanner rejects concrete Linux, macOS, root, and Windows home paths while permitting placeholders, relative repository paths, and public URL routes.
- Review screens the fully rendered body before it creates the record directory or writes record.md.
- The installed MCP service refused the exact previously leaking review and retained all 17 workflow tools.

## Hard-won conditions

- Verification commands destined for public Fluency records must use repository-relative paths or portable environment variables.
- Schema bounds guide MCP clients, but typed handlers independently reject blank discoveries and recall limits outside 1 through 100.
- Generated records that fail screening are preserved outside public paths for diagnosis rather than committed.
- Public Fluency evidence must use repository-relative verification commands because concrete user-home paths fail the shared emission guard. (Agent-Forge local MCP Phase 1 hardening)

## Decision: Reject absolute user-home paths in the shared leak scanner

- **why:** Extend scan_for_leaks with cross-platform home-path markers and verify review refuses before persistence.
- **alternative:** Redact paths during rendering -- rejected: Mutates recorded evidence; Can conceal missed sensitive context; Makes commands non-reproducible
- **trust:** spec verified -- a verification run for this spec passed; this individual decision was not separately proved
