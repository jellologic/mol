// SPDX-License-Identifier: MIT OR Apache-2.0
//! Bind/opcode engine: chained fixups, legacy binds, exports trie,
//! closure policy (docs 03, 12 L1/L3).
//!
//! Contract: pure binding logic over `mol-macho` structures. The
//! engine plans `(address, value)` applications; the loader owns the
//! mapped memory and writes them. C shims own dyld entry, TLS asm,
//! and commpage only (doc 07) — no guest-mapped async, ever.
pub mod fixups;
pub mod init;
pub mod load;
pub mod policy;

use std::fmt;

use mol_macho::map::LoadMap;
use mol_macho::Image;

/// Link/bind failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    Parse(mol_macho::ParseError),
    MissingFile { path: String },
    MissingDylib { needed_by: String, name: String },
    SymbolNotFound { needed_by: String, symbol: String },
    BadOrdinal { needed_by: String, ordinal: i64 },
    BadFixup { needed_by: String, detail: String },
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LinkError::Parse(e) => write!(f, "dyld link: parse failed: {e}"),
            LinkError::MissingFile { path } => write!(f, "dyld link: no such file: {path}"),
            LinkError::MissingDylib { needed_by, name } => {
                write!(f, "dyld link: {needed_by} needs missing {name}")
            }
            LinkError::SymbolNotFound { needed_by, symbol } => {
                write!(f, "dyld link: {needed_by} cannot bind {symbol}")
            }
            LinkError::BadOrdinal { needed_by, ordinal } => {
                write!(f, "dyld link: {needed_by} has bad ordinal {ordinal}")
            }
            LinkError::BadFixup { needed_by, detail } => {
                write!(f, "dyld link: {needed_by} has bad fixup: {detail}")
            }
        }
    }
}

impl std::error::Error for LinkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LinkError::Parse(e) => Some(e),
            _ => None,
        }
    }
}

impl From<mol_macho::ParseError> for LinkError {
    fn from(e: mol_macho::ParseError) -> Self {
        LinkError::Parse(e)
    }
}

pub type Result<T> = std::result::Result<T, LinkError>;

/// One linked image: parsed bytes, load map, slide, and the 1-based
/// ordinal table over its non-`LC_ID_DYLIB` dylib commands in file
/// order (`None` = missing weak import).
#[derive(Debug)]
pub struct LinkedImage<'a> {
    pub path: String,
    pub image: Image<'a>,
    pub map: LoadMap,
    pub slide: u64,
    pub ordinals: Vec<Option<usize>>,
}

/// Images in load order (main first).
#[derive(Debug)]
pub struct LinkedSet<'a> {
    pub images: Vec<LinkedImage<'a>>,
}

impl<'a> LinkedSet<'a> {
    pub fn main(&self) -> &LinkedImage<'a> {
        &self.images[0]
    }

    /// Preferred base address: first segment's vmaddr.
    pub fn base(&self, index: usize) -> u64 {
        self.images[index]
            .map
            .segments
            .first()
            .map(|s| s.vmaddr)
            .unwrap_or(0)
    }
}

/// Ordinal target: a specific image, a flat-namespace search, or a
/// missing weak library (binds to zero).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrdTarget {
    Image(usize),
    Flat,
    MissingWeak,
}

/// Resolve a bind ordinal from image `from`: 0=self, -1=main,
/// -2/-3=flat search, positives index the ordinal table.
pub fn resolve_ordinal(set: &LinkedSet, from: usize, ordinal: i64) -> Result<OrdTarget> {
    match ordinal {
        0 => Ok(OrdTarget::Image(from)),
        -1 => Ok(OrdTarget::Image(0)),
        -2 | -3 => Ok(OrdTarget::Flat),
        o if o > 0 => {
            let table = &set.images[from].ordinals;
            let slot = table.get((o - 1) as usize).ok_or(LinkError::BadOrdinal {
                needed_by: set.images[from].path.clone(),
                ordinal,
            })?;
            Ok(match slot {
                Some(i) => OrdTarget::Image(*i),
                None => OrdTarget::MissingWeak,
            })
        }
        o => Err(LinkError::BadOrdinal {
            needed_by: set.images[from].path.clone(),
            ordinal: o,
        }),
    }
}

