# 22 — Harness CI Integration

Where the harness runs (doc 17), what it costs, and how it gates
merges — phased from zero-capex prototype to owned-iron scale, aligned
with the doc 15 oracle phases. Tiers use the doc-README legend.

## Cost facts

| Item | Figure | Source |
|------|--------|--------|
| GitHub-hosted macOS | $0.062/min standard; 12-core $0.077/min; M2 Pro 5-core $0.102/min (larger runners: Team/Enterprise only) | [runner pricing](https://docs.github.com/en/billing/reference/actions-runner-pricing) |
| GitHub-hosted Linux/Windows | $0.006/min / $0.010/min; Linux 4-core $0.012/min; minutes rounded up per job | [runner pricing](https://docs.github.com/en/billing/reference/actions-runner-pricing) |
| Self-hosted runners | Free of GitHub minutes; user pays hardware; repo/org/enterprise levels | [self-hosted](https://docs.github.com/en/actions/hosting-your-own-runners/managing-self-hosted-runners/about-self-hosted-runners) |
| EC2 Mac | Per-second billing, 24h-minimum Dedicated Host per Apple license; sustained ~$631/mo (M2 $0.878/hr) to ~$1418/mo (M4 Pro $1.97/hr); no spot/reserved, Savings Plans only; exact M4/M4-Max on-demand $/hr unverified | [EC2 Mac types](https://aws.amazon.com/ec2/instance-types/mac/), [cost survey](https://howtohosting.guide/mac-hosting/) |
| MacStadium | List pricing removed (contact-sales); verified Feb-2026 figures: M4 mini from $119/mo, M4 Pro 64GB $399/mo | [pricing](https://macstadium.com/pricing) |
| Owned M4 mini | 16GB $599 / M4 Pro 24GB $1399; beats ~$398/mo rental payback in ~5 months; recurring ~$64/mo amortized + power | [self-hosting math](https://jeffverkoeyen.com/blog/2025/10/17/SelfHostingMacMinis/) |
| Artifact/cache storage | Included Team 2GB / Enterprise 50GB (shared with Packages); overage $0.25/GB-mo, cache $0.07/GB-mo | [billing](https://docs.github.com/en/billing/managing-billing-for-github-actions/about-billing-for-github-actions) |

## Recommended topology per phase (Tier: S–M)

- Phase 0 prototype (open-source or low volume): GitHub-hosted only,
  macos standard + linux — $0 if the repo is public, else ~$3.72/macOS-hr
  + $0.36/linux-hr. PR smoke via required Checks; nightly cron
  full-corpus on the default branch. Constraint: hosted runners cannot
  disable SIP, so Phase 0 capture must be SIP-safe (doc 18).
- Phase 1 private team (<10k macOS min/mo): Team plan + 1–2 MacStadium
  M4 minis as org-level self-hosted runners (~$119–199/mo each) for
  macOS; GitHub-hosted Linux for the rest. Golden traces as artifacts
  with short retention (7–14 days) + pinned releases for long-term
  goldens to dodge $0.25/GB-mo accrual.
- Phase 2 scale (>30k macOS min/mo, hosted bill >$2k/mo): buy 2–4
  M4/M4 Pro minis (~$600–1400 each) colo or on-prem as ephemeral
  self-hosted runners; Linux self-hosted or ARC for throughput. Keep
  one GitHub-hosted macOS leg for fork-PR safety. Nightly full-corpus
  matrix (≤256 jobs, 5-day self-hosted cap) + per-PR smoke subset as
  required checks.

EC2 Mac fits persistent runners only: the 24h-minimum Dedicated Host
allocation contradicts per-PR elastic matrix/ephemeral runners — there
is no elasticity story on EC2 Mac. Intel oracle: retain EC2 `mac1.metal`
or a used Intel mini with a post-macOS-27 sunset plan (docs 15, 18);
the ARM-mini cost model above does not cover it.

## Limits + retention (Tier: S)

- Job execution cap: 6h GitHub-hosted vs 5 days self-hosted; queue
  timeout 24h; matrix max 256 jobs:
  [limits](https://docs.github.com/en/actions/reference/limits).
- Artifacts/logs default 90-day retention (public 1–90d, private
  1–400d); from Oct 2026 retention also covers checks/runs/statuses:
  [retention](https://docs.github.com/en/organizations/managing-organization-settings/configuring-the-retention-period-for-github-actions-artifacts-and-logs-in-your-organization).
- Worst-case math before scaling: 10x mac:min ratio × 256-job matrix ×
  6h cap, screenshot/trace volume × 90d default retention ×
  $0.25/GB-mo, retry-x2 multiplier on flaky GPU tests. Set short
  retention on bulky legs; pin goldens in releases.

## Gating + reporting (Tier: S–M)

- Required status checks on protected branches must pass before merge;
  skipped jobs report Success and never block:
  [status checks](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/collaborating-on-repositories-with-code-quality-features/about-status-checks).
- Actions emits Checks (rich logs/annotations, PR Checks tab), creatable
  via REST checks API — endpoint details not directly fetched (doc 10).
- Status badges via `actions/workflows/FILE/badge.svg` with
  branch/event params; private-repo badges not embeddable externally:
  [badges](https://docs.github.com/en/actions/monitoring-and-troubleshooting-workflows/adding-a-workflow-status-badge).
- Nightly reruns feed the dashboard (pass rate, p95 duration, flake
  rate); long-term dashboard store unchosen (doc 10).

## Security (Tier: S–M)

- Self-hosted runners should almost never serve public repos — fork PRs
  can compromise runner and secrets; isolate via runner groups:
  [hardening](https://docs.github.com/en/actions/security-guides/security-hardening-for-github-actions).
- Secrets (except `GITHUB_TOKEN`) are withheld from fork-triggered
  runs; use env secrets, OIDC to cloud, base64-blob pattern for
  signing certs:
  [secrets](https://docs.github.com/en/actions/security-guides/using-secrets-in-github-actions).
- Signing certs as base64 env secrets with required reviewers; OIDC
  for cloud uploads.
- Scheduled workflows use POSIX cron, run only on the default branch,
  may delay under load, auto-disable after 60d inactivity on public
  repos:
  [events](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows).

## Checklist

- [ ] Phase topology chosen with 24h-min / SIP / fork-PR constraints honored
- [ ] Per-PR smoke subset as required checks; nightly full-corpus matrix
- [ ] Artifact retention short on bulky legs; goldens pinned in releases
- [ ] Worst-case cost math filed before enabling the 256-job matrix
- [ ] Fork PRs run hosted-only; self-hosted runners isolated + secret-free
- [ ] README badges + Checks-tab annotations live; dashboard fed by nightly
