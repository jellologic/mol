// SPDX-License-Identifier: MIT OR Apache-2.0
//! Legacy `LC_DYLD_INFO` rebase/bind opcode walkers.
//!
//! Modern linkers emit chained fixups instead, but older binaries and
//! some dylibs still carry these streams, so the loader must apply
//! both (P1.1.1). Walks are eager with an event budget so hostile
//! streams fail instead of hanging or exhausting memory.
use crate::cursor::Cursor;
use crate::{ErrorKind, ParseError, Result};

// --- rebase opcodes ---
pub const REBASE_OPCODE_DONE: u8 = 0x00;
pub const REBASE_OPCODE_SET_TYPE_IMM: u8 = 0x10;
pub const REBASE_OPCODE_SET_SEGMENT_AND_OFFSET_ULEB: u8 = 0x20;
pub const REBASE_OPCODE_ADD_ADDR_ULEB: u8 = 0x30;
pub const REBASE_OPCODE_ADD_ADDR_IMM_SCALED: u8 = 0x40;
pub const REBASE_OPCODE_DO_REBASE_IMM_TIMES: u8 = 0x50;
pub const REBASE_OPCODE_DO_REBASE_ULEB_TIMES_COUNT: u8 = 0x60;
pub const REBASE_OPCODE_DO_REBASE_ADD_ADDR_ULEB: u8 = 0x70;
pub const REBASE_OPCODE_DO_REBASE_ULEB_TIMES_SKIPPING_ULEB: u8 = 0x80;

// --- rebase/bind pointer types ---
pub const REBASE_TYPE_POINTER: u8 = 1;
pub const REBASE_TYPE_TEXT_ABSOLUTE32: u8 = 2;
pub const REBASE_TYPE_TEXT_PCREL32: u8 = 3;

// --- bind opcodes ---
pub const BIND_OPCODE_DONE: u8 = 0x00;
pub const BIND_OPCODE_SET_DYLIB_ORDINAL_IMM: u8 = 0x10;
pub const BIND_OPCODE_SET_DYLIB_ORDINAL_ULEB: u8 = 0x20;
pub const BIND_OPCODE_SET_DYLIB_SPECIAL_IMM: u8 = 0x30;
pub const BIND_OPCODE_SET_SYMBOL_TRAILING_FLAGS_IMM: u8 = 0x40;
pub const BIND_OPCODE_SET_TYPE_IMM: u8 = 0x50;
pub const BIND_OPCODE_SET_ADDEND_SLEB: u8 = 0x60;
pub const BIND_OPCODE_SET_SEGMENT_AND_OFFSET_ULEB: u8 = 0x70;
pub const BIND_OPCODE_ADD_ADDR_ULEB: u8 = 0x80;
pub const BIND_OPCODE_DO_BIND: u8 = 0x90;
pub const BIND_OPCODE_DO_BIND_ADD_ADDR_ULEB: u8 = 0xA0;
pub const BIND_OPCODE_DO_BIND_ADD_ADDR_IMM_SCALED: u8 = 0xB0;
pub const BIND_OPCODE_DO_BIND_ULEB_TIMES_SKIPPING_ULEB: u8 = 0xC0;

// --- bind specials / types ---
pub const BIND_SPECIAL_DYLIB_SELF: i64 = 0;
pub const BIND_SPECIAL_DYLIB_MAIN_EXECUTABLE: i64 = -1;
pub const BIND_SPECIAL_DYLIB_FLAT_LOOKUP: i64 = -2;
pub const BIND_SPECIAL_DYLIB_WEAK_LOOKUP: i64 = -3;
pub const BIND_TYPE_POINTER: u8 = 1;
pub const BIND_TYPE_TEXT_ABSOLUTE32: u8 = 2;
pub const BIND_TYPE_TEXT_PCREL32: u8 = 3;
pub const BIND_SYMBOL_FLAGS_WEAK_IMPORT: u8 = 0x1;
pub const BIND_SYMBOL_FLAGS_NON_WEAK_DEFINITION: u8 = 0x8;

