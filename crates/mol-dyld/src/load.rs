// SPDX-License-Identifier: MIT OR Apache-2.0
//! Dependency-graph linking: `loadDependents` breadth-first from the
//! main executable (P1.1.2).
//!
//! Pure over an [`ImageStore`]: the loader implements the store
//! (prefix files + cache), the engine only orders and validates.
use std::collections::VecDeque;

use mol_macho::load::{LC_ID_DYLIB, LC_LOAD_WEAK_DYLIB};

use crate::policy::{resolve_dylib, SearchEnv};
use crate::{LinkError, LinkedImage, LinkedSet, Result};

/// Byte provider for link-time image reads.
pub trait ImageStore<'a> {
    /// Return the full file bytes for a resolved guest path, or
    /// `None` when the path does not exist in the store. `&mut`
    /// because owning stores (files) may populate a cache.
    fn bytes(&mut self, path: &str) -> Option<&'a [u8]>;
}

/// In-memory store for tests and fixed sysroots.
#[derive(Default)]
pub struct MapStore<'a> {
    files: Vec<(String, &'a [u8])>,
}

impl<'a> MapStore<'a> {
    pub fn new() -> Self {
        MapStore { files: Vec::new() }
    }

    pub fn insert(&mut self, path: &str, bytes: &'a [u8]) {
        self.files.push((path.to_string(), bytes));
    }
}

impl<'a> ImageStore<'a> for MapStore<'a> {
    fn bytes(&mut self, path: &str) -> Option<&'a [u8]> {
        self.files.iter().find(|(p, _)| p == path).map(|(_, b)| *b)
    }
}

/// How ASLR slides are assigned per image.
pub trait SlidePolicy {
    /// Slide for an image. Non-PIE main executables must get 0.
    fn slide_for(&mut self, path: &str, is_main: bool, is_pie: bool) -> u64;
}

/// Deterministic slides for tests and `--no-aslr` runs.
pub struct FixedSlide(pub u64);

impl SlidePolicy for FixedSlide {
    fn slide_for(&mut self, _path: &str, is_main: bool, is_pie: bool) -> u64 {
        if is_main && !is_pie {
            0
        } else {
            self.0
        }
    }
}

/// Link request: main path, target arch, and search environment.
pub struct LinkRequest {
    pub main_path: String,
    pub cputype: u32,
    pub cpusubtype: u32,
    pub search: SearchEnv,
}

