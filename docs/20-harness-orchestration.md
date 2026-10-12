# 20 — Harness Orchestration

The control plane between the agents (docs 18, 19) and the CI gates
(doc 22): scheduling, the compare job, the flake lifecycle, the test
corpus, and containment for untrusted guest binaries. Everything here
serves one verdict per test-id: does mol behave identically to the Mac
leg? Tiers use the doc-README legend.

## Scheduling + fan-out (Tier: S)

- `push`/`pull_request` per-commit smoke, `schedule` nightly cron full
  corpus, `workflow_dispatch` on-demand:
  [events](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows).
- Matrix over `(macos-14, ubuntu-24.04)` with `fail-fast: false` +
  `continue-on-error` so both legs always finish for comparison:
  [matrix](https://docs.github.com/en/actions/using-jobs/using-a-matrix-for-your-jobs).
- Per-leg `upload-artifact`, compare-job `download-artifact`:
  [upload-artifact](https://github.com/actions/upload-artifact).

## Compare job (Tier: M)

Downloads both legs' JUnit + artifacts, normalizes, diffs per
test-id → MATCH/DIVERGE/FLAKE (doc 17). Normalization rules are
harness-authored — no primary source standardizes cross-OS result
normalization (doc 10):

| Rule | Strips / rewrites | Grounded in |
|------|-------------------|-------------|
| strip-timing | Durations, timestamps, `log(1)` mach timestamps | harness-authored |
| strip-paths | Absolute paths → `$ROOT`-relative | doc 11 divergences |
| strip-identity | PIDs/TIDs, ASLR/dyld addresses → module+offset, Mach port names, `.xcresult` UUIDs, `LC_UUID` per-link randomness | doc 11 + harness-authored |
| strip-env | Locale/TZ/hostname, `PYTHONHASHSEED`, interface names | harness-authored |
| sort | Sort testcases/lines into canonical order | [Playwright reporters](https://playwright.dev/docs/test-reporters) |

Compare semantics: CLI behavior + accessibility trees compare
cross-platform after normalization; pixels/traces compare only against
same-platform baselines (doc 21) — there is no universal numeric
cross-platform threshold.

## Flake lifecycle (Tier: M)

- Flaky = same test passes and fails on the same commit; retry within
  the build detects it:
  [GitHub eng](https://github.blog/engineering/engineering-principles/reducing-flaky-builds-by-18x/).
- Retry policy: re-run only failures (`--rerun-fails`), max attempts 2:
  [gotestsum](https://github.com/gotestyourself/gotestsum).
- Quarantine: flaky tests move to an advisory lane and stop blocking
  builds: [Buildkite](https://buildkite.com/docs/test-analytics).
- Registry key is `(test, platform, renderer)` — never test alone.

## Test corpus (Tier: S–XL by tier)

Layout: `corpus/{abi,cli,macho,ipc,xpc,workloads/{python,node,git,coreutils},gui,blobs}`
+ `corpus.lock` (pinned versions + digests) + per-dir README. Sources
in git; Mach-O edge binaries + workload tarballs in git-lfs
([git-lfs](https://git-lfs.com/)) or digest-addressed CAS
([REAPI CAS](https://raw.githubusercontent.com/bazelbuild/remote-apis/main/build/bazel/remote/execution/v2/remote_execution.proto)).

| Tier | Content | Sources |
|------|---------|---------|
| T1 abi+cli+macho (first, small) | Wine LGPL-2.1+ per-DLL micro-test model compiled against both native and compat libs; Darling loader split (LC_SEGMENT/LC_MAIN/dylib search/@rpath); XNU `loader.h` edge surface (APSL-2.0); Bats MIT TAP shell tests | [COPYING.LIB](https://raw.githubusercontent.com/wine-mirror/wine/master/COPYING.LIB), [Darling README](https://raw.githubusercontent.com/darlinghq/darling/master/README.md), [loader doc](https://raw.githubusercontent.com/darlinghq/www.darlinghq.org/master/content/developer-zone/mach-o-dynamic-loader.md), [loader.h](https://raw.githubusercontent.com/apple-oss-distributions/xnu/main/EXTERNAL_HEADERS/mach-o/loader.h), [Bats](https://raw.githubusercontent.com/bats-core/bats-core/master/README.md) |
| T2 mach-ipc+xpc (medium) | MPSC port-queue primitives via `mach_msg`, `bootstrap_look_up`; XPC typed objects over Mach IPC + libdispatch — libxpc never open-sourced, so clean-room micro-tests from public headers | [mach-ports](https://raw.githubusercontent.com/darlinghq/darling-docs/master/src/internals/macos-specifics/mach-ports.md), [libxpc plan](https://pkgdemon.github.io/freebsd-libxpc-plan.html) |
| T3 workloads (large, triaged) | CPython PSF-2.0 `regrtest` (`-j`, `-u`); Node MIT `test/parallel` + `common.mustCall`; git GPLv2-only TAP `t/*.sh` (`prove -j`, `GIT_TEST_*`); coreutils GPLv3 via uutils `fetch-gnu.sh` pin + `build-gnu.sh` PATH-injection | [CPython](https://docs.python.org/3.13/library/test.html), [Node tests](https://raw.githubusercontent.com/nodejs/node/main/test/README.md), [git t/README](https://raw.githubusercontent.com/git/git/master/t/README), [fetch-gnu.sh](https://raw.githubusercontent.com/uutils/coreutils/main/util/fetch-gnu.sh) |
| T4 gui smoke (late) | Blocked on graphics stack; no ready-made macOS GUI smoke corpus inspected — author per doc 21 | [Darling README](https://raw.githubusercontent.com/darlinghq/darling/master/README.md) |

Licensing: vendor PSF-2.0 / MIT / APSL-2.0 headers freely with notices
retained; keep GPLv2 (git) and GPLv3 (coreutils) suites as unmodified
pinned snapshots or external-fetch scripts to avoid copyleft mixing;
Wine LGPL tests only as standalone binaries; growth policy (M1 T1 +
CAS online → M2 T2 + subsets → M3 full workloads → M4 gui) is a
proposal, not inspected evidence (doc 10).

Hermetic-input rule: `corpus.lock` must pin the guest-image digest
(doc 19) alongside workload digests — otherwise OCI-versioned images
drift against CAS-pinned workloads undetectably, and any
external-fetch script breaks per-case hermeticity. Prefer vendored
snapshots over fetch-at-run.

## Containment (Tier: M–L)

The corpus executes arbitrary Mach-O binaries plus network-capable
suites (git/node) — contain the guest:

- VM network egress default-deny with an allow-list for fetch-script
  hosts; record every egress in the run log.
- Least-privilege guest-exec (qemu-ga/SSH user, never root where
  avoidable); secrets never mounted into the guest.
- Treat the result-upload channel as untrusted input: size caps,
  schema validation, no path traversal on artifact names.
- Fork-PR runs stay on GitHub-hosted legs only (doc 22).

## Checklist

- [ ] Per-commit smoke + nightly full + on-demand dispatch wired
- [ ] Compare job normalizes per the rule table, then verdicts per test-id
- [ ] Retry-x2 + `(test, platform, renderer)` flake registry + quarantine lane
- [ ] `corpus.lock` pins workloads, blobs, AND guest-image digests
- [ ] GPL suites vendored as pinned snapshots or pinned fetch scripts
- [ ] Guest egress allow-listed; upload channel validated; secrets out of guests