/// One cache mode + the linking facts recorded per run (doc 12 L3:
/// the harness picks one cache mode per run and records it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRecord {
    pub cache_mode: policy::CacheMode,
    pub reason: &'static str,
    pub images: Vec<String>,
    pub fixups_applied: usize,
    pub pac_stripped: usize,
    pub pac_available: bool,
    /// libSystem init order trace (P1.2.1; empty when init is skipped).
    pub init_trace: Vec<String>,
}

impl RunRecord {
    pub fn render(&self) -> String {
        let mut s = String::new();
        s.push_str("# mol dyld run record v1\n");
        s.push_str(&format!(
            "cache_mode: {:?} ({})\n",
            self.cache_mode, self.reason
        ));
        for (i, img) in self.images.iter().enumerate() {
            s.push_str(&format!("image[{i}]: {img}\n"));
        }
        s.push_str(&format!("fixups_applied: {}\n", self.fixups_applied));
        s.push_str(&format!("pac_stripped: {}\n", self.pac_stripped));
        s.push_str(&format!("pac_available: {}\n", self.pac_available));
        if !self.init_trace.is_empty() {
            s.push_str(&format!("init: {}\n", self.init_trace.join(" -> ")));
        }
        s
    }
}

#[cfg(test)]
pub(crate) mod testimg {
    //! Minimal synthetic Mach-O builder for engine tests.
    //!
    //! Layout: header + commands, then linkedit payloads (exports
    //! trie, chained fixups) inside a `__TEXT` segment covering the
    //! whole file, plus a `__DATA` segment holding chain entries.
    use mol_macho::header::*;
    use mol_macho::load::*;

    pub const TEXT_VMADDR: u64 = 0x1000;
    pub const DATA_VMADDR: u64 = 0x4000;
    pub const DATA_SIZE: usize = 0x1000;

    pub struct ImgSpec {
        pub filetype: u32,
        pub is_pie: bool,
        pub id: Option<String>,
        pub dylibs: Vec<(u32, String)>,
        pub exports: Vec<(String, u64)>,
        /// (seg_offset in __DATA, import ordinal, addend)
        pub chain_binds: Vec<(u64, u32, i64)>,
        /// arm64e authenticated binds: (seg_offset, import ordinal,
        /// diversity, addr_div, key).
        pub chain_auth_binds: Vec<(u64, u32, u16, bool, u8)>,
        /// Chained pointer format: 2 (`PTR_64`) or 9
        /// (`PTR_ARM64E_USERLAND`). Format 9 uses 8-byte stride.
        pub chain_format: u16,
        /// (seg_offset in __DATA, rebase target vmaddr)
        pub chain_rebases: Vec<(u64, u64)>,
        /// (lib ordinal, weak, name, addend)
        pub imports: Vec<(i64, bool, String, i64)>,
        /// Imports-table format: 1 (`IMPORT`) or 2 (`IMPORT_ADDEND`).
        pub imports_format: u32,
        pub routines: Vec<(u64, u64)>,
        pub entryoff: Option<u64>,
    }

    impl Default for ImgSpec {
        fn default() -> Self {
            ImgSpec {
                filetype: MH_EXECUTE,
                is_pie: true,
                id: None,
                dylibs: Vec::new(),
                exports: Vec::new(),
                chain_binds: Vec::new(),
                chain_auth_binds: Vec::new(),
                chain_format: 2,
                chain_rebases: Vec::new(),
                imports: Vec::new(),
                imports_format: 1,
                routines: Vec::new(),
                entryoff: Some(0x100),
            }
        }
    }

