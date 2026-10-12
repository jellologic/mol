// SPDX-License-Identifier: MIT OR Apache-2.0
//! `LC_CODE_SIGNATURE` blob parsing (P1.1.3).
//!
//! The loader skips cryptographic verification but must parse the
//! blob index and honor gating entitlements (JIT, library
//! validation) per doc 03. Layout reference: xnu `cs_blobs.h`
//! (struct shapes only).
use crate::cursor::Cursor;
use crate::{ErrorKind, ParseError, Result};

// --- blob magics (big-endian) ---
pub const CSMAGIC_REQUIREMENT: u32 = 0xfade_0c00;
pub const CSMAGIC_REQUIREMENTS: u32 = 0xfade_0c01;
pub const CSMAGIC_CODEDIRECTORY: u32 = 0xfade_0c02;
pub const CSMAGIC_EMBEDDED_SIGNATURE: u32 = 0xfade_0cc0;
pub const CSMAGIC_EMBEDDED_ENTITLEMENTS: u32 = 0xfade_7171;
pub const CSMAGIC_EMBEDDED_DER_ENTITLEMENTS: u32 = 0xfade_7172;
pub const CSMAGIC_BLOB_WRAPPER: u32 = 0xfade_0b01;

// --- superblob slot types ---
pub const CSSLOT_CODEDIRECTORY: u32 = 0;
pub const CSSLOT_INFOSLOT: u32 = 1;
pub const CSSLOT_REQUIREMENTS: u32 = 2;
pub const CSSLOT_RESOURCEDIR: u32 = 3;
pub const CSSLOT_APPLICATION: u32 = 4;
pub const CSSLOT_ENTITLEMENTS: u32 = 5;
pub const CSSLOT_DER_ENTITLEMENTS: u32 = 6;
pub const CSSLOT_ALTERNATE_CODEDIRECTORIES: u32 = 0x1000;
pub const CSSLOT_SIGNATURESLOT: u32 = 0x10000;

// --- CS flags (doc 03 enforcement keys) ---
pub const CS_VALID: u32 = 0x0000_0001;
pub const CS_ADHOC: u32 = 0x0000_0002;
pub const CS_GET_TASK_ALLOW: u32 = 0x0000_0004;
pub const CS_INSTALLER: u32 = 0x0000_0008;
pub const CS_HARD: u32 = 0x0000_0100;
pub const CS_KILL: u32 = 0x0000_0200;
pub const CS_CHECK_EXPIRATION: u32 = 0x0000_0400;
pub const CS_RESTRICT: u32 = 0x0000_0800;
pub const CS_ENFORCEMENT: u32 = 0x0000_1000;
pub const CS_REQUIRE_LV: u32 = 0x0000_2000;
pub const CS_ENTITLEMENTS_VALIDATED: u32 = 0x0000_4000;
pub const CS_RUNTIME: u32 = 0x0001_0000;
pub const CS_LINKER_SIGNED: u32 = 0x0002_0000;
pub const CS_PLATFORM_BINARY: u32 = 0x0400_0000;

/// Entitlements whose presence changes loader behavior. Anything else
/// is parsed (for dumps) but ignored by policy.
pub const GATING_ENTITLEMENTS: &[&str] = &[
    "com.apple.security.cs.allow-jit",
    "com.apple.security.cs.allow-unsigned-executable-memory",
    "com.apple.security.cs.disable-library-validation",
    "com.apple.security.cs.allow-dyld-environment-variables",
    "com.apple.security.cs.disable-executable-page-protection",
    "com.apple.security.get-task-allow",
];

/// Parsed code directory (hashes are not verified, P1.1.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeDirectory<'a> {
    pub version: u32,
    pub flags: u32,
    pub ident: &'a str,
    pub team_id: Option<&'a str>,
    pub hash_type: u8,
    pub hash_size: u8,
    pub n_special_slots: u32,
    pub n_code_slots: u32,
    pub code_limit: u64,
    pub platform: u8,
    pub page_size: u8,
}

impl<'a> CodeDirectory<'a> {
    pub fn is_adhoc(&self) -> bool {
        self.flags & CS_ADHOC != 0
    }

