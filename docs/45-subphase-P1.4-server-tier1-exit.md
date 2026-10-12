# 45 — Sub-phase P1.4: Server Skeleton + Tier-1 Exit

Parent: [31 — Phase P1](31-phase-P1-cli-core.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: mol-server, launchd-as-init, Tier-1 binaries on AMD64 then
ARM64.
Milestone: macOS jq, curl, ffmpeg run with byte-identical stdout
vs macOS on both arches (non-TLS surface).
Proven by: corpus board + differential diff job.

Criteria:

- [ ] P1.4.1 DPREFIX-equivalent init + overlay isolation; mol-server
  process/thread mgmt + bootstrap registry live (docs 12 L0, 16 G2).
- [ ] P1.4.2 Exit: macOS jq, curl, ffmpeg CLI binaries run on AMD64,
  then the same on ARM64 — non-TLS surface (`--version`, file://,
  local http://); TLS arrives in P3.2 (doc 09 M1, Tier 1).

Tests:

- [ ] P1.4.V1 50-binary CLI corpus pass/fail board; Tier-1 candidates
  exit 0 with byte-identical stdout vs macOS (doc 16 G2 DoD).
- [ ] P1.4.V2 Differential diff job green on the Tier 1 corpus
  (doc 14).
- [ ] P1.4.V3 F1/F6/F11/F14 suites green; `status.yaml` updated for
  P1-owned F-items (docs 11, 13).
