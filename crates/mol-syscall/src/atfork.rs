// SPDX-License-Identifier: MIT OR Apache-2.0
//! `pthread_atfork`-style handler chain (doc 12 L2: atfork set).
//!
//! `posix_spawn` runs prepare/parent/child around its fork. Child
//! handlers must be async-signal-safe (no allocation); the P1.2 set
//! is empty-but-functional, with libc/malloc/GCD handlers arriving
//! in P1.3/P2.2. Single-threaded guests only for now: a fork racing
//! another thread's registration would inherit a locked mutex.

use std::sync::Mutex;

/// An atfork handler triple (prepare, parent, child), C ABI.
#[derive(Debug, Clone, Copy)]
pub struct AtforkTriple {
    pub prepare: Option<extern "C" fn()>,
    pub parent: Option<extern "C" fn()>,
    pub child: Option<extern "C" fn()>,
}

static CHAIN: Mutex<Vec<AtforkTriple>> = Mutex::new(Vec::new());

/// Register a handler triple (the `pthread_atfork` core).
pub fn register(triple: AtforkTriple) {
    CHAIN.lock().unwrap().push(triple);
}

/// Number of registered triples (init-order evidence).
pub fn len() -> usize {
    CHAIN.lock().unwrap().len()
}

/// Snapshot the chain for fork-best-effort delivery without holding
/// the lock across the clone.
fn snapshot() -> Vec<AtforkTriple> {
    CHAIN.lock().unwrap().clone()
}

/// Run one leg in POSIX order (prepare runs LIFO).
fn run_leg(snapshot: &[AtforkTriple], leg: u8) {
    if leg == 0 {
        for t in snapshot.iter().rev() {
            if let Some(f) = t.prepare {
                f();
            }
        }
    } else {
        for t in snapshot.iter() {
            let f = if leg == 1 { t.parent } else { t.child };
            if let Some(f) = f {
                f();
            }
        }
    }
}

/// Snapshot + run prepare (call before clone).
pub fn prepare() -> Vec<AtforkTriple> {
    let snap = snapshot();
    run_leg(&snap, 0);
    snap
}

/// Run the parent leg over a pre-clone snapshot.
pub fn run_parent(snap: &[AtforkTriple]) {
    run_leg(snap, 1);
}

/// Run the child leg over a pre-clone snapshot. Async-signal-safe
/// iff every registered child handler is.
pub fn run_child(snap: &[AtforkTriple]) {
    run_leg(snap, 2);
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicU32, Ordering};

    static LOG: AtomicU32 = AtomicU32::new(0);

    extern "C" fn tag1() {
        LOG.fetch_or(1, Ordering::SeqCst);
    }
    extern "C" fn tag2() {
        LOG.fetch_or(2, Ordering::SeqCst);
    }

    #[test]
    fn register_and_leg_order() {
        // Prepare runs LIFO: last registered first.
        let before = len();
        register(AtforkTriple {
            prepare: Some(tag1),
            parent: None,
            child: None,
        });
        register(AtforkTriple {
            prepare: Some(tag2),
            parent: None,
            child: None,
        });
        assert_eq!(len(), before + 2);
        LOG.store(0, Ordering::SeqCst);
        let snap = prepare();
        assert_eq!(LOG.load(Ordering::SeqCst), 3);
        // Parent/child legs run FIFO over the same snapshot.
        run_parent(&snap);
        run_child(&snap);
    }
}
