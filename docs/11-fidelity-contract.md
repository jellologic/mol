# 11 — Fidelity Contract

What "functions identically to Mac hardware" means, pinned per surface.
Anything not listed here is best-effort until a golden trace says otherwise.

## Compat boundary

- The supported boundary is **link-against-libSystem**. The
  libSystem-to-kernel interface (incl. raw `syscall()`) is private with no
  ABI guarantee:
  [HN thread](https://news.ycombinator.com/item?id=9848411).
- BSD syscalls alone are incomplete: Mach IPC is required (e.g. `sleep()`
  uses Mach messages):
  [same](https://news.ycombinator.com/item?id=9848411).
- `syscalls.master` marks many calls `NO_SYSCALL_STUB` (exit/fork/open):
  the libSystem wrapper does extra work beyond the trap, so the wrapper
  — not the trap — is the contract:
  [syscalls.master](https://raw.githubusercontent.com/apple-oss-distributions/xnu/main/bsd/kern/syscalls.master).
- Tension (open): the differential harness (doc 14) diffs at syscall
  level, but this boundary says traps are private. Resolution: gate
  releases on libSystem-observable behavior; use syscall diffs as
  diagnostic signal with an allow-list for known-divergent calls.

## Fidelity tiers

| Tier | Meaning | Gated by |
|------|---------|----------|
| T0 bit-exact | Byte/errno/number identical | golden hashes fail the build |
| T1 ulp-bounded | Numerics/clocks within measured Apple envelope | tolerance comparators |
| T2 semantic | Same allow/deny, same lookup success, same UTI conformance | boolean matrices |
| T3 best-effort | No contract; tracked, never gating | dashboards only |

## Observable behaviors (per surface)

| # | Surface | Contract | Tier | Effort | Sources |
|---|---------|----------|------|--------|---------|
| F1 | Syscall numbers/errno + wrapper side effects | Same errno names, same wrapper-observable effects | T0 | M | [syscalls.master](https://raw.githubusercontent.com/apple-oss-distributions/xnu/main/bsd/kern/syscalls.master) |
| F2 | Mach message IDs/replies | Same msg-id → reply semantics | T0 | XL | doc 04 |
| F3 | libm/vDSP output | Within measured Apple ulp envelope, not correctly-rounded: Apple `cosf` is not correctly rounded, `vDSP_DFT_zrop` differs ~29% bins vs portable | T1 | L | [mlx-omarchy analysis](https://github.com/joshuaswarren/mlx-omarchy/commit/bd00334c73b9b7c0af29f8cf7a2936ef3aadcb36) |
| F4 | Accelerate dispatch | Any tuned impl allowed, but stays inside T1 envelope; note it picks impls per HW/problem size and may auto-split over CPUs via GCD/pthreads | T1 | L | [vecLib man](https://www.manpagez.com/man/7/vecLib/) |
| F5 | ICU collation/case/normalization/dates | Pinned to OS ICU+CLDR: macOS 26 = Apple ICU 76142 (ICU76/CLDR46), Sequoia = 74-based; Apple ICU is a patched fork | T0 for pinned version | L | [locale survey](https://raw.githubusercontent.com/patrickgwsmith/qip/HEAD/drafts/locale-versions-survey.md), [ICU/CLDR](https://learn.microsoft.com/en-us/dotnet/core/extensions/globalization-icu) |
| F6 | `mach_absolute_time` + timebase | `numer/denom` conversion correct, monotonic; `denom==0` = uninitialized; absolute ratio value may differ | T1 | M | [QA1398](https://discmaster.textfiles.com/file/12650/developer.apple.com.tar/developer.apple.com/mac/library/qa/qa2004/qa1398.html) |
| F7 | APFS lookup semantics | Case-insensitive, normalization-preserving, normalization-insensitive lookups (HFS+ enforced NFD); 255 UTF-8 bytes/component, PATH_MAX 1024; Finder `:` ↔ POSIX `/` translation | T2 | L | [APFS notes](https://raw.githubusercontent.com/rapjul/filename-checker/HEAD/docs/filesystems/apfs.md) |
| F8 | xattr/FinderInfo/resource fork | Resource fork via xattr position arg; UTF-8 names; `XATTR_NOFOLLOW`/`SHOWCOMPRESSION`; AppleDouble round-trips xattrs+ACLs+fork+FinderInfo (`COPYFILE_*` flags compose) | T0 bytes | M | [getxattr](https://www.manpagez.com/man/2/getxattr/), [go-darwin/apfs](https://pkg.go.dev/go-darwin.dev/apfs) |
| F9 | clonefile | CoW block sharing, own attrs/xattrs/ACLs; dst must not exist; recursive for dirs; post-clone overwrite may return ENOSPC | T2 (+T0 bytes) | M | [clonefile](https://www.manpagez.com/man/2/clonefile/) |
| F10 | getattrlist | attrlist-bitmap-driven enhanced stat incl `ATTR_CMN_FILEID/OBJID`, FinderInfo; volume-dependent support | T2 | M | [getattrlist](https://www.manpagez.com/man/2/getattrlist/) |
| F11 | Sockets | `AF_INET6=30`, `AF_SYSTEM=32`/`PF_SYSTEM`, `sa_len` length byte in sockaddr structs; missing `sa_len` breaks `bind()` | T0 layout | M | [socket.h](https://raw.githubusercontent.com/apple-oss-distributions/xnu/main/bsd/sys/socket.h), [beefytech](https://github.com/beefytech/beef/commit/315e573a398c2f37f9e5f38647fe37fc240f89d9) |
| F12 | Network stack extras | 4.4BSD-derived + DLIL to IOKit; NKE/KPI filters | T3 | XL | [networking chapter](https://flylib.com/books/en/3.126.1.30/1/) |
| F13 | Sandbox | Inherited by children, enforced at acquisition; pre-opened fds stay usable; `sandbox_init` deprecated (NoInternet/NoNetwork/NoWrite/PureComputation), App Sandbox is current; `sandbox-exec -f/-n/-p -D` selection | T2 allow/deny | L | [sandbox.7](https://www.manpagez.com/man/7/sandbox/), [sandbox_init.3](https://www.manpagez.com/man/3/sandbox_init/), [sandbox-exec.1](https://www.manpagez.com/man/1/sandbox-exec/) |
| F14 | Codesign shape | Entitlement data embedded/preserved; each fat arch verified separately | T2 | S–M | [codesign.1](https://www.manpagez.com/man/1/codesign/) |
| F15 | Pasteboard/UTI | Multiple representations per item; custom UTIs carry full-fidelity in-app data; drag-drop = one-step copy-paste; UTI harmonizes pasteboard+file types | T0 bytes / T2 conformance | M | [UTI history](https://appleinsider.com/articles/09/09/22/inside_snow_leopards_uti_apple_fixes_the_creator_code), [copy-paste](https://learn.microsoft.com/en-us/previous-versions/xamarin/mac/app-fundamentals/copy-paste) |

Open per-surface gaps: per-release Apple libm ulp tables + Accelerate
per-CPU dispatch map; Apple ICU fork patch list; CFAbsoluteTime
epoch/calendars primary; SBPL grammar + entitlement catalog;
carry-flag error convention + Mach trap numbers primary; pboard wire
protocol + promised-file staging; PF_SYSTEM family details
(see doc 10).

## Acceptable divergences (never gated)

PIDs; ASLR addresses (normalize to module+offset); absolute paths
(rewrite to `$ROOT`-relative); wall-clock values; timebase ratio value
(conversion must still hold); HW dispatch choice (must stay in T1);
volume UUIDs; interface names.

## Testing contract (how the tiers are enforced)

Adopt the Android/Weave/relibc model into mol's CI gate:

- Golden-guest corpus (Cuttlefish model: configurable device with full
  framework fidelity for compliance testing):
  [Cuttlefish](https://source.android.com/docs/devices/cuttlefish).
- CTS-like CI gate run per build against corpus + emulator:
  [CTS](https://source.android.com/docs/compatibility/cts).
- CDD-style policy doc (this file + doc 16) enumerating must-meet
  requirements: [CDD](https://source.android.com/docs/compatibility/cdd).
- Weave-style `status.yaml` export registry (2506 registered exports,
  214 milestone-qualified gauge in Weave):
  [Weave README](https://raw.githubusercontent.com/The-Libre-Project/Weave/main/README.md).
- relibc-style `check.sh` arch matrix (`--test/--host/--arch` over
  x86_64/aarch64/…), C tests green per MR:
  [relibc](https://gitlab.redox-os.org/redox-os/relibc/-/raw/master/README.md).
- Tier: M (harness glue; corpus content grows per milestone).

## Checklist

- [ ] Every F-item has a named suite + comparator (hash/tolerance/boolean)
- [ ] Divergence list applied in the normalizer, not per-test (doc 14)
- [ ] `status.yaml` registry tracks each F-item's gate state per milestone
- [ ] T0/T1/T2 failures block merge; T3 failures file issues only
