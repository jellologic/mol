// SPDX-License-Identifier: MIT OR Apache-2.0
//! `applyFixups` planning: turn every image's fixups into ordered
//! `(address, size, value)` writes plus symbol resolution (P1.1.2).
//!
//! Pure: reads original pointer values from the image bytes for
//! legacy rebases, never writes memory. Pointer authentication is
//! stripped per the matrix-#1 policy (P1.1.5); each stripped fixup is
//! counted in the run record.
use mol_macho::map::FixupOp;
use mol_macho::opcodes::{
    BIND_TYPE_POINTER, BIND_TYPE_TEXT_ABSOLUTE32, BIND_TYPE_TEXT_PCREL32, REBASE_TYPE_POINTER,
    REBASE_TYPE_TEXT_ABSOLUTE32, REBASE_TYPE_TEXT_PCREL32,
};
use mol_macho::trie::EXPORT_SYMBOL_FLAGS_KIND_MASK;

use crate::{LinkError, LinkedSet, OrdTarget, Result};

/// Fixup-planning inputs.
pub struct FixupInputs {
    /// Whether the host CPU signs/verifies arm64e pointers. Recorded
    /// for the key-compat audit; auth is stripped either way (P1.1.5).
    pub pac_available: bool,
}

/// One planned memory write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixupApply {
    pub image: usize,
    /// Slid write address.
    pub addr: u64,
    /// Write width in bytes (4 or 8).
    pub size: u8,
    pub value: u64,
    /// True when an arm64e authenticated fixup was stripped.
    pub stripped_pac: bool,
    pub desc: String,
}

/// Plan every fixup for the linked set in load order.
pub fn plan_fixups(set: &LinkedSet, inp: &FixupInputs) -> Result<Vec<FixupApply>> {
    let mut out = Vec::new();
    for (idx, img) in set.images.iter().enumerate() {
        for f in &img.map.fixups {
            let loc = f.vmaddr.wrapping_add(img.slide);
            match &f.op {
                FixupOp::ChainRebase { resolved, auth, .. } => {
                    out.push(FixupApply {
                        image: idx,
                        addr: loc,
                        size: 8,
                        value: resolved.wrapping_add(img.slide),
                        stripped_pac: auth.is_some(),
                        desc: format!("rebase {} -> {:#x}", img.path, resolved),
                    });
                }
                FixupOp::ChainBind {
                    lib,
                    symbol,
                    addend,
                    weak,
                    auth,
                    ..
                } => {
                    let value = resolve_bind(set, idx, *lib, symbol, *addend, *weak)?;
                    out.push(FixupApply {
                        image: idx,
                        addr: loc,
                        size: 8,
                        value,
                        stripped_pac: auth.is_some(),
                        desc: format!("bind {symbol} from {}", img.path),
                    });
                }
                FixupOp::InfoRebase { kind } => {
                    if let Some(a) = plan_legacy_rebase(set, idx, f.vmaddr, *kind)? {
                        out.push(a);
                    }
                }
                FixupOp::InfoBind {
                    kind,
                    lib,
                    symbol,
                    addend,
                    weak,
                    ..
                } => {
                    let value = resolve_bind(set, idx, *lib, symbol, *addend, *weak)?;
                    let (size, val) = match *kind {
                        BIND_TYPE_POINTER => (8, value),
                        BIND_TYPE_TEXT_ABSOLUTE32 => (4, value & 0xffff_ffff),
                        BIND_TYPE_TEXT_PCREL32 => {
                            (4, value.wrapping_sub(loc.wrapping_add(4)) & 0xffff_ffff)
                        }
                        k => {
                            return Err(LinkError::BadFixup {
                                needed_by: img.path.clone(),
                                detail: format!("{symbol} (bad bind kind {k})"),
                            })
                        }
                    };
                    out.push(FixupApply {
                        image: idx,
                        addr: loc,
                        size,
                        value: val,
                        stripped_pac: false,
                        desc: format!("legacy bind {symbol} from {}", img.path),
                    });
                }
            }
        }
    }
    let _ = inp.pac_available;
    Ok(out)
}

