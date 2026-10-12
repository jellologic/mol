# 33 — Phase P3: Text and Media Foundations (→M3, gate G4)

Parent: [29 — Implementation plan](29-implementation-plan.md).
Entry: P2 exit milestone green (Tier 2 headless).

Objective: headless text shaping, 2D graphics, audio, video, and
windowing protocols proven without a GUI shell.

Requirements: CoreText over adopted HarfBuzz/FreeType/Fontconfig;
CoreGraphics-2D over Cairo/Skia; ImageIO png/jpeg/tiff decode;
CoreAudio headless render over PipeWire; AVFoundation core over
GStreamer/FFmpeg; Accelerate dispatch; SecureTransport-compatible
TLS over adopted BoringSSL; WebKitGTK embed; Wayland xdg-shell +
decoration + fractional-scale + text-input.

Sub-phases (in order):

- [P3.1 — Text + 2D graphics](49-subphase-P3.1-text-2d.md):
  text + 2D match references within tolerance.
- [P3.2 — Audio + video + web](50-subphase-P3.2-audio-video-web.md):
  headless audio/video/image-decode match oracle.
- [P3.3 — Wayland protocols + tolerance metric + MIG fuzzer](51-subphase-P3.3-wayland-tolerance-fuzzer.md):
  Wayland block works; tolerance metric enforced; fuzzer in CI.
- [P3.4 — Headless exit suite](52-subphase-P3.4-headless-exit.md):
  headless exit suite passes on both arches.

Exit milestone: PDF→PNG, text layout, audio sine, video transcode
headless tests pass on both arches (P3.4 green). Proven by: exit
suite green + zero parser crashers.
