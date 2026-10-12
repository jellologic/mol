// SPDX-License-Identifier: MIT OR Apache-2.0
//! Mach-O thin headers and fat (universal) containers.
//!
//! Layout reference: `mach-o/loader.h` and `mach-o/fat.h` (struct
//! shapes only; all parsing here is hand-rolled and bounds-checked).
use crate::cursor::Cursor;
use crate::{ErrorKind, ParseError, Result};

// --- magic ---
pub const MH_MAGIC_64: u32 = 0xfeed_facf;
/// Byte-swapped 64-bit magic: present on disk only for cross-endian
/// objects, which the loader refuses (P1.1.1).
pub const MH_CIGAM_64: u32 = 0xcfaf_eedf;
pub const FAT_MAGIC: u32 = 0xcafe_babe;
pub const FAT_MAGIC_64: u32 = 0xcafe_babf;

// --- cputype ---
pub const CPU_TYPE_X86_64: u32 = 0x0100_0007;
pub const CPU_TYPE_ARM64: u32 = 0x0100_000c;
pub const CPU_TYPE_ARM64_32: u32 = 0x0200_000c;

// --- cpusubtype (masked with !CPU_SUBTYPE_LIB64) ---
pub const CPU_SUBTYPE_LIB64: u32 = 0x8000_0000;
pub const CPU_SUBTYPE_X86_64_ALL: u32 = 3;
pub const CPU_SUBTYPE_X86_64_H: u32 = 8;
pub const CPU_SUBTYPE_ARM64_ALL: u32 = 0;
pub const CPU_SUBTYPE_ARM64_V8: u32 = 1;
/// arm64e: pointer authentication. See [`crate::chained`] and the
/// matrix-#1 policy (doc 06): strip PAC on non-PAC Linux ARM64.
pub const CPU_SUBTYPE_ARM64E: u32 = 2;

// --- filetype ---
pub const MH_OBJECT: u32 = 0x1;
pub const MH_EXECUTE: u32 = 0x2;
pub const MH_FVMLIB: u32 = 0x3;
pub const MH_CORE: u32 = 0x4;
pub const MH_PRELOAD: u32 = 0x5;
pub const MH_DYLIB: u32 = 0x6;
pub const MH_DYLINKER: u32 = 0x7;
pub const MH_BUNDLE: u32 = 0x8;
pub const MH_DYLIB_STUB: u32 = 0x9;
pub const MH_DSYM: u32 = 0xa;
pub const MH_KEXT_BUNDLE: u32 = 0xb;
pub const MH_FILESET: u32 = 0xc;

// --- header flags ---
pub const MH_NOUNDEFS: u32 = 0x1;
pub const MH_DYLDLINK: u32 = 0x4;
pub const MH_TWOLEVEL: u32 = 0x80;
pub const MH_WEAK_DEFINES: u32 = 0x8_0000;
pub const MH_BINDS_TO_WEAK: u32 = 0x80_0000;
pub const MH_ALLOW_STACK_EXECUTION: u32 = 0x2_0000;
pub const MH_PIE: u32 = 0x20_0000;
pub const MH_NO_HEAP_EXECUTION: u32 = 0x100_0000;
pub const MH_HAS_TLV_DESCRIPTORS: u32 = 0x800_0000;

pub const SIZEOF_MACH_HEADER_64: usize = 32;
pub const SIZEOF_FAT_HEADER: usize = 8;
pub const SIZEOF_FAT_ARCH: usize = 20;
pub const SIZEOF_FAT_ARCH_64: usize = 32;

/// One slice of a fat binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FatArch {
    pub cputype: u32,
    pub cpusubtype: u32,
    pub offset: u64,
    pub size: u64,
    pub align_pow: u32,
}

impl FatArch {
    /// Subtype with the `_LIB64` capability bit masked off, matching
    /// dyld's `forEachSlice` comparison.
    pub fn subtype(&self) -> u32 {
        self.cpusubtype & !CPU_SUBTYPE_LIB64
    }

    pub fn is_arm64e(&self) -> bool {
        self.cputype == CPU_TYPE_ARM64 && self.subtype() == CPU_SUBTYPE_ARM64E
    }
}

/// Parsed fat container (offsets validated against the file length).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fat {
    pub arches: Vec<FatArch>,
    pub is_64: bool,
}

/// 64-bit Mach-O header (host-endian only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header64 {
    pub cputype: u32,
    pub cpusubtype: u32,
    pub filetype: u32,
    pub flags: u32,
    pub ncmds: u32,
    pub sizeofcmds: u32,
}

impl Header64 {
    pub fn subtype(&self) -> u32 {
        self.cpusubtype & !CPU_SUBTYPE_LIB64
    }

    pub fn is_arm64e(&self) -> bool {
        self.cputype == CPU_TYPE_ARM64 && self.subtype() == CPU_SUBTYPE_ARM64E
    }