/// Default cap on emitted events per stream (hostile-input guard).
pub const DEFAULT_EVENT_BUDGET: usize = 1 << 22;

/// One rebase location: segment index + offset within the segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RebaseLoc {
    pub seg_index: u32,
    pub seg_offset: u64,
    pub kind: u8,
}

/// One bind location with its symbol reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindLoc<'a> {
    pub seg_index: u32,
    pub seg_offset: u64,
    pub kind: u8,
    /// 1-based dylib ordinal, or a `BIND_SPECIAL_DYLIB_*` negative.
    pub lib_ordinal: i64,
    pub symbol: &'a str,
    pub addend: i64,
    pub weak_import: bool,
}

fn advance(pos: &mut u64, delta: u64, at: usize) -> Result<()> {
    *pos = pos.checked_add(delta).ok_or(ParseError {
        offset: at,
        kind: ErrorKind::OffsetOverflow,
    })?;
    Ok(())
}

/// Walk a rebase-opcode stream, returning every location to slide.
pub fn walk_rebase(stream: &[u8], budget: usize) -> Result<Vec<RebaseLoc>> {
    let cur = Cursor::new(stream);
    let mut pos = 0usize;
    let mut out = Vec::new();
    let mut kind = REBASE_TYPE_POINTER;
    let mut seg_index = 0u32;
    let mut seg_offset = 0u64;
    macro_rules! emit {
        ($at:expr) => {{
            if out.len() >= budget {
                return Err(ParseError {
                    offset: $at,
                    kind: ErrorKind::BudgetExhausted { budget },
                });
            }
            out.push(RebaseLoc {
                seg_index,
                seg_offset,
                kind,
            });
        }};
    }
    let ptr_size = 8u64; // 64-bit images only.
    loop {
        let at = pos;
        let byte = cur.u8(pos)?;
        pos += 1;
        let opcode = byte & 0xF0;
        let imm = byte & 0x0F;
        match opcode {
            REBASE_OPCODE_DONE => break,
            REBASE_OPCODE_SET_TYPE_IMM => kind = imm,
            REBASE_OPCODE_SET_SEGMENT_AND_OFFSET_ULEB => {
                seg_index = u32::from(imm);
                seg_offset = cur.uleb(&mut pos)?;
            }
            REBASE_OPCODE_ADD_ADDR_ULEB => {
                advance(&mut seg_offset, cur.uleb(&mut pos)?, at)?;
            }
            REBASE_OPCODE_ADD_ADDR_IMM_SCALED => {
                advance(&mut seg_offset, u64::from(imm) * ptr_size, at)?;
            }
            REBASE_OPCODE_DO_REBASE_IMM_TIMES => {
                for _ in 0..imm {
                    emit!(at);
                    advance(&mut seg_offset, ptr_size, at)?;
                }
            }
            REBASE_OPCODE_DO_REBASE_ULEB_TIMES_COUNT => {
                let count = cur.uleb(&mut pos)?;
                for _ in 0..count {
                    emit!(at);
                    advance(&mut seg_offset, ptr_size, at)?;
                    if out.len() >= budget {
                        return Err(ParseError {
                            offset: at,
                            kind: ErrorKind::BudgetExhausted { budget },
                        });
                    }
                }
            }
            REBASE_OPCODE_DO_REBASE_ADD_ADDR_ULEB => {
                emit!(at);
                advance(
                    &mut seg_offset,
                    cur.uleb(&mut pos)?
                        .checked_add(ptr_size)
                        .ok_or(ParseError {
                            offset: at,
                            kind: ErrorKind::OffsetOverflow,
                        })?,
                    at,
                )?;
            }
            REBASE_OPCODE_DO_REBASE_ULEB_TIMES_SKIPPING_ULEB => {
                let count = cur.uleb(&mut pos)?;
                let skip = cur.uleb(&mut pos)?;
                for _ in 0..count {
                    emit!(at);
                    advance(
                        &mut seg_offset,
                        skip.checked_add(ptr_size).ok_or(ParseError {
                            offset: at,
                            kind: ErrorKind::OffsetOverflow,
                        })?,
                        at,
                    )?;
                    if out.len() >= budget {
                        return Err(ParseError {
                            offset: at,
                            kind: ErrorKind::BudgetExhausted { budget },
                        });
                    }
                }
            }
            _ => {
                return Err(ParseError {
                    offset: at,
                    kind: ErrorKind::BadRebaseOpcode(byte),
                })
            }
        }
    }
    Ok(out)
}

