// SPDX-License-Identifier: MIT OR Apache-2.0
//! `+load` scheduling + draining, exactly per `objc-loadmethod.mm`:
//! `schedule_class_load` recurses superclass-first with a visited
//! guard (`RW_LOADED`); `call_load_methods` detaches and drains
//! class loads repeatedly, then category loads once per round,
//! outermost-call-only for re-entrancy.

use std::collections::BTreeMap;

use crate::meta::{LoadMethod, ObjcImage};

/// A class scheduled for `+load`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadableClass {
    /// Owning image index (load order).
    pub image: usize,
    /// Class record index within the image.
    pub class: usize,
    /// Slid class VA (the `id` argument).
    pub cls: u64,
    pub method: LoadMethod,
}

/// A category scheduled for `+load`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadableCategory {
    pub image: usize,
    pub category: usize,
    /// Slid parent class VA (the `id` argument).
    pub cls: u64,
    pub method: LoadMethod,
}

/// Pending `+load` state across all mapped images.
#[derive(Debug, Default)]
pub struct LoadState {
    classes: Vec<LoadableClass>,
    categories: Vec<LoadableCategory>,
    loading: bool,
}

/// Class identity: image index + class address.
type ClassId = (usize, u64);

/// Collect loadables from all images in load order: classes first
/// (superclass-first per image), then categories in image order.
/// Superclass and category-parent links resolve against the global
/// table, so cross-image hierarchies order correctly. Unresolvable
/// links wait forever (weak-missing behavior), never error.
pub fn prepare(images: &[ObjcImage]) -> LoadState {
    // Global class table: address -> (image, index).
    let mut table: BTreeMap<u64, (usize, usize)> = BTreeMap::new();
    for (img, image) in images.iter().enumerate() {
        for (idx, cls) in image.classes.iter().enumerate() {
            table.entry(cls.addr).or_insert((img, idx));
        }
    }
    let mut st = LoadState::default();
    // Global visited set (`RW_LOADED`): a class schedules once
    // even when reached from several images.
    let mut scheduled: Vec<ClassId> = Vec::new();
    // Per image in load order: schedule classes superclass-first.
    for (img, image) in images.iter().enumerate() {
        for cls in &image.classes {
            schedule_class(&table, images, img, cls.addr, &mut scheduled, &mut st);
        }
    }
    // Then categories in image order; unresolvable parents wait.
    for (img, image) in images.iter().enumerate() {
        for (idx, cat) in image.categories.iter().enumerate() {
            let Some(method) = cat.load else { continue };
            if !table.contains_key(&cat.cls) {
                continue;
            }
            st.categories.push(LoadableCategory {
                image: img,
                category: idx,
                cls: cat.cls,
                method,
            });
        }
    }
    st
}

/// `schedule_class_load`: superclass first, then self; visited
/// classes (`RW_LOADED`) and in-progress cycles add once.
fn schedule_class(
    table: &BTreeMap<u64, (usize, usize)>,
    images: &[ObjcImage],
    img: usize,
    addr: u64,
    scheduled: &mut Vec<ClassId>,
    st: &mut LoadState,
) {
    // Resolve in the requesting image first (same-image wins on
    // duplicate addresses), else globally.
    let entry = images[img]
        .classes
        .iter()
        .position(|c| c.addr == addr)
        .map(|idx| (img, idx))
        .or_else(|| table.get(&addr).copied());
    let Some((cimg, cidx)) = entry else {
        return;
    };
    let id: ClassId = (cimg, images[cimg].classes[cidx].addr);
    if scheduled.contains(&id) {
        return;
    }
    // Cycle guard: mark before recursing (matches RW_REALIZING).
    scheduled.push(id);
    let superclass = images[cimg].classes[cidx].superclass;
    if let Some(sup) = superclass {
        schedule_class(table, images, cimg, sup, scheduled, st);
    }
    if let Some(method) = images[cimg].classes[cidx].load {
        st.classes.push(LoadableClass {
            image: cimg,
            class: cidx,
            cls: images[cimg].classes[cidx].addr,
            method,
        });
    }
}

/// Drain all pending `+load`s: repeatedly exhaust class loads,
/// then run category loads once; repeat while classes remain or
/// new categories appeared. Outermost-call-only: re-entrant
/// drains return immediately. `call` receives `(imp, cls, sel)`.
pub fn drain(st: &mut LoadState, call: &mut dyn FnMut(u64, u64, u64)) {
    if st.loading {
        return;
    }
    st.loading = true;
    loop {
        while !st.classes.is_empty() {
            call_class_loads(st, call);
        }
        let more = call_category_loads(st, call);
        if st.classes.is_empty() && !more {
            break;
        }
    }
    st.loading = false;
}

/// Detach the class list and call each in order. Classes added
/// during the drain wait for the next round.
fn call_class_loads(st: &mut LoadState, call: &mut dyn FnMut(u64, u64, u64)) {
    let classes = std::mem::take(&mut st.classes);
    for c in classes {
        call(c.method.imp, c.cls, c.method.sel);
    }
}

/// Detach the category list and call each in order; reattach
/// any added during the round. `prepare` admits only categories
/// with resolved (connected) parents, which is exactly the set
/// Apple's per-round `isLoadable` gate would call here — the
/// skip-and-retry path only differs under re-entrant mapping
/// (dlopen), which statically never happens. Returns whether new
/// categories appeared during the round.
fn call_category_loads(st: &mut LoadState, call: &mut dyn FnMut(u64, u64, u64)) -> bool {
    let cats = std::mem::take(&mut st.categories);
    let mut waiting = Vec::new();
    for c in cats {
        call(c.method.imp, c.cls, c.method.sel);
    }
    let more = !st.categories.is_empty();
    let mut new = std::mem::take(&mut st.categories);
    waiting.append(&mut new);
    st.categories = waiting;
    more
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reentrant_drain_is_outermost_only() {
        // The guard only matters once IMPs can map images
        // (dlopen); pinned here so the branch never rots.
        let mut st = LoadState::default();
        st.loading = true;
        drain(&mut st, &mut |_, _, _| {
            panic!("re-entrant drain must not call")
        });
    }
}