    pub fn is_pie(&self) -> bool {
        self.flags & MH_PIE != 0
    }
}

fn parse_fat_arch(cur: Cursor<'_>, off: usize, wide: bool) -> Result<FatArch> {
    if wide {
        Ok(FatArch {
            cputype: cur.u32be(off)?,
            cpusubtype: cur.u32be(off + 4)?,
            offset: cur.u64be(off + 8)?,
            size: cur.u64be(off + 16)?,
            align_pow: cur.u32be(off + 24)?,
        })
    } else {
        Ok(FatArch {
            cputype: cur.u32be(off)?,
            cpusubtype: cur.u32be(off + 4)?,
            offset: u64::from(cur.u32be(off + 8)?),
            size: u64::from(cur.u32be(off + 12)?),
            align_pow: cur.u32be(off + 16)?,
        })
    }
}

/// Parse a fat header; validates every slice range against `len`.
pub fn parse_fat(bytes: &[u8]) -> Result<Fat> {
    let cur = Cursor::new(bytes);
    let magic = cur.u32be(0)?;
    let wide = match magic {
        FAT_MAGIC => false,
        FAT_MAGIC_64 => true,
        m => {
            return Err(ParseError {
                offset: 0,
                kind: ErrorKind::BadMagic(m),
            })
        }
    };
    let nfat = cur.u32be(4)? as usize;
    // Each entry is fixed-width; cap count so cmdsize math cannot
    // overflow and hostile counts fail fast.
    let entry = if wide {
        SIZEOF_FAT_ARCH_64
    } else {
        SIZEOF_FAT_ARCH
    };
    let table_end = SIZEOF_FAT_HEADER
        .checked_add(nfat.checked_mul(entry).ok_or(ParseError {
            offset: 4,
            kind: ErrorKind::OffsetOverflow,
        })?)
        .ok_or(ParseError {
            offset: 4,
            kind: ErrorKind::OffsetOverflow,
        })?;
    if table_end > bytes.len() {
        return Err(ParseError {
            offset: 4,
            kind: ErrorKind::Truncated {
                need: table_end,
                len: bytes.len(),
            },
        });
    }
    let mut arches = Vec::with_capacity(nfat.min(64));
    for i in 0..nfat {
        let arch = parse_fat_arch(cur, SIZEOF_FAT_HEADER + i * entry, wide)?;
        let end = arch.offset.checked_add(arch.size).ok_or(ParseError {
            offset: SIZEOF_FAT_HEADER + i * entry,
            kind: ErrorKind::OffsetOverflow,
        })?;
        if end > bytes.len() as u64 || arch.size < SIZEOF_MACH_HEADER_64 as u64 {
            return Err(ParseError {
                offset: SIZEOF_FAT_HEADER + i * entry,
                kind: ErrorKind::SliceOutOfRange {
                    offset: arch.offset,
                    size: arch.size,
                    len: bytes.len(),
                },
            });
        }
        arches.push(arch);
    }
    Ok(Fat {
        arches,
        is_64: wide,
    })
}

impl Fat {
    /// Select a slice: exact (cputype, subtype) match first, then any
    /// slice with the cputype — mirroring dyld slice selection.
    pub fn select(&self, cputype: u32, cpusubtype: u32) -> Option<&FatArch> {
        self.arches
            .iter()
            .find(|a| a.cputype == cputype && a.subtype() == cpusubtype)
            .or_else(|| self.arches.iter().find(|a| a.cputype == cputype))
    }
}

/// Parse a thin 64-bit header at the start of `bytes`.
pub fn parse_header(bytes: &[u8]) -> Result<Header64> {
    parse_header_at(bytes, 0)
}

/// Parse a thin 64-bit header at byte `base`.
pub fn parse_header_at(bytes: &[u8], base: usize) -> Result<Header64> {
    let cur = Cursor::new(bytes);
    let magic = cur.u32le(base)?;
    if magic == MH_CIGAM_64 {
        return Err(ParseError {
            offset: base,
            kind: ErrorKind::SwappedEndian,
        });
    }
    if magic != MH_MAGIC_64 {
        return Err(ParseError {
            offset: base,
            kind: ErrorKind::BadMagic(magic),
        });
    }
    Ok(Header64 {
        cputype: cur.u32le(base + 4)?,
        cpusubtype: cur.u32le(base + 8)?,
        filetype: cur.u32le(base + 12)?,
        ncmds: cur.u32le(base + 16)?,
        sizeofcmds: cur.u32le(base + 20)?,
        flags: cur.u32le(base + 24)?,
    })
}

