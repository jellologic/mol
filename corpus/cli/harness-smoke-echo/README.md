# Golden corpus seed: harness/smoke/echo (mac leg)

Captured locally (Apple Silicon Mac, CLT only): stdout/stderr/exit +
`dyld`/`log` backends are real signal; syscall backends are
`skipped` (lldb attach denied — developer mode off; xctrace needs
full Xcode; dtruss needs root).

Upgrade path (P0.4.2): run the CI `legs` job on a macOS runner and
replace `mac/trace.json` with the downloaded artifact once a
syscall backend reports `ok`. See doc 41, Oracle access note.
