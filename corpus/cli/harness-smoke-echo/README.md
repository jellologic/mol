# Golden corpus: harness/smoke/echo (mac leg)

Captured on a GHA-hosted macOS runner (Apple Silicon, full Xcode):
stdout/stderr/exit plus a real lldb breakpoint transcript (11
libc-wrapper hits: open/openat/mmap/close/exit paths) and working
`dyld`/`log` backends. `dtruss` remains `skipped` (needs root).

The `xctrace` backend records only the `.trace` path — the file
evaporates with the runner. Export/summary format for `.trace`
goldens is OQ-119 (P1 differential-harness work).

Provenance: CI run 38272248540, `results-mac` artifact.
Refresh: re-run the `legs` job and repeat doc 41's runbook.
