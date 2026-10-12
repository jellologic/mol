# 23 — Dependency Philosophy

Project default: **handroll whenever possible within reason** (see doc 01).
This doc defines "within reason": what to handroll, what to adopt, and the
seams that keep either choice reversible. Evidence: musl, relibc/Redox,
suckless, OpenBSD, Cargo Book, Rust API guidelines.

## Core rules

1. **Handroll Mac-observable behavior; adopt invisible infrastructure.**
   Anything whose quirks a guest binary can observe (syscall numbers, errno
   values, Mach message layout, dyld bind order) is handrolled so mol owns
   the quirks. Anything the guest cannot see (TLS, Unicode tables, codecs)
   is adopted from a mature crate — see doc 26.
   Rationale: musl favors "simple algorithms, minimal abstractions,
   self-contained uncoupled code" and correctness via careful spec reading
   ([about](https://musl.libc.org/about.html)).
2. **Keep the compat core dependency-free; push third-party code to edge
   adapters.** OpenBSD keeps third-party code out of the audited base: each
   app lives in ports as a Makefile with explicit deps, build, and test
   steps ([ports](https://www.openbsd.org/faq/ports/ports.html)). mol
   equivalent: `mol-macho`, `mol-syscall`, `mol-mach`, `mol-dyld` stay
   dependency-minimal; adapters (`mol-metal-vk`, audio, GUI) hold the deps.
3. **Put every handrolled unit behind a trait seam.** Rust traits define
   shared behavior across types, giving a seam where a handrolled impl can
   later be swapped for an adopted one
   ([book](https://doc.rust-lang.org/book/ch10-02-traits.html)). Use a
   custom trait as an abstraction seam with a blanket impl, enabling
   alternate impls and top-level type erasure
   ([patterns](https://rust-unofficial.github.io/patterns/patterns/structural/trait-for-bounds.html)).
   The musl-gcc thin-wrapper pattern shows how a shim defers commitment to
   one backend ([about](https://musl.libc.org/about.html)).
4. **Spec-first, with a quirk diff log.** musl-vs-glibc proves
   spec-conformant reimplementations still diverge on quirks (printf `%Ld`,
   sticky EOF, regex API), so identical-behavior goals need quirk-by-quirk
   differential tests
   ([diff log](https://wiki.musl-libc.org/functional-differences-from-glibc.html)).
   mol keeps the same style of log per subsystem (Apple behavior vs mol).
5. **Differential-test against a Mac oracle in layers.** relibc requires
   every MR to pass the C test suite with platform backends isolated under
   `src/platform/{redox,linux}`
   ([README](https://raw.githubusercontent.com/redox-os/relibc/master/README.md));
   Redox tests in layers (acid, relibc-tests, os-test), rebuilds and
   boot-tests on every change, and treats real-hardware runs as catching
   more than QEMU
   ([practices](https://doc.redox-os.org/book/testing-practices.html)).
   mol mirrors this: unit → integration → Mac-oracle differential (doc 14).
6. **Re-audit handrolled code on every behavior change.** OpenBSD audits
   file-by-file, re-audits when new flaw classes appear, and accepts
   diminishing returns on mature code
   ([security](https://www.openbsd.org/security.html)). Fewer deps remove
   mock/shim burden and version-drift failures but move cost into
   unit+integration+differential tests mol owns. Rust types cannot verify
   behavior (`add_two` may return +10), so handrolled parsers/loops need
   both unit (isolated, incl. private fns) and integration (public API)
   tests ([book](https://doc.rust-lang.org/book/ch11-00-testing.html)).
7. **Pin everything.** `Cargo.lock` pins exact revisions for reproducible
   builds; fewer deps shrink the lock surface and the blast radius of
   updates
   ([guide](https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html)).
   See doc 28 for the lock/vendor/deny pipeline.

## Decision matrix

| Criterion | Handroll | Adopt |
|-----------|----------|-------|
| Mac-observable incl. quirks | Default + differential tests | Only if the crate provably reproduces the quirks |
| Untrusted-input parsing, security-critical | Only with fuzzing + re-audit | Mature audited crate (default) |
| Small / stable / spec-defined (loops, shims, tiny parsers) | Default | Never |
| Large / complex / fast-moving (Unicode, regex, crypto, TLS) | Never (doc 26) | Default |
| Dep drags a transitive tree or hidden behavior | Handroll or vendor+pin | Only leaf-quality deps (doc 24) |
| Upstream may drift from Mac behavior | Wrap in a trait seam either way | Wrap in a trait seam either way |
| Test budget small | Adopt (pay mock/drift tests) | Handroll needs unit+integration+differential suites |

API guidance for the seams: C-GENERIC (minimize parameter assumptions with
generics) and C-INTERMEDIATE (expose intermediate results to avoid duplicate
work)
([guidelines](https://rust-lang.github.io/api-guidelines/flexibility.html)).

## Reconciled contradictions (cross-evidence review)

- **Handroll-default vs never-handroll:** no conflict once scoped. Doc 26
  items (crypto, Unicode, regex, TLS, YAML, codecs, storage) are
  Mac-*invisible* infrastructure: adopt. Mac-*observable* behavior with
  trusted or fully-specified inputs: handroll. Untrusted-byte parsers that
  are also Mac-observable (Mach-O, plists from guests): handroll only
  behind a fuzzer + differential suite (doc 13); see OQ-6.
- **Logging (`log` vs `tracing`):** libraries and the compat core use the
  zero-dep `log` facade (S). Binaries/servers (`mol-server`,
  `mol-launchd`) may add `tracing` + subscriber only when span correlation
  is needed; its tree cost is unmeasured (doc 24).
- **Async (`mio` vs `tokio`):** default to `mio` Poll or std threads for
  the Mach reactor (S/M). `tokio` with `rt+sync` only is allowed where
  spawned tasks genuinely help; never `tokio/full` (doc 24).

## New-dependency checklist

- [ ] No Mac-observable quirk is delegated without a differential test.
- [ ] `default-features = false` + explicit features (doc 24).
- [ ] Transitive tree measured with `cargo tree -p <crate>` (normal,
      build, and host edges) and recorded in doc 24.
- [ ] MSRV/edition checked against `rust-toolchain.toml` (doc 28).
- [ ] License on the `cargo deny` allow-list (doc 28).
- [ ] Behind a trait seam if upstream could drift from Mac behavior.
- [ ] `Cargo.lock` + `vendor/` updated in the same change (doc 28).

## Tiers

S: adopt a leaf crate behind a seam. M: shim over a Linux equivalent.
L: handrolled subsystem with its own differential suite. XL: never-handroll
territory — adopt, never reimplement (doc 26).
