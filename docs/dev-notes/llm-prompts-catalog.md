# LLM Prompts Catalog (Patch 15)

Reference for every LLM prompt that can be overridden at runtime via the dynamic prompt overlay mechanism introduced by VOCSAP Patch 15. The mechanism lives in `kleos-lib/src/llm/prompts.rs`; the embedded defaults are bundled in the binary from `kleos-lib/prompts/<service>/<purpose>/{system,user}.txt` via `include_str!()`. Operator overrides live in a separate file tree on disk; see the Override section below.

## Override mechanism

### Resolution cascade

Each call site resolves its prompt id via `kleos_lib::llm::prompts::load_prompt(id, embedded_default)` or `load_pair(prefix, def_sys, def_user)`. The resolver looks for a matching `.txt` file in this order:

1. `${KLEOS_LLM_PROMPT_REPOSITORY}/<id>.txt` -- explicit override path. If the env var is set to a non-empty value, that directory is the override root.
2. `${KLEOS_DATA_DIR}/prompts/<id>.txt` (or `${ENGRAM_DATA_DIR}/prompts/<id>.txt`) -- implicit convention. If the directory exists, it becomes the override root. This is the default deploy convention on LXC 121 (`KLEOS_DATA_DIR=/var/lib/kleos`).
3. Embedded default bundled in the binary at build time. Zero I/O, zero regression for operators who never deploy an override directory.

### Cache

- Each prompt is read at most once every 5 seconds (`TTL_SECS` in `kleos-lib/src/llm/prompts.rs`).
- The cache stores `Arc<String>` for cheap clones across concurrent callers.
- The file modification time (`mtime`) is rechecked after the TTL window; if it changed, the content is reloaded. Otherwise the cached copy is reused.
- Restart of `kleos-server` is not required to pick up an edit: wait up to 5 seconds.
- No file present at all: the resolver remembers the miss for `TTL_SECS` and falls back to the embedded default for that period before re-checking.

### Operator deploy

```bash
# Initial deploy on a Kleos host:
cd /var/lib/kleos
git clone https://github.com/VOCSAP/Kleos.prompts prompts

# Subsequent updates (no restart needed):
git -C /var/lib/kleos/prompts pull
```

Layout convention: `<root>/<service>/<purpose>/system.txt` (and optional `user.txt` for prompts with a templated user message). Templates use `{{var}}` interpolation honoured by the same `kleos-lib::llm::template::interpolate` helper used by Loom.

## Prompt catalog

The list below enumerates every prompt overridable at runtime. The column "User template" lists the `{{var}}` placeholders honoured by the user-side message; absent rows have no user file (system-only prompt).

| ID | Source call site | Trigger | User template variables |
|---|---|---|---|
| `broca/ask_plan` | `kleos-lib/src/services/broca.rs::ask_plan_call` | POST `/broca/ask` (plan step) | `{{question}}` |
| `broca/narrate` | `kleos-lib/src/services/broca.rs::llm_narrate` | POST `/broca/actions/{id}/narrate`, ingest fallback | `{{agent}}`, `{{service}}`, `{{action}}`, `{{payload}}` |
| `broca/ask_summary` | `kleos-lib/src/services/broca.rs::ask_summarize_call` | POST `/broca/ask` (summary step, after fetch) | `{{question}}`, `{{rows_excerpt}}` |
| `chiasm/generate_plan` | `kleos-lib/src/services/chiasm/tasks.rs::generate_plan` | POST `/chiasm/tasks/{id}/plan` | `{{title}}`, `{{project}}`, `{{agent}}`, `{{expected_block}}`, `{{summary_block}}` |
| `skills/fix_prompt` | `kleos-lib/src/skills/evolver.rs::fix_skill` | Skill evolution after recorded failures | -- (system only; user is built by caller) |
| `skills/derive_prompt` | `kleos-lib/src/skills/evolver.rs::derive_skill` | Skill derivation from parent skills | -- |
| `skills/capture_prompt` | `kleos-lib/src/skills/evolver.rs::capture_skill` | Skill capture from a workflow description | -- |
| `skills/analyze_execution` | `kleos-lib/src/skills/analyzer.rs` | Post-execution analysis of a skill invocation | -- |
| `skills/interactive_execute` | `kleos-server/src/routes/skills/mod.rs::execute_skills_handler` | POST `/skills/execute` (interactive helper) | `{{skill_context_block}}` (preformatted by the caller) |
| `extraction/atoms` | `kleos-lib/src/handoffs/atoms.rs::extract_llm` | Session handoff atom extraction | -- |
| `extraction/facts` | `kleos-lib/src/ingestion/processors/extract.rs::extract_facts` | Memory ingestion of long-form text | -- |
| `memory/reflect_action` | `kleos-lib/src/intelligence/reflections.rs::llm_reflect_on_memory` | Consolidation cycle (7+ day candidates) | `{{category}}`, `{{importance}}`, `{{snippet}}` |
| `memory/decompose` | `kleos-lib/src/intelligence/decomposition.rs::try_llm_decomposition` | Memory atomization on ingest / consolidation | -- |
| `growth/kleos_reflection` | `kleos-lib/src/intelligence/growth.rs::get_prompt_for_service` | Dreamer cycle for `kleos` (and legacy `engram`) | -- |
| `growth/claude_code_reflection` | same as above | Dreamer cycle for `claude-code` | -- |
| `growth/eidolon_reflection` | same as above | Dreamer cycle for `eidolon` | -- |
| `growth/default_reflection` | same as above | Fallback for any other service | -- |
| `loom/llm_step_fallback` | `kleos-lib/src/services/loom.rs::execute_llm_step` | Workflow LLM step where `config.system` is absent | -- |

