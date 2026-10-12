# 26 — Never Handroll

Domains where handrolling fails on security, spec size, or compat burden.
Every item is XL to reimplement: adopt the named pick and isolate it behind
a narrow interface. Guest-visible *results* still get differential tests;
the *implementation* is never mol's.

## Crypto — XL

- Handrolled ciphers fail on modes/padding/RNG pitfalls. OWASP on custom
  algorithms: "Don't do this" — use a maintained lib's GCM/CCM/OAEP
  ([cheatsheet](https://cheatsheetseries.owasp.org/cheatsheets/Cryptographic_Storage_Cheat_Sheet.html)).
- Pick: vendor BoringSSL/AWS-LC (Chrome/Android TLS stack; apps ship their
  own copy, no ABI stability promise)
  ([README](https://raw.githubusercontent.com/google/boringssl/master/README.md)).
- Isolate: narrow iface exposing only seal/open, TLS connect, verify, and
  CSPRNG bytes; no raw primitives.

## Unicode — XL

- UAX29 grapheme/word/sentence rules need property tables, state machines,
  and CJK dictionary tailoring per Unicode version
  ([UAX29](https://unicode.org/reports/tr29/)).
- Pick: ICU4C/ICU4X as the reference implementation of UAX29 segmentation,
  collation, normalization ([ICU](https://icu.unicode.org/)).
- Isolate: wrap UBreakIterator + normalizer + collator behind
  `segment(text)→boundaries`, fold/compare ops.

## Text shaping + fonts — XL

- Correct output needs cmap/GSUB/GPOS lookups, ligatures, positional forms,
  mark attachment per script
  ([why-shaping](https://harfbuzz.github.io/why-do-i-need-a-shaping-engine.html)).
- Pick: HarfBuzz shape + FreeType raster/hint — modular font service with
  TTF/OTF/WOFF/CFF, auto-hinter, bytecode
  ([FreeType](https://freetype.org/freetype2/docs/index.html),
  [HarfBuzz](https://harfbuzz.github.io/)).
- Isolate: `shape_run(font,text)→glyphs/positions` then
  `rasterize(glyph)→bitmap`; never hand-parse sfnt tables.

## Color management — XL

- A handrolled CMM fails on ICC v2/v4 curves, matrices, CLUTs, PCS
  roundtrips, and profile bugs
  ([LittleCMS](https://littlecms.com/color-engine/)).
- Pick: LittleCMS2, MIT-licensed full ICC 4.4 CMM, standalone without OS
  ColorSync.
- Isolate: `transform(src_prof,dst_prof,pixels)` only; ColorSync parity by
  feeding the same system ICC profiles. Numeric parity deltas vs Apple's
  private CMM are uninspected (doc 10).

## Audio/video codecs — XL

- Formats span decades of specs; decoders touch untrusted bytes so the
  security-review burden is huge
  ([FFmpeg](https://ffmpeg.org/about.html)).
- Pick: FFmpeg libavcodec/libavformat/libswscale — portable, FATE-tested,
  security-patched.
- Isolate: demux/decode/convert behind a frame-in/frame-out iface; disable
  encoders and unneeded network protocols. Patent-licensing exposure for
  H.264/AAC distribution is unverified (doc 10).

## Regex — XL

- Backtracking engines blow up exponentially on crafted pattern+input
  (ReDoS) ([RE2](https://raw.githubusercontent.com/google/re2/main/README.md)).
- Pick: RE2 — linear-time guarantee, budgeted memory, no recursion; drops
  backrefs/lookaround.
- Isolate: compile/match/extract-submatches API only; reject unsupported
  constructs at compile time.

## Compression — XL

- Formats have subtle framing (xz streams/blocks/filters/checks), encoder
  tuning, and compat traps
  ([xz format](https://tukaani.org/xz/xz-file-format.txt)).
- Pick: xz-embedded/liblzma + zstd reference lib (RFC 8878, BSD) + brotli
  reference lib (RFC 7932, MIT)
  ([zstd](https://facebook.github.io/zstd/),
  [brotli](https://raw.githubusercontent.com/google/brotli/master/README.md)).
- Isolate: compress/decompress-bytes streaming iface per format; pin
  versions for bit-compat.

## Embedded storage — XL

- Crash-safe ACID needs WAL recovery and OOM/IO-fault paths; SQLite ships
  590× test code with 100% branch coverage
  ([testing](https://www.sqlite.org/testing.html)).
- Pick: SQLite amalgamation — public-domain, zero-config, single file.
- Isolate: SQL-only access via prepared statements; no direct pager/B-tree
  use.

## Also never handroll (from doc 25)

- **TLS/HTTPS** (XL): std has only `TcpStream`; adopt reqwest/rustls.
- **YAML** (XL): multi-chapter spec; adopt serde_yaml.
- **io_uring reactor** (XL): raw kernel/liburing steps uninspected; use
  the mio reactor abstraction.

## Adoption checklist

- [ ] Pinned version recorded; bit-compat-sensitive picks (compression,
      color) never float.
- [ ] Narrow iface written; raw primitives unreachable from mol code.
- [ ] License checked against the `cargo deny` allow-list + patent review
      where applicable (doc 28, doc 10).
- [ ] RustSec / upstream-security watch in place (doc 28).
- [ ] Guest-visible output covered by differential tests (doc 14).
