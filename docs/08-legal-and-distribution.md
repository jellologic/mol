# 08 — Legal and Distribution

> Research synthesis only — not legal advice. Counsel review required.

## Apple SLA constraints

- macOS Tahoe SLA §2J bars installing/using/running Apple Software on any
  **non-Apple-branded computer**:
  [SLA](https://www.apple.com/legal/sla/docs/macOSTahoe.pdf).
- §2A/§2B license macOS only on Apple-branded computers; §2B(iii) allows max
  2 extra virtualized copies per Apple host for dev/test/Server/personal use,
  no service-bureau use.
- §2N bans copying/decompiling/reverse-engineering/disassembling macOS except
  where law or open-source terms allow.
- §2M permits modifying/replacing only Open-Sourced Components from
  [opensource.apple.com](https://opensource.apple.com/), still for use on
  Apple hardware.

## Xcode / SDK constraints

- Xcode+SDKs Agreement grants only a limited non-transferable internal-use
  license on Apple-branded computers you own/control:
  [agreement](https://www.apple.com/legal/sla/docs/xcode.pdf).
- It forbids redistributing/sublicensing/hosting Apple Software incl. SDK
  headers; macOS-SDK-built apps/libraries may ship without a separate Apple
  agreement if compliant.
- It bars using FOSS so that non-FOSS Apple Software/certs become subject to
  FOSS terms.
- **Conflict (open):** stub generation on a real Mac via SDK + class-dump
  (Darling practice) vs the SDK redistribution ban — no legal path named yet
  (doc 10). Whether Apple would grant redistribution rights is unknown.

## Clean-room + API-copying posture

- Follow Wine Clean Room Guidelines: black-box test programs OK; never
  disassemble/MS-source/debug-symbols/ReactOS-headers; avoid debugging against
  native components; no LLM-generated code:
  [guidelines](https://gitlab.winehq.org/wine/wine/-/wikis/Clean-Room-Guidelines).
- SCOTUS (Google v. Oracle) held copying only-needed Java SE API lines for a
  new transformative program was fair use as matter of law (assuming
  copyrightability arguendo):
  [opinion](https://www.supremecourt.gov/opinions/20pdf/18-956_d18f.pdf).
- EU Software Directive Art. 6 interoperability exception vs the SLA
  reverse-engineering ban: not researched (doc 10).

## License model

- Darling code is GPL-3.0; XNU/libSystem/Security from Apple open-source
  releases: [darling](https://github.com/darlinghq/darling).
- FSF: GPL (not LGPL) library infects the whole linked combination; linking
  GPLed code requires releasing under a GPL-compatible license:
  [FAQ](https://www.gnu.org/licenses/gpl-faq.html).
- **Conflict (open):** GPL-3.0 whole-work copyleft vs permissive Rust
  crates / relibc model — reuse conflict unnamed; decide before reusing
  Darling or Apple OSS code paths.
- MIT is permissive (preserve notices only):
  [license](https://choosealicense.com/licenses/mit/).

## Trademarks

- Apple word marks only in referential phrases ("compatible with …"), never in
  the product name, less prominent, no Apple logo, no endorsement impression:
  [guidelines](https://www.apple.com/legal/intellectual-property/guidelinesfor3rdparties.html).
- "Mac" and "macOS" are Apple trademarks requiring generic term + attribution:
  [list](https://www.apple.com/legal/intellectual-property/trademark/appletmlist.html).

## Ubuntu distribution

- Debian packages via `cargo-deb` (doc 07); repo/PPA install via
  `add-apt-repository` (`ppa:USER/PPA` shortcut, imports signing key; updates
  flow via apt):
  [manpage](https://manpages.ubuntu.com/manpages/noble/en/man1/add-apt-repository.1.html).
- Snap confinement: strict denies system access except via interfaces; classic
  gives deb-like access but needs manual review + `--classic` install; devmode
  cannot ship stable:
  [model](https://forum.snapcraft.io/t/choosing-a-security-model/6847).
- Snaps auto-refresh (snapd checks 4×/day, holdable):
  [thread](https://forum.snapcraft.io/t/7022).
- Launchpad PPA per-release build matrix and moderation rules not inspected
  (official docs login-gated) — doc 10.
- Recommendation: deb + PPA primary (prefixes need mount/namespace powers
  that fight strict snap confinement); snap classic only if demanded.
