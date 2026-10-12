# 64 — Tracker Tooling

How progress stays honest: one CLI keeps the checkboxes truthful,
and two git hooks enforce the habit. No third-party dependencies —
`mol-track` is Python 3 stdlib only.

## The tool: `tools/mol-track`

Single source of truth is the checkboxes in docs 38–63. Status and
reports are always derived, never stored, so nothing can drift.

| Command | What it does |
|---------|--------------|
| `status [--phase P1]` | Per-milestone table: done/total boxes, GREEN/red proof |
| `check <ID>...` | Tick checkbox(es), e.g. `check P1.1.1 P1.1.V1` |
| `uncheck <ID>...` | Untick (rework happens; say why in the commit) |
| `validate` | Links resolve; ids well-formed, unique, correctly filed; doc 29 map covers every sub-phase doc; Milestone/Proven-by present |
| `report` | Markdown per-phase summary for pasting into updates |

A milestone is GREEN only when every box in its sub-phase doc is
ticked. Tick criteria only when the work is done and its proof
suite is green — the `V` test boxes exist to make that checkable.

## The hooks: `.githooks/`

Install once per clone (repo-local config, not committed):

```sh
tools/install-hooks.sh
```

- `commit-msg`: every commit must reference the tracker — a
  `[P1.1]`-style tag (phase, sub-phase, or criterion id) or a
  `Tracker: P1.1.1, P1.1.V1` trailer. Referenced ids must exist.
  Merge commits are exempt.
- `pre-commit`: runs `mol-track validate`; broken links, duplicate
  or misfiled ids, or a sub-phase doc missing from the doc 29 map
  block the commit.

Emergency escape for either hook is `git commit --no-verify`.
Using it to land tracker changes without validation is a process
bug — say why in the commit message.

## Workflow

1. Pick the next red milestone from `mol-track status`.
2. Do the work; keep its proof suite green.
3. `mol-track check <ids>` for finished criteria and tests.
4. Commit with a tracker ref, e.g. `[P1.1] zero-diff load maps`.
   The hooks verify the ref exists and the docs still validate.
5. A sub-phase is done when `status` shows it GREEN; only then does
   the next sub-phase start (doc 29 sequencing rule).