For the **intent** of each prompt (what we ask the model + what Kleos expects in return + pitfalls), see `prompts-overrides/CLAUDE.md` in the override repository. That file is the single source of truth for operators tuning prompts.

### Out of scope

| Call site | Reason |
|---|---|
| `kleos-lib/src/prompts.rs::build_living_prompt` (id would be `prompts/agent_header`) | Dynamic builder assembling multiple sections (memories, contradictions, server table, safety rules) from runtime data. Externalising it would require a templating engine, outside the Patch 15 scope. |
| `kleos-lib/src/services/brain.rs::ORACLE_SYSTEM_PROMPT` | Marked `#[allow(dead_code)]` upstream. No runtime consumer. |

## Procedure for adding a new override

1. Identify the prompt ID in the table above.
2. Copy the embedded default file from `kleos-lib/prompts/<id>/...` into the override repo (or directly into the LXC `/var/lib/kleos/prompts/<id>/...` for a quick local test).
3. Edit the file. Preserve the `{{var}}` placeholders documented in the table.
4. For a quick local test: drop the file in `/var/lib/kleos/prompts/<id>/...` on LXC 121, wait up to 5 seconds, hit the matching endpoint, validate the response.
5. For a tracked change: edit inside the `prompts-overrides/` submodule, commit + push, then `git -C /var/lib/kleos/prompts pull` on LXC 121.
6. Update the catalog in `prompts-overrides/CLAUDE.md` so other operators see which overrides are active.

## Procedure for upstream rebase

When Ghost-Frame upstream changes a prompt body that lives in this catalog:

1. The merge conflict appears in the corresponding call site (or in `kleos-lib/prompts/<id>/...` if upstream eventually adopts a similar layout -- not the case as of 2026-05-19).
2. Resolve the conflict by updating the relevant `kleos-lib/prompts/<id>/system.txt` (or `user.txt`) to match upstream's new content. The byte-equivalence is what makes the embedded default stay aligned with upstream.
3. If the override repo (`VOCSAP/Kleos.prompts`) carries a manual override for that same prompt, re-evaluate it: maybe the upstream change supersedes the local fix, or maybe the override still applies on top of the new default.

## Migration progress

| Lot | Commit | Prompts |
|---|---|---|
| 1 -- foundations + pilot | `04eabaa` | `broca/ask_plan` |
| 2 -- broca + chiasm | `e994c69` | `broca/narrate`, `broca/ask_summary`, `chiasm/generate_plan` |
| 3 -- skills | `14b921b` | `skills/{fix,derive,capture}_prompt`, `skills/analyze_execution`, `skills/interactive_execute` |
| 4 -- extraction + memory | `3d6ab46` | `extraction/atoms`, `extraction/facts`, `memory/reflect_action`, `memory/decompose` |
| 5 -- growth | `41ad013` | `growth/{kleos,claude_code,eidolon,default}_reflection` |
| 6 -- loom | `a84221a` | `loom/llm_step_fallback` |

Final count: **17 active call sites surchargeables**, 1 out of scope (agent_header dynamic builder), 1 dead code (brain oracle).