/// Resolve a bind to its slid absolute target address. Missing weak
/// symbols (or missing weak libraries) resolve to zero.
fn resolve_bind(
    set: &LinkedSet,
    from: usize,
    lib: i64,
    symbol: &str,
    addend: i64,
    weak: bool,
) -> Result<u64> {
    let target = crate::resolve_ordinal(set, from, lib)?;
    let found: Option<(usize, u64, u64)> = match target {
        OrdTarget::Image(i) => find_export(set, i, symbol).map(|(a, f)| (i, a, f)),
        OrdTarget::Flat => {
            (0..set.images.len()).find_map(|i| find_export(set, i, symbol).map(|(a, f)| (i, a, f)))
        }
        OrdTarget::MissingWeak => None,
    };
    match found {
        Some((i, addr, flags)) => {
            // Absolute-kind exports are already absolute; regular
            // (and TLV-descriptor) addresses are image-base-relative.
            let base = if flags & EXPORT_SYMBOL_FLAGS_KIND_MASK
                == mol_macho::trie::EXPORT_SYMBOL_FLAGS_KIND_ABSOLUTE
            {
                0
            } else {
                set.base(i)
            };
            Ok(base
                .wrapping_add(addr)
                .wrapping_add(set.images[i].slide)
                .wrapping_add(addend as u64))
        }
        None if weak => Ok(0),
        None => Err(LinkError::SymbolNotFound {
            needed_by: set.images[from].path.clone(),
            symbol: symbol.to_string(),
        }),
    }
}

fn find_export(set: &LinkedSet, image: usize, symbol: &str) -> Option<(u64, u64)> {
    set.images[image]
        .map
        .exports
        .iter()
        .find(|e| e.name == symbol)
        .map(|e| (e.address, e.flags))
}

/// Plan one legacy rebase by reading the original pointer value from
/// the image bytes. `TEXT_PCREL32` is slide-invariant and emits no
/// write. Returns `None` for the no-op case.
fn plan_legacy_rebase(
    set: &LinkedSet,
    idx: usize,
    vmaddr: u64,
    kind: u8,
) -> Result<Option<FixupApply>> {
    let img = &set.images[idx];
    let loc = vmaddr.wrapping_add(img.slide);
    // Translate preferred vmaddr -> file offset.
    let seg = img
        .map
        .segments
        .iter()
        .find(|s| vmaddr >= s.vmaddr && vmaddr < s.vmaddr + s.vmsize);
    let orig: u64 = match (seg, kind) {
        (Some(s), REBASE_TYPE_POINTER) => {
            let at = (s.fileoff + (vmaddr - s.vmaddr)) as usize;
            read_u64(img.image.bytes, at)?
        }
        (Some(s), REBASE_TYPE_TEXT_ABSOLUTE32) => {
            let at = (s.fileoff + (vmaddr - s.vmaddr)) as usize;
            read_u32(img.image.bytes, at)? as u64
        }
        (_, REBASE_TYPE_TEXT_PCREL32) => return Ok(None),
        (None, _) => {
            return Err(LinkError::BadFixup {
                needed_by: img.path.clone(),
                detail: format!("rebase outside segments at {vmaddr:#x}"),
            })
        }
        (_, k) => {
            return Err(LinkError::BadFixup {
                needed_by: img.path.clone(),
                detail: format!("bad rebase kind {k}"),
            })
        }
    };
    let (size, value) = match kind {
        REBASE_TYPE_POINTER => (8, orig.wrapping_add(img.slide)),
        REBASE_TYPE_TEXT_ABSOLUTE32 => (4, orig.wrapping_add(img.slide) & 0xffff_ffff),
        _ => unreachable!(),
    };
    Ok(Some(FixupApply {
        image: idx,
        addr: loc,
        size,
        value,
        stripped_pac: false,
        desc: format!("legacy rebase {} at {vmaddr:#x}", img.path),
    }))
}