    pub fn is_hardened_runtime(&self) -> bool {
        self.flags & CS_RUNTIME != 0
    }

    pub fn requires_library_validation(&self) -> bool {
        self.flags & CS_REQUIRE_LV != 0
    }
}

/// One blob in the signature, with its superblob slot
/// (`u32::MAX` when the payload is a bare blob).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blob<'a> {
    CodeDirectory(CodeDirectory<'a>),
    /// Raw XML plist bytes (parsed on demand for gating keys).
    Entitlements(&'a [u8]),
    Requirements(&'a [u8]),
    Unknown {
        magic: u32,
        bytes: &'a [u8],
    },
}

/// Parsed `LC_CODE_SIGNATURE` payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature<'a> {
    pub blobs: Vec<(u32, Blob<'a>)>,
}

impl<'a> Signature<'a> {
    /// First code directory, if any.
    pub fn code_directory(&self) -> Option<&CodeDirectory<'a>> {
        self.blobs.iter().find_map(|(_, b)| match b {
            Blob::CodeDirectory(cd) => Some(cd),
            _ => None,
        })
    }

    /// Gating entitlements present with a boolean value, as
    /// (key, value) in `GATING_ENTITLEMENTS` order.
    pub fn gating_entitlements(&self) -> Vec<(&'static str, bool)> {
        let mut out = Vec::new();
        for (_, blob) in &self.blobs {
            if let Blob::Entitlements(xml) = blob {
                for key in GATING_ENTITLEMENTS {
                    if let Some(v) = plist_bool(xml, key) {
                        if !out.iter().any(|(k, _)| k == key) {
                            out.push((*key, v));
                        }
                    }
                }
            }
        }
        out
    }
}

/// Parse a signature payload: superblob or bare blob.
pub fn parse(data: &[u8]) -> Result<Signature<'_>> {
    let cur = Cursor::new(data);
    let magic = cur.u32be(0)?;
    if magic == CSMAGIC_EMBEDDED_SIGNATURE {
        parse_superblob(data)
    } else {
        let len = cur.u32be(4)? as usize;
        if len > data.len() || len < 8 {
            return Err(ParseError {
                offset: 0,
                kind: ErrorKind::BadBlobLength { magic, len },
            });
        }
        Ok(Signature {
            blobs: vec![(u32::MAX, decode_blob(magic, &data[..len], 0)?)],
        })
    }
}

fn parse_superblob(data: &[u8]) -> Result<Signature<'_>> {
    let cur = Cursor::new(data);
    let total = cur.u32be(4)? as usize;
    if total > data.len() {
        return Err(ParseError {
            offset: 0,
            kind: ErrorKind::Truncated {
                need: total,
                len: data.len(),
            },
        });
    }
    let count = cur.u32be(8)? as usize;
    if count > 64 {
        return Err(ParseError {
            offset: 8,
            kind: ErrorKind::TooManyBlobs { count },
        });
    }
    let mut blobs = Vec::with_capacity(count.min(8));
    for i in 0..count {
        let at = 12 + i * 8;
        let slot = cur.u32be(at)?;
        let off = cur.u32be(at + 4)? as usize;
        if off + 8 > total {
            return Err(ParseError {
                offset: at,
                kind: ErrorKind::Truncated {
                    need: off + 8,
                    len: data.len(),
                },
            });
        }
        let magic = cur.u32be(off)?;
        let len = cur.u32be(off + 4)? as usize;
        if len < 8 || off + len > total {
            return Err(ParseError {
                offset: off,
                kind: ErrorKind::BadBlobLength { magic, len },
            });
        }
        blobs.push((slot, decode_blob(magic, &data[off..off + len], off)?));
    }
    Ok(Signature { blobs })
}

fn decode_blob(magic: u32, bytes: &[u8], base: usize) -> Result<Blob<'_>> {
    match magic {
        CSMAGIC_CODEDIRECTORY => Ok(Blob::CodeDirectory(decode_directory(bytes, base)?)),
        CSMAGIC_EMBEDDED_ENTITLEMENTS => Ok(Blob::Entitlements(&bytes[8..])),
        CSMAGIC_REQUIREMENT | CSMAGIC_REQUIREMENTS => Ok(Blob::Requirements(bytes)),
        _ => Ok(Blob::Unknown { magic, bytes }),
    }
}