    fn uleb(mut v: u64, out: &mut Vec<u8>) {
        loop {
            let mut b = (v & 0x7f) as u8;
            v >>= 7;
            if v != 0 {
                b |= 0x80;
            }
            out.push(b);
            if v == 0 {
                break;
            }
        }
    }

    fn build_trie(exports: &[(String, u64)]) -> Vec<u8> {
        // Root with one child per symbol; each child is a leaf.
        let mut v = vec![0u8, exports.len() as u8];
        let mut patches = Vec::new();
        for (name, _) in exports {
            v.extend_from_slice(name.as_bytes());
            v.push(0);
            patches.push(v.len());
            v.push(0);
        }
        for (i, (_, addr)) in exports.iter().enumerate() {
            let leaf = v.len();
            // Patch the child offset (single byte: images are tiny).
            assert!(leaf < 128);
            v[patches[i]] = leaf as u8;
            let mut term = Vec::new();
            term.push(0u8); // flags
            uleb(*addr, &mut term);
            v.push(term.len() as u8);
            v.extend_from_slice(&term);
            v.push(0); // no children
        }
        v
    }

    fn build_chained(
        imports: &[(i64, bool, String, i64)],
        imports_format: u32,
        chain_format: u16,
        binds: &[(u64, u32, i64)],
        auth_binds: &[(u64, u32, u16, bool, u8)],
        rebases: &[(u64, u64)],
        data: &mut [u8],
    ) -> Vec<u8> {
        assert!(imports_format == 1 || imports_format == 2);
        assert!(chain_format == 2 || chain_format == 9);
        // Format-9 rebases use the offset encoding, which this
        // builder does not model; binds suffice for the strip tests.
        assert!(rebases.is_empty() || chain_format == 2);
        let stride = if chain_format == 9 { 8u64 } else { 4u64 };
        // Fixup entries first (chain in __DATA seg order).
        let mut links: Vec<(u64, u64)> = Vec::new();
        for (off, ordinal, addend) in binds {
            assert_eq!(*off % stride, 0);
            if chain_format == 9 {
                assert!(*addend >= 0 && *addend < (1 << 19));
                links.push((
                    *off,
                    (*ordinal as u64) | ((*addend as u64) << 32) | (1u64 << 62),
                ));
            } else {
                assert!(*addend >= 0 && *addend < 256);
                links.push((
                    *off,
                    (*ordinal as u64) | ((*addend as u64) << 24) | (1u64 << 63),
                ));
            }
        }
        for (off, ordinal, div, addr_div, key) in auth_binds {
            assert_eq!(chain_format, 9);
            assert_eq!(*off % 8, 0);
            links.push((
                *off,
                (*ordinal as u64)
                    | ((*div as u64) << 32)
                    | ((*addr_div as u64) << 48)
                    | ((*key as u64) << 49)
                    | (1u64 << 62)
                    | (1u64 << 63),
            ));
        }
        for (off, target) in rebases {
            assert_eq!(*off % stride, 0);
            links.push((*off, *target & 0xf_ffff_ffff));
        }
        links.sort_by_key(|(off, _)| *off);
        // Stitch `next` fields.
        for i in 0..links.len() {
            let next = if i + 1 < links.len() {
                (links[i + 1].0 - links[i].0) / stride
            } else {
                0
            };
            links[i].1 |= next << 51;
            let at = links[i].0 as usize;
            data[at..at + 8].copy_from_slice(&links[i].1.to_le_bytes());
        }
        // Payload: header + starts + imports + symbols.
        let mut symbols = Vec::new();
        let mut name_offs = Vec::new();
        for (_, _, name, _) in imports {
            name_offs.push(symbols.len() as u32);
            symbols.extend_from_slice(name.as_bytes());
            symbols.push(0);
        }
        let mut p = vec![0u8; 28];
        let starts_off = 28u32;
        // starts: seg_count=2, seg0 -> 0 (none), seg1 -> info.
        let info_rel = 4 + 2 * 4; // after count + 2 offsets
        p.extend_from_slice(&2u32.to_le_bytes());
        p.extend_from_slice(&0u32.to_le_bytes());
        p.extend_from_slice(&(info_rel as u32).to_le_bytes());
        // starts_in_segment: size, page 0x1000, fmt PTR_64, seg_off,
        // max_valid 0, page_count 1, page_start[0].
        let first = links.first().map(|(off, _)| *off as u16).unwrap_or(0xFFFF);
        let mut seg = vec![0u8; 24];
        seg[0..4].copy_from_slice(&24u32.to_le_bytes());
        seg[4..6].copy_from_slice(&0x1000u16.to_le_bytes());
        seg[6..8].copy_from_slice(&chain_format.to_le_bytes());
        seg[20..22].copy_from_slice(&1u16.to_le_bytes());
        seg[22..24].copy_from_slice(&first.to_le_bytes());
        p.extend_from_slice(&seg);
        let imports_off = p.len() as u32;
        for (i, (lib, weak, _, addend)) in imports.iter().enumerate() {
            let libb = (*lib as i8) as u8 as u32;
            let w = libb | ((*weak as u32) << 8) | (name_offs[i] << 9);
            p.extend_from_slice(&w.to_le_bytes());
            if imports_format == 2 {
                p.extend_from_slice(&(*addend as i32).to_le_bytes());
            }
        }
        let symbols_off = p.len() as u32;
        p.extend_from_slice(&symbols);
        // Header.
        p[0..4].copy_from_slice(&0u32.to_le_bytes()); // version
        p[4..8].copy_from_slice(&starts_off.to_le_bytes());
        p[8..12].copy_from_slice(&imports_off.to_le_bytes());
        p[12..16].copy_from_slice(&symbols_off.to_le_bytes());
        p[16..20].copy_from_slice(&(imports.len() as u32).to_le_bytes());
        p[20..24].copy_from_slice(&imports_format.to_le_bytes());
        p
    }

