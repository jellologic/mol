// SPDX-License-Identifier: MIT OR Apache-2.0
//! Darwin process stack layout (P1.1.1).
//!
//! At `LC_MAIN` entry the stack pointer addresses:
//!
//! ```text
//! [argc:u64][argv0..argvN:u64][0][envp0..:u64][0][apple0..:u64][0]
//! [zero padding][NUL-terminated strings, packed high]
//! ```
//!
//! `sp` is 16-byte aligned. Pure constructor: the loader copies the
//! image into a mapped stack.
//!
//! Layout reference: xnu `exec` stack setup + dyld `__main` argument
//! block (argc/argv/envp/apple quartet).

/// Built stack image: bytes to copy at `sp`, ending at `top`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackImage {
    /// Stack pointer value (16-aligned).
    pub sp: u64,
    /// Bytes covering `[sp, top)`.
    pub bytes: Vec<u8>,
}

/// Build the argument block for a stack ending at `top` (exclusive,
/// 16-aligned). All pointers are absolute guest addresses.
pub fn build_stack(top: u64, argv: &[&str], envp: &[&str], apple: &[&str]) -> StackImage {
    assert!(!argv.is_empty(), "argv must hold at least argv[0]");
    // Strings packed high, each NUL-terminated.
    let mut strings = Vec::new();
    let mut addrs: Vec<u64> = Vec::new();
    let mut str_size = 0usize;
    for s in argv.iter().chain(envp.iter()).chain(apple.iter()) {
        str_size += s.len() + 1;
    }
    let str_base = top - str_size as u64;
    let mut at = str_base;
    for s in argv.iter().chain(envp.iter()).chain(apple.iter()) {
        addrs.push(at);
        strings.extend_from_slice(s.as_bytes());
        strings.push(0);
        at += (s.len() + 1) as u64;
    }
    // Pointer tables + argc below the strings, 16-aligned.
    let table_words = 1 + (argv.len() + 1) + (envp.len() + 1) + (apple.len() + 1);
    let sp = (str_base - (table_words * 8) as u64) & !15;
    let mut bytes = vec![0u8; (top - sp) as usize];
    let mut w = |off: usize, val: u64| {
        bytes[off..off + 8].copy_from_slice(&val.to_le_bytes());
    };
    let mut pos = 0usize;
    w(pos, argv.len() as u64);
    pos += 8;
    let mut ai = 0usize;
    for _ in 0..argv.len() {
        w(pos, addrs[ai]);
        ai += 1;
        pos += 8;
    }
    pos += 8; // argv NULL
    for _ in 0..envp.len() {
        w(pos, addrs[ai]);
        ai += 1;
        pos += 8;
    }
    pos += 8; // envp NULL
    for _ in 0..apple.len() {
        w(pos, addrs[ai]);
        ai += 1;
        pos += 8;
    }
    pos += 8; // apple NULL
    debug_assert_eq!(ai, addrs.len());
    // Strings land at str_base.
    let soff = (str_base - sp) as usize;
    bytes[soff..soff + strings.len()].copy_from_slice(&strings);
    let _ = pos;
    StackImage { sp, bytes }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r64(bytes: &[u8], off: usize) -> u64 {
        u64::from_le_bytes(bytes[off..off + 8].try_into().unwrap())
    }

    #[test]
    fn exact_layout_small_case() {
        // top 0x1000, argv ["hi"], no env/apple.
        let img = build_stack(0x1000, &["hi"], &[], &[]);
        // strings: "hi\0" (3 bytes) at 0xFFD; tables: 1+2+1+1 words.
        assert_eq!(img.sp, (0x1000 - 3 - 5 * 8) & !15);
        assert_eq!(img.sp % 16, 0);
        assert_eq!(img.bytes.len() as u64, 0x1000 - img.sp);
        assert_eq!(r64(&img.bytes, 0), 1); // argc
        let argv0 = r64(&img.bytes, 8);
        assert_eq!(argv0, 0x1000 - 3);
        assert_eq!(r64(&img.bytes, 16), 0); // argv NULL
        assert_eq!(r64(&img.bytes, 24), 0); // envp NULL
        assert_eq!(r64(&img.bytes, 32), 0); // apple NULL
        let s = &img.bytes[(argv0 - img.sp) as usize..];
        assert_eq!(&s[..3], b"hi\0");
    }

    #[test]
    fn tables_point_into_packed_strings() {
        let argv = vec!["/bin/prog", "-x", "yz"];
        let envp = vec!["A=1", "B=22"];
        let apple = vec!["/bin/prog"];
        let top = 0x7fff_fff0_0000u64;
        let img = build_stack(top, &argv, &envp, &apple);
        assert_eq!(img.sp % 16, 0);
        // Walk the tables and resolve every pointer.
        let mut pos = 8usize;
        let mut seen = Vec::new();
        let groups = [argv.len(), envp.len(), apple.len()];
        for g in groups {
            for _ in 0..g {
                let p = r64(&img.bytes, pos);
                pos += 8;
                let s = &img.bytes[(p - img.sp) as usize..];
                let end = s.iter().position(|&b| b == 0).unwrap();
                seen.push(String::from_utf8_lossy(&s[..end]).into_owned());
            }
            assert_eq!(r64(&img.bytes, pos), 0);
            pos += 8;
        }
        assert_eq!(
            seen,
            vec!["/bin/prog", "-x", "yz", "A=1", "B=22", "/bin/prog"]
        );
        // No gaps or overlaps: last string ends exactly at top.
        assert_eq!(img.sp + img.bytes.len() as u64, top);
    }

    #[test]
    fn odd_sizes_stay_aligned() {
        let img = build_stack(0x2000, &["a"], &["b"], &["c"]);
        assert_eq!(img.sp % 16, 0);
        assert_eq!(r64(&img.bytes, 0), 1);
        // Padding between tables and strings is zero.
        let table_end = 8 * (1 + 2 + 2 + 2);
        let str_start = img.bytes.len() - 6; // "a\0b\0c\0"
        assert!(img.bytes[table_end..str_start].iter().all(|&b| b == 0));
    }
}
