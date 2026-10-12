// SPDX-License-Identifier: MIT OR Apache-2.0
//! ObjC metadata parsing: `__objc_classlist` / `__objc_catlist` to
//! class + category records with resolved `+load` IMPs (doc 03).
//!
//! Layouts per `objc-runtime-new.h` (stable 64-bit ABI). All reads
//! go through bounds-checked helpers; hostile bytes yield `Err`,
//! never panic. Supports small (relative-pointer) and big method
//! lists; categories use absolute pointers.

use mol_macho::map::{LoadMap, MapSegment};

use crate::{ObjcError, Result};

// class_t (40 bytes): isa, superclass, cache, vtable, data.
const CLASS_T_SIZE: usize = 40;
const CLASS_SUPER_OFFSET: usize = 8;
const CLASS_DATA_OFFSET: usize = 32;
// class_t.data low bits are flags (FAST_DATA_MASK).
const FAST_DATA_MASK: u64 = 0x0000_7fff_ffff_fff8;

// class_ro_t: name @24, baseMethods @32.
const RO_NAME_OFFSET: usize = 24;
const RO_METHODS_OFFSET: usize = 32;

// method_list_t: entsizeAndFlags u32, count u32.
const MLIST_SMALL_FLAG: u32 = 0x8000_0000;
const MLIST_SMALL_ENTSIZE: u32 = 12;
const MLIST_BIG_ENTSIZE: u32 = 24;
// Hostile-count cap (real classes carry < 10K methods).
const MAX_METHODS: u64 = 1 << 20;

// category_t (56 bytes): name, cls, instanceMethods, classMethods...
const CATEGORY_T_SIZE: usize = 56;
const CATEGORY_CLS_OFFSET: usize = 8;
const CATEGORY_CLASS_METHODS_OFFSET: usize = 24;

const SEL_LOAD: &[u8] = b"load";

/// One parsed class: identity, lineage, own `+load`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassRec {
    /// Slid class_t VA (identity across images).
    pub addr: u64,
    pub name: String,
    /// Slid superclass VA (`None` = nil root).
    pub superclass: Option<u64>,
    /// Own `+load` IMP + SEL VAs (`None` = no own `+load`).
    /// Scanned from the metaclass base methods only, so category
    /// `+load`s never double-report here.
    pub load: Option<LoadMethod>,
}

/// One parsed category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryRec {
    /// Slid category_t VA.
    pub addr: u64,
    pub name: String,
    /// Raw parent class pointer (0 = nil). Resolved against the
    /// global class table at schedule time so cross-image
    /// categories link; unresolvable parents wait forever.
    pub cls: u64,
    /// Category `+load` IMP + SEL VAs (`None` = no `+load`).
    pub load: Option<LoadMethod>,
}

/// A `+load` method: code address + selector address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadMethod {
    /// Slid IMP VA (call with `(cls, sel)`).
    pub imp: u64,
    /// Slid SEL VA (points at the `"load"` methname).
    pub sel: u64,
}

/// One image's parsed ObjC metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObjcImage {
    pub classes: Vec<ClassRec>,
    pub categories: Vec<CategoryRec>,
}

/// Byte source for metadata reads: file bytes (tools, fixtures)
/// or live mapped memory (the loader, post-fixup).
pub trait Backing {
    /// `len` bytes at a VA (`slid` for mapped, file VA for files).
    fn window(&self, vma: u64, len: usize) -> Result<&[u8]>;
}

/// File-bytes backing. File pointers are unslid by definition, so
/// this view takes no slide (slid images must use [`MappedBacking`],
/// whose pointers were fixed up in memory).
pub struct FileBacking<'a> {
    bytes: &'a [u8],
    segments: &'a [MapSegment],
}

impl<'a> FileBacking<'a> {
    pub fn new(bytes: &'a [u8], map: &'a LoadMap) -> Self {
        FileBacking {
            bytes,
            segments: &map.segments,
        }
    }
}

impl Backing for FileBacking<'_> {
    fn window(&self, vma: u64, len: usize) -> Result<&[u8]> {
        for seg in self.segments {
            let rel = vma.wrapping_sub(seg.vmaddr);
            if rel < seg.vmsize {
                let off = seg.fileoff.checked_add(rel).ok_or(ObjcError::Malformed {
                    offset: vma,
                    detail: "file offset overflows",
                })?;
                let end = (off as usize).checked_add(len).ok_or(ObjcError::Malformed {
                    offset: vma,
                    detail: "window overflows",
                })?;
                return self.bytes.get(off as usize..end).ok_or(ObjcError::Truncated {
                    offset: off as usize,
                    len: self.bytes.len(),
                });
            }
        }
        Err(ObjcError::Malformed {
            offset: vma,
            detail: "pointer outside image segments",
        })
    }
}

