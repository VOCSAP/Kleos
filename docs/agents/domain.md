# Domain Docs

How the engineering skills should consume this repo's domain documentation when
exploring the codebase. This repo is **single-context**: one `CONTEXT.md` at the
root, one `docs/adr/` directory.

## Before exploring, read these

- **`CONTEXT.md`** at the repo root: the glossary of domain terms.
- **`docs/adr/`**: read the ADRs that touch the area you are about to work in.

If any of these files do not exist, **proceed silently**. Do not flag their
absence; do not suggest creating them upfront. The `/domain-modeling` skill
creates them lazily, when terms or decisions actually get resolved.

## File structure

```
/
├── CONTEXT.md
├── docs/adr/
│   ├── 0001-<slug>.md
│   └── 0002-<slug>.md
└── kleos-lib/, kleos-server/, kleos-cli/, ...
```

The workspace has many crates, but one domain. Per-crate `CONTEXT.md` files are
deliberately not used: a crate boundary here is a deployment boundary, not a
bounded context.

## Use the glossary's vocabulary

When your output names a domain concept (a card title, a refactor proposal, a
hypothesis, a test name), use the term as defined in `CONTEXT.md`. Do not drift
to synonyms the glossary explicitly avoids.

If the concept you need is not in the glossary yet, that is a signal: either you
are inventing language the project does not use (reconsider), or there is a real
gap (note it for `/domain-modeling`).

## Flag ADR conflicts

If your output contradicts an existing ADR, surface it explicitly rather than
silently overriding:

> _Contradicts ADR-0007 (<title>), but worth reopening because..._

## Relationship to the existing docs

`docs/dev-notes/local-patches.md` is the reference for numbered local patches
against upstream, not an ADR set. An ADR records a design decision for this
fork; a patch section records a divergence from `Ghost-Frame/Kleos`. When a
decision produces a numbered patch, write the ADR and reference the patch
number, do not duplicate the patch body.