fn read_u64(bytes: &[u8], at: usize) -> Result<u64> {
    bytes
        .get(at..at + 8)
        .and_then(|w| w.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or(LinkError::BadFixup {
            needed_by: "<bytes>".to_string(),
            detail: format!("rebase read out of range at {at:#x}"),
        })
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32> {
    bytes
        .get(at..at + 4)
        .and_then(|w| w.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or(LinkError::BadFixup {
            needed_by: "<bytes>".to_string(),
            detail: format!("rebase read out of range at {at:#x}"),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load::{link, FixedSlide, LinkRequest, MapStore};
    use crate::policy::SearchEnv;
    use crate::testimg::{self, ImgSpec, DATA_VMADDR, TEXT_VMADDR};
    use crate::LinkError;
    use mol_macho::header::{CPU_TYPE_ARM64, MH_DYLIB, MH_EXECUTE};
    use mol_macho::load::LC_LOAD_DYLIB;

    fn req(main: &str) -> LinkRequest {
        LinkRequest {
            main_path: main.to_string(),
            cputype: CPU_TYPE_ARM64,
            cpusubtype: 0,
            search: SearchEnv {
                executable_path: main.to_string(),
                ..SearchEnv::default()
            },
        }
    }

    /// main binds `_foo` (lib 1) at `__DATA+0x10` and rebases
    /// `__DATA+0x20` to `__DATA`; libB exports `_foo` at `0x200`.
    fn two_image_set() -> (Vec<u8>, Vec<u8>) {
        let main = testimg::build(&ImgSpec {
            filetype: MH_EXECUTE,
            dylibs: vec![(LC_LOAD_DYLIB, "/libB.dylib".into())],
            imports: vec![(1, false, "_foo".into(), 0)],
            chain_binds: vec![(0x10, 0, 0)],
            chain_rebases: vec![(0x20, DATA_VMADDR)],
            ..ImgSpec::default()
        });
        let libb = testimg::build(&ImgSpec {
            filetype: MH_DYLIB,
            id: Some("/libB.dylib".into()),
            entryoff: None,
            exports: vec![("_foo".into(), 0x200)],
            ..ImgSpec::default()
        });
        (main, libb)
    }

    fn linked<'a>(
        main: &'a [u8],
        libb: &'a [u8],
        slide: u64,
    ) -> (MapStore<'a>, crate::LinkedSet<'a>) {
        let mut store = MapStore::new();
        store.insert("/main", main);
        store.insert("/libB.dylib", libb);
        let mut slides = FixedSlide(slide);
        let set = link(&req("/main"), &mut store, &mut slides).unwrap();
        (store, set)
    }

    #[test]
    fn chain_bind_resolves_to_slid_export() {
        let (main, libb) = two_image_set();
        let (_store, set) = linked(&main, &libb, 0x1000);
        let plan = plan_fixups(
            &set,
            &FixupInputs {
                pac_available: false,
            },
        )
        .unwrap();
        assert_eq!(plan.len(), 2);
        let bind = plan
            .iter()
            .find(|a| a.addr == DATA_VMADDR + 0x10 + 0x1000)
            .unwrap();
        assert_eq!(bind.size, 8);
        assert_eq!(bind.value, TEXT_VMADDR + 0x200 + 0x1000);
        assert!(!bind.stripped_pac);
        let rebase = plan
            .iter()
            .find(|a| a.addr == DATA_VMADDR + 0x20 + 0x1000)
            .unwrap();
        assert_eq!(rebase.value, DATA_VMADDR + 0x1000);
    }

    #[test]
    fn missing_symbol_errors_unless_weak() {
        let main = testimg::build(&ImgSpec {
            filetype: MH_EXECUTE,
            dylibs: vec![(LC_LOAD_DYLIB, "/libB.dylib".into())],
            imports: vec![(1, false, "_nope".into(), 0)],
            chain_binds: vec![(0x10, 0, 0)],
            ..ImgSpec::default()
        });
        let libb = testimg::build(&ImgSpec {
            filetype: MH_DYLIB,
            id: Some("/libB.dylib".into()),
            entryoff: None,
            ..ImgSpec::default()
        });
        let (_store, set) = linked(&main, &libb, 0);
        let err = plan_fixups(
            &set,
            &FixupInputs {
                pac_available: false,
            },
        )
        .unwrap_err();
        assert_eq!(
            err,
            LinkError::SymbolNotFound {
                needed_by: "/main".to_string(),
                symbol: "_nope".to_string(),
            }
        );

        // Same shape, but the import is weak: binds to zero.
        let main = testimg::build(&ImgSpec {
            filetype: MH_EXECUTE,
            dylibs: vec![(LC_LOAD_DYLIB, "/libB.dylib".into())],
            imports: vec![(1, true, "_nope".into(), 0)],
            chain_binds: vec![(0x10, 0, 0)],
            ..ImgSpec::default()
        });
        let (_store, set) = linked(&main, &libb, 0);
        let plan = plan_fixups(
            &set,
            &FixupInputs {
                pac_available: false,
            },
        )
        .unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].value, 0);
    }

    #[test]
    fn self_main_and_flat_ordinals() {
        // main exports `_me`, binds it via SELF (0); libB binds
        // `_me` via MAIN_EXECUTABLE (-1) and `_any` via FLAT (-2).
        let main = testimg::build(&ImgSpec {
            filetype: MH_EXECUTE,
            dylibs: vec![(LC_LOAD_DYLIB, "/libB.dylib".into())],
            exports: vec![("_me".into(), 0x300), ("_any".into(), 0x400)],
            imports: vec![(0, false, "_me".into(), 0)],
            chain_binds: vec![(0x10, 0, 0)],
            ..ImgSpec::default()
        });
        let libb = testimg::build(&ImgSpec {
            filetype: MH_DYLIB,
            id: Some("/libB.dylib".into()),
            entryoff: None,
            imports: vec![(-1, false, "_me".into(), 0), (-2, false, "_any".into(), 0)],
            chain_binds: vec![(0x10, 0, 0), (0x18, 1, 4)],
            ..ImgSpec::default()
        });
        let (_store, set) = linked(&main, &libb, 0);
        let plan = plan_fixups(
            &set,
            &FixupInputs {
                pac_available: false,
            },
        )
        .unwrap();
        assert_eq!(plan.len(), 3);
        let me_self = plan.iter().find(|a| a.image == 0).unwrap();
        assert_eq!(me_self.value, TEXT_VMADDR + 0x300);
        let mut lib_vals: Vec<u64> = plan
            .iter()
            .filter(|a| a.image == 1)
            .map(|a| a.value)
            .collect();
        lib_vals.sort();
        assert_eq!(lib_vals, vec![TEXT_VMADDR + 0x300, TEXT_VMADDR + 0x400 + 4]);
    }

    #[test]
    fn import_and_entry_addends_sum() {
        // IMPORT_ADDEND format: total = table addend + entry addend.
        let main = testimg::build(&ImgSpec {
            filetype: MH_EXECUTE,
            dylibs: vec![(LC_LOAD_DYLIB, "/libB.dylib".into())],
            imports: vec![(1, false, "_foo".into(), 0x100)],
            imports_format: 2,
            chain_binds: vec![(0x10, 0, 5)],
            ..ImgSpec::default()
        });
        let libb = testimg::build(&ImgSpec {
            filetype: MH_DYLIB,
            id: Some("/libB.dylib".into()),
            entryoff: None,
            exports: vec![("_foo".into(), 0x200)],
            ..ImgSpec::default()
        });
        let (_store, set) = linked(&main, &libb, 0);
        let plan = plan_fixups(
            &set,
            &FixupInputs {
                pac_available: false,
            },
        )
        .unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].value, TEXT_VMADDR + 0x200 + 0x105);
    }

    #[test]
    fn arm64e_auth_binds_strip_with_plain_value() {
        // P1.1.5: authenticated binds resolve to the plain target and
        // are flagged stripped (matrix-#1 policy).
        let main = testimg::build(&ImgSpec {
            filetype: MH_EXECUTE,
            dylibs: vec![(LC_LOAD_DYLIB, "/libB.dylib".into())],
            imports: vec![(1, false, "_foo".into(), 0)],
            chain_format: 9,
            chain_auth_binds: vec![(0x10, 0, 0x1234, true, 2)],
            ..ImgSpec::default()
        });
        let libb = testimg::build(&ImgSpec {
            filetype: MH_DYLIB,
            id: Some("/libB.dylib".into()),
            entryoff: None,
            exports: vec![("_foo".into(), 0x200)],
            ..ImgSpec::default()
        });
        for pac in [false, true] {
            let (_store, set) = linked(&main, &libb, 0x100);
            let plan = plan_fixups(&set, &FixupInputs { pac_available: pac }).unwrap();
            assert_eq!(plan.len(), 1);
            assert_eq!(plan[0].value, TEXT_VMADDR + 0x200 + 0x100);
            assert!(plan[0].stripped_pac);
        }
    }

    #[test]
    fn bad_ordinal_is_an_error() {
        let main = testimg::build(&ImgSpec {
            filetype: MH_EXECUTE,
            dylibs: vec![(LC_LOAD_DYLIB, "/libB.dylib".into())],
            imports: vec![(9, false, "_foo".into(), 0)],
            chain_binds: vec![(0x10, 0, 0)],
            ..ImgSpec::default()
        });
        let libb = testimg::build(&ImgSpec {
            filetype: MH_DYLIB,
            id: Some("/libB.dylib".into()),
            entryoff: None,
            exports: vec![("_foo".into(), 0x200)],
            ..ImgSpec::default()
        });
        let (_store, set) = linked(&main, &libb, 0);
        let err = plan_fixups(
            &set,
            &FixupInputs {
                pac_available: false,
            },
        )
        .unwrap_err();
        assert_eq!(
            err,
            LinkError::BadOrdinal {
                needed_by: "/main".to_string(),
                ordinal: 9,
            }
        );
    }
}
