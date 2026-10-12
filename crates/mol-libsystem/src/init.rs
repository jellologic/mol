// SPDX-License-Identifier: MIT OR Apache-2.0
//! `libSystem_initializer` call order (doc 12 L2, P1.2.1).
//!
//! Exact order, after Apple's `Libsystem/init.c`:
//! `__libkernel_init` → `__libplatform_init` → `__pthread_init` →
//! `_libc_initializer` → `__malloc_init` → `_dyld_initializer` →
//! `libdispatch_init` → `_libxpc_initializer`.
//!
//! The first six steps are real (each leaves checkable evidence);
//! dispatch/XPC are documented stubs whose full-order proof
//! completes in P2.2. The loader runs this before guest
//! initializers and records the trace in the run record.

use mol_syscall::atfork::{self, AtforkTriple};

/// Evidence left by the init order.
#[derive(Debug, Default)]
pub struct InitCtx {
    /// Step names in execution order.
    pub trace: Vec<&'static str>,
    /// `__libkernel_init` zeroed the guest errno cell.
    pub errno_zeroed: bool,
    /// `__libkernel_init` reset the runtime's signal state (Linux).
    pub runtime_handlers_reset: bool,
    /// `__libplatform_init` observed the host page size.
    pub page_size: usize,
    /// `__pthread_init` captured the main thread id (Linux).
    pub main_tid: u64,
    /// Triples registered when `_dyld_initializer` ran (proves it
    /// runs after `_libc_initializer`'s registration, not just after
    /// in the trace).
    pub atfork_at_dyld_init: usize,
    /// Total triples registered by the end of init.
    pub atfork_total: usize,
    /// Stub flags (documented, full proof in P2.2).
    pub dispatch_stub: bool,
    pub xpc_stub: bool,
}

extern "C" fn libc_atfork_prepare() {}
extern "C" fn libc_atfork_parent() {}
extern "C" fn libc_atfork_child() {}

fn libkernel_init(ctx: &mut InitCtx) {
    ctx.trace.push("__libkernel_init");
    // errno starts zeroed (doc 12 L2 done-criteria).
    mol_syscall::errcell::set(0);
    ctx.errno_zeroed = mol_syscall::errcell::get() == 0;
    // Pristine dispositions for guests (Rust catches BUS/SEGV).
    #[cfg(target_os = "linux")]
    {
        mol_syscall::signal::reset_runtime_handlers();
        ctx.runtime_handlers_reset = true;
    }
}

fn libplatform_init(ctx: &mut InitCtx) {
    ctx.trace.push("__libplatform_init");
    // Platform facts are libplatform's job: capture the host page
    // size (Linux via AT_PAGESZ, fallback 4K).
    #[cfg(target_os = "linux")]
    {
        ctx.page_size = host_page_size();
    }
    #[cfg(not(target_os = "linux"))]
    {
        ctx.page_size = 0x4000;
    }
}

#[cfg(target_os = "linux")]
fn host_page_size() -> usize {
    const AT_PAGESZ: u64 = 6;
    if let Ok(auxv) = std::fs::read("/proc/self/auxv") {
        for chunk in auxv.chunks_exact(16) {
            let tag = u64::from_ne_bytes(chunk[0..8].try_into().unwrap());
            let val = u64::from_ne_bytes(chunk[8..16].try_into().unwrap());
            if tag == AT_PAGESZ && val.is_power_of_two() && val >= 0x1000 {
                return val as usize;
            }
        }
    }
    0x1000
}

fn pthread_init(ctx: &mut InitCtx) {
    ctx.trace.push("__pthread_init");
    // Single-threaded in P1.2: capture the main thread id as the
    // identity later thread creation compares against (P1.3).
    #[cfg(target_os = "linux")]
    {
        ctx.main_tid = mol_syscall::proc::gettid();
    }
}

