// SPDX-License-Identifier: MIT OR Apache-2.0
//! `runAllInitializersForMain`: depth-first post-order over the
//! dependency graph, dependencies before dependents, main last
//! (P1.1.2). `LC_ROUTINES_64` entries land here; ObjC `+load` and
//! Swift initializers plug in as further kinds in P1.3.

use crate::LinkedSet;

/// Initializer kinds in per-image run order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitKind {
    Routines,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Initializer {
    pub image: usize,
    /// Slid call address.
    pub address: u64,
    pub module: u64,
    pub kind: InitKind,
}

/// Order all initializers: post-order DFS from the main image over
/// ordinal edges (cycles broken at first revisit, weak misses
/// skipped), `LC_ROUTINES_64` entries in file order per image.
pub fn order_initializers(set: &LinkedSet) -> Vec<Initializer> {
    let mut order = Vec::new();
    let mut visited = vec![false; set.images.len()];
    visit(set, 0, &mut visited, &mut order);
    let mut out = Vec::new();
    for idx in order {
        let img = &set.images[idx];
        for (addr, module) in &img.map.routines {
            out.push(Initializer {
                image: idx,
                address: addr.wrapping_add(img.slide),
                module: *module,
                kind: InitKind::Routines,
            });
        }
    }
    out
}

fn visit(set: &LinkedSet, idx: usize, visited: &mut [bool], order: &mut Vec<usize>) {
    if visited[idx] {
        return;
    }
    visited[idx] = true;
    for target in set.images[idx].ordinals.iter().flatten() {
        visit(set, *target, visited, order);
    }
    order.push(idx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load::{link, FixedSlide, LinkRequest, MapStore};
    use crate::policy::SearchEnv;
    use crate::testimg::{self, ImgSpec};
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

    #[test]
    fn deps_first_main_last_with_slid_addresses() {
        // main -> B -> C; each carries one LC_ROUTINES_64.
        let main = testimg::build(&ImgSpec {
            filetype: MH_EXECUTE,
            dylibs: vec![(LC_LOAD_DYLIB, "/b".into())],
            routines: vec![(0x5000, 1)],
            ..ImgSpec::default()
        });
        let b = testimg::build(&ImgSpec {
            filetype: MH_DYLIB,
            id: Some("/b".into()),
            entryoff: None,
            dylibs: vec![(LC_LOAD_DYLIB, "/c".into())],
            routines: vec![(0x6000, 2)],
            ..ImgSpec::default()
        });
        let c = testimg::build(&ImgSpec {
            filetype: MH_DYLIB,
            id: Some("/c".into()),
            entryoff: None,
            routines: vec![(0x7000, 3)],
            ..ImgSpec::default()
        });
        let mut store = MapStore::new();
        store.insert("/main", &main);
        store.insert("/b", &b);
        store.insert("/c", &c);
        let mut slides = FixedSlide(0x100);
        let set = link(&req("/main"), &mut store, &mut slides).unwrap();
        let inits = order_initializers(&set);
        let addrs: Vec<(usize, u64)> = inits.iter().map(|i| (i.image, i.address)).collect();
        // Load order is main, B, C; init order is C, B, main.
        assert_eq!(addrs, vec![(2, 0x7100), (1, 0x6100), (0, 0x5100)]);
        assert!(inits.iter().all(|i| i.kind == InitKind::Routines));
    }

    #[test]
    fn cycles_terminate_without_duplicates() {
        let a = testimg::build(&ImgSpec {
            filetype: MH_EXECUTE,
            dylibs: vec![(LC_LOAD_DYLIB, "/b".into())],
            routines: vec![(0x5000, 0)],
            ..ImgSpec::default()
        });
        let b = testimg::build(&ImgSpec {
            filetype: MH_DYLIB,
            id: Some("/b".into()),
            entryoff: None,
            dylibs: vec![(LC_LOAD_DYLIB, "/main".into())],
            routines: vec![(0x6000, 0)],
            ..ImgSpec::default()
        });
        let mut store = MapStore::new();
        store.insert("/main", &a);
        store.insert("/b", &b);
        let mut slides = FixedSlide(0);
        let set = link(&req("/main"), &mut store, &mut slides).unwrap();
        let inits = order_initializers(&set);
        assert_eq!(inits.len(), 2);
    }
}
