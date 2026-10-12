// SPDX-License-Identifier: MIT OR Apache-2.0
//! Bounds-checked readers over untrusted Mach-O bytes.
//!
//! Every read validates bounds up front and reports [`ParseError`];
//! no caller of this module can panic on hostile input (P1.1, doc 13).
use crate::{ErrorKind, ParseError, Result};

/// Read cursor over an immutable byte slice.
#[derive(Clone, Copy, Debug)]
pub struct Cursor<'a> {
    b: &'a [u8],
}

impl<'a> Cursor<'a> {
    pub fn new(b: &'a [u8]) -> Self {
        Cursor { b }
    }

    fn window(&self, off: usize, len: usize) -> Result<&'a [u8]> {
        let end = off.checked_add(len).ok_or(ParseError {
            offset: off,
            kind: ErrorKind::OffsetOverflow,
        })?;
        if end > self.b.len() {
            return Err(ParseError {
                offset: off,
                kind: ErrorKind::Truncated {
                    need: end,
                    len: self.b.len(),
                },
            });
        }
        Ok(&self.b[off..end])
    }

    pub fn u8(&self, off: usize) -> Result<u8> {
        Ok(self.window(off, 1)?[0])
    }

    pub fn u16le(&self, off: usize) -> Result<u16> {
        let w = self.window(off, 2)?;
        Ok(u16::from_le_bytes([w[0], w[1]]))
    }

    pub fn u32le(&self, off: usize) -> Result<u32> {
        let w = self.window(off, 4)?;
        Ok(u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
    }

    pub fn u32be(&self, off: usize) -> Result<u32> {
        let w = self.window(off, 4)?;
        Ok(u32::from_be_bytes([w[0], w[1], w[2], w[3]]))
    }

    pub fn u64le(&self, off: usize) -> Result<u64> {
        let w = self.window(off, 8)?;
        Ok(u64::from_le_bytes([
            w[0], w[1], w[2], w[3], w[4], w[5], w[6], w[7],
        ]))
    }

    pub fn u64be(&self, off: usize) -> Result<u64> {
        let w = self.window(off, 8)?;
        Ok(u64::from_be_bytes([
            w[0], w[1], w[2], w[3], w[4], w[5], w[6], w[7],
        ]))
    }

    /// Raw byte window; bounds-checked.
    pub fn bytes(&self, off: usize, len: usize) -> Result<&'a [u8]> {
        self.window(off, len)
    }

    /// NUL-terminated byte string at `off`, searched at most `max` bytes.
    /// Returns the bytes without the terminator.
    pub fn cstr(&self, off: usize, max: usize) -> Result<&'a [u8]> {
        let cap = off
            .checked_add(max)
            .ok_or(ParseError {
                offset: off,
                kind: ErrorKind::OffsetOverflow,
            })?
            .min(self.b.len());
        if off > self.b.len() {
            return Err(ParseError {
                offset: off,
                kind: ErrorKind::Truncated {
                    need: off.saturating_add(1),
                    len: self.b.len(),
                },
            });
        }
        let hay = &self.b[off..cap];
        match hay.iter().position(|&c| c == 0) {
            Some(i) => Ok(&hay[..i]),
            None => Err(ParseError {
                offset: off,
                kind: ErrorKind::UnterminatedString { max },
            }),
        }
    }

    /// Fixed-width name field (segname/sectname): bytes up to the first
    /// NUL, must be UTF-8 (Mach-O names are ASCII in practice).
    pub fn name(&self, off: usize, width: usize) -> Result<&'a str> {
        let raw = self.window(off, width)?;
        let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
        std::str::from_utf8(&raw[..end]).map_err(|_| ParseError {
            offset: off,
            kind: ErrorKind::InvalidName,
        })
    }

    /// Unsigned LEB128 at `*pos`; advances past it.
    pub fn uleb(&self, pos: &mut usize) -> Result<u64> {
        let mut out: u64 = 0;
        let mut shift = 0u32;
        loop {
            let byte = self.u8(*pos)?;
            *pos += 1;
            if shift >= 64 && byte & 0x7f != 0 {
                return Err(ParseError {
                    offset: *pos - 1,
                    kind: ErrorKind::UlebOverflow,
                });
            }
            if shift < 64 {
                out |= ((byte & 0x7f) as u64) << shift;
            }
            if byte & 0x80 == 0 {
                return Ok(out);
            }
            shift += 7;
            if shift > 70 {
                return Err(ParseError {
                    offset: *pos - 1,
                    kind: ErrorKind::UlebOverflow,
                });
            }
        }
    }

    /// Signed LEB128 at `*pos`; advances past it.
    pub fn sleb(&self, pos: &mut usize) -> Result<i64> {
        let mut out: u64 = 0;
        let mut shift = 0u32;
        let mut byte;
        loop {
            byte = self.u8(*pos)?;
            *pos += 1;
            if shift < 64 {
                out |= ((byte & 0x7f) as u64) << shift;
            } else if byte & 0x7f != 0 && byte & 0x7f != 0x7f {
                return Err(ParseError {
                    offset: *pos - 1,
                    kind: ErrorKind::UlebOverflow,
                });
            }
            shift += 7;
            if shift > 70 {
                return Err(ParseError {
                    offset: *pos - 1,
                    kind: ErrorKind::UlebOverflow,
                });
            }
            if byte & 0x80 == 0 {
                break;
            }
        }
        if shift < 64 && byte & 0x40 != 0 {
            out |= !0u64 << shift;
        }
        Ok(out as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cstr_max_offset_reports_truncated() {
        // `need: off + 1` wrapped at usize::MAX.
        let c = Cursor::new(b"");
        assert!(matches!(
            c.cstr(usize::MAX, 0).unwrap_err().kind,
            ErrorKind::Truncated { .. }
        ));
    }

    #[test]
    fn window_rejects_oob() {
        let c = Cursor::new(&[1, 2, 3]);
        assert!(matches!(
            c.u32le(0).unwrap_err().kind,
            ErrorKind::Truncated { .. }
        ));
        assert!(matches!(
            c.u8(usize::MAX).unwrap_err().kind,
            ErrorKind::OffsetOverflow | ErrorKind::Truncated { .. }
        ));
    }

    #[test]
    fn endian_reads() {
        let c = Cursor::new(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
        assert_eq!(c.u16le(0).unwrap(), 0x0201);
        assert_eq!(c.u32le(0).unwrap(), 0x04030201);
        assert_eq!(c.u32be(0).unwrap(), 0x01020304);
        assert_eq!(c.u64le(0).unwrap(), 0x0807060504030201);
        assert_eq!(c.u64be(0).unwrap(), 0x0102030405060708);
    }

    #[test]
    fn cstr_needs_terminator_in_range() {
        let c = Cursor::new(b"ab\0cd");
        assert_eq!(c.cstr(0, 8).unwrap(), b"ab");
        assert!(matches!(
            c.cstr(3, 2).unwrap_err().kind,
            ErrorKind::UnterminatedString { .. }
        ));
    }

    #[test]
    fn uleb_sleb_roundtrip() {
        // 624485 (wikipedia LEB128 example), -123456.
        let c = Cursor::new(&[0xE5, 0x8E, 0x26, 0xC0, 0xBB, 0x78]);
        let mut pos = 0;
        assert_eq!(c.uleb(&mut pos).unwrap(), 624485);
        assert_eq!(c.sleb(&mut pos).unwrap(), -123456);
        assert_eq!(pos, 6);
    }

    #[test]
    fn uleb_rejects_overflow_and_truncation() {
        let c = Cursor::new(&[0x80; 12]);
        let mut pos = 0;
        assert!(matches!(
            c.uleb(&mut pos).unwrap_err().kind,
            ErrorKind::UlebOverflow
        ));
        let mut pos = 11;
        assert!(matches!(
            c.uleb(&mut pos).unwrap_err().kind,
            ErrorKind::Truncated { .. }
        ));
    }
}
