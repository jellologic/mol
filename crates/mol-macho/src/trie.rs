// SPDX-License-Identifier: MIT OR Apache-2.0
//! Exports-trie walking (`LC_DYLD_EXPORTS_TRIE` and the `LC_DYLD_INFO`
//! export stream share the encoding).
//!
//! Iterative walk with depth and node budgets so hostile tries fail
//! instead of recursing or looping forever.
use crate::cursor::Cursor;
use crate::{ErrorKind, ParseError, Result};

// --- export flags ---
pub const EXPORT_SYMBOL_FLAGS_KIND_MASK: u64 = 0x03;
pub const EXPORT_SYMBOL_FLAGS_KIND_REGULAR: u64 = 0x00;
pub const EXPORT_SYMBOL_FLAGS_KIND_THREAD_LOCAL: u64 = 0x01;
pub const EXPORT_SYMBOL_FLAGS_KIND_ABSOLUTE: u64 = 0x02;
pub const EXPORT_SYMBOL_FLAGS_WEAK_DEFINITION: u64 = 0x04;
pub const EXPORT_SYMBOL_FLAGS_REEXPORT: u64 = 0x08;
pub const EXPORT_SYMBOL_FLAGS_STUB_AND_RESOLVER: u64 = 0x10;

/// Maximum trie depth (dyld recurses; we iterate with a cap).
pub const MAX_TRIE_DEPTH: usize = 128;

/// One exported symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Export {
    pub name: String,
    pub flags: u64,
    /// Meaning depends on flags: symbol vmaddr-or-offset for regular
    /// exports, stub address for stub/resolver, ordinal for reexport.
    pub address: u64,
    /// Resolver address for `STUB_AND_RESOLVER` exports.
    pub resolver: Option<u64>,
    /// (library ordinal, imported name or None) for reexports.
    pub reexport: Option<(u64, Option<String>)>,
}

struct Frame {
    /// Resume position inside the child list.
    pos: usize,
    children_left: u8,
    /// Prefix accumulated from the root to this node.
    prefix: String,
}

/// Walk the whole trie, returning every export.
pub fn walk(data: &[u8]) -> Result<Vec<Export>> {
    let cur = Cursor::new(data);
    // Each node consumes >= 2 bytes (terminal-size + child-count),
    // so this bounds total node visits against aliasing loops.
    let node_budget = data.len() / 2 + 16;
    let mut nodes = 0usize;
    let mut out = Vec::new();
    // Process the root node's terminal, then descend iteratively.
    let (term, term_end) = read_terminal(cur, 0)?;
    if let Some(e) = term {
        out.push(e.with_prefix(""));
    }
    let root_children_at = term_end;
    let root_count = cur.u8(root_children_at)?;
    let mut stack = vec![Frame {
        pos: root_children_at + 1,
        children_left: root_count,
        prefix: String::new(),
    }];
    nodes += 1;

    loop {
        // Pop exhausted frames first; each borrow below is scoped so
        // it ends before any push.
        if stack.last().map(|t| t.children_left) == Some(0) {
            stack.pop();
            continue;
        }
        if stack.is_empty() {
            break;
        }
        let (prefix, child_off) = {
            let top = match stack.last_mut() {
                None => break,
                Some(t) => t,
            };
            top.children_left -= 1;
            let mut pos = top.pos;
            let label_raw = cur.cstr(pos, data.len().saturating_sub(pos))?;
            pos += label_raw.len() + 1;
            let label = std::str::from_utf8(label_raw).map_err(|_| ParseError {
                offset: pos,
                kind: ErrorKind::InvalidName,
            })?;
            let child_off = cur.uleb(&mut pos)? as usize;
            top.pos = pos;
            let mut prefix = top.prefix.clone();
            prefix.push_str(label);
            (prefix, child_off)
        };
        if stack.len() >= MAX_TRIE_DEPTH {
            return Err(ParseError {
                offset: child_off,
                kind: ErrorKind::DepthExceeded,
            });
        }
        nodes += 1;
        if nodes > node_budget {
            return Err(ParseError {
                offset: child_off,
                kind: ErrorKind::BudgetExhausted {
                    budget: node_budget,
                },
            });
        }
        let (term, term_end) = read_terminal(cur, child_off)?;
        if let Some(e) = term {
            out.push(e.with_prefix(&prefix));
        }
        let count = cur.u8(term_end)?;
        stack.push(Frame {
            pos: term_end + 1,
            children_left: count,
            prefix,
        });
    }
    Ok(out)
}

struct Terminal {
    flags: u64,
    address: u64,
    resolver: Option<u64>,
    reexport: Option<(u64, Option<String>)>,
}

impl Terminal {
    fn with_prefix(self, name: &str) -> Export {
        Export {
            name: name.to_string(),
            flags: self.flags,
            address: self.address,
            resolver: self.resolver,
            reexport: self.reexport,
        }
    }
}

