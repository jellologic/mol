# 28 — Vendoring and Supply Chain

Concrete policy for dependency-minimal Rust + PPA debs. Tiers: S for each
pipeline step (tooling-led), M for the initial deny/vendoring setup.

## 1. Lockfile discipline

- Commit `Cargo.lock`; keep `Cargo.toml` ranges minimal. The lock holds
  exact resolved versions and should be checked into VCS; manifests declare
  broad ranges
  ([guide](https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html)).
- Deterministic CI flags: `--locked` asserts the lock is fresh, `--offline`
  blocks all network, `--frozen` equals both
  ([build](https://doc.rust-lang.org/cargo/commands/cargo-build.html)).
- `cargo fetch` pre-downloads locked registry+git deps so later commands
  run offline unless the lockfile changes
  ([fetch](https://doc.rust-lang.org/cargo/commands/cargo-fetch.html)).

## 2. Vendoring

- Vendor with `cargo vendor --locked --versioned-dirs vendor/` +
  `[source.crates-io] replace-with = "vendored-sources"` in
  `.cargo/config.toml`. `cargo vendor` copies all crates.io+git deps into a
  dir and prints the config snippet; vendored sources are read-only — use
  `[patch]`/path deps to modify. Supports `--versioned-dirs` for history
  tracking, `-s/--sync` for extra manifests, `--locked`/`--offline`
  ([vendor](https://doc.rust-lang.org/cargo/commands/cargo-vendor.html)).
- Source replacement via `[source.crates-io] replace-with` +
  `[source.vendored-sources] directory = "vendor"` covers vendoring and
  registry mirroring; the replacement must contain identical code, no extra
  crates
  ([replacement](https://doc.rust-lang.org/cargo/reference/source-replacement.html)).
- Ship the vendor tarball inside the `.dsc`, since Debian builds forbid
  network: required build targets must not access network except loopback
  services started by the build
  ([policy](https://www.debian.org/doc/debian-policy/ch-source.html)).
- Outage fallback: the vendored directory source is primary; an optional
  registry mirror goes through a second `replace-with` registry URL.

## 3. Version floor / ceiling CI

- Nightly floor job: `cargo +nightly update -Z direct-minimal-versions`
  then `check --locked`. `-Z direct-minimal-versions` resolves direct deps
  to lowest satisfying versions; full `-Z minimal-versions` is explicitly
  not recommended
  ([unstable](https://doc.rust-lang.org/cargo/reference/unstable.html)).
- Scheduled stable job: `cargo update -w` then test (ceiling). `cargo
  update` refreshes the lock; with a spec it does a conservative update,
  `--precise` pins an exact version/git rev
  ([update](https://doc.rust-lang.org/cargo/commands/cargo-update.html)).
- The Cargo CI guide recommends a scheduled latest-deps job or
  Dependabot/RenovateBot update PRs over uncommitted lockfiles
  ([CI guide](https://doc.rust-lang.org/cargo/guide/continuous-integration.html)).

## 4. PR gates: deny + audit

Gate every PR on: `cargo build/test --locked --frozen`, `cargo deny check`,
`cargo audit` (or deny advisories alone to avoid duplication).

- **cargo-deny** lints the dep graph: advisories + bans + licenses +
  sources via `deny.toml`; quickstart is install+init+check with
  EmbarkStudios/cargo-deny-action for CI
  ([deny](https://embarkstudios.github.io/cargo-deny/),
  [check](https://embarkstudios.github.io/cargo-deny/cli/check.html)).
  - advisories: uses the RustSec DB incl. unmaintained-crate advisories,
    tunable via `unmaintained = "workspace"`
    ([advisories](https://embarkstudios.github.io/cargo-deny/checks/advisories/index.html)).
  - bans: blocks named crates, flags duplicate versions of the same crate
    ([bans](https://embarkstudios.github.io/cargo-deny/checks/bans/index.html)).
  - sources: restricts registries/git remotes, can enforce
    vendored-file-only deps
    ([sources](https://embarkstudios.github.io/cargo-deny/checks/sources/index.html)).
  - licenses: evaluates every crate against an SPDX allow-list with
    Cargo.toml-then-LICENSE-files precedence and manual clarify overrides;
    all licenses denied unless in `allow`, SPDX WITH-exception support,
    include-dev/include-build toggles
    ([licenses](https://embarkstudios.github.io/cargo-deny/checks/licenses/index.html),
    [cfg](https://embarkstudios.github.io/cargo-deny/checks/licenses/cfg.html)).
  - Suggested `deny.toml`: allow MIT/Apache-2.0 (+LLVM-exception);
    `unmaintained = "deny"`; duplicates deny/warn; sources allowlist
    crates-io only.
- **cargo-audit** scans `Cargo.lock` against the RustSec Advisory DB
  (requires Rust 1.74+): `cargo install cargo-audit --locked`, `cargo
  audit` ([rustsec](https://rustsec.org/),
  [README](https://github.com/rustsec/rustsec/blob/main/cargo-audit/README.md)).
  Categories: vulnerability (fail default), unmaintained/unsound/yanked
  informational; harden with
  `cargo audit --deny warnings|unmaintained|unsound|yanked`
  ([guide](https://tuxcare.com/blog/cargo-audit-rust-security/)).
  Advisories are exported to OSV/GitHub Advisory DB, enabling
  Dependabot/Trivy; CI pattern is the rustsec/audit-check action.
  `cargo audit` also supports `--ignore`/audit.toml ignores, an
  experimental `fix` subcommand, bin scanning, and `cargo-auditable` embeds
  dep trees in binaries
  ([audit](https://raw.githubusercontent.com/RustSec/rustsec/main/cargo-audit/README.md)).

## 5. Licenses + reproducibility

- License: deny licenses allow-list + `cargo tree` review on new deps;
  record `debian/copyright` from SPDX data. FFmpeg/BoringSSL picks (doc 26)
  are not yet checked against the allow-list (doc 10).
- Reproducibility: `--locked` builds, pinned toolchain
  (`rust-toolchain.toml`), vendor dir ensures offline; verify with `cargo
  fetch --locked` + `cargo build --frozen`. Debian defines reproducible
  builds as bit-for-bit identical binaries given same source path,
  build-deps, env, and arches
  ([policy](https://www.debian.org/doc/debian-policy/ch-source.html)).

## 6. Debian / PPA route

- PPA cannot use per-crate archive packaging (Debian Rust policy wants each
  crate as one source package `rust-foo`/`librust-foo-dev` with
  per-feature binaries and semver-suffixed co-installable variants via
  debcargo + dh-cargo,
  [policy](https://rust-team.pages.debian.net/book/policy.html)), so
  vendor-in-`.dsc` is the pragmatic route with `README.source` documenting
  regeneration (`cargo update` + `cargo vendor`).
- Tension (doc 10): vendor-in-`.dsc` directly contradicts Debian Policy's
  no-convenience-copies rule (use archived libs or package prereqs
  separately,
  [policy](https://www.debian.org/doc/debian-policy/ch-source.html)); PPA
  acceptance is unresolved, and the Launchpad no-network text itself is
  unfetched. Keep deps minimal to shrink the `debian/copyright` burden;
  each adopted dep needs versioned tracking and a RustSec watch.

## 7. Cadence

- Weekly automated update PR; immediate patch on a RustSec advisory;
  re-vendor on every lock change with diff review.

## Checklist (new dependency lands)

- [ ] `deny.toml` allow-list + sources check pass.
- [ ] `cargo audit` clean (or advisory triaged in audit.toml).
- [ ] `Cargo.lock` + `vendor/` re-generated and diff-reviewed.
- [ ] Floor (`direct-minimal-versions`) and ceiling (`update -w`) jobs green.
- [ ] `debian/copyright` entry added.
