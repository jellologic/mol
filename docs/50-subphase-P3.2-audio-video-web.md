# 50 — Sub-phase P3.2: Audio + Video + Web

Parent: [33 — Phase P3](33-phase-P3-text-media.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: CoreAudio headless render over PipeWire; AVFoundation core
over GStreamer/FFmpeg; ImageIO decode; WebKitGTK embed; Accelerate
dispatch; SecureTransport-compatible TLS.
Milestone: headless audio, video, and image-decode match oracle
output on both arches; TLS smoke green.
Proven by: headless render suite + snapshot job + TLS smoke.

Criteria:

- [ ] P3.2.1 Headless audio sine render + video transcode pass on
  both arches (doc 09 M3).
- [ ] P3.2.2 ImageIO png/jpeg/tiff decode matches oracle bytes
  (doc 16 G4).
- [ ] P3.2.3 Accelerate dispatch stays inside the T1 envelope across
  the hw/problem-size matrix (F4, doc 11).
- [ ] P3.2.4 SecureTransport-compatible TLS over adopted BoringSSL;
  curl https + python ssl smoke green on local endpoints (doc 26;
  fidelity surface pending OQ-117).

Tests:

- [ ] P3.2.V1 GUI snapshot job with git-hash-keyed baselines live
  (reg-suit pattern, doc 13).
- [ ] P3.2.V2 TLS smoke: curl https + python `ssl` against local
  endpoints, byte-identical payloads vs macOS (doc 16 G4 pattern).
