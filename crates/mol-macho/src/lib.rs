// SPDX-License-Identifier: MIT OR Apache-2.0
//! Mach-O/fat parsing boundary (docs 03, 12 L1).
//!
//! Contract: hostile input. Every public parse entry point is fuzzed
//! (doc 13, P1.1.V2) and differentially checked against `otool -l` /
//! `dyld_info` (P1.1.V1). No I/O, no syscalls here — pure bytes-in,
//! structures-out.
//!
//! Entry points:
//!
//! - [`parse`] — detect thin vs fat, validate headers.
//! - [`MachFile::select`] — pick a thin [`Image`] for a cputype.
//! - [`Image::decoded`] / [`Image::load_map`] — commands and the
//!   canonical load map used by golden tests and the loader.
pub mod chained;
pub mod codesign;
pub(crate) mod cursor;
pub mod header;
pub mod load;
pub mod map;
pub mod opcodes;
pub mod trie;

use std::fmt;

use header::{Fat, Header64};

/// Parse failure with the byte offset that caused it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub offset: usize,
    pub kind: ErrorKind,
}

/// Every way untrusted bytes can be rejected. Variants carry the
/// values needed to diagnose the input (never panics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    OffsetOverflow,
    Truncated { need: usize, len: usize },
    BadMagic(u32),
    SwappedEndian,
    SliceOutOfRange { offset: u64, size: u64, len: usize },
    NoSliceForArch { cputype: u32, cpusubtype: u32 },
    CommandTooSmall { cmd: u32, cmdsize: u32 },
    CommandOverflow { cmd: u32, cmdsize: u32 },
    StringOutOfRange,
    UnterminatedString { max: usize },
    InvalidName,
    UlebOverflow,
    TooManyBuildTools { ntools: usize },
    TooManyLinkerOptions { count: usize },
    TooManyBlobs { count: usize },
    BudgetExhausted { budget: usize },
    DepthExceeded,
    BadRebaseOpcode(u8),
    BadBindOpcode(u8),
    BadFixupsVersion(u32),
    BadStartsSegCount(usize),
    BadStartsPage { page_size: usize, page_count: usize },
    BadSegment(usize),
    UnsupportedPointerFormat(u16),
    UnsupportedImportsFormat(u32),
    UnsupportedSymbolsFormat(u32),
    FixupOutsideSegment { vmaddr: u64 },
    ChainLeftPage { vmaddr: u64 },
    ChainTooLong { vmaddr: u64 },
    TerminalOverrun,
    BadBlobLength { magic: u32, len: usize },
    PayloadOutOfRange { off: usize, size: usize, len: usize },
    EntryOutsideSegments { entryoff: u64 },
    UnknownThreadFlavor(u32),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "mach-o parse error at {:#x}: {:?}",
            self.offset, self.kind
        )
    }
}

impl std::error::Error for ParseError {}

pub type Result<T> = std::result::Result<T, ParseError>;

/// One thin image: header plus the borrowed slice bytes.
#[derive(Debug, Clone, Copy)]
pub struct Image<'a> {
    pub header: Header64,
    /// Byte offset of this slice within the file (0 for thin files).
    pub base: usize,
    /// Slice bytes (whole file for thin images).
    pub bytes: &'a [u8],
}

impl<'a> Image<'a> {
    /// Raw load-command table walk.
    pub fn commands(&self) -> Result<load::Commands<'a>> {
        load::Commands::new(self.bytes, 0, self.header.ncmds, self.header.sizeofcmds)
    }

    /// Fully decoded load commands.
    pub fn decoded(&self) -> Result<Vec<load::LoadCommand<'a>>> {
        self.commands()?
            .map(|raw| raw.and_then(|r| load::decode(&r)))
            .collect()
    }

    /// Canonical load map at `slide` (P1.1.V1 goldens).
    pub fn load_map(&self, slide: u64) -> Result<map::LoadMap> {
        let cmds = self.decoded()?;
        map::build(&self.header, &cmds, self.bytes, slide)
    }
}

/// A fat file with its validated container.
#[derive(Debug, Clone)]
pub struct FatImage<'a> {
    pub fat: Fat,
    pub bytes: &'a [u8],
}

