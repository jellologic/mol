# 18 — macOS Test Agent

The Mac-side leg of the harness (doc 17): runs each test on real Mac
hardware and captures ground-truth outputs mol must match. Resolved
form: owned Apple Silicon mini + Tart ephemeral VM + self-hosted
runner agent + SIP-safe collectors. Tiers use the doc-README legend.

## Topology (Tier: S–M)

- Self-hosted runner on an owned mini or Tart VM (repo/org/enterprise
  level; never serve public repos from persistent runners):
  [add runners](https://docs.github.com/en/actions/how-tos/manage-runners/self-hosted-runners/add-runners).
- Tart VMs on Apple Silicon via Virtualization.Framework, near-native,
  OCI push/pull for versioned golden images, CI integration:
  [Tart](https://raw.githubusercontent.com/cirruslabs/tart/main/README.md),
  [quick-start](https://tart.run/quick-start/).
- Clones per run (`tart clone` + `tart run`, macOS 13+ Apple Silicon)
  keep runs hermetic; see doc 15 P1 for cost math.
- Legal cap: macOS guests only on Apple-branded hardware, max 2 VM
  instances per host (SLA §2B(iii), doc 08) — this caps parallel
  hermetic-run density on owned minis.

## Arch matrix (Tier: M)

Test Universal + Intel slices on both hardware kinds. Rosetta
translates Intel apps on Apple Silicon but is curtailed after macOS 27:
[Apple support](https://support.apple.com/en-us/102527).
Apple's canonical arch-coverage reference:
[universal binaries](https://developer.apple.com/documentation/apple-silicon/building-a-universal-macos-binary).
Keep Intel coverage via EC2 `mac1.metal` or a retained used Intel mini
with a post-macOS-27 sunset plan (doc 15).

## Runners (Tier: S)

| Runner | Use | Source |
|--------|-----|--------|
| Shell runner | CLI corpus via `/usr/bin` shims + `xcrun` toolchain dispatch | [TN2339](https://developer.apple.com/library/archive/technotes/tn2339/_index.html) |
| `xcodebuild test` | Unit/GUI tests: `-workspace/-scheme/-destination`, build-for-testing + test-without-building split, `-only-testing/-skip-testing`, `-resultBundlePath`, `-enableCodeCoverage` | [TN2339](https://developer.apple.com/library/archive/technotes/tn2339/_index.html), [xcodebuild.1](https://keith.github.io/xcode-man-pages/xcodebuild.1.html) |

## Collectors

SIP-safe by default — hosted runners cannot disable SIP, so root-only
DTrace/dtruss must never be the sole capture path:

| Collector | What it captures | Tier | Source |
|-----------|------------------|------|--------|
| `xcrun xctrace record --template 'System Trace'` | Syscall/API trace, parse `.trace` XML, no SIP disable (mactrace model) | S | [mactrace](https://raw.githubusercontent.com/jarred-sumner/mactrace/HEAD/README.md) |
| `fs_usage -w -f filesys\|pathname\|network\|exec\|diskio` | Path+IO enrichment (xctrace gives register values only, no dereferenced path strings); root-only, PID/cmd filter, `-t` timeout | S | [fs_usage](https://ss64.com/mac/fs_usage.html), [mactrace](https://raw.githubusercontent.com/jarred-sumner/mactrace/HEAD/README.md) |
| `log stream/show/collect` | Live ULS tail, predicate filtering, json/syslog styles, `.logarchive` bundles | S | [log](https://ss64.com/mac/log.html) |
| `dtrace` / `dtruss` | Generic probes (`-p pid`, `-c cmd`); per-process syscall details (`-p/-n/-f/-t`). Root-only AND needs SIP exemption — owned-minis/Tart only, never hosted | M | [dtrace](https://www.unix.com/man_page/osx/1/dtrace/), [dtruss](https://www.unix.com/man_page/osx/1m/dtruss/), [SIP](https://support.apple.com/en-us/102149) |
| `screencapture` | png/jpg/pdf/tiff, `-l` windowid, `-R` rect, `-T` delay, `-x` silent, `-v/-V` video; headless runs must sit in the loginwindow bootstrap via `launchctl bsexec`; needs Screen-Recording TCC grant | S–M | [screencapture](https://ss64.com/mac/screencapture.html) |
| ScreenCaptureKit | Programmatic display/window capture (SCStream/SCShareableContent API surface JS-gated, uninspected — doc 10) | L | [ScreenCaptureKit](https://developer.apple.com/documentation/screencapturekit) |
| AX dump tool | Accessibility hierarchy (roles/attributes/actions/notifications) — the stable text-comparable surface for parity diffs, immune to pixels | M | [AX model](https://developer.apple.com/library/archive/documentation/Accessibility/Conceptual/AccessibilityMacOSX/OSXAXmodel.html) |
| Mach-msg shim logger | Interposed `mach_msg` logging (msg-id→method decode still sourceless — doc 10) | L | doc 14 |

## Install + service (Tier: M)

- launchd agents (per-user, WindowServer access for GUI capture) vs
  daemons (root, no WindowServer; IPC via Unix-domain sockets);
  on-demand via sockets/Mach ports preferred:
  [launchd jobs](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingLaunchdJobs.html),
  [TN2083](https://developer.apple.com/library/archive/technotes/tn2083/_index.html).
- `launchctl load/unload`; LaunchDaemons must be root-owned, not
  group/world-writable:
  [launchctl](https://www.unix.com/man-page/osx/1/launchctl/).
- Modern `SMAppService` daemon registration details uninspected
  (JS-blocked) — doc 10.
- Packaging: `pkgbuild` (`--root` DSTROOT or `--component`, `--scripts`,
  stable `--identifier` for upgrades) + `productbuild` flat `.pkg`
  (`--distribution/--package-path`):
  [pkgbuild](https://keith.github.io/xcode-man-pages/pkgbuild.1.html),
  [productbuild](https://keith.github.io/xcode-man-pages/productbuild.1.html).
- Notarization/stapling install path on fresh minis not inspected — doc 10.

## Result export (Tier: S–M)

- `xcresulttool get/export/merge/compare` turns `.xcresult` into JSON
  summaries, attachments, merged baselines for upload:
  [xcresulttool](https://keith.github.io/xcode-man-pages/xcresulttool.1.html).
- Upload transport has no Apple primary source; generic CI store over
  HTTPS assumed (doc 10). Pin the golden-trace redistribution review
  from doc 15 before sharing fixtures.

## Checklist

- [ ] Owned mini + Tart VM reproduces hosted-runner traces for Tier 1 corpus
- [ ] Default capture is SIP-safe (xctrace + fs_usage + log); DTrace only on owned iron
- [ ] Universal + Intel slices tested per doc 15 arch plan
- [ ] AX dumps captured for every GUI case (doc 21 assertions)
- [ ] Agent installs via signed pkg + launchd job; notarization path cleared
- [ ] 2-VM-per-host cap reflected in parallel-run capacity math
