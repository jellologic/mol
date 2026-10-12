# 25 — Handroll Playbook

For each everyday need: the handroll recipe, what it lacks, and the exact
trigger to adopt a crate. Tier = handroll effort on the S/M/L/XL scale.

## CLI parsing — handroll S, adopt at subcommands

- Handroll: `std::env::args` for <5 flags. Caveat: the iterator panics on
  non-Unicode, so use `args_os`
  ([args](https://doc.rust-lang.org/std/env/fn.args.html)). No
  help/version/validation — write those by hand or adopt.
- Adopt when subcommands, `--help`/completions, or validation are needed:
  clap, full-featured parser with derive+builder
  ([clap](https://docs.rs/clap/latest/clap/)). Prefer builder-only per
  doc 24, or pico-args for simple CLIs.

## Logging — handroll S, adopt for structure

- Handroll: `eprintln!` — no levels/filter/structure/spans.
- Adopt `tracing` for structured spans/events/subscribers; async-aware
  ([tracing](https://docs.rs/tracing/latest/tracing/)). Per doc 23: `log`
  facade in libs, `tracing` in servers only when spans are needed.

## Errors — handroll S/M, adopt anyhow in apps

- Handroll: `std::error::Error` needs only `Display`+`Debug`+`source`
  chaining; typed enums are easy, context/backtrace is the tedious part
  ([Error](https://doc.rust-lang.org/std/error/trait.Error.html)).
- Adopt `anyhow` for apps: trait-object Error + context chaining with `?`;
  libraries keep typed errors
  ([anyhow](https://docs.rs/anyhow/latest/anyhow/)).

## JSON — handroll M, adopt serde_json by default

- Handroll ≈ 200 lines: string escapes, number grammar, nesting depth.
- Adopt `serde_json`: typed+untyped+no_std
  ([serde_json](https://docs.rs/serde_json/latest/serde_json/)). Handroll
  only behind a fuzzer + differential suite if guest-observable.

## TOML — handroll M/L, adopt for config

- Handroll MED-HIGH: datetimes, dotted/quoted keys, tables/arrays-of-tables,
  strict ABNF grammar ([TOML 1.0](https://toml.io/en/v1.0.0)). Adopt a
  `toml` crate for any config surface.

## YAML — never handroll (XL)

- Full spec is multi-chapter (tags/anchors/aliases/schema); adopt
  `serde_yaml` ([spec](https://yaml.org/spec/1.2.2/)). See doc 26.

## plist — handroll L, adopt

- Dual XML+binary encodings plus date/base64 make handroll HIGH. The
  `plist` crate gives Value+serde file helpers
  ([plist](https://docs.rs/plist/latest/plist/)). Guest plists also need
  differential tests against Mac output (doc 14).

## Async runtime — handroll L, adopt reactor only

- Handroll HIGH: `mio` Poll/Events wraps epoll/kqueue/IOCP per-platform;
  a raw epoll loop is Linux-only, io_uring VERY HIGH
  ([mio](https://docs.rs/mio/latest/mio/)). Use `mio` (or tokio `rt+sync`
  only) per doc 24; never tokio full.

## HTTP client — handroll M (plain) / XL (TLS), adopt reqwest

- std has only `TcpStream` (connect/read/write timeouts): zero TLS, HTTP,
  redirects, proxies
  ([TcpStream](https://doc.rust-lang.org/std/net/struct.TcpStream.html)).
- Adopt `reqwest` for upload paths: TLS backends, redirects, proxies,
  multipart/JSON, async+blocking
  ([reqwest](https://docs.rs/reqwest/latest/reqwest/)). HTTPS is
  never-handroll (doc 26).

## Hashing (non-crypto) — handroll S, never DefaultHasher on disk

- FNV/CRC handroll LOW, but `DefaultHasher`'s algorithm is explicitly
  unspecified and unstable across releases — not for persisted checksums
  ([DefaultHasher](https://doc.rust-lang.org/std/hash/struct.DefaultHasher.html)).
  Pin and test any persisted hash (see doc 26 for crypto).

## Tempfiles — adopt (handroll M + risky)

- `std::env::temp_dir` only returns the shared dir and warns fixed names
  are insecure + may be a symlink
  ([temp_dir](https://doc.rust-lang.org/std/env/fn.temp_dir.html)).
- Adopt `tempfile`: O_EXCL unique creation, auto-delete on drop,
  named/dir/spooled variants
  ([tempfile](https://docs.rs/tempfile/latest/tempfile/)).

## Test asserts — handroll S, cheap upgrade available

- `assert_eq!` is always checked with a Debug dump, no diff
  ([assert_eq](https://doc.rust-lang.org/std/macro.assert_eq.html)).
- `pretty_assertions` is a one-line drop-in colorful diff over diff+yansi
  only
  ([pretty_assertions](https://docs.rs/pretty_assertions/latest/pretty_assertions/)).

## Checklist (before handrolling)

- [ ] Inputs are trusted or fully specified; untrusted bytes need a fuzzer.
- [ ] Guest-observable output has a differential test against the Mac oracle.
- [ ] Behind a trait seam so adoption later is a swap (doc 23).
- [ ] Re-audit scheduled on every behavior change (doc 23).
