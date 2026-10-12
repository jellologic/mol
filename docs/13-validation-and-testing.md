# 13 — Validation and Testing

Recommended stack: Box64-style golden CTest + FEX-style ASM/syscall
suites + dtruss-vs-strace diff + record/replay traces + hyperfine
benches + reg-suit snapshots + AFL++/libFuzzer parser harnesses +
MACHFUZZER-model MIG fuzzer. Tiers use the doc-README legend
(S = embed dep, M = shim, L = subsystem re-impl, XL = full re-impl).

## Conformance suites (behavioral drift)

| Method | Catches | Tier | Source |
|--------|---------|------|--------|
| Wine per-DLL `tests/*.c` on `wine/test.h`, run on Windows and Wine, winetest dashboard | API behavioral drift | XL (suite scale is the cost) | [winetest](https://test.winehq.org/data/) |
| Wine `ntdll/tests` (~20 files: file, pipe, reg, exception); one test C file per API area, cross-built via crosstest | Per-area regressions | XL | [ntdll tests](https://github.com/wine-mirror/wine/tree/master/dlls/ntdll/tests) |
| Darling `tests/src` smoke tests (fcntl, fork_wait, mach_semaphore, libproc, objc_*, pthread, tls) | Basic syscall/ObjC regressions | S | [darling tests](https://github.com/darlinghq/darling/tree/master/tests/src) |
| Box64 CTest + `runTest.cmake` golden-ref stdout compare (`testNN` vs `refNN.txt`, 36+ tests) | Emulation regressions incl FPU/SSE/x87 | S | [tests CMake](https://raw.githubusercontent.com/ptitSeb/box64/main/tests/CMakeLists.txt) |
| FEX unittests: handwritten ASM + gcc-target-tests + posixtest + gvisor bins via CTest/python | ISA + syscall drift | M | [unittests readme](https://raw.githubusercontent.com/FEX-Emu/FEX/main/unittests/Readme.md) |
| ABI Compliance Checker: ABI dumps from headers/shared objects compared across versions | ABI breaks (symbols, vtables, calling conv) | S–M | [README](https://raw.githubusercontent.com/lvc/abi-compliance-checker/master/README.md) |
| Apple `shell_cmds` OSS (find, kill, date, hexdump, xargs-class) as CLI corpus speaking real syscalls | CLI behavioral drift | S | [shell_cmds](https://github.com/apple-oss-distributions/shell_cmds/tree/main) |
| `apple-oss-distributions/Libc` headers/tests reuse | libc conformance | M | [libc](https://github.com/apple-oss-distributions/libc) |
| `darwin-xnu/tests` (coalition_info, debug_control_port_for_pid, bpf, contextswitch) | Mach/syscall semantic drift | M | [xnu tests](https://github.com/apple/darwin-xnu/tree/main/tests) |
| gVisor `test/` syscall suite, local runner, no extra config | Coverage gaps | M | [gvisor test](https://raw.githubusercontent.com/google/gvisor/master/test/README.md) |
| LTP supported-syscalls corpus methodology | Coverage gaps | M | [LTP docs](https://linux-test-project.readthedocs.io/en/latest/) |

## Differential + record/replay

| Method | Catches | Tier | Source |
|--------|---------|------|--------|
| dtruss (DTrace-based) on Mac vs strace on Linux | Syscall semantic/errno diffs | M | [dtruss.1](https://www.manpagez.com/man/1/dtruss/) |
| strace (syscalls, signals, clock/id) as Linux half | Baseline capture | S | [strace.io](https://strace.io/) |
| rr record-once → deterministic replay under gdb, portable traces + chaos mode | Nondeterministic/Heisenbug failures | L | [rr](https://rr-project.org/) |
| QEMU record/replay: deterministic icount-based replay, single-CPU TCG, gdb reverse debug | Golden-trace model for syscall+IRQ streams | XL | [record/replay](https://wiki.qemu.org/Features/record-replay) |

Note: rr is Linux-only, so Mac-side capture must be macOS-native
(dtracing/xctrace/Instruments); rr covers the Linux replay side and
the trace-portability design. See doc 14.

## GUI + performance

| Method | Catches | Tier | Source |
|--------|---------|------|--------|
| reg-suit CLI snapshot image compare, HTML diffs, S3/GCS stores, git-hash keying, CI notifiers | GUI regressions | M | [reg-suit](https://raw.githubusercontent.com/reg-viz/reg-suit/master/README.md) |
| hyperfine statistical multi-run CLI benches (warmup, outlier detection, param scans, JSON/CSV) | Compat-vs-Mac latency drift | S | [hyperfine](https://raw.githubusercontent.com/sharkdp/hyperfine/master/README.md) |

Open: no NSEvent-timestamp / runloop-ordering / vsync timing tier;
GPU golden images need a vendor-tolerance metric (SSIM/perceptual),
not raw per-pixel; audio has no latency/jitter/clock tier yet
(see doc 10).

## CI matrix

| Method | Catches | Tier | Source |
|--------|---------|------|--------|
| GitHub-hosted runners (Ubuntu/Windows/macOS images) with arch matrix for ARM64+AMD64 | Arch-specific breaks | M | [hosted runners](https://docs.github.com/en/actions/using-github-hosted-runners/using-github-hosted-runners/about-github-hosted-runners) |
| FEX per-concern workflows (ccpp, instcountci, vixl_simulator, hostrunner) | Emulator regressions incl instruction-count drift | M | [FEX workflows](https://github.com/FEX-Emu/FEX/tree/main/.github/workflows) |

## Fuzzing

| Method | Catches | Tier | Source |
|--------|---------|------|--------|
| AFL++ (`afl-cc` + `afl-fuzz`, x86_64/arm64 docker images) on Mach-O parser + syscall black-box | Parser crashes/hangs | M | [AFL++](https://raw.githubusercontent.com/AFLplusplus/AFLplusplus/stable/README.md) |
| libFuzzer in-process `LLVMFuzzerTestOneInput` on targeted Mach-O/MIG parser functions | Targeted parser bugs | M | [libFuzzer](https://llvm.org/docs/LibFuzzer.html) |
| MACHFUZZER-class IDL/state-aware Mach IPC fuzzer (SRDS19) | macOS IPC vulns | XL (harness must be authored; paper is design only) | [paper (PDF)](https://szu-se.github.io/FuzzingPaper/Paper/SRDS19_MachFuzzer.pdf) |

Open: Wine Conformance-Tests wiki body is JS-gated (findings rest on
test sources + dashboard); no ready-made MIG `.defs` grammar fuzzer
exists; Apple GUI snapshot/XCTest specifics uninspected
(see doc 10).

## Checklist

- [ ] Golden-ref CTest wired per milestone corpus (Box64 pattern)
- [ ] ASM + syscall suites running on both arches (FEX pattern)
- [ ] Differential diff job green on Tier 1 corpus (doc 14)
- [ ] Record/replay attached to failing cases only (cost control)
- [ ] GUI snapshot job with keyed baselines (reg-suit pattern)
- [ ] hyperfine benches with regression thresholds (doc 16)
- [ ] Parser fuzzers at zero crashers over budget before release
- [ ] MIG fuzzer authored and running in CI (XL item, M3+)