    fn cmd(code: u32, payload: &[u8]) -> Vec<u8> {
        let size = ((8 + payload.len()).div_ceil(8) * 8) as u32;
        let mut v = Vec::new();
        v.extend_from_slice(&code.to_le_bytes());
        v.extend_from_slice(&size.to_le_bytes());
        v.extend_from_slice(payload);
        v.resize(size as usize, 0);
        v
    }

    fn dylib_cmd(code: u32, name: &str) -> Vec<u8> {
        let mut payload = vec![0u8; 16];
        payload[0..4].copy_from_slice(&24u32.to_le_bytes());
        payload.extend_from_slice(name.as_bytes());
        payload.push(0);
        cmd(code, &payload)
    }

    fn segment(name: &str, vmaddr: u64, fileoff: u64, filesize: u64) -> Vec<u8> {
        let mut payload = vec![0u8; 64];
        payload[..name.len()].copy_from_slice(name.as_bytes());
        payload[16..24].copy_from_slice(&vmaddr.to_le_bytes());
        payload[24..32].copy_from_slice(&filesize.to_le_bytes());
        payload[32..40].copy_from_slice(&fileoff.to_le_bytes());
        payload[40..48].copy_from_slice(&filesize.to_le_bytes());
        payload[48..52].copy_from_slice(&7u32.to_le_bytes());
        payload[52..56].copy_from_slice(&7u32.to_le_bytes());
        cmd(LC_SEGMENT_64, &payload)
    }

