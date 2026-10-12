// SPDX-License-Identifier: MIT OR Apache-2.0
//! `LC_DYLD_CHAINED_FIXUPS` payload parsing and chain walking.
//!
//! Field layout follows `mach-o/fixup-chains.h`. Chains are walked
//! against the mapped image bytes with per-page link caps so hostile
//! inputs fail instead of looping.
use crate::cursor::Cursor;
use crate::{ErrorKind, ParseError, Result};

// --- pointer formats (userland subset the loader applies) ---
pub const PTR_ARM64E: u16 = 1;
pub const PTR_64: u16 = 2;
pub const PTR_64_OFFSET: u16 = 6;
pub const PTR_ARM64E_USERLAND: u16 = 9;
pub const PTR_ARM64E_USERLAND24: u16 = 12;

// --- page-start sentinels ---
pub const START_NONE: u16 = 0xFFFF;
pub const START_MULTI: u16 = 0x8000;
pub const START_LAST: u16 = 0x8000;

// --- imports formats ---
pub const IMPORT: u32 = 1;
pub const IMPORT_ADDEND: u32 = 2;
pub const IMPORT_ADDEND64: u32 = 3;

// --- chained-import specials share opcode-bind semantics ---
pub const CHAINED_SELF: i64 = 0;
pub const CHAINED_MAIN_EXECUTABLE: i64 = -1;
pub const CHAINED_FLAT_LOOKUP: i64 = -2;
pub const CHAINED_WEAK_LOOKUP: i64 = -3;

/// Segment geometry for vmaddr↔file translation while walking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SegRange {
    pub vmaddr: u64,
    pub vmsize: u64,
    pub fileoff: u64,
    pub filesize: u64,
}

impl SegRange {
    fn file_pos(&self, vmaddr: u64, at: usize) -> Result<u64> {
        let rel = vmaddr.checked_sub(self.vmaddr).ok_or(ParseError {
            offset: at,
            kind: ErrorKind::FixupOutsideSegment { vmaddr },
        })?;
        if rel >= self.vmsize {
            return Err(ParseError {
                offset: at,
                kind: ErrorKind::FixupOutsideSegment { vmaddr },
            });
        }
        let pos = self.fileoff.checked_add(rel).ok_or(ParseError {
            offset: at,
            kind: ErrorKind::OffsetOverflow,
        })?;
        // `rel + 8 <= filesize`, overflow-free: both raw sums
        // (`pos + 8`, `fileoff + filesize`) wrap on hostile u64s.
        if rel >= self.filesize || 8 > self.filesize - rel {
            return Err(ParseError {
                offset: at,
                kind: ErrorKind::FixupOutsideSegment { vmaddr },
            });
        }
        Ok(pos)
    }
}

/// Pointer-authentication qualifier on an arm64e fixup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Auth {
    pub diversity: u16,
    pub addr_div: bool,
    pub key: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixupKind {
    /// Slide/add-base relocation. `target` is the raw target field;
    /// `is_offset` selects vm-offset (formats 6/9/12) vs full-vmaddr
    /// (formats 1/2) semantics. `auth` is `Some` for arm64e
    /// authenticated rebases (loader strips on non-PAC, P1.1.5).
    Rebase {
        target: u64,
        high8: u8,
        is_offset: bool,
        auth: Option<Auth>,
    },
    /// Import bind: `ordinal` indexes the imports table.
    Bind {
        ordinal: u32,
        addend: i64,
        auth: Option<Auth>,
    },
}

/// One walked fixup: segment index, segment-relative offset,
/// preferred (unslid) vmaddr, and the fixup itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fixup {
    pub seg_index: usize,
    pub seg_offset: u64,
    pub vmaddr: u64,
    pub kind: FixupKind,
}

/// One imports-table entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import<'a> {
    /// 1-based dylib ordinal, or a `CHAINED_*` negative.
    pub lib_ordinal: i64,
    pub weak: bool,
    pub name: &'a str,
    pub addend: i64,
}