/// Live-memory backing over content segments mapped at
/// `vmaddr + slide` in this process.
pub struct MappedBacking<'a> {
    segments: &'a [MapSegment],
    slide: u64,
}

impl<'a> MappedBacking<'a> {
    /// Caller guarantees every content segment is mapped readable
    /// at its slid VA for the backing's lifetime.
    pub fn new(map: &'a LoadMap, slide: u64) -> Self {
        MappedBacking {
            segments: &map.segments,
            slide,
        }
    }
}

impl Backing for MappedBacking<'_> {
    fn window(&self, vma: u64, len: usize) -> Result<&[u8]> {
        for seg in self.segments {
            // Content segments only: reservation holes share the
            // table but are mapped PROT_NONE.
            if seg.vmsize == 0 || seg.filesize == 0 {
                continue;
            }
            let Some(base) = seg.vmaddr.checked_add(self.slide) else {
                continue;
            };
            let rel = vma.wrapping_sub(base);
            if rel < seg.vmsize && (len as u64) <= seg.vmsize - rel {
                // SAFETY: validated inside a live readable mapping.
                return Ok(unsafe {
                    std::slice::from_raw_parts(vma as *const u8, len)
                });
            }
        }
        Err(ObjcError::Malformed {
            offset: vma,
            detail: "pointer outside mapped segments",
        })
    }
}

/// Struct reader over a backing.
struct View<B> {
    backing: B,
}

impl<B: Backing> View<B> {
    fn u64_va(&self, vma: u64) -> Result<u64> {
        let w = self.backing.window(vma, 8)?;
        Ok(u64::from_le_bytes(w.try_into().unwrap()))
    }

    fn u32_va(&self, vma: u64) -> Result<u32> {
        let w = self.backing.window(vma, 4)?;
        Ok(u32::from_le_bytes(w.try_into().unwrap()))
    }

    /// NUL-terminated string at a VA. Searches the enclosing
    /// segment tail; unterminated data errors.
    fn cstr_va(&self, vma: u64) -> Result<Vec<u8>> {
        // Probe byte-by-byte through windows: total reads bounded
        // by the segment the pointer validated into.
        let mut out = Vec::new();
        let mut at = vma;
        loop {
            let w = self.backing.window(at, 1)?;
            let b = w[0];
            if b == 0 {
                return Ok(out);
            }
            // Names are short; cap before a hostile loop matters.
            if out.len() >= 4096 {
                return Err(ObjcError::Malformed {
                    offset: vma,
                    detail: "overlong string",
                });
            }
            out.push(b);
            at = at.checked_add(1).ok_or(ObjcError::Malformed {
                offset: vma,
                detail: "string address overflows",
            })?;
        }
    }
}

/// Find a section by name across all segments.
fn find_section(map: &LoadMap, sectname: &str) -> Option<(u64, u64)> {
    for seg in &map.segments {
        for sc in &seg.sections {
            if sc.sectname == sectname {
                return Some((sc.addr, sc.size));
            }
        }
    }
    None
}

