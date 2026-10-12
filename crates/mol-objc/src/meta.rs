// SPDX-License-Identifier: MIT OR Apache-2.0
//! ObjC metadata parsing: `__objc_classlist` / `__objc_catlist` to
//! class + category records with resolved `+load` IMPs (doc 03).
//!
//! Layouts per `objc-runtime-new.h` (stable 64-bit ABI). All reads
//! go through bounds-checked helpers; hostile bytes yield `Err`,
//! never panic. Supports small (relative-pointer) and big method
//! lists; categories use absolute pointers.

use mol_macho::map::{LoadMap, MapSection, MapSegment};

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
const RO_MIN_SIZE: usize = 40;

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

/// Byte view over one mapped image: file bytes + segment table +
/// slide for VA translation.
struct View<'a> {
    bytes: &'a [u8],
    segments: &'a [MapSegment],
    slide: u64,
}

impl<'a> View<'a> {
    fn new(bytes: &'a [u8], map: &'a LoadMap, slide: u64) -> Self {
        View {
            bytes,
            segments: &map.segments,
            slide,
        }
    }

    /// Slid VA to file offset. Total: unknown VAs yield `None`.
    fn vma_to_off(&self, vma: u64) -> Option<usize> {
        for seg in self.segments {
            let base = seg.vmaddr.checked_add(self.slide)?;
            let rel = vma.wrapping_sub(base);
            if rel < seg.vmsize {
                let off = seg.fileoff.checked_add(rel)?;
                if off < self.bytes.len() as u64 {
                    return Some(off as usize);
                }
            }
        }
        None
    }

    fn u64_at(&self, off: usize) -> Result<u64> {
        let w = self
            .bytes
            .get(off..)
            .and_then(|s| s.get(..8))
            .ok_or(ObjcError::Truncated {
                offset: off,
                len: self.bytes.len(),
            })?;
        Ok(u64::from_le_bytes(w.try_into().unwrap()))
    }

    fn u32_at(&self, off: usize) -> Result<u32> {
        let w = self
            .bytes
            .get(off..)
            .and_then(|s| s.get(..4))
            .ok_or(ObjcError::Truncated {
                offset: off,
                len: self.bytes.len(),
            })?;
        Ok(u32::from_le_bytes(w.try_into().unwrap()))
    }

    fn u64_va(&self, vma: u64) -> Result<u64> {
        let off = self.vma_to_off(vma).ok_or(ObjcError::Malformed {
            offset: vma,
            detail: "pointer outside image segments",
        })?;
        self.u64_at(off)
    }

