# Mach-O fixture corpus (P1.1)

Small compiler-produced fixtures for the load-map goldens. Every
value in the `.loadmap` goldens was verified field-by-field against
`otool -l`, `dyld_info -fixups -exports`, and `codesign -dvvv` by
`crates/mol-macho/tests/oracle.rs` (macOS) at generation time;
`tests/golden.rs` asserts byte equality everywhere (including Linux
CI, where the Apple tools are absent).

## Provenance

Built on Apple Silicon with Apple clang 21 (`clang-2100.3.34.2`),
macOS 27 SDK, October 2026:

- `hello.c`: `int main(void){return 0;}`
- `lib.c`: `int shared_counter = 41; int bump(void){ return ++shared_counter; }`
- `main.c`: `extern int bump(void); extern int shared_counter; int main(void){ return bump() + shared_counter; }`

```sh
clang -arch arm64 -o hello-arm64 hello.c
clang -arch x86_64 -o hello-x86_64 hello.c
lipo -create hello-arm64 hello-x86_64 -output hello-fat
clang -arch arm64 -dynamiclib -o librich-arm64.dylib lib.c
clang -arch arm64 -o rich-arm64 main.c librich-arm64.dylib
clang -arch x86_64 -dynamiclib -o librich-x86_64.dylib lib.c
clang -arch x86_64 -o rich-x86_64 main.c librich-x86_64.dylib
lipo -create rich-arm64 rich-x86_64 -output rich-fat
lipo -create librich-arm64.dylib librich-x86_64.dylib -output librich-fat.dylib
```

Notes:

- x86_64 slices are unsigned (cross-compile skips ad-hoc signing),
  so the corpus covers both signed and unsigned inputs.
- All slices are PIE (`MH_PIE`); modern `ld` ignores `-no_pie` on
  arm64. The parser is flag-agnostic (slide is a caller input), and
  the non-PIE `LC_UNIXTHREAD` entry path is covered by synthetic
  unit tests in `mol-macho`.
- `LC_UUID` is `0x1b` (not `0x1`, which is 32-bit `LC_SEGMENT`) —
  confirmed against the macOS SDK `mach-o/loader.h`.
- Real `LC_DYLD_CHAINED_FIXUPS` payloads use `seg_info_offset == 0`
  for segments without fixups, and arm64 userland uses the
  `PTR_64_OFFSET` (format 6) pointer format.
