// SPDX-License-Identifier: MIT OR Apache-2.0
//! V2 ulp-envelope machinery (doc 11 F3, P1.2.3): oracle-trace
//! parsing, ulp distance, and the exact-function set. Shared by the
//! envelope generator (`examples/gen_envelope.rs`) and the
//! comparator (`tests/ulp.rs`).

use std::collections::BTreeMap;
use std::path::Path;

/// Functions that are bit-exact across libms by definition or IEEE
/// mandate: `floor`/`ceil`/`fabs`/`fmod` perform no rounding, and
/// `sqrt` is correctly rounded. The comparator asserts bitwise
/// identity (modulo NaN payloads) for these; the rest get envelope
/// bounds.
pub const EXACT_FNS: &[&str] = &["sqrt", "floor", "ceil", "fmod", "fabs"];

/// One oracle-trace row: bit patterns as `u64`, errno as `i32`.
#[derive(Debug, Clone)]
pub struct Row {
    pub func: String,
    pub in1: u64,
    pub in2: Option<u64>,
    pub out: u64,
    pub errno: i32,
}

fn hex_u64(s: &str) -> Result<u64, String> {
    u64::from_str_radix(s.trim(), 16).map_err(|e| format!("bad hex '{s}': {e}"))
}

/// Parse an oracle-trace CSV (`func,in1,in2,out,errno` with a header
/// row; `in2` empty for one-argument functions).
pub fn parse_trace(path: &Path) -> Result<Vec<Row>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut rows = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if n == 0 {
            if line.trim() != "func,in1,in2,out,errno" {
                return Err(format!("{}: bad header '{line}'", path.display()));
            }
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split(',').collect();
        if f.len() != 5 {
            return Err(format!("{}:{}: want 5 fields", path.display(), n + 1));
        }
        rows.push(Row {
            func: f[0].to_string(),
            in1: hex_u64(f[1])?,
            in2: if f[2].trim().is_empty() {
                None
            } else {
                Some(hex_u64(f[2])?)
            },
            out: hex_u64(f[3])?,
            errno: f[4]
                .trim()
                .parse::<i32>()
                .map_err(|e| format!("bad errno '{}': {e}", f[4]))?,
        });
    }
    Ok(rows)
}

pub fn is_nan_bits(u: u64) -> bool {
    u & 0x7ff0_0000_0000_0000 == 0x7ff0_0000_0000_0000 && u & 0x000f_ffff_ffff_ffff != 0
}

/// Order-preserving map of a finite-or-infinite bit pattern to the
/// integer line (`-0.0` normalizes to `+0.0`; NaN is a caller error).
fn ordered(u: u64) -> i64 {
    debug_assert!(!is_nan_bits(u));
    if u << 1 == 0 {
        return 0; // either zero
    }
    if u >> 63 == 0 {
        u as i64
    } else {
        -((u & 0x7fff_ffff_ffff_ffff) as i64) - 1
    }
}

/// Ulp distance between two non-NaN bit patterns.
pub fn ulp_dist(a: u64, b: u64) -> u64 {
    (ordered(a) as i128 - ordered(b) as i128).unsigned_abs() as u64
}


/// Parse `envelope.json` (`{"sin": 2, ...}` plus a `_meta` object,
/// which is skipped). Minimal scanner: string keys, unsigned
/// values or skipped `{...}` objects.
pub fn parse_envelope(path: &Path) -> Result<BTreeMap<String, u64>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let b = text.as_bytes();
    let mut map = BTreeMap::new();
    let mut i = 0;
    let err = |i: usize| format!("{}: malformed near byte {i}", path.display());
    while i < b.len() {
        if b[i] != b'"' {
            i += 1;
            continue;
        }
        let ks = i + 1;
        let ke = b[ks..]
            .iter()
            .position(|&c| c == b'"')
            .ok_or_else(|| err(i))?;
        let key = std::str::from_utf8(&b[ks..ks + ke]).map_err(|_| err(i))?;
        i = ks + ke + 1;
        while i < b.len() && (b[i] as char).is_whitespace() {
            i += 1;
        }
        if i < b.len() && b[i] == b':' {
            i += 1;
        }
        while i < b.len() && (b[i] as char).is_whitespace() {
            i += 1;
        }
        if i < b.len() && b[i] == b'"' {
            // Skip a string value (inside `_meta`).
            i += 1;
            while i < b.len() && b[i] != b'"' {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        if i < b.len() && b[i] == b'{' {
            // Skip a nested object (the `_meta` block).
            let mut depth = 0;
            while i < b.len() {
                if b[i] == b'{' {
                    depth += 1;
                } else if b[i] == b'}' {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                i += 1;
            }
            continue;
        }
        let vs = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if vs == i {
            continue;
        }
        let v: u64 = std::str::from_utf8(&b[vs..i])
            .map_err(|_| err(i))?
            .parse()
            .map_err(|_| err(i))?;
        map.insert(key.to_string(), v);
    }
    Ok(map)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances_sane() {
        assert_eq!(ulp_dist(0x3ff0_0000_0000_0000, 0x3ff0_0000_0000_0000), 0);
        assert_eq!(ulp_dist(0x3ff0_0000_0000_0000, 0x3ff0_0000_0000_0001), 1);
        assert_eq!(ulp_dist(0x8000_0000_0000_0000, 0x0000_0000_0000_0000), 0);
        assert_eq!(ulp_dist(0x7ff0_0000_0000_0000, 0x7ff0_0000_0000_0000), 0);
        // Adjacent across the 1.0 boundary both ways.
        assert_eq!(ulp_dist(0x3ff0_0000_0000_0000, 0x3fef_ffff_ffff_ffff), 1);
        // Negative side mirrors: -1.0 to next-after away from zero.
        assert_eq!(ulp_dist(0xbff0_0000_0000_0000, 0xbff0_0000_0000_0001), 1);
        // Ordering spot-checks (monotonic through zero and inf).
        assert!(ordered(0xfff0_0000_0000_0000) < ordered(0xbff0_0000_0000_0000));
        assert!(ordered(0xbff0_0000_0000_0000) < ordered(0x8000_0000_0000_0000));
        assert_eq!(ordered(0x8000_0000_0000_0000), ordered(0x0));
        assert!(ordered(0x0) < ordered(0x3ff0_0000_0000_0000));
        assert!(ordered(0x3ff0_0000_0000_0000) < ordered(0x7ff0_0000_0000_0000));
        assert!(is_nan_bits(0x7ff8_0000_0000_0000));
        assert!(is_nan_bits(0xfff4_0000_0000_0000));
        assert!(!is_nan_bits(0x7ff0_0000_0000_0000));
    }

    #[test]
    fn envelope_parser_skips_meta() {
        let dir = std::env::temp_dir();
        let p = dir.join(format!("mol-ulp-test-{}.json", std::process::id()));
        std::fs::write(
            &p,
            "{\n  \"sin\": 2,\n  \"_meta\": {\"trace\": \"x\", \"rows\": 3, \"arch\": \"aarch64\"}\n}\n",
        )
        .unwrap();
        let m = parse_envelope(&p).unwrap();
        assert_eq!(m.get("sin"), Some(&2));
        assert!(!m.contains_key("_meta"));
        std::fs::remove_file(&p).ok();
    }
}