fn decode_directory(bytes: &[u8], base: usize) -> Result<CodeDirectory<'_>> {
    let cur = Cursor::new(bytes);
    let err_at = |off: usize| ParseError {
        offset: base + off,
        kind: ErrorKind::Truncated {
            need: base + off + 4,
            len: base + bytes.len(),
        },
    };
    let version = cur.u32be(8).map_err(|_| err_at(8))?;
    let flags = cur.u32be(12).map_err(|_| err_at(12))?;
    let hash_offset = cur.u32be(16).map_err(|_| err_at(16))? as usize;
    let ident_offset = cur.u32be(20).map_err(|_| err_at(20))? as usize;
    let n_special = cur.u32be(24).map_err(|_| err_at(24))?;
    let n_code = cur.u32be(28).map_err(|_| err_at(28))?;
    let code_limit = cur.u32be(32).map_err(|_| err_at(32))?;
    let hash_size = cur.u8(36).map_err(|_| err_at(36))?;
    let hash_type = cur.u8(37).map_err(|_| err_at(37))?;
    let platform = cur.u8(38).map_err(|_| err_at(38))?;
    let page_size = cur.u8(39).map_err(|_| err_at(39))?;
    let _spare2 = cur.u32be(40).map_err(|_| err_at(40))?;
    let _ = hash_offset;
    let mut code_limit64 = u64::from(code_limit);
    let mut team_id = None;
    if version >= 2 {
        let team_off = cur.u32be(52).map_err(|_| err_at(52))? as usize;
        if team_off != 0 {
            let raw = cur.cstr(team_off, bytes.len().saturating_sub(team_off))?;
            team_id = Some(std::str::from_utf8(raw).map_err(|_| ParseError {
                offset: base + team_off,
                kind: ErrorKind::InvalidName,
            })?);
        }
    }
    if version >= 3 {
        code_limit64 = cur.u64be(60).map_err(|_| ParseError {
            offset: base + 60,
            kind: ErrorKind::Truncated {
                need: base + 68,
                len: base + bytes.len(),
            },
        })?;
    }
    let ident_raw = cur.cstr(ident_offset, bytes.len().saturating_sub(ident_offset))?;
    let ident = std::str::from_utf8(ident_raw).map_err(|_| ParseError {
        offset: base + ident_offset,
        kind: ErrorKind::InvalidName,
    })?;
    Ok(CodeDirectory {
        version,
        flags,
        ident,
        team_id,
        hash_type,
        hash_size,
        n_special_slots: n_special,
        n_code_slots: n_code,
        code_limit: code_limit64,
        platform,
        page_size,
    })
}

