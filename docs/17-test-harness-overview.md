# 17 — Test Harness Overview

How mol proves apps function identically to Mac hardware: run the same
test on a real Mac and under mol on Ubuntu, normalize away benign
divergence (doc 11), and diff the rest. Releases gate on doc 11
behavior; this doc is the harness that enforces it. Tiers use the
doc-README legend (S = embed dep, M = shim, L = subsystem re-impl,
XL = full re-impl).

## Architecture

```text
scheduler (push/PR per-commit, nightly cron, on-demand dispatch)
        │  matrix fan-out, fail-fast false
        ├── mac leg ──► macos-test-agent (doc 18)
        └── linux leg ─► ubuntu-vm-test-agent (doc 19)
                │  JUnit XML + traces/logs/screenshots/exit/timing artifacts
                ▼
        compare job: normalize → diff per test-id → MATCH/DIVERGE/FLAKE
                │  required check gates merge; nightly feeds dashboard
                ▼
        flake registry → quarantine lane (advisory, never gating)
```

Sources: matrix os axis + fail-fast/continue-on-error
([matrix](https://docs.github.com/en/actions/using-jobs/using-a-matrix-for-your-jobs)),
scheduler event kinds
([events](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows)),
artifact up/download per leg
([upload-artifact](https://github.com/actions/upload-artifact)).

## Components

| Component | Role | Tier | Source |
|-----------|------|------|--------|
| Scheduler | Per-commit + nightly cron + `workflow_dispatch` on-demand | S | [events](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows) |
| Matrix fan-out | Same test on macos + ubuntu legs, both finish for comparison | S | [matrix](https://docs.github.com/en/actions/using-jobs/using-a-matrix-for-your-jobs) |
| Per-leg runner | Executes YAML test defs, emits JUnit + artifacts | M | [gotestsum](https://github.com/gotestyourself/gotestsum) |
| JUnit interchange | `results.xml` per leg, merged in compare job | S | [Playwright reporters](https://playwright.dev/docs/test-reporters) |
| Sharding | Split suite across machines, blob-report merge | M | [sharding](https://playwright.dev/docs/test-sharding) |
| Trace viewer | Open recorded CI traces locally / hosted | S | [trace viewer](https://playwright.dev/docs/trace-viewer) |
| Compare job | Normalize (strip timing/paths/PIDs, sort) → MATCH/DIVERGE/FLAKE | M | [reporters](https://playwright.dev/docs/test-reporters) |
| Flake registry | Same-commit pass+fail → quarantine, stops blocking builds | M | [GitHub eng](https://github.blog/engineering/engineering-principles/reducing-flaky-builds-by-18x/), [Buildkite](https://buildkite.com/docs/test-analytics) |
| Retry | Re-run only failures, max attempts 2 | S | [gotestsum](https://github.com/gotestyourself/gotestsum) |
| Merge gate | Branch protection requires compare check | S | [protected branches](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches) |

## Test definition schema (harness-authored, Tier: M)

No single primary source standardizes cross-OS YAML/TOML test defs;
the schema below is harness-authored (see doc 10). One YAML per test:

```yaml
id: cli/jq/parse-basic            # stable test-id, diff key
command: ["jq", ".", "in.json"]   # argv, no shell
platforms: [mac, ubuntu]          # legs that must run it
timeout_s: 60
artifacts: [stdout, stderr, exit, trace]  # uploaded per leg
normalize: [strip-timing, strip-paths, sort-lines]  # doc 20 rule set
baseline: per-platform            # pixels/traces: separate mac vs linux goldens
```

Repo layout: `tests/`, `harness/{runner,compare,quarantine}/`,
`workflows/{ci,nightly,ondemand}.yml`, `dashboards/`.

## Verdict semantics

- `MATCH`: normalized outputs identical per test-id.
- `DIVERGE`: normalized outputs differ → blocks merge until re-baselined
  with written justification + reviewer sign-off (doc 16 policy).
- `FLAKE`: same test passes and fails on the same commit → quarantine
  lane; key is `(test, platform, renderer)`, never test alone, so a
  mac-GPU flake cannot poison Linux verdicts.

Pixels/traces compare only against same-platform baselines (doc 21);
cross-platform equality is asserted on accessibility trees and
normalized CLI behavior, never raw pixels.

## Checklist

- [ ] Matrix runs mac + linux legs per commit with fail-fast false
- [ ] Every leg emits JUnit + named artifacts the compare job consumes
- [ ] Compare job verdicts MATCH/DIVERGE/FLAKE per test-id
- [ ] Flake registry keyed `(test, platform, renderer)`; quarantine advisory-only
- [ ] Required compare check gates merge; nightly feeds dashboard
- [ ] Test-def schema validated; normalization rules checked in (doc 20)
