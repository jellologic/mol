// SPDX-License-Identifier: MIT OR Apache-2.0
//! P1.3.1 / P1.3.V1: `+load` ordering fixtures. Byte-level images
//! (TEXT methnames + small relative method lists, DATA class pairs
//! + categories) parsed by the real `parse_image`, scheduled and
//! drained by the real `prepare` / `drain`; sequences asserted
//! exactly.

use std::collections::BTreeMap;

use mol_macho::header::{CPU_TYPE_ARM64, MH_EXECUTE};
use mol_macho::map::{LoadMap, MapSection, MapSegment};
use mol_objc::load::{drain, prepare};
use mol_objc::meta::parse_image;

const FILE_LEN: usize = 0x2000;
const METHNAME_OFF: usize = 0x100;
const CLASSNAME_OFF: usize = 0x180;
const CONST_OFF: usize = 0x300;
const DATA_OFF: usize = 0x1000;
const CLASSLIST_OFF: usize = 0x1700;
const CATLIST_OFF: usize = 0x1780;

/// Byte-level fixture image builder.
struct Img {
    bytes: Vec<u8>,
    text: u64,
    data: u64,
    load_va: u64,
    imp_next: u64,
    const_ptr: usize,
    name_ptr: usize,
    data_ptr: usize,
    classes: Vec<u64>,
    cats: Vec<u64>,
    labels: BTreeMap<u64, String>,
    names: BTreeMap<String, u64>,
}

impl Img {
    fn new(text: u64) -> Self {
        let mut bytes = vec![0u8; FILE_LEN];
        bytes[METHNAME_OFF..METHNAME_OFF + 5].copy_from_slice(b"load\0");
        bytes[METHNAME_OFF + 5..METHNAME_OFF + 9].copy_from_slice(b"v@:\0");
        Img {
            bytes,
            text,
            data: text + 0x1000,
            load_va: text + METHNAME_OFF as u64,
            imp_next: 0,
            const_ptr: CONST_OFF,
            name_ptr: CLASSNAME_OFF,
            data_ptr: DATA_OFF,
            classes: Vec::new(),
            cats: Vec::new(),
            labels: BTreeMap::new(),
            names: BTreeMap::new(),
        }
    }

    fn w64(&mut self, off: usize, v: u64) {
        self.bytes[off..off + 8].copy_from_slice(&v.to_le_bytes());
    }

