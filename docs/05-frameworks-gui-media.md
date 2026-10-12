# 05 — Frameworks, GUI, Media

Windowing target: Wayland-first on Ubuntu Desktop (xdg-shell); X11 ICCCM/EWMH
not yet researched (see doc 10).

## GUI toolkits

| Framework | Strategy | Tier | Source |
|-----------|----------|------|--------|
| AppKit | xdg-shell client toolkit + custom widgets; main event loop, responder chain (NSApplication/NSWindow/NSView) | XL | [Event overview](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/EventOverview/Introduction/Introduction.html) |
| UIKit | Same event-loop shim minus touch stack; low desktop-Linux priority | XL | [View controllers](https://developer.apple.com/library/archive/documentation/WindowsViews/Conceptual/ViewControllerCatalog/Introduction.html) |
| CoreGraphics/Quartz2D | 2D engine (paths, layers, color, PDF) over Cairo/Skia or Vulkan vector renderer | L | [Quartz2D](https://developer.apple.com/library/archive/documentation/GraphicsImaging/Conceptual/drawingwithquartz2d/Introduction/Introduction.html) |
| CoreAnimation | CALayer tree → Vulkan scene graph / Wayland subsurfaces (compositing, not drawing) | L | [CA guide](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/CoreAnimation_guide/Introduction/Introduction.html) |

Darling status quo (schedule warning): 10+ years in, GUI still
hello-world-only; AppKit via Cocotron fork, Foundation via Apportable fork,
CoreFoundation via CFLite.

## Metal + MSL (largest work item)

- Metal: low-overhead GPU 3D+compute API — re-implement the API surface over
  Vulkan. Tier XL:
  [Metal guide](https://developer.apple.com/library/archive/documentation/Miscellaneous/Conceptual/MetalProgrammingGuide/Introduction/Introduction.html).
- MSL: C++-based GPU functions compiled to libraries — needs a **new**
  MSL→SPIR-V frontend; no blessed tool exists. Tier L (pending spec review).
- No existing Metal-on-Vulkan layer exists. MoltenVK and gfx-portability solve
  the **inverse** problem (Vulkan apps on Metal) — reference only:
  [MoltenVK](https://github.com/KhronosGroup/MoltenVK) (M to study),
  [gfx-portability](https://github.com/gfx-rs/portability),
  [Khronos pattern](https://www.khronos.org/blog/fighting-fragmentation-vulkan-portability-extension-released-implementations-shipping).
- SPIRV-Cross translates SPIR-V→MSL (wrong direction for capture):
  [repo](https://github.com/KhronosGroup/SPIRV-Cross).
- Rust: `ash` (lightweight unsafe Vulkan wrapper, Vulkan 1.1–1.4) for the
  core, `vulkano` as optional safe layer, `cxx` for C++ driver interop.

## Text

| Item | Strategy | Tier | Source |
|------|----------|------|--------|
| CoreText | Glyph-run/line/frame layout over HarfBuzz + FreeType + Fontconfig | M | [CoreText](https://developer.apple.com/library/archive/documentation/StringsTextFonts/Conceptual/CoreText_Programming/Introduction/Introduction.html) |
| FreeType | Glyph rasterization dependency | S | [FreeType](https://freetype.org/freetype2/docs/index.html) |
| Fontconfig | Discovery/matching/substitution (CTFontManager/collections) | S | [Fontconfig](https://www.freedesktop.org/wiki/Software/fontconfig/) |

## Audio / video / web

| Item | Strategy | Tier | Source |
|------|----------|------|--------|
| CoreAudio/AudioUnit | HAL/AU graph → PipeWire streams/filters | M (L for AU plugin compat) | [CoreAudio](https://developer.apple.com/library/archive/documentation/MusicAudio/Conceptual/CoreAudioOverview/Introduction/Introduction.html), [AU guide](https://developer.apple.com/library/archive/documentation/MusicAudio/Conceptual/AudioUnitProgrammingGuide/Introduction/Introduction.html) |
| PipeWire | Audio/video transport dependency | S | [PipeWire API](https://docs.pipewire.org/page_api.html) |
| AVFoundation | Media graph over GStreamer/FFmpeg; DRM out of scope | L | [AVFoundation](https://developer.apple.com/library/archive/documentation/AudioVideo/Conceptual/AVFoundationPG/Articles/00_Introduction.html) |
| WebKit2/WKWebView | Embed system WebKitGTK, do not port Apple WebKit | S embed / XL port | [WebKit intro](https://raw.githubusercontent.com/WebKit/WebKit/main/Introduction.md) |

## Wayland protocol block (tier M overall, S items noted)

- Core compositor-client protocol; `wl_data_device_manager` covers clipboard
  + drag-drop: [spec](https://wayland.freedesktop.org/docs/html/).
- xdg-shell/`xdg_wm_base`: desktop surfaces, toplevels, popups ← NSWindow:
  [xdg-shell](https://wayland.app/protocols/xdg-shell).
- xdg-decoration: server- vs client-side decorations (default CSD fits
  macOS style): [protocol](https://wayland.app/protocols/xdg-decoration-unstable-v1).
- wp-fractional-scale-v1: per-surface fractional scale ← Retina
  backingScaleFactor (S within block):
  [protocol](https://wayland.app/protocols/fractional-scale-v1).
- zwp_text_input_v3: text-input + input-method per seat ←
  NSTextInputClient/IME: [protocol](https://wayland.app/protocols/text-input-unstable-v3).

## Rust-versus-C

Rust for all framework shims and the Metal-over-Vulkan layer; C only for
vendored Apple OSS drops and C++-ABI media/codec interop (via `cxx`/`bindgen`).