/// A parsed file: thin image or fat container.
#[derive(Debug, Clone)]
pub enum MachFile<'a> {
    Thin(Image<'a>),
    Fat(FatImage<'a>),
}

impl<'a> MachFile<'a> {
    /// Select the thin image for `(cputype, cpusubtype)`. Thin files
    /// must match the cputype; fat files use dyld slice selection.
    pub fn select(&self, cputype: u32, cpusubtype: u32) -> Result<Image<'a>> {
        match self {
            MachFile::Thin(img) => {
                if img.header.cputype != cputype {
                    return Err(ParseError {
                        offset: 0,
                        kind: ErrorKind::NoSliceForArch {
                            cputype,
                            cpusubtype,
                        },
                    });
                }
                Ok(*img)
            }
            MachFile::Fat(f) => {
                let arch = f.fat.select(cputype, cpusubtype).ok_or(ParseError {
                    offset: 0,
                    kind: ErrorKind::NoSliceForArch {
                        cputype,
                        cpusubtype,
                    },
                })?;
                // u64 fat fields: validate the range with checked
                // math before narrowing (raw `off + size` overflows
                // on hostile slices).
                let (off, size) = (arch.offset, arch.size);
                let fblen = f.bytes.len() as u64;
                let end = off.checked_add(size).unwrap_or(u64::MAX);
                let bytes = if off <= fblen && end <= fblen {
                    // off <= end <= fblen: exact, no truncation.
                    &f.bytes[off as usize..end as usize]
                } else {
                    return Err(ParseError {
                        offset: off.min(fblen) as usize,
                        kind: ErrorKind::Truncated {
                            need: end.min(usize::MAX as u64) as usize,
                            len: f.bytes.len(),
                        },
                    });
                };
                Ok(Image {
                    header: header::parse_header(bytes)?,
                    base: off as usize,
                    bytes,
                })
            }
        }
    }
}

/// Parse a file: fat container or thin 64-bit image. Fails on 32-bit
/// and byte-swapped objects (loader scope is 64-bit host-endian).
pub fn parse(bytes: &[u8]) -> Result<MachFile<'_>> {
    let cur = cursor::Cursor::new(bytes);
    let magic_le = cur.u32le(0)?;
    if magic_le == header::MH_MAGIC_64 {
        return Ok(MachFile::Thin(Image {
            header: header::parse_header(bytes)?,
            base: 0,
            bytes,
        }));
    }
    if magic_le == header::MH_CIGAM_64 {
        return Err(ParseError {
            offset: 0,
            kind: ErrorKind::SwappedEndian,
        });
    }
    Ok(MachFile::Fat(FatImage {
        fat: header::parse_fat(bytes)?,
        bytes,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thin_roundtrip_through_top_api() {
        let mut b = vec![0u8; 32];
        b[0..4].copy_from_slice(&header::MH_MAGIC_64.to_le_bytes());
        b[4..8].copy_from_slice(&header::CPU_TYPE_ARM64.to_le_bytes());
        b[12..16].copy_from_slice(&header::MH_EXECUTE.to_le_bytes());
        let f = parse(&b).unwrap();
        let img = f.select(header::CPU_TYPE_ARM64, 0).unwrap();
        assert_eq!(img.base, 0);
        assert!(f.select(header::CPU_TYPE_X86_64, 0).is_err());
        assert!(img.decoded().unwrap().is_empty());
    }

    #[test]
    fn fat_select_rejects_wrapping_slice() {
        // Fat64 fields are full u64: raw `off + size` overflowed.
        // (`parse_fat` pre-validates; this pins `select` itself.)
        let raw = [0u8; 64];
        let file = MachFile::Fat(FatImage {
            fat: header::Fat {
                arches: vec![header::FatArch {
                    cputype: header::CPU_TYPE_ARM64,
                    cpusubtype: 0,
                    offset: u64::MAX - 10,
                    size: 100,
                    align_pow: 0,
                }],
                is_64: true,
            },
            bytes: &raw,
        });
        assert!(matches!(
            file.select(header::CPU_TYPE_ARM64, 0)
                .unwrap_err()
                .kind,
            ErrorKind::Truncated { .. }
        ));
    }
}
