# 27 — System Deps and FFI

Strategy: **dynamic-link all system libs on Ubuntu LTS** (amd64+arm64 noble
debs confirmed below); pure-Rust where mature; **handwritten/sys-crate FFI
over build-time bindgen** (avoids the libclang build dep); probe via
pkg-config or system-deps crates, no vcpkg. Static/bundled only for a
FreeType fallback.

## Library table

| Lib | Ubuntu noble pkg | Rust path | Tier |
|-----|------------------|-----------|------|
| Vulkan loader | libvulkan1 1.3.275 | ash `Entry::load` (dlopen), prefer over `linked` | S |
| Wayland client | pure-Rust, no C link | wayland-client over rustix; backend `rs` always available, `sys` opt-in | S |
| Wayland sys (if needed) | libwayland-client0 1.22.0 (deps: libc6+libffi8 only) | wayland-backend `client_system`/`server_system`/`dlopen` feats | S |
| X11 | system X libs | x11 crate handwritten FFI + pkg-config probe | S |
| PulseAudio | libpulse0 16.1 | libpulse-binding over libpulse-sys (hand FFI, pkg-config build dep, no bindgen) | M |
| PipeWire | system libpipewire | pipewire-rs over pipewire-sys/libspa-sys, pkg-config probing | M |
| Fontconfig | libfontconfig1-dev | fontconfig-rs safe wrapper (`dlopen` feat for cross builds) | S |
| FreeType | system or vendored | freetype-rs over freetype-sys (cc + pkg-config + libz-sys; supports system or vendored source); dynamic preferred | M |
| Mesa/GBM | libgbm1 (Mesa 25.2.x) | gbm crate over gbm-sys + drm-fourcc, pairs with drm-rs | M |
| libudev | libudev1 (systemd 255.4, amd64+arm64) | libudev crate over libudev-sys hand FFI (Context/Device/Monitor) | S |
| libinput | system libinput | input crate over input-sys + optional udev feat | M |
| xkbcommon | no C link (common path) | xkbcommon-rs (libc+xkeysym) | S |
| evdev | none (/dev/input) | pure-Rust evdev over libc/nix | S |

Sources:
[Vulkan pkg](https://packages.ubuntu.com/noble/libvulkan1),
[ash](https://docs.rs/ash/latest/ash/),
[wayland-client](https://docs.rs/wayland-client/latest/wayland_client/),
[wayland-backend](https://docs.rs/wayland-backend/latest/wayland_backend/),
[wayland pkg](https://packages.ubuntu.com/noble/libwayland-client0),
[x11](https://docs.rs/x11/latest/x11/),
[libpulse0](https://packages.ubuntu.com/noble/libpulse0),
[libpulse-sys](https://docs.rs/crate/libpulse-sys/latest),
[pipewire](https://docs.rs/crate/pipewire/latest),
[fontconfig-rs](https://docs.rs/fontconfig-rs/latest/fontconfig/),
[freetype-rs](https://docs.rs/crate/freetype-rs/latest),
[freetype-sys](https://docs.rs/crate/freetype-sys/latest),
[libgbm1](https://packages.ubuntu.com/noble/libgbm1),
[gbm](https://docs.rs/gbm/latest/gbm/),
[libudev1](https://packages.ubuntu.com/noble/libudev1),
[libudev](https://docs.rs/libudev/latest/libudev/),
[input](https://docs.rs/input/latest/input/),
[xkbcommon](https://docs.rs/xkbcommon/latest/xkbcommon/),
[evdev](https://docs.rs/evdev/latest/evdev/).

## FFI policy

- **Handwritten FFI first.** bindgen requires libclang/Clang 9+ (apt
  libclang-dev on Debian) — avoid at build; prefer checked-in/hand crates
  ([requirements](https://rust-lang.github.io/rust-bindgen/requirements.html)).
  New bindgen sys crates are effort L; the standard build.rs pattern (emit
  `cargo:rustc-link-lib` for the sys lib, generate bindings to OUT_DIR) is
  M per sys crate
  ([tutorial](https://rust-lang.github.io/rust-bindgen/tutorial-3.html)).
- **Probe with pkg-config or system-deps.** The pkg_config crate shells to
  pkg-config in build scripts; FOO_STATIC/FOO_DYNAMIC/PKG_CONFIG_ALL_*
  select static vs dynamic
  ([pkg-config](https://docs.rs/pkg-config/latest/pkg_config/)).
  Freedesktop pkg-config injects cflags/libs so builds avoid hardcoded
  paths — the vcpkg-free system probing standard
  ([pkg-config](https://www.freedesktop.org/wiki/Software/pkg-config/)).
  system-deps gives a declarative `[package.metadata.system-deps]` over
  pkg-config with version checks, replacing hand probe code
  ([system-deps](https://docs.rs/crate/system-deps/latest)).
- **`unsafe` stays in `-sys`/bridge crates** (doc 01, doc 07).

## Per-item tiers

S: Vulkan, Wayland, X11, Fontconfig, udev, xkbcommon, evdev, pkg-config /
system-deps probing. M: PulseAudio, PipeWire, FreeType, GBM, libinput,
per-crate bindgen pattern. L: new bindgen sys crates.

## Checklist (new system dep)

- [ ] Noble amd64+arm64 deb confirmed with minimal transitive deps.
- [ ] Safe wrapper or hand `-sys` chosen; no build-time bindgen.
- [ ] Probe via pkg-config/system-deps; dynamic link default.
- [ ] Offline/vendored build still works (doc 28).

## Gaps (→ doc 10)

Ubuntu SRU/ABI-guarantee policy text; Mesa EGL/libEGL + libpipewire-0.3
noble pages; fontconfig-sys upstream-vs-distro diff (docs page 404);
per-lib static-link feasibility (inferred, no link trial).