    /// NUL-terminated string at a slid VA.
    fn cstr_va(&self, vma: u64) -> Result<&'a [u8]> {
        let off = self.vma_to_off(vma).ok_or(ObjcError::Malformed {
            offset: vma,
            detail: "string pointer outside image segments",
        })?;
        let rest = self.bytes.get(off..).ok_or(ObjcError::Truncated {
            offset: off,
            len: self.bytes.len(),
        })?;
        let end = rest.iter().position(|&b| b == 0).ok_or(ObjcError::Malformed {
            offset: vma,
            detail: "unterminated string",
        })?;
        Ok(&rest[..end])
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

/// Scan a method list at a slid VA for `+load`. Returns the IMP +
/// SEL VAs, or `None` when the list carries no `+load`.
fn find_load(view: &View<'_>, list_va: u64) -> Result<Option<LoadMethod>> {
    if list_va == 0 {
        return Ok(None);
    }
    let off = view.vma_to_off(list_va).ok_or(ObjcError::Malformed {
        offset: list_va,
        detail: "method list outside image segments",
    })?;
    let flags = view.u32_at(off)?;
    let count = view.u32_at(off.checked_add(4).ok_or(ObjcError::Malformed {
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
            let eoff = view.vma_to_off(entry_va).ok_or(ObjcError::Malformed {
                offset: entry_va,
                detail: "method entry outside image segments",
            })?;
            let name_off = view.u32_at(eoff)? as i32 as i64;
            let imp_off = view.u32_at(eoff.checked_add(8).ok_or(ObjcError::Malformed {
                offset: entry_va,
                detail: "method entry overflows",
            })?)? as i32 as i64;
            let base = entry_va as i64;
            (
                base.wrapping_add(name_off) as u64,
                base.wrapping_add(imp_off) as u64,
            )
        } else {
            let eoff = view.vma_to_off(entry_va).ok_or(ObjcError::Malformed {
                offset: entry_va,
                detail: "method entry outside image segments",
            })?;
            let name = view.u64_at(eoff)?;
            let imp = view.u64_at(eoff.checked_add(16).ok_or(ObjcError::Malformed {
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

/// Class name via `data -> class_ro -> name`.
fn class_name(view: &View<'_>, class_va: u64) -> Result<String> {
    let coff = view.vma_to_off(class_va).ok_or(ObjcError::Malformed {
        offset: class_va,
        detail: "class outside image segments",
    })?;
    let data = view.u64_at(coff.checked_add(CLASS_DATA_OFFSET).ok_or(
        ObjcError::Malformed {
            offset: class_va,
            detail: "class header overflows",
        },
    )?)? & FAST_DATA_MASK;
    let ro = view.vma_to_off(data).ok_or(ObjcError::Malformed {
        offset: data,
        detail: "class_ro outside image segments",
    })?;
    let name_va = view.u64_at(ro.checked_add(RO_NAME_OFFSET).ok_or(
        ObjcError::Malformed {
            offset: data,
            detail: "class_ro header overflows",
        },
    )?)?;
    let bytes = view.cstr_va(name_va)?;
    String::from_utf8(bytes.to_vec()).map_err(|_| ObjcError::Malformed {
        offset: name_va,
        detail: "class name not UTF-8",
    })
}

/// Own `+load` via the metaclass base methods (`cls->ISA()`).
fn class_load(view: &View<'_>, class_va: u64) -> Result<Option<LoadMethod>> {
    let coff = view.vma_to_off(class_va).ok_or(ObjcError::Malformed {
        offset: class_va,
        detail: "class outside image segments",
    })?;
    let isa = view.u64_at(coff)?;
    if isa == 0 {
        return Err(ObjcError::Malformed {
            offset: class_va,
            detail: "class without metaclass",
        });
    }
    let moff = view.vma_to_off(isa).ok_or(ObjcError::Malformed {
        offset: isa,
        detail: "metaclass outside image segments",
    })?;
    let data = view.u64_at(moff.checked_add(CLASS_DATA_OFFSET).ok_or(
        ObjcError::Malformed {
            offset: isa,
            detail: "metaclass header overflows",
        },
    )?)? & FAST_DATA_MASK;
    let ro = view.vma_to_off(data).ok_or(ObjcError::Malformed {
        offset: data,
        detail: "metaclass_ro outside image segments",
    })?;
    // class_ro must at least cover the baseMethods field.
    if ro.checked_add(RO_MIN_SIZE).is_none_or(|end| end > view.bytes.len()) {
        return Err(ObjcError::Truncated {
            offset: ro,
            len: view.bytes.len(),
        });
    }
    let methods = view.u64_at(ro + RO_METHODS_OFFSET)?;
    find_load(view, methods)
}

fn parse_class(view: &View<'_>, class_va: u64) -> Result<ClassRec> {
    let coff = view.vma_to_off(class_va).ok_or(ObjcError::Malformed {
        offset: class_va,
        detail: "class outside image segments",
    })?;
    if coff.checked_add(CLASS_T_SIZE).is_none_or(|end| end > view.bytes.len()) {
        return Err(ObjcError::Truncated {
            offset: coff,
            len: view.bytes.len(),
        });
    }
    let superclass = view.u64_at(coff + CLASS_SUPER_OFFSET)?;
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

fn parse_category(view: &View<'_>, cat_va: u64) -> Result<CategoryRec> {
    let coff = view.vma_to_off(cat_va).ok_or(ObjcError::Malformed {
        offset: cat_va,
        detail: "category outside image segments",
    })?;
    if coff.checked_add(CATEGORY_T_SIZE).is_none_or(|end| end > view.bytes.len()) {
        return Err(ObjcError::Truncated {
            offset: coff,
            len: view.bytes.len(),
        });
    }
    let name_va = view.u64_at(coff)?;
    let cls = view.u64_at(coff + CATEGORY_CLS_OFFSET)?;
    let methods = view.u64_at(coff + CATEGORY_CLASS_METHODS_OFFSET)?;
    let name_bytes = view.cstr_va(name_va)?;
    let name = String::from_utf8(name_bytes.to_vec()).map_err(|_| ObjcError::Malformed {
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

/// Parse one image's ObjC metadata. Images without ObjC sections
/// parse to empty records (plain C binaries).
pub fn parse_image(bytes: &[u8], map: &LoadMap, slide: u64) -> Result<ObjcImage> {
    let view = View::new(bytes, map, slide);
    let mut out = ObjcImage::default();
    if let Some((addr, size)) = find_section(map, "__objc_classlist") {
        let count = size / 8;
        for i in 0..count {
            let va = addr.checked_add(i.checked_mul(8).ok_or(ObjcError::Malformed {
                offset: addr,
                detail: "classlist index overflows",
            })?).ok_or(ObjcError::Malformed {
                offset: addr,
                detail: "classlist entry overflows",
            })?;
            let class_va = view.u64_va(va)?;
            if class_va == 0 {
                continue;
            }
            out.classes.push(parse_class(&view, class_va)?);
        }
    }
    if let Some((addr, size)) = find_section(map, "__objc_catlist") {
        let count = size / 8;
        for i in 0..count {
            let va = addr.checked_add(i.checked_mul(8).ok_or(ObjcError::Malformed {
                offset: addr,
                detail: "catlist index overflows",
            })?).ok_or(ObjcError::Malformed {
                offset: addr,
                detail: "catlist entry overflows",
            })?;
            let cat_va = view.u64_va(va)?;
            if cat_va == 0 {
                continue;
            }
            out.categories.push(parse_category(&view, cat_va)?);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let img = parse_image(&b, &map, 0).unwrap();
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
        let img = parse_image(&b, &map, 0).unwrap();
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
            parse_image(&b, &map, 0).unwrap_err(),
            ObjcError::Malformed { .. }
        ));
    }

    #[test]
    fn method_cap_is_enforced() {
        let (mut b, map) = tiny(true);
        b[0x204..0x208].copy_from_slice(&0xffff_ffffu32.to_le_bytes());
        assert!(matches!(
            parse_image(&b, &map, 0).unwrap_err(),
            ObjcError::Malformed { .. }
        ));
    }

    #[test]
    fn dangling_class_pointer_is_malformed() {
        let (mut b, map) = tiny(true);
        b[0x1400..0x1408].copy_from_slice(&0x9u64.to_le_bytes());
        assert!(parse_image(&b, &map, 0).is_err());
    }
}