    /// Build a complete minimal image. Two passes: all commands
    /// first (sizes are known up front), then offset patching.
    pub fn build(spec: &ImgSpec) -> Vec<u8> {
        let trie = build_trie(&spec.exports);
        let mut data = vec![0u8; DATA_SIZE];
        let chained = if spec.imports.is_empty()
            && spec.chain_binds.is_empty()
            && spec.chain_auth_binds.is_empty()
            && spec.chain_rebases.is_empty()
        {
            Vec::new()
        } else {
            build_chained(
                &spec.imports,
                spec.imports_format,
                spec.chain_format,
                &spec.chain_binds,
                &spec.chain_auth_binds,
                &spec.chain_rebases,
                &mut data,
            )
        };
        // Pass 1: commands with placeholder offsets.
        let mut cmds: Vec<Vec<u8>> = vec![
            segment("__TEXT", TEXT_VMADDR, 0, 0),
            segment("__DATA", DATA_VMADDR, 0, DATA_SIZE as u64),
        ];
        if let Some(id) = &spec.id {
            cmds.push(dylib_cmd(LC_ID_DYLIB, id));
        }
        for (code, name) in &spec.dylibs {
            cmds.push(dylib_cmd(*code, name));
        }
        if let Some(entryoff) = spec.entryoff {
            let mut e = vec![0u8; 16];
            e[..8].copy_from_slice(&entryoff.to_le_bytes());
            cmds.push(cmd(LC_MAIN, &e));
        }
        for (addr, module) in &spec.routines {
            let mut r = vec![0u8; 56];
            r[..8].copy_from_slice(&addr.to_le_bytes());
            r[8..16].copy_from_slice(&module.to_le_bytes());
            cmds.push(cmd(LC_ROUTINES_64, &r));
        }
        let mut trie_idx = None;
        let mut chain_idx = None;
        if !spec.exports.is_empty() {
            trie_idx = Some(cmds.len());
            cmds.push(cmd(LC_DYLD_EXPORTS_TRIE, &[0u8; 8]));
        }
        if !chained.is_empty() {
            chain_idx = Some(cmds.len());
            cmds.push(cmd(LC_DYLD_CHAINED_FIXUPS, &[0u8; 8]));
        }
        // Pass 2: layout + patching.
        let cmds_len: usize = cmds.iter().map(Vec::len).sum();
        let trie_off = 32 + cmds_len;
        let chain_off = trie_off + trie.len();
        let data_off = (chain_off + chained.len()).next_multiple_of(0x1000);
        let text_size = (data_off + DATA_SIZE) as u64;
        // __TEXT is cmds[0]: vmsize @ blob+32, filesize @ blob+48.
        cmds[0][32..40].copy_from_slice(&text_size.to_le_bytes());
        cmds[0][48..56].copy_from_slice(&text_size.to_le_bytes());
        // __DATA is cmds[1]: fileoff @ blob+40.
        cmds[1][40..48].copy_from_slice(&(data_off as u64).to_le_bytes());
        // Linkedit payloads: dataoff @ blob+8, datasize @ blob+12.
        if let Some(i) = trie_idx {
            let blob = &mut cmds[i];
            blob[8..12].copy_from_slice(&(trie_off as u32).to_le_bytes());
            blob[12..16].copy_from_slice(&(trie.len() as u32).to_le_bytes());
        }
        if let Some(i) = chain_idx {
            let blob = &mut cmds[i];
            blob[8..12].copy_from_slice(&(chain_off as u32).to_le_bytes());
            blob[12..16].copy_from_slice(&(chained.len() as u32).to_le_bytes());
        }
        // Pass 3: emit.
        let mut file = vec![0u8; 32];
        file[..4].copy_from_slice(&MH_MAGIC_64.to_le_bytes());
        file[4..8].copy_from_slice(&CPU_TYPE_ARM64.to_le_bytes());
        file[12..16].copy_from_slice(&spec.filetype.to_le_bytes());
        file[16..20].copy_from_slice(&(cmds.len() as u32).to_le_bytes());
        file[20..24].copy_from_slice(&(cmds_len as u32).to_le_bytes());
        file[24..28].copy_from_slice(&(if spec.is_pie { MH_PIE } else { 0 }).to_le_bytes());
        for c in &cmds {
            file.extend_from_slice(c);
        }
        file.extend_from_slice(&trie);
        file.extend_from_slice(&chained);
        file.resize(data_off, 0);
        file.extend_from_slice(&data);
        file
    }
}