/// Scan a method list at a VA for `+load`. Returns the IMP +
/// SEL VAs, or `None` when the list carries no `+load`.
fn find_load<B: Backing>(view: &View<B>, list_va: u64) -> Result<Option<LoadMethod>> {
    if list_va == 0 {
        return Ok(None);
    }
    let flags = view.u32_va(list_va)?;
    let count = view.u32_va(list_va.checked_add(4).ok_or(ObjcError::Malformed {
        offset: list_va,
        detail: "method list header overflows",
    })?)?;
    let small = flags & MLIST_SMALL_FLAG != 0;
    let entsize = flags & 0xffff;
    let expect = if small {
        MLIST_SMALL_ENTSIZE
    } else {
        MLIST_BIG_ENTSIZE
    };
    if entsize != expect {
        return Err(ObjcError::Malformed {
            offset: list_va,
            detail: "bad method list entsize",
        });
    }
    if count as u64 > MAX_METHODS {
        return Err(ObjcError::Malformed {
            offset: list_va,
            detail: "method count exceeds cap",
        });
    }
    for i in 0..count as u64 {
        let entry_va = list_va
            .checked_add(8)
            .and_then(|b| b.checked_add(i.checked_mul(entsize as u64)?))
            .ok_or(ObjcError::Malformed {
                offset: list_va,
                detail: "method entry overflows",
            })?;
        let (name_va, imp_va) = if small {
            let name_off = view.u32_va(entry_va)? as i32 as i64;
            let imp_off = view.u32_va(entry_va.checked_add(8).ok_or(ObjcError::Malformed {
                offset: entry_va,
                detail: "method entry overflows",
            })?)? as i32 as i64;
            let base = entry_va as i64;
            (
                base.wrapping_add(name_off) as u64,
                base.wrapping_add(imp_off) as u64,
            )
        } else {
            let name = view.u64_va(entry_va)?;
            let imp = view.u64_va(entry_va.checked_add(16).ok_or(ObjcError::Malformed {
                offset: entry_va,
                detail: "method entry overflows",
            })?)?;
            (name, imp)
        };
        if view.cstr_va(name_va)? == SEL_LOAD {
            return Ok(Some(LoadMethod {
                imp: imp_va,
                sel: name_va,
            }));
        }
    }
    Ok(None)
}

fn add_va(base: u64, off: usize) -> Result<u64> {
    base.checked_add(off as u64).ok_or(ObjcError::Malformed {
        offset: base,
        detail: "struct field overflows",
    })
}

/// Class name via `data -> class_ro -> name`.
fn class_name<B: Backing>(view: &View<B>, class_va: u64) -> Result<String> {
    let data = view.u64_va(add_va(class_va, CLASS_DATA_OFFSET)?)? & FAST_DATA_MASK;
    let name_va = view.u64_va(add_va(data, RO_NAME_OFFSET)?)?;
    let bytes = view.cstr_va(name_va)?;
    String::from_utf8(bytes).map_err(|_| ObjcError::Malformed {
        offset: name_va,
        detail: "class name not UTF-8",
    })
}

/// Own `+load` via the metaclass base methods (`cls->ISA()`).
fn class_load<B: Backing>(view: &View<B>, class_va: u64) -> Result<Option<LoadMethod>> {
    let isa = view.u64_va(class_va)?;
    if isa == 0 {
        return Err(ObjcError::Malformed {
            offset: class_va,
            detail: "class without metaclass",
        });
    }
    let data = view.u64_va(add_va(isa, CLASS_DATA_OFFSET)?)? & FAST_DATA_MASK;
    let methods = view.u64_va(add_va(data, RO_METHODS_OFFSET)?)?;
    find_load(view, methods)
}

fn parse_class<B: Backing>(view: &View<B>, class_va: u64) -> Result<ClassRec> {
    // Touch the full struct so short mappings truncate cleanly.
    view.u64_va(add_va(class_va, CLASS_T_SIZE - 8)?)?;
    let superclass = view.u64_va(add_va(class_va, CLASS_SUPER_OFFSET)?)?;
    Ok(ClassRec {
        addr: class_va,
        name: class_name(view, class_va)?,
        superclass: if superclass == 0 {
            None
        } else {
            Some(superclass)
        },
        load: class_load(view, class_va)?,
    })
}

fn parse_category<B: Backing>(view: &View<B>, cat_va: u64) -> Result<CategoryRec> {
    view.u64_va(add_va(cat_va, CATEGORY_T_SIZE - 8)?)?;
    let name_va = view.u64_va(cat_va)?;
    let cls = view.u64_va(add_va(cat_va, CATEGORY_CLS_OFFSET)?)?;
    let methods = view.u64_va(add_va(cat_va, CATEGORY_CLASS_METHODS_OFFSET)?)?;
    let name_bytes = view.cstr_va(name_va)?;
    let name = String::from_utf8(name_bytes).map_err(|_| ObjcError::Malformed {
        offset: name_va,
        detail: "category name not UTF-8",
    })?;
    Ok(CategoryRec {
        addr: cat_va,
        name,
        cls,
        load: find_load(view, methods)?,
    })
}

fn list_entry(list_va: u64, i: u64) -> Result<u64> {
    list_va
        .checked_add(i.checked_mul(8).ok_or(ObjcError::Malformed {
            offset: list_va,
            detail: "list index overflows",
        })?)
        .ok_or(ObjcError::Malformed {
            offset: list_va,
            detail: "list entry overflows",
        })
}