/// Walk a bind-opcode stream (bind, weak-bind, and lazy-bind share
/// the encoding; the caller knows which stream it passed).
pub fn walk_bind(stream: &[u8], budget: usize) -> Result<Vec<BindLoc<'_>>> {
    let cur = Cursor::new(stream);
    let mut pos = 0usize;
    let mut out = Vec::new();
    let mut kind = BIND_TYPE_POINTER;
    let mut seg_index = 0u32;
    let mut seg_offset = 0u64;
    let mut lib_ordinal: i64 = 0;
    let mut symbol = "";
    let mut addend = 0i64;
    let mut weak_import = false;
    let ptr_size = 8u64;
    loop {
        let at = pos;
        let byte = cur.u8(pos)?;
        pos += 1;
        let opcode = byte & 0xF0;
        let imm = byte & 0x0F;
        // Emitted inline (closure borrow of `out` + `pos` conflicts).
        macro_rules! emit {
            () => {{
                if out.len() >= budget {
                    return Err(ParseError {
                        offset: at,
                        kind: ErrorKind::BudgetExhausted { budget },
                    });
                }
                out.push(BindLoc {
                    seg_index,
                    seg_offset,
                    kind,
                    lib_ordinal,
                    symbol,
                    addend,
                    weak_import,
                });
            }};
        }
        match opcode {
            BIND_OPCODE_DONE => break,
            BIND_OPCODE_SET_DYLIB_ORDINAL_IMM => lib_ordinal = i64::from(imm),
            BIND_OPCODE_SET_DYLIB_ORDINAL_ULEB => {
                lib_ordinal = cur.uleb(&mut pos)? as i64;
            }
            BIND_OPCODE_SET_DYLIB_SPECIAL_IMM => {
                lib_ordinal = if imm == 0 { 0 } else { -(i64::from(imm)) };
            }
            BIND_OPCODE_SET_SYMBOL_TRAILING_FLAGS_IMM => {
                weak_import = imm & BIND_SYMBOL_FLAGS_WEAK_IMPORT != 0;
                let raw = cur.cstr(pos, stream.len() - pos)?;
                pos += raw.len() + 1;
                symbol = std::str::from_utf8(raw).map_err(|_| ParseError {
                    offset: pos,
                    kind: ErrorKind::InvalidName,
                })?;
            }
            BIND_OPCODE_SET_TYPE_IMM => kind = imm,
            BIND_OPCODE_SET_ADDEND_SLEB => {
                addend = cur.sleb(&mut pos)?;
            }
            BIND_OPCODE_SET_SEGMENT_AND_OFFSET_ULEB => {
                seg_index = u32::from(imm);
                seg_offset = cur.uleb(&mut pos)?;
            }
            BIND_OPCODE_ADD_ADDR_ULEB => {
                advance(&mut seg_offset, cur.uleb(&mut pos)?, at)?;
            }
            BIND_OPCODE_DO_BIND => {
                emit!();
                advance(&mut seg_offset, ptr_size, at)?;
            }
            BIND_OPCODE_DO_BIND_ADD_ADDR_ULEB => {
                emit!();
                advance(
                    &mut seg_offset,
                    cur.uleb(&mut pos)?
                        .checked_add(ptr_size)
                        .ok_or(ParseError {
                            offset: at,
                            kind: ErrorKind::OffsetOverflow,
                        })?,
                    at,
                )?;
            }
            BIND_OPCODE_DO_BIND_ADD_ADDR_IMM_SCALED => {
                emit!();
                advance(&mut seg_offset, u64::from(imm) * ptr_size + ptr_size, at)?;
            }
            BIND_OPCODE_DO_BIND_ULEB_TIMES_SKIPPING_ULEB => {
                let count = cur.uleb(&mut pos)?;
                let skip = cur.uleb(&mut pos)?;
                for _ in 0..count {
                    emit!();
                    advance(
                        &mut seg_offset,
                        skip.checked_add(ptr_size).ok_or(ParseError {
                            offset: at,
                            kind: ErrorKind::OffsetOverflow,
                        })?,
                        at,
                    )?;
                    if out.len() >= budget {
                        return Err(ParseError {
                            offset: at,
                            kind: ErrorKind::BudgetExhausted { budget },
                        });
                    }
                }
            }
            _ => {
                return Err(ParseError {
                    offset: at,
                    kind: ErrorKind::BadBindOpcode(byte),
                })
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebase_single_and_run() {
        // set seg 2 off 0x10; rebase; add 8; rebase x2 imm.
        let stream = [
            0x20 | 2,
            0x10,
            0x70,
            0x00, // rebase, add 0+8
            0x50 | 2,
            0x00, // rebase x2
        ];
        let locs = walk_rebase(&stream, 1024).unwrap();
        assert_eq!(
            locs,
            vec![
                RebaseLoc {
                    seg_index: 2,
                    seg_offset: 0x10,
                    kind: REBASE_TYPE_POINTER
                },
                RebaseLoc {
                    seg_index: 2,
                    seg_offset: 0x18,
                    kind: REBASE_TYPE_POINTER
                },
                RebaseLoc {
                    seg_index: 2,
                    seg_offset: 0x20,
                    kind: REBASE_TYPE_POINTER
                },
            ]
        );
    }

    #[test]
    fn rebase_budget_stops_hostile_counts() {
        // set seg 0 off 0; rebase uleb-times with huge count.
        let stream = [0x20, 0x00, 0x60, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 0x00];
        let err = walk_rebase(&stream, 16).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::BudgetExhausted { .. }));
    }

    #[test]
    fn rebase_rejects_bad_opcode_and_truncation() {
        assert!(matches!(
            walk_rebase(&[0xF0], 8).unwrap_err().kind,
            ErrorKind::BadRebaseOpcode(0xF0)
        ));
        assert!(matches!(
            walk_rebase(&[0x20], 8).unwrap_err().kind,
            ErrorKind::Truncated { .. }
        ));
    }

    #[test]
    fn bind_classic_got_entry() {
        // ordinal 1, symbol _printf, seg 2 off 0x1000, bind.
        let mut stream = vec![0x10 | 1, 0x40];
        stream.extend_from_slice(b"_printf\0");
        stream.extend_from_slice(&[0x70 | 2, 0x80, 0x20, 0x90, 0x00]);
        let binds = walk_bind(&stream, 1024).unwrap();
        assert_eq!(binds.len(), 1);
        let b = &binds[0];
        assert_eq!(b.lib_ordinal, 1);
        assert_eq!(b.symbol, "_printf");
        assert_eq!((b.seg_index, b.seg_offset), (2, 0x1000));
        assert_eq!(b.kind, BIND_TYPE_POINTER);
        assert!(!b.weak_import);
    }

    #[test]
    fn bind_special_ordinal_and_addend() {
        let mut stream = vec![0x32, 0x41]; // special -2, weak-import flag
        stream.extend_from_slice(b"_weak\0");
        stream.extend_from_slice(&[0x60, 0x7F]); // addend -1
        stream.extend_from_slice(&[0x70, 0x00, 0x90, 0x00]);
        let binds = walk_bind(&stream, 1024).unwrap();
        assert_eq!(binds[0].lib_ordinal, BIND_SPECIAL_DYLIB_FLAT_LOOKUP);
        assert!(binds[0].weak_import);
        assert_eq!(binds[0].addend, -1);
    }
}
