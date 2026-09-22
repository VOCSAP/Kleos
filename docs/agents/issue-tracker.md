# Issue Tracker

Issues for this repo live in the **shared roadmap**, reached through the
`mcp__claude-peers__roadmap_*` tools. Not GitHub Issues, not local markdown.
GitHub Issues on VOCSAP/Kleos exist but are not the agent-facing queue.

## Reading

- `roadmap_list` with a filter (never unfiltered: the board is large).
  Filters: `statuses`, `kinds`, `triages`, `priorities`, `tags`, `q`.
  `order: "queue"` gives the real dispatch order instead of MoSCoW groups.
- `roadmap_get <id>` for the full card (description, rationale, context,
  dependencies, authorship). Accepts a unique id prefix.

## Writing

- New issue: `roadmap_add`. Always fill `context`: it is the briefing for a
  future session with none of this one's context (objective, scope boundaries,
  relevant files and tests, acceptance criteria, decisions already made).
- `kind`: feature | bug | debt | idea | chore. `priority`: MoSCoW
  (must | should | could | wont).
- `triage`: needs-triage | needs-info | ready-for-agent | ready-for-human |
  wontfix. These five roles are the canonical vocabulary; use them as-is.
  `wontfix` requires `priority: wont`.
- Status lifecycle: idea -> planned -> in_progress -> done. `in_progress` LOCKS
  the card under your peer id: set it only when work really starts, and set it
  back to `planned` if you stop before finishing.
- Adding to someone else's card: `roadmap_append_context`. The work-lock does
  not block it.

## Overflow: when a card is too small for the context

A roadmap card holds a briefing, not a document. When the context a skill wants
to attach exceeds what a card can carry (a long spec, a full investigation
report, a multi-file plan, or any `roadmap_append_context` refused for exceeding
its cap), write the body to a **gitignored local markdown file** under
`docs/dev-notes/<slug>.md`. That whole directory is gitignored except
`local-patches.md`. Put only the path plus a two-line summary in the card's
`context`.

Consequence, accepted deliberately: such a file resolves only on the machine
that produced it. Anything another clone must be able to read belongs in the
card itself, in `docs/`, or in Kleos.

## Skills that read this

`to-tickets`, `to-spec`, `code-review` (spec axis), and any skill whose
instructions say "create an issue" or "read the originating issue".