/// Breadth-first `loadDependents` from the main image. Returns images
/// in load order (main first) with per-image ordinal tables.
pub fn link<'a>(
    req: &LinkRequest,
    store: &mut dyn ImageStore<'a>,
    slides: &mut dyn SlidePolicy,
) -> Result<LinkedSet<'a>> {
    let mut set = LinkedSet { images: Vec::new() };
    let mut queue = VecDeque::from([req.main_path.clone()]);
    // Canonical loaded path -> image index.
    let mut loaded: Vec<(String, usize)> = Vec::new();
    // (dependent image, raw dylib name) in discovery order; the
    // ordinal table is rebuilt per image from this list.
    let mut edges: Vec<(usize, String)> = Vec::new();

    while let Some(path) = queue.pop_front() {
        if loaded.iter().any(|(p, _)| p == &path) {
            continue;
        }
        let bytes = store
            .bytes(&path)
            .ok_or_else(|| LinkError::MissingFile { path: path.clone() })?;
        let file = mol_macho::parse(bytes)?;
        let image = file.select(req.cputype, req.cpusubtype)?;
        let is_main = set.images.is_empty();
        let is_pie = image.header.flags & mol_macho::header::MH_PIE != 0;
        let slide = slides.slide_for(&path, is_main, is_pie);
        let map = image.load_map(slide)?;
        let index = set.images.len();
        loaded.push((path.clone(), index));
        for d in map.dylibs.iter().filter(|d| d.cmd != LC_ID_DYLIB) {
            let weak = d.cmd == LC_LOAD_WEAK_DYLIB;
            edges.push((index, d.name.clone()));
            let mut env = req.search.clone();
            env.loader_path = path.clone();
            let found = resolve_dylib(&d.name, &env)
                .into_iter()
                .find(|c| store.bytes(c).is_some());
            match found {
                Some(c) => {
                    if !loaded.iter().any(|(p, _)| p == &c) && !queue.contains(&c) {
                        queue.push_back(c);
                    }
                }
                None if weak => {}
                None => {
                    return Err(LinkError::MissingDylib {
                        needed_by: path.clone(),
                        name: d.name.clone(),
                    })
                }
            }
        }
        set.images.push(LinkedImage {
            path,
            image,
            map,
            slide,
            ordinals: Vec::new(),
        });
    }

    // Per-image ordinal tables: 1-based over non-Id dylib commands in
    // file order; weak misses resolve to None.
    for (idx, img) in set.images.iter_mut().enumerate() {
        let mut env = req.search.clone();
        env.loader_path = img.path.clone();
        for (_, name) in edges.iter().filter(|(i, _)| *i == idx) {
            let target = resolve_dylib(name, &env)
                .into_iter()
                .find_map(|c| loaded.iter().find(|(p, _)| p == &c).map(|(_, i)| *i));
            img.ordinals.push(target);
        }
    }
    Ok(set)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testimg::{self, ImgSpec, TEXT_VMADDR};
    use mol_macho::header::{CPU_TYPE_ARM64, MH_DYLIB, MH_EXECUTE};
    use mol_macho::load::{LC_LOAD_DYLIB, LC_LOAD_WEAK_DYLIB};

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

    fn main_spec(deps: Vec<(u32, String)>) -> ImgSpec {
        ImgSpec {
            filetype: MH_EXECUTE,
            dylibs: deps,
            ..ImgSpec::default()
        }
    }

    fn lib_spec(id: &str) -> ImgSpec {
        ImgSpec {
            filetype: MH_DYLIB,
            id: Some(id.to_string()),
            entryoff: None,
            ..ImgSpec::default()
        }
    }

    #[test]
    fn links_main_and_dep_with_ordinals() {
        let main = testimg::build(&main_spec(vec![(LC_LOAD_DYLIB, "/libB.dylib".into())]));
        let libb = testimg::build(&lib_spec("/libB.dylib"));
        let mut store = MapStore::new();
        store.insert("/main", &main);
        store.insert("/libB.dylib", &libb);
        let mut slides = FixedSlide(0x100);
        let set = link(&req("/main"), &mut store, &mut slides).unwrap();
        assert_eq!(set.images.len(), 2);
        assert_eq!(set.images[0].path, "/main");
        assert_eq!(set.images[0].ordinals, vec![Some(1)]);
        assert!(set.images[1].ordinals.is_empty());
        assert_eq!(set.images[0].slide, 0x100);
        assert_eq!(
            set.main().map.entry.as_ref().unwrap().vmaddr,
            TEXT_VMADDR + 0x100
        );
    }

    #[test]
    fn missing_dep_names_needy_and_target() {
        let main = testimg::build(&main_spec(vec![(LC_LOAD_DYLIB, "/libX.dylib".into())]));
        let mut store = MapStore::new();
        store.insert("/main", &main);
        let mut slides = FixedSlide(0);
        let err = link(&req("/main"), &mut store, &mut slides).unwrap_err();
        assert_eq!(
            err,
            LinkError::MissingDylib {
                needed_by: "/main".to_string(),
                name: "/libX.dylib".to_string(),
            }
        );
    }

    #[test]
    fn weak_missing_dep_resolves_none() {
        let main = testimg::build(&main_spec(vec![(LC_LOAD_WEAK_DYLIB, "/libW.dylib".into())]));
        let mut store = MapStore::new();
        store.insert("/main", &main);
        let mut slides = FixedSlide(0);
        let set = link(&req("/main"), &mut store, &mut slides).unwrap();
        assert_eq!(set.images.len(), 1);
        assert_eq!(set.images[0].ordinals, vec![None]);
    }

    #[test]
    fn cycles_load_once_and_terminate() {
        let a = testimg::build(&main_spec(vec![(LC_LOAD_DYLIB, "/b".into())]));
        let b = testimg::build(&ImgSpec {
            filetype: MH_DYLIB,
            id: Some("/b".into()),
            entryoff: None,
            dylibs: vec![(LC_LOAD_DYLIB, "/main".into())],
            ..ImgSpec::default()
        });
        let mut store = MapStore::new();
        store.insert("/main", &a);
        store.insert("/b", &b);
        let mut slides = FixedSlide(0);
        let set = link(&req("/main"), &mut store, &mut slides).unwrap();
        assert_eq!(set.images.len(), 2);
        assert_eq!(set.images[0].ordinals, vec![Some(1)]);
        assert_eq!(set.images[1].ordinals, vec![Some(0)]);
    }

    #[test]
    fn non_pie_main_gets_zero_slide() {
        let main = testimg::build(&ImgSpec {
            filetype: MH_EXECUTE,
            is_pie: false,
            ..ImgSpec::default()
        });
        let mut store = MapStore::new();
        store.insert("/main", &main);
        let mut slides = FixedSlide(0x5000);
        let set = link(&req("/main"), &mut store, &mut slides).unwrap();
        assert_eq!(set.images[0].slide, 0);
    }

    #[test]
    fn transitive_bfs_discovers_diamond_once() {
        // main -> B, main -> C, B -> D, C -> D.
        let main = testimg::build(&main_spec(vec![
            (LC_LOAD_DYLIB, "/b".into()),
            (LC_LOAD_DYLIB, "/c".into()),
        ]));
        let b = testimg::build(&ImgSpec {
            filetype: MH_DYLIB,
            id: Some("/b".into()),
            entryoff: None,
            dylibs: vec![(LC_LOAD_DYLIB, "/d".into())],
            ..ImgSpec::default()
        });
        let c = testimg::build(&ImgSpec {
            filetype: MH_DYLIB,
            id: Some("/c".into()),
            entryoff: None,
            dylibs: vec![(LC_LOAD_DYLIB, "/d".into())],
            ..ImgSpec::default()
        });
        let d = testimg::build(&lib_spec("/d"));
        let mut store = MapStore::new();
        store.insert("/main", &main);
        store.insert("/b", &b);
        store.insert("/c", &c);
        store.insert("/d", &d);
        let mut slides = FixedSlide(0);
        let set = link(&req("/main"), &mut store, &mut slides).unwrap();
        let paths: Vec<&str> = set.images.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(paths, vec!["/main", "/b", "/c", "/d"]);
    }
}
