# 14 — Differential Harness

Mac-vs-Linux differential design: capture the same workload on real
Mac hardware and under mol, normalize away benign divergence, and
diff the rest. Diagnostic signal for syscall-level drift; releases
gate on doc 11 behavior, not on raw trace equality.

## Collectors

Mac side (capture must be macOS-native; rr is Linux-only):

| Collector | What it captures | Tier | Source |
|-----------|------------------|------|--------|
| DTrace script (`syscall:::entry/return`, arg capture); `dtruss` prints `SYSCALL(args)=retval+errno` per call — needs root and, under SIP, `csrutil enable --without dtrace` in recovery | Per-syscall args/retval/errno | M | [dtruss notes](https://jameshfisher.com/2017/08/01/dtruss/), [DTrace/dtruss](https://www.shogan.co.uk/devops/dtrace-dtruss-an-alternative-to-strace-on-macos/) |
| `fs_usage -w` with `-f filesys\|pathname\|network\|diskio`, PID/cmd scoping | Per-call TIMESTAMP/CALL/FD/BYTECOUNT/PATHNAME/ELAPSED path+IO enrichment | S | [fs_usage](https://ss64.com/mac/fs_usage.html) |
| lldb SB-API breakpoint tracer (`breakpoint command add --script-type python`, callbacks get SBFrame/SBThread/SBProcess, auto-continue) | Fallback tracing when SIP blocks DTrace | M | [lldb scripts](https://lldb.llvm.org/use/tutorials/breakpoint-triggered-scripts.html) |
| Mach-msg logger: interpose shim on `mach_msg` logging msg-id/size/ports (kernel-hook variant: inline-hook `mach_msg_overwrite_trap`/`ipc_kmsg_send`) | Mach IPC stream | L (shim) / XL (kernel hook) | [Mach sniffing](https://www.fortinet.com/blog/threat-research/inspecting-mach-messages-in-macos-kernel-mode--part-i--sniffing-) |

Linux side:

| Collector | What it captures | Tier | Source |
|-----------|------------------|------|--------|
| `strace -ff -yy -s -X`: name/args/retval per line, `-o/-ff/-e/-s/-y/-X`, `-c` count/time summary; multithread calls marked unfinished/resumed in event order | Raw baseline + ordering hook | S | [strace.1](https://man7.org/linux/man-pages/man1/strace.1.html) |
| eBPF tracepoints (`SEC(tracepoint/syscalls/sys_enter_openat)`, bpftrace `tracepoint:syscalls:sys_enter_*`) | Low-overhead CI capture | M | [eBPF lesson](https://github.com/corsair-cxs/ebpf-tutorial/blob/HEAD/lesson_8_tracepoint_en.md) |
| strace `--seccomp-bpf`: cBPF returning TRACE only for wanted syscalls (halved kernel-build trace time) | Filtered fast capture | S | [seccomp-bpf](https://pchaigno.github.io/strace/2019/10/02/introducing-strace-seccomp-bpf.html) |
| seccomp-notify supervisor (`SECCOMP_RET_USER_NOTIF` → `struct seccomp_notif` on a `NEW_LISTENER` fd) | Userspace syscall supervision/emulation/fault injection | M | [seccomp_filter](https://www.kernel.org/doc/html/latest/userspace-api/seccomp_filter.html) |

Open: exact Apple-Silicon DTrace probe surface after the csrutil
exemption (which `syscall:::return` probes still fault); userspace-only
Mach MIG semantic decoding (msg-id → method/args without kernel hooks
or private MIG tables); DTrace-vs-SIP incompatible with GHA-hosted
runners and unconfirmed on Tart/EC2 VMs (see doc 10, doc 15).

## Normalizer (Tier: M)

Strip timestamps/PIDs/TIDs; rebase ASLR addresses to module+offset via
dyld/ASLR slide capture; rewrite absolute paths to `$ROOT`-relative;
rewrite fds to open-order tokens; canonicalize errno names; sort
per-thread by seq and merge threads by Lamport/causal clock with
unfinished/resumed pairing; emit a canonical event stream.

Open: Lamport merge of unfinished/resumed pairs is unvalidated for
Mach IPC causality — needs a causality experiment before gating on
multi-threaded Mach traces.

## Differ (Tier: M)

- Per-syscall-class semantic comparators (file/net/proc/Mach) plus a
  strict byte mode.
- Allow-list of known-divergent calls (`stat64` vs `statx`, Mach-only
  IPC, …). The allow-list is a checked-in file with justification per
  entry; additions need reviewer sign-off (doc 16 regression policy).

## Coverage (Tier: M)

- Syscall-name bitmap + Mach msg-id bitmap per run.
- Optional: KCOV on Linux (per-task precise single-syscall coverage,
  interrupt noise skipped):
  [KCOV](https://www.kernel.org/doc/html/latest/dev-tools/kcov.html);
  DTrace ustack sampling on Mac.
- Corpus dir drives regression; libFuzzer-style evolving `corpus/` +
  seeds, minimized via `-merge=1 NEW FULL` keeping only new-coverage
  items: [libFuzzer](https://llvm.org/docs/LibFuzzer.html).

## Determinism controls (Tier: L)

Lessons from rr (record/replay design prior art):

- Run one thread at a time with preemptive recorded scheduling to
  avoid data-race nondeterminism:
  [rr paper](https://arxiv.org/html/1705.05937v1).
- Time async events with a deterministic counter (rr uses the x86 RCB
  perf counter) paired with full register state incl. PC:
  [same](https://arxiv.org/html/1705.05937v1).
- Rewrite syscall insns to call an interception lib; use seccomp-bpf
  to suppress ptrace traps for fast syscalls: [same](https://arxiv.org/html/1705.05937v1).
- Replay reproduces address spaces, registers, syscall data
  identically; traces are durable/compact/portable:
  [rr](https://rr-project.org/).

mol controls: fixed TZ/LANG/seed; single-thread or recorded-schedule
mode; fixed-seed malloc shim; virtual clock + fixed sysroot.

Open: no single deterministic allocator picked — DieHard
([repo](https://github.com/emeryberger/DieHard)) randomizes for
safety; the harness needs a fixed-seed deterministic mode evaluated
and chosen. Virtual-clock + ARM time-counter pick open (rr's RCB is
x86-only).

## Data formats

- Raw JSONL events:
  `{t_raw, pid, tid, class, name, args_raw, ret, errno}`.
- Normalized JSONL:
  `{seq, tid_causal, class, name, args_norm, ret_norm}`.
- `corpus/` seeds + `minimized/` via merge-style dedup.
- `coverage/` bitmaps (JSON).
- `diffs/` unified per-case reports.

## Phased plan

| Phase | Scope | Tier |
|-------|-------|------|
| P0 | strace + DTrace collectors + raw JSONL | S–M |
| P1 | Normalizer + golden corpus + differ | M |
| P2 | eBPF + seccomp-notify + Mach logger | M–L |
| P3 | Determinism controls + coverage tracking + CI gating | L |
| P4 | Record/replay integration for failing cases | L–XL |

## Checklist

- [ ] P0 collectors produce raw JSONL on both sides for one CLI binary
- [ ] Normalizer strips every doc-11 divergence item, nothing more
- [ ] Differ allow-list checked in with per-entry justification
- [ ] Coverage bitmaps tracked per run; corpus minimization wired
- [ ] Determinism controls reproduce a trace bit-for-bit on retry
- [ ] P4 replay attached to failing cases (rr model, macOS-native capture)