fn parse<B: Backing>(view: &View<B>, map: &LoadMap, slide: Option<u64>) -> Result<ObjcImage> {
    // Section addrs are file VAs; mapped reads slide them.
    let base = |sectname: &str| -> Result<Option<(u64, u64)>> {
        match find_section(map, sectname) {
            None => Ok(None),
            Some((addr, size)) => {
                let va = match slide {
                    None => addr,
                    Some(s) => addr.checked_add(s).ok_or(ObjcError::Malformed {
                        offset: addr,
                        detail: "section address overflows",
                    })?,
                };
                Ok(Some((va, size)))
            }
        }
    };
    let mut out = ObjcImage::default();
    if let Some((addr, size)) = base("__objc_classlist")? {
        for i in 0..size / 8 {
            let class_va = view.u64_va(list_entry(addr, i)?)?;
            if class_va == 0 {
                continue;
            }
            out.classes.push(parse_class(view, class_va)?);
        }
    }
    if let Some((addr, size)) = base("__objc_catlist")? {
        for i in 0..size / 8 {
            let cat_va = view.u64_va(list_entry(addr, i)?)?;
            if cat_va == 0 {
                continue;
            }
            out.categories.push(parse_category(view, cat_va)?);
        }
    }
    Ok(out)
}

/// Parse one image's ObjC metadata from file bytes. Images without
/// ObjC sections parse to empty records (plain C binaries).
pub fn parse_image(bytes: &[u8], map: &LoadMap) -> Result<ObjcImage> {
    let view = View {
        backing: FileBacking::new(bytes, map),
    };
    parse(&view, map, None)
}

