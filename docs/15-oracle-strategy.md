# 15 — Oracle Strategy

How mol gets ground truth from real Mac hardware: legally, cheaply,
and with enough fidelity control to gate releases. Default: capture
once on hardware, replay differentially on Linux (doc 14).

## Phase plan

| Phase | Oracle | Cost signal | Tier | Sources |
|-------|--------|-------------|------|---------|
| P0 start | GitHub macOS runners: $0.062/min standard, $0.077/min 12-core, $0.102/min M2 Pro 5-core — zero capex, legal, lowest fidelity control | per-minute | S (wire up) | [runner pricing](https://docs.github.com/en/billing/reference/actions-runner-pricing) |
| P1 dev loop | 1 owned Apple Silicon Mac mini + Tart VMs (Virtualization.framework, near-native, OCI-registry images for CI); one-time capex beats ~$468/mo always-on EC2 M1 equivalent; full fidelity, SLA-clean | one-time capex | M | [Tart](https://tart.run/), [EC2 M1 pricing](https://aws.amazon.com/blogs/aws/use-amazon-ec2-m1-mac-instances-to-build-test-macos-ios-ipados-tvos-and-watchos-apps/) |
| P2 scale | EC2 Mac dedicated hosts or MacStadium + Anka/Orchard for parallel CI; EC2 = bare-metal Dedicated Host, 24h minimum, 1 instance/host, Intel mac1 + M1/M2/M4 types | hourly + 24h-min | M | [EC2 Mac](https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/ec2-mac-instances.html), [MacStadium](https://macstadium.com/pricing), [Anka](https://veertu.com/anka-build/) |

Licensing notes: Tart/Orchard are Fair Source — personal use
royalty-free, free tier to 100 host CPU cores / 4 Orchard workers:
[licensing](https://tart.run/licensing/). Anka Apple Silicon
licensing is per host count+model, commercial yearly subscription,
30-day trial:
[Anka licensing](https://docs.veertu.com/anka/licensing/).

Open costing gaps: EC2 24h-min idle billing vs GHA per-minute
unmodeled; new Mac mini MSRP unverified from primary (secondaries:
M4 $599 discontinued, M6 from $899); used Intel Mac pricing
uninspected; Orka Desktop free-tier status secondary-only; Scaleway
Apple Silicon rates secondary-only (see doc 10).

## Intel oracle (Rosetta is not durable)

- Rosetta translates Intel apps on Apple Silicon, but full Rosetta
  ends after macOS 27; macOS 28 keeps it only for old games:
  [Apple support](https://support.apple.com/en-us/102527).
- Keep Intel coverage via EC2 `mac1.metal` or a used Intel Mac mini
  while supported. Tier: M (accounting + runner wiring).

## Golden traces (cut live-oracle minutes ~10–100x)

Capture syscall/API traces once on a real Mac
(dtracing/Instruments/xctrace), check in as fixtures, run
differential replay on Linux. Model: rr proves
record-once-replay-deterministically with portable traces:
[rr](https://rr-project.org/) — but rr itself is Linux-only, so
capture tooling must be macOS-native.

Open: Tart OCI-registry macOS image redistribution vs the SLA 2-VM
limit unexamined; golden-trace check-in redistribution risk
unexamined (see doc 10).

## Fidelity facts the oracle must preserve

| Fact | Why it matters | Source |
|------|----------------|--------|
| XNU Intel pages fixed 4K (4096/shift 12) | Page-size assumptions in guests | [i386 vm_param.h](https://raw.githubusercontent.com/apple/darwin-xnu/main/osfmk/mach/i386/vm_param.h) |
| XNU arm64 supports 16K (shift 14) and 4K (shift 12) via `PAGE_SHIFT_CONST` | 16K/4K matrix in M6 translator | [arm vm_param.h](https://raw.githubusercontent.com/apple/darwin-xnu/main/osfmk/mach/arm/vm_param.h) |
| Apple M1 implements ARM weak ordering + x86 TSO (Rosetta 2); TSO ~8.94% slower avg | Perf gates must absorb TSO variance (doc 16 tension) | [TSO study](https://www.springerprofessional.de/tosting-investigating-total-store-ordering-on-arm/25966222) |

## Legality order

Clean: owned Mac / Tart / cloud Macs / GHA — the macOS SLA allows
install/use only on Apple-branded computers plus up to 2 virtualized
copies per Mac for dev/testing:
[Sequoia SLA](https://www.apple.com/legal/sla/docs/macOSSequoia.pdf).
Rejected: Hackintosh / OSX-KVM on non-Apple hardware violates the
macOS EULA (only carve-out: good-faith Apple bounty research):
[analysis](https://sick.codes/is-hackintosh-osx-kvm-or-docker-osx-legal/) —
and fidelity is poisoned anyway (wrong page size, no TSO, no SEP,
Apple Silicon unemulatable there). Full SLA analysis stays in doc 08.

## Checklist

- [ ] P0: GHA macOS runner job captures reference traces for Tier 1 corpus
- [ ] P1: owned mini + Tart VMs reproduce P0 traces; capex-vs-EC2 math filed
- [ ] P2: dedicated-host / MacStadium scale-up path documented with 24h-min math
- [ ] Intel oracle live (EC2 mac1 or used mini) with sunset plan past macOS 27
- [ ] Golden traces checked in; replay-on-Linux job green
- [ ] Redistribution review (images + traces) cleared before sharing fixtures