/// Minimal XML-plist scan: value of `<key>key</key>` when followed by
/// `<true/>` or `<false/>` (ASCII whitespace allowed between).
pub fn plist_bool(xml: &[u8], key: &str) -> Option<bool> {
    let open = format!("<key>{key}</key>");
    let mut search = 0usize;
    while let Some(found) = find_subslice(&xml[search..], open.as_bytes()) {
        let mut pos = search + found + open.len();
        while pos < xml.len() && xml[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if xml[pos..].starts_with(b"<true/>") {
            return Some(true);
        }
        if xml[pos..].starts_with(b"<false/>") {
            return Some(false);
        }
        search = pos.min(xml.len().saturating_sub(1) + 1);
        if search >= xml.len() {
            break;
        }
    }
    None
}

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Slot mnemonic for dumps.
pub fn slot_name(slot: u32) -> &'static str {
    match slot {
        CSSLOT_CODEDIRECTORY => "CODEDIRECTORY",
        CSSLOT_INFOSLOT => "INFOSLOT",
        CSSLOT_REQUIREMENTS => "REQUIREMENTS",
        CSSLOT_RESOURCEDIR => "RESOURCEDIR",
        CSSLOT_APPLICATION => "APPLICATION",
        CSSLOT_ENTITLEMENTS => "ENTITLEMENTS",
        CSSLOT_DER_ENTITLEMENTS => "DER_ENTITLEMENTS",
        CSSLOT_ALTERNATE_CODEDIRECTORIES => "ALTERNATE_CODEDIRECTORIES",
        CSSLOT_SIGNATURESLOT => "SIGNATURESLOT",
        _ => "UNKNOWN",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codedir(flags: u32, ident: &str) -> Vec<u8> {
        // v2 directory: fixed 56 bytes + ident + team.
        let mut v = vec![0u8; 56];
        v[0..4].copy_from_slice(&CSMAGIC_CODEDIRECTORY.to_be_bytes());
        v[8..12].copy_from_slice(&0x20400u32.to_be_bytes()); // version 2.4
        v[12..16].copy_from_slice(&flags.to_be_bytes());
        v[20..24].copy_from_slice(&56u32.to_be_bytes()); // identOffset
        v[28..32].copy_from_slice(&4u32.to_be_bytes()); // nCodeSlots
        v[36] = 32; // hashSize (sha256)
        v[37] = 2; // hashType
        let team_off = 56 + ident.len() + 1;
        v[52..56].copy_from_slice(&(team_off as u32).to_be_bytes());
        v.extend_from_slice(ident.as_bytes());
        v.push(0);
        v.extend_from_slice(b"TEAMID1234\0");
        let len = v.len() as u32;
        v[4..8].copy_from_slice(&len.to_be_bytes());
        v
    }

    fn entitlements(allow_jit: bool) -> Vec<u8> {
        let mut v = vec![0u8; 8];
        v[0..4].copy_from_slice(&CSMAGIC_EMBEDDED_ENTITLEMENTS.to_be_bytes());
        let xml = format!(
            "<?xml version=\"1.0\"?><plist><dict><key>com.apple.security.cs.allow-jit</key><{}/><key>other.key</key><string>x</string></dict></plist>",
            if allow_jit { "true" } else { "false" }
        );
        v.extend_from_slice(xml.as_bytes());
        let len = v.len() as u32;
        v[4..8].copy_from_slice(&len.to_be_bytes());
        v
    }

    fn superblob(cd: &[u8], ent: &[u8]) -> Vec<u8> {
        let mut v = vec![0u8; 12 + 16];
        v[0..4].copy_from_slice(&CSMAGIC_EMBEDDED_SIGNATURE.to_be_bytes());
        v[8..12].copy_from_slice(&2u32.to_be_bytes());
        let cd_off = v.len() as u32;
        v[12..16].copy_from_slice(&CSSLOT_CODEDIRECTORY.to_be_bytes());
        v[16..20].copy_from_slice(&cd_off.to_be_bytes());
        let ent_off = cd_off + cd.len() as u32;
        v[20..24].copy_from_slice(&CSSLOT_ENTITLEMENTS.to_be_bytes());
        v[24..28].copy_from_slice(&ent_off.to_be_bytes());
        v.extend_from_slice(cd);
        v.extend_from_slice(ent);
        let len = v.len() as u32;
        v[4..8].copy_from_slice(&len.to_be_bytes());
        v
    }

    #[test]
    fn superblob_decodes_dir_and_gating_key() {
        let cd = codedir(CS_ADHOC | CS_RUNTIME, "hello");
        let ent = entitlements(true);
        let sb = superblob(&cd, &ent);
        let sig = parse(&sb).unwrap();
        assert_eq!(sig.blobs.len(), 2);
        let dir = sig.code_directory().unwrap();
        assert_eq!(dir.ident, "hello");
        assert_eq!(dir.team_id, Some("TEAMID1234"));
        assert!(dir.is_adhoc());
        assert!(dir.is_hardened_runtime());
        assert!(!dir.requires_library_validation());
        assert_eq!(
            sig.gating_entitlements(),
            vec![("com.apple.security.cs.allow-jit", true)]
        );
    }

    #[test]
    fn bare_blob_and_false_value() {
        let ent = entitlements(false);
        let sig = parse(&ent).unwrap();
        assert_eq!(
            sig.gating_entitlements(),
            vec![("com.apple.security.cs.allow-jit", false)]
        );
    }

    #[test]
    fn rejects_truncated_index() {
        let mut sb = superblob(&codedir(0, "a"), &entitlements(true));
        sb.truncate(sb.len() - 4);
        assert!(parse(&sb).is_err());
    }
}