/// Parse one image's ObjC metadata from live mapped memory (slid,
/// post-fixup). The mapping must outlive the call.
pub fn parse_mapped(map: &LoadMap, slide: u64) -> Result<ObjcImage> {
    let view = View {
        backing: MappedBacking::new(map, slide),
    };
    parse(&view, map, Some(slide))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mol_macho::map::MapSection;

    /// Minimal valid image: one class with a big-list `+load`.
    /// TEXT @0x1000 (file 0), DATA @0x2000 (file 0x1000).
    fn tiny(big: bool) -> (Vec<u8>, LoadMap) {
        let mut b = vec![0u8; 0x2000];
        // "load" methname at TEXT+0x100.
        b[0x100..0x105].copy_from_slice(b"load\0");
        // Method list at TEXT+0x200: one entry.
        let lo = 0x200;
        if big {
            b[lo..lo + 4].copy_from_slice(&24u32.to_le_bytes());
            b[lo + 4..lo + 8].copy_from_slice(&1u32.to_le_bytes());
            b[lo + 8..lo + 16].copy_from_slice(&0x1100u64.to_le_bytes());
            b[lo + 24..lo + 32].copy_from_slice(&0x10u64.to_le_bytes());
        } else {
            b[lo..lo + 4].copy_from_slice(&0x8000_000cu32.to_le_bytes());
            b[lo + 4..lo + 8].copy_from_slice(&1u32.to_le_bytes());
            // entry VA 0x1208: name -0x108, imp -0x1f8.
            b[lo + 8..lo + 12].copy_from_slice(&(-0x108i32).to_le_bytes());
            b[lo + 12..lo + 16].copy_from_slice(&0i32.to_le_bytes());
            b[lo + 16..lo + 20].copy_from_slice(&(-0x1f8i32).to_le_bytes());
        }
        // "Klass" name at TEXT+0x300.
        b[0x300..0x306].copy_from_slice(b"Klass\0");
        // class_ro at DATA+0.
        let ro = 0x1000;
        b[ro + 24..ro + 32].copy_from_slice(&0x1300u64.to_le_bytes());
        // metaclass_ro at DATA+0x100 with the method list.
        let mro = 0x1100;
        b[mro + 24..mro + 32].copy_from_slice(&0x1300u64.to_le_bytes());
        b[mro + 32..mro + 40].copy_from_slice(&0x1200u64.to_le_bytes());
        // metaclass at DATA+0x200, class at DATA+0x300.
        let mm = 0x1200;
        b[mm..mm + 8].copy_from_slice(&0x2200u64.to_le_bytes());
        b[mm + 32..mm + 40].copy_from_slice(&0x2100u64.to_le_bytes());
        let cc = 0x1300;
        b[cc..cc + 8].copy_from_slice(&0x2200u64.to_le_bytes());
        b[cc + 32..cc + 40].copy_from_slice(&0x2000u64.to_le_bytes());
        // classlist at DATA+0x400.
        b[0x1400..0x1408].copy_from_slice(&0x2300u64.to_le_bytes());
        let seg = |name: &str, vmaddr: u64, fileoff: u64, sections: Vec<MapSection>| MapSegment {
            segname: name.to_string(),
            vmaddr,
            vmsize: 0x1000,
            fileoff,
            filesize: 0x1000,
            maxprot: 7,
            initprot: 7,
            flags: 0,
            sections,
        };
        let sc = |sectname: &str, addr: u64, size: u64, offset: u32| MapSection {
            sectname: sectname.to_string(),
            addr,
            size,
            offset,
            flags: 0,
        };
        let map = LoadMap {
            cputype: 0,
            cpusubtype: 0,
            filetype: 0,
            flags: 0,
            slide: 0,
            segments: vec![
                seg("__TEXT", 0x1000, 0, vec![sc("__text", 0x1000, 0x1000, 0)]),
                seg(
                    "__DATA",
                    0x2000,
                    0x1000,
                    vec![sc("__objc_classlist", 0x2400, 8, 0x1400)],
                ),
            ],
            entry: None,
            routines: Vec::new(),
            dylinker: None,
            dylibs: Vec::new(),
            uuid: None,
            build: None,
            codesign: None,
            exports: Vec::new(),
            fixups: Vec::new(),
            unknown_cmds: Vec::new(),
        };
        (b, map)
    }

    #[test]
    fn big_method_list_parses_load() {
        let (b, map) = tiny(true);
        let img = parse_image(&b, &map).unwrap();
        assert_eq!(img.classes.len(), 1);
        assert_eq!(img.classes[0].name, "Klass");
        assert_eq!(
            img.classes[0].load,
            Some(LoadMethod {
                imp: 0x10,
                sel: 0x1100
            })
        );
    }

    #[test]
    fn small_method_list_parses_load() {
        let (b, map) = tiny(false);
        let img = parse_image(&b, &map).unwrap();
        assert_eq!(img.classes.len(), 1);
        // entry 0x1208: name 0x1208-0x108, imp 0x1208-0x1f8.
        assert_eq!(
            img.classes[0].load,
            Some(LoadMethod {
                imp: 0x1010,
                sel: 0x1100
            })
        );
    }

    #[test]
    fn bad_entsize_is_malformed() {
        let (mut b, map) = tiny(false);
        b[0x200..0x204].copy_from_slice(&0x8000_0010u32.to_le_bytes());
        assert!(matches!(
            parse_image(&b, &map).unwrap_err(),
            ObjcError::Malformed { .. }
        ));
    }

    #[test]
    fn method_cap_is_enforced() {
        let (mut b, map) = tiny(true);
        b[0x204..0x208].copy_from_slice(&0xffff_ffffu32.to_le_bytes());
        assert!(matches!(
            parse_image(&b, &map).unwrap_err(),
            ObjcError::Malformed { .. }
        ));
    }

    #[test]
    fn dangling_class_pointer_is_malformed() {
        let (mut b, map) = tiny(true);
        b[0x1400..0x1408].copy_from_slice(&0x9u64.to_le_bytes());
        assert!(parse_image(&b, &map).is_err());
    }

    #[test]
    fn mapped_window_reads_live_memory() {
        // MappedBacking over a heap buffer: in-range reads see
        // live bytes, out-of-range reads error (never fault).
        let buf = vec![0x41u8; 64];
        let base = buf.as_ptr() as u64;
        let map = LoadMap {
            cputype: 0,
            cpusubtype: 0,
            filetype: 0,
            flags: 0,
            slide: 0,
            segments: vec![MapSegment {
                segname: "__DATA".to_string(),
                vmaddr: base,
                vmsize: 64,
                fileoff: 0,
                filesize: 64,
                maxprot: 3,
                initprot: 3,
                flags: 0,
                sections: Vec::new(),
            }],
            entry: None,
            routines: Vec::new(),
            dylinker: None,
            dylibs: Vec::new(),
            uuid: None,
            build: None,
            codesign: None,
            exports: Vec::new(),
            fixups: Vec::new(),
            unknown_cmds: Vec::new(),
        };
        let backing = MappedBacking::new(&map, 0);
        assert_eq!(backing.window(base + 8, 4).unwrap(), &[0x41; 4]);
        assert!(backing.window(base + 63, 2).is_err());
        assert!(backing.window(base + 64, 1).is_err());
        assert!(backing.window(0x1000, 1).is_err());
    }
}