/// Human-readable filetype name for dumps.
pub fn filetype_name(filetype: u32) -> &'static str {
    match filetype {
        MH_OBJECT => "OBJECT",
        MH_EXECUTE => "EXECUTE",
        MH_FVMLIB => "FVMLIB",
        MH_CORE => "CORE",
        MH_PRELOAD => "PRELOAD",
        MH_DYLIB => "DYLIB",
        MH_DYLINKER => "DYLINKER",
        MH_BUNDLE => "BUNDLE",
        MH_DYLIB_STUB => "DYLIB_STUB",
        MH_DSYM => "DSYM",
        MH_KEXT_BUNDLE => "KEXT_BUNDLE",
        MH_FILESET => "FILESET",
        _ => "UNKNOWN",
    }
}

/// Human-readable cputype name for dumps.
pub fn cputype_name(cputype: u32) -> &'static str {
    match cputype {
        CPU_TYPE_X86_64 => "x86_64",
        CPU_TYPE_ARM64 => "arm64",
        CPU_TYPE_ARM64_32 => "arm64_32",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thin_header(cputype: u32, subtype: u32, filetype: u32, flags: u32) -> Vec<u8> {
        let mut v = Vec::new();
        for w in [
            MH_MAGIC_64,
            cputype,
            subtype,
            filetype,
            0u32,
            0u32,
            flags,
            0u32,
        ] {
            v.extend_from_slice(&w.to_le_bytes());
        }
        v
    }

    #[test]
    fn thin_header_fields() {
        let b = thin_header(CPU_TYPE_ARM64, CPU_SUBTYPE_ARM64_ALL, MH_EXECUTE, MH_PIE);
        let h = parse_header(&b).unwrap();
        assert_eq!(h.cputype, CPU_TYPE_ARM64);
        assert!(h.is_pie());
        assert!(!h.is_arm64e());
    }

    #[test]
    fn thin_rejects_bad_and_swapped_magic() {
        let mut b = thin_header(CPU_TYPE_ARM64, 0, MH_EXECUTE, 0);
        b[0] = 0x7f;
        assert!(matches!(
            parse_header(&b).unwrap_err().kind,
            ErrorKind::BadMagic(_)
        ));
        let mut s = thin_header(CPU_TYPE_ARM64, 0, MH_EXECUTE, 0);
        s[0..4].copy_from_slice(&MH_CIGAM_64.to_le_bytes());
        assert!(matches!(
            parse_header(&s).unwrap_err().kind,
            ErrorKind::SwappedEndian
        ));
    }

    #[test]
    fn arm64e_detected_with_lib64_bit() {
        let b = thin_header(
            CPU_TYPE_ARM64,
            CPU_SUBTYPE_ARM64E | CPU_SUBTYPE_LIB64,
            MH_DYLIB,
            0,
        );
        let h = parse_header(&b).unwrap();
        assert!(h.is_arm64e());
        assert_eq!(h.subtype(), CPU_SUBTYPE_ARM64E);
    }

    fn fat_two_slices() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&FAT_MAGIC.to_be_bytes());
        v.extend_from_slice(&2u32.to_be_bytes());
        // arm64 slice at 0x4000, x86_64 at 0x8000.
        for (cpu, off) in [(CPU_TYPE_ARM64, 0x4000u32), (CPU_TYPE_X86_64, 0x8000u32)] {
            v.extend_from_slice(&cpu.to_be_bytes());
            v.extend_from_slice(&3u32.to_be_bytes());
            v.extend_from_slice(&off.to_be_bytes());
            v.extend_from_slice(&0x4000u32.to_be_bytes());
            v.extend_from_slice(&14u32.to_be_bytes());
        }
        v.resize(0xc000, 0);
        // Stamp thin magics so slices are plausible.
        v[0x4000..0x4004].copy_from_slice(&MH_MAGIC_64.to_le_bytes());
        v[0x8000..0x8004].copy_from_slice(&MH_MAGIC_64.to_le_bytes());
        v
    }

    #[test]
    fn fat_select_prefers_exact_subtype() {
        let b = fat_two_slices();
        let fat = parse_fat(&b).unwrap();
        assert_eq!(fat.arches.len(), 2);
        assert!(!fat.is_64);
        let a = fat.select(CPU_TYPE_X86_64, CPU_SUBTYPE_X86_64_ALL).unwrap();
        assert_eq!(a.offset, 0x8000);
        // Unknown subtype still matches the cputype.
        let a = fat.select(CPU_TYPE_ARM64, 99).unwrap();
        assert_eq!(a.offset, 0x4000);
        assert!(fat.select(0xdead, 0).is_none());
    }

    #[test]
    fn fat_rejects_oob_slice() {
        let mut b = fat_two_slices();
        // Point the first slice past EOF.
        b[16..20].copy_from_slice(&0xffff_0000u32.to_be_bytes());
        assert!(matches!(
            parse_fat(&b).unwrap_err().kind,
            ErrorKind::SliceOutOfRange { .. }
        ));
    }
}