fn libc_initializer(ctx: &mut InitCtx) {
    ctx.trace.push("_libc_initializer");
    // libc registers its atfork triple first; bodies stay no-ops
    // until stdio locks arrive (P1.3), but the chain mechanics are
    // live (exercised by every posix_spawn).
    atfork::register(AtforkTriple {
        prepare: Some(libc_atfork_prepare),
        parent: Some(libc_atfork_parent),
        child: Some(libc_atfork_child),
    });
}

fn malloc_init(ctx: &mut InitCtx) {
    ctx.trace.push("__malloc_init");
    // No zones yet: guests allocate via mmap in P1.2; the first
    // malloc zone arrives with the P1.3 allocator. The trace entry
    // plus the zone count below are the execution evidence.
    ctx.atfork_total = atfork::len();
}

fn dyld_initializer(ctx: &mut InitCtx) {
    ctx.trace.push("_dyld_initializer");
    // Order evidence beyond the trace: dyld must observe libc's
    // registration, proving _libc_initializer ran before.
    ctx.atfork_at_dyld_init = atfork::len();
    ctx.atfork_total = atfork::len();
}

/// `libdispatch_init` — documented stub (full proof in P2.2).
/// The real step creates TSD keys, binds the main queue, sets
/// pthread_atfork handlers, and inits hw/time/vtable/os_object/
/// voucher; root queues come up lazily via `_pthread_workqueue`
/// ([queue.c](https://raw.githubusercontent.com/apple/swift-corelibs-libdispatch/main/src/queue.c)).
fn libdispatch_init(ctx: &mut InitCtx) {
    ctx.trace.push("libdispatch_init");
    ctx.dispatch_stub = true;
}

/// `_libxpc_initializer` — documented stub (full proof in P2.2).
/// Internals are unpublished (libxpc source closed); bring-up is
/// specified at the launchd/XPC API level only (doc 12 L5).
fn libxpc_initializer(ctx: &mut InitCtx) {
    ctx.trace.push("_libxpc_initializer");
    ctx.xpc_stub = true;
}

/// Run the full init order into `ctx`.
pub fn run_init_order(ctx: &mut InitCtx) {
    libkernel_init(ctx);
    libplatform_init(ctx);
    pthread_init(ctx);
    libc_initializer(ctx);
    malloc_init(ctx);
    dyld_initializer(ctx);
    libdispatch_init(ctx);
    libxpc_initializer(ctx);
}

/// The exact expected order (single source for tests + docs).
pub const EXPECTED_ORDER: &[&str] = &[
    "__libkernel_init",
    "__libplatform_init",
    "__pthread_init",
    "_libc_initializer",
    "__malloc_init",
    "_dyld_initializer",
    "libdispatch_init",
    "_libxpc_initializer",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_is_exact() {
        let mut ctx = InitCtx::default();
        run_init_order(&mut ctx);
        assert_eq!(ctx.trace, EXPECTED_ORDER);
    }

    #[test]
    fn errno_zeroed_and_atfork_set() {
        mol_syscall::errcell::set(99);
        let before = atfork::len();
        let mut ctx = InitCtx::default();
        run_init_order(&mut ctx);
        assert!(ctx.errno_zeroed);
        assert_eq!(mol_syscall::errcell::get(), 0);
        // libc registered exactly one triple during this init.
        assert_eq!(ctx.atfork_total, before + 1);
        // dyld observed it: order evidence, not just trace order.
        assert_eq!(ctx.atfork_at_dyld_init, before + 1);
    }

    #[test]
    fn platform_facts_captured() {
        let mut ctx = InitCtx::default();
        run_init_order(&mut ctx);
        assert!(ctx.page_size >= 0x1000 && ctx.page_size.is_power_of_two());
        #[cfg(target_os = "linux")]
        assert_ne!(ctx.main_tid, 0);
    }

    #[test]
    fn dispatch_xpc_are_documented_stubs() {
        let mut ctx = InitCtx::default();
        run_init_order(&mut ctx);
        assert!(ctx.dispatch_stub && ctx.xpc_stub);
    }
}