/// Read a node's terminal info. Returns the export (if terminal) and
/// the offset where the child list starts.
fn read_terminal(cur: Cursor<'_>, node_off: usize) -> Result<(Option<Terminal>, usize)> {
    let mut pos = node_off;
    let term_size = cur.uleb(&mut pos)? as usize;
    let term_start = pos;
    let term_end = term_start.checked_add(term_size).ok_or(ParseError {
        offset: node_off,
        kind: ErrorKind::OffsetOverflow,
    })?;
    if term_size == 0 {
        return Ok((None, pos));
    }
    let flags = cur.uleb(&mut pos)?;
    let mut t = Terminal {
        flags,
        address: 0,
        resolver: None,
        reexport: None,
    };
    if flags & EXPORT_SYMBOL_FLAGS_REEXPORT != 0 {
        let ordinal = cur.uleb(&mut pos)?;
        let rest = term_end.saturating_sub(pos);
        let name = if rest > 0 {
            let raw = cur.cstr(pos, rest)?;
            if raw.is_empty() {
                None
            } else {
                Some(
                    std::str::from_utf8(raw)
                        .map_err(|_| ParseError {
                            offset: pos,
                            kind: ErrorKind::InvalidName,
                        })?
                        .to_string(),
                )
            }
        } else {
            None
        };
        // Reexport with an empty name re-exports the same name.
        t.address = ordinal;
        t.reexport = Some((ordinal, name));
    } else if flags & EXPORT_SYMBOL_FLAGS_STUB_AND_RESOLVER != 0 {
        t.address = cur.uleb(&mut pos)?;
        t.resolver = Some(cur.uleb(&mut pos)?);
    } else {
        t.address = cur.uleb(&mut pos)?;
    }
    if pos > term_end {
        return Err(ParseError {
            offset: node_off,
            kind: ErrorKind::TerminalOverrun,
        });
    }
    Ok((Some(t), term_end))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trie with `_bump` (addr 0x370) and `_shared_counter` (0x4000):
    /// root -> "_b" -> ("ump" leaf, "ump..." no—) — simpler: root has
    /// two children "bump" and "shared_counter" under prefix "_".
    /// Layout: root("", children: ["_" -> n1]); n1(prefix "_",
    /// children: ["bump" leaf 0x370, "shared_counter" leaf 0x4000]).
    fn two_export_trie() -> Vec<u8> {
        let mut v = Vec::new();
        // root node at 0: term_size=0, 1 child.
        v.push(0x00);
        v.push(0x01);
        // child "_" -> offset patched later.
        v.extend_from_slice(b"_\0");
        let root_child_off_pos = v.len();
        v.push(0x00); // placeholder uleb
                      // n1 at n1_off: term_size=0, 2 children.
        let n1_off = v.len();
        v.push(0x00);
        v.push(0x02);
        // child "bump" -> leaf1.
        v.extend_from_slice(b"bump\0");
        let c1_pos = v.len();
        v.push(0x00);
        // child "shared_counter" -> leaf2.
        v.extend_from_slice(b"shared_counter\0");
        let c2_pos = v.len();
        v.push(0x00);
        // leaf1: term_size=3, flags=0, addr=0x370 (2-byte uleb), 0 children.
        let l1_off = v.len();
        v.extend_from_slice(&[0x03, 0x00, 0xF0, 0x06, 0x00]);
        // leaf2: term_size=4, flags=0, addr=0x4000 (3-byte uleb), 0 children.
        let l2_off = v.len();
        v.extend_from_slice(&[0x04, 0x00, 0x80, 0x80, 0x01, 0x00]);
        // Patch offsets (all fit in one uleb byte here).
        v[root_child_off_pos] = n1_off as u8;
        v[c1_pos] = l1_off as u8;
        v[c2_pos] = l2_off as u8;
        v
    }

    #[test]
    fn walks_two_exports() {
        let t = two_export_trie();
        let mut exports = walk(&t).unwrap();
        exports.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(exports.len(), 2);
        assert_eq!(exports[0].name, "_bump");
        assert_eq!(exports[0].address, 0x370);
        assert_eq!(exports[1].name, "_shared_counter");
        assert_eq!(exports[1].address, 0x4000);
    }

    #[test]
    fn reexport_and_stub_forms() {
        // Single node: terminal reexport ordinal 2, same name.
        let t = [0x02u8, 0x08, 0x02, 0x00];
        let e = walk(&t).unwrap();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].reexport, Some((2, None)));
        // Stub + resolver: term_size covers flags + 2 ulebs (1+2+2).
        let t = [0x05u8, 0x10, 0x80, 0x01, 0x81, 0x01, 0x00];
        let e = walk(&t).unwrap();
        assert_eq!(e[0].address, 0x80);
        assert_eq!(e[0].resolver, Some(0x81));
    }

    #[test]
    fn cyclic_trie_fails_not_hangs() {
        // root -> child "a" pointing back at root (offset 0).
        let t = [0x00u8, 0x01, b'a', 0x00, 0x00];
        let res = walk(&t);
        assert!(matches!(
            res.unwrap_err().kind,
            ErrorKind::DepthExceeded | ErrorKind::BudgetExhausted { .. }
        ));
    }
}