/// Parsed `LC_DYLD_CHAINED_FIXUPS` payload (header + imports; chain
/// starts are walked on demand against image bytes).
#[derive(Debug, Clone)]
pub struct ChainedFixups<'a> {
    data: &'a [u8],
    starts_offset: usize,
    imports_offset: usize,
    symbols_offset: usize,
    pub imports_count: usize,
    imports_format: u32,
}

impl<'a> ChainedFixups<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        let cur = Cursor::new(data);
        let version = cur.u32le(0)?;
        if version != 0 {
            return Err(ParseError {
                offset: 0,
                kind: ErrorKind::BadFixupsVersion(version),
            });
        }
        let starts_offset = cur.u32le(4)? as usize;
        let imports_offset = cur.u32le(8)? as usize;
        let symbols_offset = cur.u32le(12)? as usize;
        let imports_count = cur.u32le(16)? as usize;
        let imports_format = cur.u32le(20)?;
        let symbols_format = cur.u32le(24)?;
        if symbols_format == 1 {
            return Err(ParseError {
                offset: 24,
                kind: ErrorKind::UnsupportedSymbolsFormat(symbols_format),
            });
        }
        if symbols_format != 0 {
            return Err(ParseError {
                offset: 24,
                kind: ErrorKind::UnsupportedSymbolsFormat(symbols_format),
            });
        }
        if starts_offset > data.len() || imports_offset > data.len() || symbols_offset > data.len()
        {
            return Err(ParseError {
                offset: 4,
                kind: ErrorKind::Truncated {
                    need: starts_offset.max(imports_offset).max(symbols_offset),
                    len: data.len(),
                },
            });
        }
        Ok(ChainedFixups {
            data,
            starts_offset,
            imports_offset,
            symbols_offset,
            imports_count,
            imports_format,
        })
    }

    /// Decode the full imports table.
    pub fn imports(&self) -> Result<Vec<Import<'a>>> {
        let entry = match self.imports_format {
            IMPORT => 4usize,
            IMPORT_ADDEND => 8,
            IMPORT_ADDEND64 => 16,
            f => {
                return Err(ParseError {
                    offset: self.imports_offset,
                    kind: ErrorKind::UnsupportedImportsFormat(f),
                })
            }
        };
        let total = self.imports_count.checked_mul(entry).ok_or(ParseError {
            offset: self.imports_offset,
            kind: ErrorKind::OffsetOverflow,
        })?;
        if self.imports_offset + total > self.data.len() {
            return Err(ParseError {
                offset: self.imports_offset,
                kind: ErrorKind::Truncated {
                    need: self.imports_offset + total,
                    len: self.data.len(),
                },
            });
        }
        let cur = Cursor::new(self.data);
        let mut out = Vec::with_capacity(self.imports_count.min(1024));
        for i in 0..self.imports_count {
            let at = self.imports_offset + i * entry;
            let (lib_ordinal, weak, name_off, addend) = match self.imports_format {
                IMPORT | IMPORT_ADDEND => {
                    let w = cur.u32le(at)?;
                    let lib = (w & 0xff) as u8 as i8 as i64;
                    let weak = w & 0x100 != 0;
                    let name_off = (w >> 9) as usize;
                    let addend = if self.imports_format == IMPORT_ADDEND {
                        i64::from(cur.u32le(at + 4)? as i32)
                    } else {
                        0
                    };
                    (lib, weak, name_off, addend)
                }
                _ => {
                    let w = cur.u64le(at)?;
                    let lib = (w & 0xffff) as u16 as i16 as i64;
                    let weak = w & 0x1_0000 != 0;
                    let name_off = (w >> 32) as usize;
                    let addend = cur.u64le(at + 8)? as i64;
                    (lib, weak, name_off, addend)
                }
            };
            let name_at = self
                .symbols_offset
                .checked_add(name_off)
                .ok_or(ParseError {
                    offset: at,
                    kind: ErrorKind::OffsetOverflow,
                })?;
            let raw = cur.cstr(name_at, self.data.len().saturating_sub(name_at))?;
            let name = std::str::from_utf8(raw).map_err(|_| ParseError {
                offset: name_at,
                kind: ErrorKind::InvalidName,
            })?;
            out.push(Import {
                lib_ordinal,
                weak,
                name,
                addend,
            });
        }
        Ok(out)
    }

    /// Walk every chain start, invoking `f` per fixup. `file` is the
    /// whole image slice and `segs` its load-command segment order.
    pub fn each_fixup(
        &self,
        file: &[u8],
        segs: &[SegRange],
        budget: usize,
        f: &mut dyn FnMut(Fixup) -> Result<()>,
    ) -> Result<usize> {
        let cur = Cursor::new(self.data);
        let seg_count = cur.u32le(self.starts_offset)? as usize;
        if seg_count > segs.len() + 8 || seg_count > 256 {
            return Err(ParseError {
                offset: self.starts_offset,
                kind: ErrorKind::BadStartsSegCount(seg_count),
            });
        }
        let mut done = 0usize;
        for seg_index in 0..seg_count {
            let info_rel = cur.u32le(self.starts_offset + 4 + seg_index * 4)? as usize;
            // Zero offset means the segment carries no fixups (dyld
            // skips it); it is not a pointer to the starts struct.
            if info_rel == 0 {
                continue;
            }
            let info = self.starts_offset.checked_add(info_rel).ok_or(ParseError {
                offset: self.starts_offset,
                kind: ErrorKind::OffsetOverflow,
            })?;
            done += self.walk_segment(file, segs, seg_index, info, budget - done.min(budget), f)?;
        }
        Ok(done)
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_segment(
        &self,
        file: &[u8],
        segs: &[SegRange],
        seg_index: usize,
        info: usize,
        budget: usize,
        f: &mut dyn FnMut(Fixup) -> Result<()>,
    ) -> Result<usize> {
        if seg_index >= segs.len() {
            return Err(ParseError {
                offset: info,
                kind: ErrorKind::BadSegment(seg_index),
            });
        }
        let seg = &segs[seg_index];
        let cur = Cursor::new(self.data);
        let size = cur.u32le(info)? as usize;
        let page_size = cur.u16le(info + 4)? as usize;
        let pointer_format = cur.u16le(info + 6)?;
        let page_count = cur.u16le(info + 20)? as usize;
        if !page_size.is_power_of_two() || page_count > (1 << 20) {
            return Err(ParseError {
                offset: info,
                kind: ErrorKind::BadStartsPage {
                    page_size,
                    page_count,
                },
            });
        }
        let stride = match pointer_format {
            PTR_64 | PTR_64_OFFSET => 4u64,
            PTR_ARM64E | PTR_ARM64E_USERLAND | PTR_ARM64E_USERLAND24 => 8u64,
            p => {
                return Err(ParseError {
                    offset: info + 6,
                    kind: ErrorKind::UnsupportedPointerFormat(p),
                })
            }
        };
        // page_start array must fit the declared `size`.
        let array_end = 22usize
            + page_count.checked_mul(2).ok_or(ParseError {
                offset: info,
                kind: ErrorKind::OffsetOverflow,
            })?;
        if array_end > size {
            return Err(ParseError {
                offset: info,
                kind: ErrorKind::Truncated {
                    need: info + array_end,
                    len: self.data.len(),
                },
            });
        }
        let mut done = 0usize;
        for page in 0..page_count {
            let start = cur.u16le(info + 22 + page * 2)?;
            if start == START_NONE {
                continue;
            }
            let page_base = (page as u64)
                .checked_mul(page_size as u64)
                .ok_or(ParseError {
                    offset: info,
                    kind: ErrorKind::OffsetOverflow,
                })?;
            if start & START_MULTI != 0 {
                // Index into the chain_starts pool following page_start[].
                let mut ci = (start & !START_MULTI) as usize;
                loop {
                    let at = info + 22 + page_count * 2 + ci * 2;
                    if at + 2 > info + size {
                        return Err(ParseError {
                            offset: at,
                            kind: ErrorKind::Truncated {
                                need: at + 2,
                                len: self.data.len(),
                            },
                        });
                    }
                    let entry = cur.u16le(at)?;
                    let last = entry & START_LAST != 0;
                    let off = (entry & !START_LAST) as u64;
                    done += self.walk_chain(
                        file,
                        seg,
                        seg_index,
                        pointer_format,
                        stride,
                        page_size,
                        page_base + off,
                        budget - done.min(budget),
                        f,
                    )?;
                    if done > budget {
                        return Err(ParseError {
                            offset: at,
                            kind: ErrorKind::BudgetExhausted { budget },
                        });
                    }
                    if last {
                        break;
                    }
                    ci += 1;
                    if ci > page_count + 1024 {
                        return Err(ParseError {
                            offset: at,
                            kind: ErrorKind::BudgetExhausted { budget },
                        });
                    }
                }
            } else {
                done += self.walk_chain(
                    file,
                    seg,
                    seg_index,
                    pointer_format,
                    stride,
                    page_size,
                    page_base + start as u64,
                    budget - done.min(budget),
                    f,
                )?;
                if done > budget {
                    return Err(ParseError {
                        offset: info,
                        kind: ErrorKind::BudgetExhausted { budget },
                    });
                }
            }
        }
        Ok(done)
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_chain(
        &self,
        file: &[u8],
        seg: &SegRange,
        seg_index: usize,
        pointer_format: u16,
        stride: u64,
        page_size: usize,
        mut rel: u64,
        budget: usize,
        f: &mut dyn FnMut(Fixup) -> Result<()>,
    ) -> Result<usize> {
        // Every link advances by >= stride within the page, so this
        // cap also breaks hostile cycles.
        let max_links = page_size as u64 / stride + 1;
        let page_base = rel & !(page_size as u64 - 1);
        let page_end = page_base.saturating_add(page_size as u64);
        let mut done = 0usize;
        for _ in 0..max_links {
            if rel >= page_end {
                return Err(ParseError {
                    offset: 0,
                    kind: ErrorKind::ChainLeftPage {
                        vmaddr: seg.vmaddr.saturating_add(rel),
                    },
                });
            }
            let vmaddr = seg.vmaddr.checked_add(rel).ok_or(ParseError {
                offset: 0,
                kind: ErrorKind::OffsetOverflow,
            })?;
            let pos = seg.file_pos(vmaddr, 0)? as usize;
            // `pos + 8 <= len`, overflow-free (`pos` is a hostile
            // u64 narrowed to usize).
            if pos >= file.len() || 8 > file.len() - pos {
                return Err(ParseError {
                    offset: 0,
                    kind: ErrorKind::Truncated {
                        need: pos.saturating_add(8),
                        len: file.len(),
                    },
                });
            }
            let raw = u64::from_le_bytes(file[pos..pos + 8].try_into().unwrap());
            let (kind, next) = decode_entry(pointer_format, raw)?;
            if done >= budget {
                return Err(ParseError {
                    offset: 0,
                    kind: ErrorKind::BudgetExhausted { budget },
                });
            }
            f(Fixup {
                seg_index,
                seg_offset: rel,
                vmaddr,
                kind,
            })?;
            done += 1;
            if next == 0 {
                return Ok(done);
            }
            rel = rel
                .checked_add(next.saturating_mul(stride))
                .ok_or(ParseError {
                    offset: 0,
                    kind: ErrorKind::OffsetOverflow,
                })?;
        }
        Err(ParseError {
            offset: 0,
            kind: ErrorKind::ChainTooLong {
                vmaddr: seg.vmaddr.saturating_add(rel),
            },
        })
    }
}

/// Decode one 8-byte chain entry per its pointer format.
/// Returns the fixup kind and the `next` stride count.
fn decode_entry(pointer_format: u16, raw: u64) -> Result<(FixupKind, u64)> {
    match pointer_format {
        PTR_64 | PTR_64_OFFSET => {
            let is_offset = pointer_format == PTR_64_OFFSET;
            let next = (raw >> 51) & 0xfff;
            if raw >> 63 == 0 {
                Ok((
                    FixupKind::Rebase {
                        target: raw & 0xf_ffff_ffff,
                        high8: ((raw >> 36) & 0xff) as u8,
                        is_offset,
                        auth: None,
                    },
                    next,
                ))
            } else {
                Ok((
                    FixupKind::Bind {
                        ordinal: (raw & 0xff_ffff) as u32,
                        addend: (((raw >> 24) & 0xff) as u8) as i64,
                        auth: None,
                    },
                    next,
                ))
            }
        }
        PTR_ARM64E | PTR_ARM64E_USERLAND => {
            let is_offset = pointer_format == PTR_ARM64E_USERLAND;
            let next = (raw >> 51) & 0x7ff;
            let bind = (raw >> 62) & 1 != 0;
            let auth = (raw >> 63) & 1 != 0;
            if !bind && !auth {
                Ok((
                    FixupKind::Rebase {
                        target: raw & 0x7ff_ffff_ffff,
                        high8: ((raw >> 43) & 0xff) as u8,
                        is_offset,
                        auth: None,
                    },
                    next,
                ))
            } else if bind && !auth {
                let add = ((raw >> 32) & 0x7_ffff) as u32;
                let addend = if add & 0x4_0000 != 0 {
                    (add | 0xfff8_0000) as i32 as i64
                } else {
                    add as i64
                };
                Ok((
                    FixupKind::Bind {
                        ordinal: (raw & 0xffff) as u32,
                        addend,
                        auth: None,
                    },
                    next,
                ))
            } else {
                let auth = Auth {
                    diversity: ((raw >> 32) & 0xffff) as u16,
                    addr_div: (raw >> 48) & 1 != 0,
                    key: ((raw >> 49) & 3) as u8,
                };
                if !bind {
                    Ok((
                        FixupKind::Rebase {
                            target: raw & 0xffff_ffff,
                            high8: 0,
                            is_offset,
                            auth: Some(auth),
                        },
                        next,
                    ))
                } else {
                    Ok((
                        FixupKind::Bind {
                            ordinal: (raw & 0xffff) as u32,
                            addend: 0,
                            auth: Some(auth),
                        },
                        next,
                    ))
                }
            }
        }
        PTR_ARM64E_USERLAND24 => {
            let next = (raw >> 51) & 0x7ff;
            let bind = (raw >> 62) & 1 != 0;
            let auth = (raw >> 63) & 1 != 0;
            if !bind && !auth {
                // Rebase shares the arm64e layout (vm offset).
                Ok((
                    FixupKind::Rebase {
                        target: raw & 0x7ff_ffff_ffff,
                        high8: ((raw >> 43) & 0xff) as u8,
                        is_offset: true,
                        auth: None,
                    },
                    next,
                ))
            } else if bind && !auth {
                let add = ((raw >> 32) & 0x7_ffff) as u32;
                let addend = if add & 0x4_0000 != 0 {
                    (add | 0xfff8_0000) as i32 as i64
                } else {
                    add as i64
                };
                Ok((
                    FixupKind::Bind {
                        ordinal: (raw & 0xff_ffff) as u32,
                        addend,
                        auth: None,
                    },
                    next,
                ))
            } else if bind {
                Ok((
                    FixupKind::Bind {
                        ordinal: (raw & 0xff_ffff) as u32,
                        addend: 0,
                        auth: Some(Auth {
                            diversity: ((raw >> 32) & 0xffff) as u16,
                            addr_div: (raw >> 48) & 1 != 0,
                            key: ((raw >> 49) & 3) as u8,
                        }),
                    },
                    next,
                ))
            } else {
                // Auth rebase shares the 32-bit runtime-offset layout.
                Ok((
                    FixupKind::Rebase {
                        target: raw & 0xffff_ffff,
                        high8: 0,
                        is_offset: true,
                        auth: Some(Auth {
                            diversity: ((raw >> 32) & 0xffff) as u16,
                            addr_div: (raw >> 48) & 1 != 0,
                            key: ((raw >> 49) & 3) as u8,
                        }),
                    },
                    next,
                ))
            }
        }
        p => Err(ParseError {
            offset: 0,
            kind: ErrorKind::UnsupportedPointerFormat(p),
        }),
    }
}

/// Resolve a rebase to its preferred (unslid) target vmaddr given the
/// image's preferred base address.
pub fn resolve_rebase_target(target: u64, high8: u8, is_offset: bool, image_base: u64) -> u64 {
    let t = target | ((high8 as u64) << 56);
    if is_offset {
        image_base.wrapping_add(t)
    } else {
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal payload: 1 import (`_bump`, lib 1), starts for 1
    /// segment with a 2-link bind chain on page 0.
    fn payload() -> Vec<u8> {
        let mut v = vec![0u8; 28];
        // header: version=0, starts=28, imports=64, symbols=68, count=1, fmt=1, symfmt=0
        v[4..8].copy_from_slice(&28u32.to_le_bytes());
        v[8..12].copy_from_slice(&64u32.to_le_bytes());
        v[12..16].copy_from_slice(&68u32.to_le_bytes());
        v[16..20].copy_from_slice(&1u32.to_le_bytes());
        v[20..24].copy_from_slice(&IMPORT.to_le_bytes());
        // starts_in_image: seg_count=1, seg_info_offset=[8]
        v.extend_from_slice(&1u32.to_le_bytes());
        v.extend_from_slice(&8u32.to_le_bytes());
        // starts_in_segment at 36: size=26, page_size=0x1000, fmt=64,
        // seg_offset=0, max_valid=0, page_count=1, page_start=[0x10]
        let mut seg = vec![0u8; 26];
        seg[0..4].copy_from_slice(&26u32.to_le_bytes());
        seg[4..6].copy_from_slice(&0x1000u16.to_le_bytes());
        seg[6..8].copy_from_slice(&PTR_64.to_le_bytes());
        seg[20..22].copy_from_slice(&1u16.to_le_bytes());
        seg[22..24].copy_from_slice(&0x10u16.to_le_bytes());
        v.extend_from_slice(&seg);
        // Pad to imports at 64, then import {lib 1, name_off 0}.
        v.resize(64, 0);
        v.extend_from_slice(&1u32.to_le_bytes());
        // symbols at 68: "_bump\0"
        v.extend_from_slice(b"_bump\0");
        v
    }

    /// Image bytes: segment 0 at vmaddr 0x10000, fileoff 0, with two
    /// chained bind entries at file offsets 0x10 and 0x18.
    fn image() -> Vec<u8> {
        let mut img = vec![0u8; 0x1000];
        // bind ordinal 0, addend 0, next=2 strides (2*4=8), bind bit.
        let e0 = (2u64 << 51) | (1u64 << 63);
        // bind ordinal 0, next=0 (end).
        let e1 = 1u64 << 63;
        img[0x10..0x18].copy_from_slice(&e0.to_le_bytes());
        img[0x18..0x20].copy_from_slice(&e1.to_le_bytes());
        img
    }

    #[test]
    fn imports_decode_lib_and_name() {
        let p = payload();
        let cf = ChainedFixups::parse(&p).unwrap();
        let imps = cf.imports().unwrap();
        assert_eq!(imps.len(), 1);
        assert_eq!(imps[0].lib_ordinal, 1);
        assert_eq!(imps[0].name, "_bump");
        assert!(!imps[0].weak);
    }

    #[test]
    fn chain_walk_yields_two_binds() {
        let p = payload();
        let img = image();
        let cf = ChainedFixups::parse(&p).unwrap();
        let segs = [SegRange {
            vmaddr: 0x10000,
            vmsize: 0x1000,
            fileoff: 0,
            filesize: 0x1000,
        }];
        let mut out = Vec::new();
        let n = cf
            .each_fixup(&img, &segs, 1024, &mut |fx| {
                out.push(fx);
                Ok(())
            })
            .unwrap();
        assert_eq!(n, 2);
        assert_eq!(out[0].seg_offset, 0x10);
        assert_eq!(out[0].vmaddr, 0x10010);
        assert_eq!(
            out[0].kind,
            FixupKind::Bind {
                ordinal: 0,
                addend: 0,
                auth: None
            }
        );
        assert_eq!(out[1].seg_offset, 0x18);
    }

    #[test]
    fn arm64e_auth_bind_decodes() {
        // ordinal 5, diversity 0x1234, addrDiv 1, key 2, bind+auth.
        let raw =
            5u64 | (0x1234u64 << 32) | (1u64 << 48) | (2u64 << 49) | (1u64 << 62) | (1u64 << 63);
        let (kind, next) = decode_entry(PTR_ARM64E_USERLAND, raw).unwrap();
        assert_eq!(next, 0);
        assert_eq!(
            kind,
            FixupKind::Bind {
                ordinal: 5,
                addend: 0,
                auth: Some(Auth {
                    diversity: 0x1234,
                    addr_div: true,
                    key: 2
                })
            }
        );
    }

    #[test]
    fn rebase64_resolves_vmaddr_and_offset() {
        // rebase target 0x10000, high8 0.
        let raw = 0x10000u64;
        let (kind, _) = decode_entry(PTR_64, raw).unwrap();
        match kind {
            FixupKind::Rebase {
                target,
                high8,
                is_offset,
                ..
            } => {
                assert!(!is_offset);
                assert_eq!(resolve_rebase_target(target, high8, is_offset, 0), 0x10000);
            }
            other => panic!("expected rebase, got {other:?}"),
        }
        let (kind, _) = decode_entry(PTR_64_OFFSET, raw).unwrap();
        match kind {
            FixupKind::Rebase { is_offset, .. } => {
                assert!(is_offset);
                assert_eq!(
                    resolve_rebase_target(0x10000, 0, true, 0x1000_0000),
                    0x1001_0000
                );
            }
            other => panic!("expected rebase, got {other:?}"),
        }
    }

    #[test]
    fn hostile_chain_capped() {
        // page_start points into the segment but `next` loops forever.
        let mut p = payload();
        // Point page_start at 0x100 (inside segment).
        p[36 + 22] = 0x00;
        p[36 + 23] = 0x01;
        let mut img = vec![0u8; 0x1000];
        // All-set bytes: every window decodes next=0xFFF, so the first
        // link jumps far past the page end.
        for b in img.iter_mut().skip(0x100) {
            *b = 0xFF;
        }
        let cf = ChainedFixups::parse(&p).unwrap();
        let segs = [SegRange {
            vmaddr: 0x10000,
            vmsize: 0x1000,
            fileoff: 0,
            filesize: 0x1000,
        }];
        let mut n = 0;
        let res = cf.each_fixup(&img, &segs, 1 << 20, &mut |_| {
            n += 1;
            Ok(())
        });
        assert!(matches!(
            res.unwrap_err().kind,
            ErrorKind::ChainLeftPage { .. }
        ));
        assert_eq!(n, 1);
    }

    #[test]
    fn file_pos_survives_wrapping_geometry() {
        // Same class as the map.rs fuzzer panic: `pos + 8` and
        // `fileoff + filesize` both wrapped on hostile u64s.
        let seg = SegRange {
            vmaddr: 0x10000,
            vmsize: 0x2000,
            fileoff: u64::MAX - 10,
            filesize: 100,
        };
        // rel = 5, rel + 8 = 13 <= 100: exact hit, no panic.
        assert_eq!(seg.file_pos(0x10005, 0).unwrap(), u64::MAX - 5);
        // rel past filesize: clean error.
        assert!(seg.file_pos(0x10000 + 200, 0).is_err());
    }

    #[test]
    fn chain_left_page_saturates_hostile_vmaddr() {
        // The ChainLeftPage diagnostic added `seg.vmaddr + rel`
        // raw: with vmaddr near u64::MAX the error path panicked.
        // (The walk itself passes its checked_add; only the
        // diagnostic wrapped.)
        let mut p = payload();
        p[36 + 22] = 0x00;
        p[36 + 23] = 0x01;
        let mut img = vec![0u8; 0x1000];
        for b in img.iter_mut().skip(0x100) {
            *b = 0xFF;
        }
        let cf = ChainedFixups::parse(&p).unwrap();
        let segs = [SegRange {
            vmaddr: u64::MAX - 0x200,
            vmsize: 0x1000,
            fileoff: 0,
            filesize: 0x1000,
        }];
        let mut n = 0;
        let res = cf.each_fixup(&img, &segs, 1 << 20, &mut |_| {
            n += 1;
            Ok(())
        });
        assert!(matches!(
            res.unwrap_err().kind,
            ErrorKind::ChainLeftPage { vmaddr: u64::MAX }
        ));
        assert_eq!(n, 1);
    }
}