    fn w32(&mut self, off: usize, v: u32) {
        self.bytes[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }

    fn alloc_const(&mut self, n: usize) -> usize {
        let off = self.const_ptr;
        self.const_ptr += n;
        assert!(self.const_ptr < 0x800, "const region overflow");
        off
    }

    fn alloc_name(&mut self, s: &str) -> u64 {
        let off = self.name_ptr;
        self.bytes[off..off + s.len()].copy_from_slice(s.as_bytes());
        self.bytes[off + s.len()] = 0;
        self.name_ptr += s.len() + 1;
        assert!(self.name_ptr < CONST_OFF, "classname overflow");
        self.text + off as u64
    }

    fn alloc_data(&mut self, n: usize) -> usize {
        let off = self.data_ptr;
        self.data_ptr += n;
        assert!(self.data_ptr < CLASSLIST_OFF, "data region overflow");
        off
    }

    fn data_va(&self, off: usize) -> u64 {
        self.data + (off - DATA_OFF) as u64
    }

    /// Small (relative) one-method `+load` list; returns its VA.
    fn load_list(&mut self, label: &str) -> u64 {
        let imp_va = self.text + 0x10 + self.imp_next * 0x10;
        self.imp_next += 1;
        self.labels.insert(imp_va, label.to_string());
        let off = self.alloc_const(8 + 12);
        let va = self.text + off as u64;
        self.w32(off, 0x8000_000c);
        self.w32(off + 4, 1);
        let entry_va = va + 8;
        let rel = |target: u64| -> u32 {
            i32::try_from(target as i64 - entry_va as i64).unwrap() as u32
        };
        self.w32(off + 8, rel(self.load_va));
        self.w32(off + 12, rel(self.load_va + 5));
        self.w32(off + 16, rel(imp_va));
        va
    }

    /// Class + metaclass pair; returns the class VA.
    fn add_class(&mut self, name: &str, superclass: u64, load: bool) -> u64 {
        let label = name.to_string();
        let methods = if load {
            self.load_list(&label)
        } else {
            0
        };
        let name_va = self.alloc_name(name);
        // class_ro (no instance methods matter for +load).
        let ro = self.alloc_data(72);
        self.w64(ro + 24, name_va);
        // metaclass_ro with the +load list.
        let mro = self.alloc_data(72);
        self.w64(mro + 24, name_va);
        self.w64(mro + 32, methods);
        // metaclass (isa self; parser reads data only).
        let moff = self.alloc_data(40);
        let mva = self.data_va(moff);
        self.w64(moff, mva);
        self.w64(moff + 32, self.data_va(mro));
        // class.
        let coff = self.alloc_data(40);
        let cva = self.data_va(coff);
        self.w64(coff, mva);
        self.w64(coff + 8, superclass);
        self.w64(coff + 32, self.data_va(ro));
        self.classes.push(cva);
        self.names.insert(name.to_string(), cva);
        cva
    }

    /// Rewrite a class's superclass pointer (for cycles).
    fn patch_super(&mut self, class_va: u64, super_va: u64) {
        let off = DATA_OFF + (class_va - self.data) as usize;
        self.w64(off + 8, super_va);
    }

    /// Category; returns its VA.
    fn add_category(&mut self, name: &str, cls: u64, load: bool) -> u64 {
        let methods = if load {
            self.load_list(name)
        } else {
            0
        };
        let name_va = self.alloc_name(name);
        let coff = self.alloc_data(56);
        let cva = self.data_va(coff);
        self.w64(coff, name_va);
        self.w64(coff + 8, cls);
        self.w64(coff + 24, methods);
        self.cats.push(cva);
        cva
    }

    fn section(name: &str, addr: u64, size: u64, offset: u32) -> MapSection {
        MapSection {
            sectname: name.to_string(),
            addr,
            size,
            offset,
            flags: 0,
        }
    }

    /// Lay out classlist/catlist in declaration order and build the map.
    fn finish(mut self, class_order: &[usize], cat_order: &[usize]) -> Fixture {
        for (i, c) in class_order.iter().enumerate() {
            let va = self.classes[*c];
            self.w64(CLASSLIST_OFF + i * 8, va);
        }
        for (i, c) in cat_order.iter().enumerate() {
            let va = self.cats[*c];
            self.w64(CATLIST_OFF + i * 8, va);
        }
        let text_seg = MapSegment {
            segname: "__TEXT".to_string(),
            vmaddr: self.text,
            vmsize: 0x1000,
            fileoff: 0,
            filesize: 0x1000,
            maxprot: 5,
            initprot: 5,
            flags: 0,
            sections: vec![
                Self::section("__text", self.text, 0x100, 0),
                Self::section(
                    "__objc_methname",
                    self.text + METHNAME_OFF as u64,
                    0x80,
                    METHNAME_OFF as u32,
                ),
                Self::section(
                    "__objc_classname",
                    self.text + CLASSNAME_OFF as u64,
                    0x180,
                    CLASSNAME_OFF as u32,
                ),
                Self::section(
                    "__objc_const",
                    self.text + CONST_OFF as u64,
                    0x500,
                    CONST_OFF as u32,
                ),
            ],
        };
        let data_seg = MapSegment {
            segname: "__DATA".to_string(),
            vmaddr: self.data,
            vmsize: 0x1000,
            fileoff: 0x1000,
            filesize: 0x1000,
            maxprot: 3,
            initprot: 3,
            flags: 0,
            sections: vec![
                Self::section("__objc_data", self.data, 0x700, 0x1000),
                Self::section(
                    "__objc_classlist",
                    self.data + 0x700,
                    class_order.len() as u64 * 8,
                    CLASSLIST_OFF as u32,
                ),
                Self::section(
                    "__objc_catlist",
                    self.data + 0x780,
                    cat_order.len() as u64 * 8,
                    CATLIST_OFF as u32,
                ),
            ],
        };
        let map = LoadMap {
            cputype: CPU_TYPE_ARM64,
            cpusubtype: 0,
            filetype: MH_EXECUTE,
            flags: 0,
            slide: 0,
            segments: vec![text_seg, data_seg],
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
        Fixture {
            bytes: self.bytes,
            map,
            labels: self.labels,
            names: self.names,
            load_va: self.load_va,
        }
    }
}

struct Fixture {
    bytes: Vec<u8>,
    map: LoadMap,
    labels: BTreeMap<u64, String>,
    names: BTreeMap<String, u64>,
    load_va: u64,
}

/// Drain `images` through parse/prepare/drain; return the label
/// sequence plus raw `(imp, cls, sel)` triples.
fn run(images: &[&Fixture]) -> (Vec<String>, Vec<(u64, u64, u64)>) {
    let parsed: Vec<mol_objc::meta::ObjcImage> = images
        .iter()
        .map(|f| parse_image(&f.bytes, &f.map).unwrap())
        .collect();
    let mut st = prepare(&parsed);
    let mut seq = Vec::new();
    let mut raw = Vec::new();
    drain(&mut st, &mut |imp, cls, sel| {
        let label = images
            .iter()
            .find_map(|f| f.labels.get(&imp))
            .unwrap_or_else(|| panic!("unknown imp {imp:#x}"))
            .clone();
        seq.push(label);
        raw.push((imp, cls, sel));
    });
    (seq, raw)
}

#[test]
fn deep_hierarchy_cross_image_order() {
    // Image A (main): 3-level hierarchy + sibling, scrambled
    // declaration order, categories incl. load-less.
    let mut a = Img::new(0x1000_0000);
    let root = a.add_class("Root", 0, true);
    let mid = a.add_class("Mid", root, true);
    let leaf = a.add_class("Leaf", mid, false);
    a.add_class("Other", root, true);
    a.add_category("CatMid", mid, true);
    a.add_category("CatLeaf", leaf, true);
    a.add_category("DeadCat", mid, false);
    // Declaration: Other, Leaf, Mid, Root (scrambled).
    let a = a.finish(&[3, 2, 1, 0], &[0, 1, 2]);
    // Image B (dylib): cross-image subclass + cross-image
    // category + dangling-parent category.
    let mut b = Img::new(0x2000_0000);
    b.add_class("SubB", leaf, true);
    b.add_category("CatRootB", root, true);
    b.add_category("Dangling", 0xdead_beef, true);
    let b = b.finish(&[0], &[0, 1]);

    let (seq, raw) = run(&[&a, &b]);
    // Siblings follow declaration (Other before Mid), ancestors
    // precede descendants across images, categories run last in
    // image order; load-less and dangling entries never run.
    assert_eq!(
        seq,
        vec!["Root", "Other", "Mid", "SubB", "CatMid", "CatLeaf", "CatRootB"]
    );
    // (cls, sel) arguments are exact: cls is the class (or the
    // category parent), sel points at a "load" methname.
    let expect_cls = [
        a.names["Root"],
        a.names["Other"],
        a.names["Mid"],
        b.names["SubB"],
        a.names["Mid"],
        a.names["Leaf"],
        a.names["Root"],
    ];
    for (i, (_, cls, sel)) in raw.iter().enumerate() {
        assert_eq!(*cls, expect_cls[i], "cls for {}", seq[i]);
        assert!(
            *sel == a.load_va || *sel == b.load_va,
            "sel for {}",
            seq[i]
        );
    }
}

#[test]
fn natural_order_preserved() {
    let mut a = Img::new(0x1000_0000);
    let root = a.add_class("Root", 0, true);
    a.add_class("Mid", root, true);
    a.add_class("Other", root, true);
    let a = a.finish(&[0, 1, 2], &[]);
    let (seq, _) = run(&[&a]);
    assert_eq!(seq, vec!["Root", "Mid", "Other"]);
}

#[test]
fn superclass_cycle_breaks_cleanly() {
    // A <-> B: no hang; the cycle breaks at first revisit, so
    // B (reached via A's super) drains before A.
    let mut a = Img::new(0x1000_0000);
    let ava = a.add_class("A", 0, true);
    let bva = a.add_class("B", ava, true);
    a.patch_super(ava, bva);
    let a = a.finish(&[0, 1], &[]);
    let (seq, _) = run(&[&a]);
    assert_eq!(seq, vec!["B", "A"]);
}

#[test]
fn dangling_superclass_still_schedules() {
    // Unresolvable super: the class still drains (its ancestors
    // are just unknown). Documents the choice; hostile-safe
    // either way since no pointer is dereferenced.
    let mut a = Img::new(0x1000_0000);
    a.add_class("Lone", 0xdead_beef, true);
    let a = a.finish(&[0], &[]);
    let (seq, _) = run(&[&a]);
    assert_eq!(seq, vec!["Lone"]);
}

#[test]
fn plain_image_drains_nothing() {
    // Empty classlist/catlist: empty records, no calls.
    let a = Img::new(0x1000_0000).finish(&[], &[]);
    let (seq, _) = run(&[&a]);
    assert!(seq.is_empty());
}
