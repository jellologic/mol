# 21 — GUI Validation

How the harness proves GUI apps function identically to Mac hardware
(doc 16 G5): scripted input on both legs, then assertions ordered by
stability — accessibility trees first, pixels last, and pixels only
against same-platform baselines. Tiers use the doc-README legend.

## Stability ranking (most → least stable)

1. Accessibility-tree assertions (AX vs AT-SPI role/name/state/value)
   — immune to pixels.
2. SSIM with ~1% failure threshold.
3. pixelmatch/odiff with threshold 0.1–0.2 + `maxDiffPixels` cap + AA
   ignore.
4. pHash Hamming distance as pre-filter only — never the sole gate.

## Tree assertions (Tier: M — the cross-platform gate)

| Surface | API | Tooling | Source |
|---------|-----|---------|--------|
| macOS AX | App UI as hierarchy of accessible elements: roles, attributes, actions, notifications | cliclick scripts (low); atomacos via pyobjc direct AX access (medium, raises `AXErrorAPIDisabled` without access); Appium mac2 driver via XCTest, Appium 3+ (high) | [AX model](https://developer.apple.com/library/archive/documentation/Accessibility/Conceptual/AccessibilityMacOSX/), [cliclick](https://raw.githubusercontent.com/BlueM/cliclick/master/README.md), [atomacos](https://raw.githubusercontent.com/daveenguyen/atomacos/master/README.md), [mac2](https://raw.githubusercontent.com/appium/appium-mac2-driver/master/README.md) |
| Linux AT-SPI2 | `Accessible`, `Action`, `Component`, `Text`, `Collection` interfaces via atspi-2 for tree walk + control | Accerciser explorer (inspect); Dogtail + pyatspi2 (automate — current docs 404/uninspected, doc 10) | [AT-SPI2](https://docs.gtk.org/atspi2/), [GNOME testing](https://wiki.gnome.org/Accessibility/Testing), [Accerciser](https://wiki.gnome.org/Apps/Accerciser) |

- The high tier is a unified AX/AT-SPI abstraction normalizing roles,
  coordinates, and per-platform baselines (Tier: L).
- Current `AXUIElement` API reference body is JS-gated (title shell
  only) — doc 10.
- macOS input playback needs an Accessibility permission grant for the
  driving terminal/app, else actions silently do nothing.

## Pixel comparators (Tier: S–M — same-platform baselines only)

| Comparator | Behavior | Threshold guidance | Source |
|------------|----------|--------------------|--------|
| pixelmatch | OKLab/HyAB perceptual space, AA-pixel detection, same-size images required | threshold 0.1–0.2 + `maxDiffPixels` | [pixelmatch](https://raw.githubusercontent.com/mapbox/pixelmatch/main/README.md) |
| odiff | SIMD native differ, YIQ NTSC, AA detection, ignore-regions | same band as pixelmatch | [odiff](https://raw.githubusercontent.com/dmtrKovalenko/odiff/main/README.md) |
| SSIM (`structural_similarity`) | Mean SSIM, `win_size`, gaussian weights, `data_range` | ~1% (0.01) failure threshold, fewer false positives than pixel diff | [skimage](https://scikit-image.org/docs/stable/api/skimage.metrics.html), [jest-image-snapshot](https://raw.githubusercontent.com/americanexpress/jest-image-snapshot/main/README.md) |
| ImageHash pHash/dHash | Hamming distance, larger `hashsize` = more detail-sensitive | Pre-filter only | [ImageHash](https://raw.githubusercontent.com/JohannesBuchner/imagehash/master/README.rst) |

Playwright mandates separate baselines per browser and platform
([snapshots](https://playwright.dev/docs/test-snapshots)) — no primary
source gives exact numeric cross-platform GPU/font thresholds
(doc 10). Resolution for the compare job (doc 20): gate
cross-platform on tree assertions + normalized CLI behavior; gate
pixels only against the same leg's baselines.

## Input automation per leg (Tier: S–M)

| Leg | Tool | Notes | Source |
|-----|------|-------|--------|
| macOS | cliclick | CLI playback, needs Accessibility grant | [cliclick](https://raw.githubusercontent.com/BlueM/cliclick/master/README.md) |
| Ubuntu Xorg | xdotool + wmctrl | XTEST input + EWMH list/move/resize/state | [xdotool](https://raw.githubusercontent.com/jordansissel/xdotool/master/xdotool.pod), [wmctrl](http://manpages.ubuntu.com/manpages/focal/man1/wmctrl.1.html) |
| Ubuntu Wayland | ydotool (root daemon) → libei long-term | No Wayland window-ops replacement; see doc 19 | doc 19 |

## Determinism controls (Tier: M)

- `animations: disabled` (fast-forward finite, reset infinite),
  `caret: hide`, `mask` pink-box overlays on clocks/dynamic regions:
  [Playwright](https://playwright.dev/docs/api/class-page).
- Pin viewport, `deviceScaleFactor`, `colorScheme` dark/light,
  `locale`, `timezone` per project; capture light + dark baselines
  separately:
  [emulation](https://playwright.dev/docs/emulation),
  [emulateMedia](https://playwright.dev/docs/api/class-page).
- Renderer pinning is mandatory even for same-platform baselines:
  virtio-gpu vs llvmpipe vs paravirtualized graphics, font sets, DPR
  (doc 19) — plus fixed clock and pinned fonts.

## Checklist

- [ ] Every GUI case asserts the AX/AT-SPI tree cross-platform first
- [ ] Pixel gates use same-platform baselines with stated thresholds
- [ ] pHash used only as pre-filter, never as sole gate
- [ ] Motion frozen, caret hidden, dynamic regions masked, DPR/theme/locale pinned
- [ ] Renderer + font set pinned per leg (doc 19)
- [ ] Accessibility permissions granted on macOS leg; Xorg/Wayland input path matches guest
