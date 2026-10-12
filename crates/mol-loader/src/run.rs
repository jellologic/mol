// SPDX-License-Identifier: MIT OR Apache-2.0
//! Guest execution: map, fix up, protect, and transfer (P1.1.1).
//!
//! Linux-only, same-ISA. The loader links with `mol-dyld`, maps each
//! image's segments at `vmaddr + slide`, applies the planned fixups,
//! `mprotect`s to `initprot`, builds the Darwin stack, runs
//! initializers, and jumps to `LC_MAIN` — all in-process. Guest exit
//! becomes the loader's exit code.

use std::collections::HashMap;
use std::fmt;

use mol_dyld::load::{ImageStore, LinkRequest, SlidePolicy};
use mol_dyld::policy::SearchEnv;
use mol_dyld::{fixups, init, load, policy, LinkedSet, RunRecord};
use mol_macho::header::MH_EXECUTE;

use crate::stack;
use crate::sys_linux as sys;

// ---------------------------------------------------------------------------
// errors
// ---------------------------------------------------------------------------

/// Guest launch failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunError {
    Io(String),
    Parse(mol_macho::ParseError),
    Link(mol_dyld::LinkError),
    /// No slice for the host ISA (cross-ISA is P6).
    ArchMismatch {
        want: String,
        have: String,
    },
    /// No `LC_MAIN` / `LC_UNIXTHREAD` entry in the main image.
    NoEntry {
        path: String,
    },
    Mmap {
        addr: u64,
        len: u64,
        errno: i32,
    },
    Mprotect {
        addr: u64,
        len: u64,
        errno: i32,
    },
    Munmap {
        addr: u64,
        len: u64,
        errno: i32,
    },
    /// Address space stayed occupied after retries.
    SlideExhausted {
        attempts: u32,
    },
    /// Library validation denied a dependency edge (P1.1.3).
    LibraryValidation {
        requirer: String,
        dep: String,
    },
    /// Segment file range exceeds its mapping.
    BadSegment {
        segname: String,
    },
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunError::Io(e) => write!(f, "run: i/o: {e}"),
            RunError::Parse(e) => write!(f, "run: {e}"),
            RunError::Link(e) => write!(f, "run: {e}"),
            RunError::ArchMismatch { want, have } => {
                write!(f, "run: {want} binary cannot run on {have} (see P6)")
            }
            RunError::NoEntry { path } => write!(f, "run: {path} has no entry point"),
            RunError::Mmap { addr, len, errno } => {
                write!(f, "run: mmap {addr:#x}+{len:#x} failed: errno {errno}")
            }
            RunError::Mprotect { addr, len, errno } => {
                write!(f, "run: mprotect {addr:#x}+{len:#x} failed: errno {errno}")
            }
            RunError::Munmap { addr, len, errno } => {
                write!(f, "run: munmap {addr:#x}+{len:#x} failed: errno {errno}")
            }
            RunError::SlideExhausted { attempts } => {
                write!(
                    f,
                    "run: address space occupied after {attempts} slide retries"
                )
            }
            RunError::LibraryValidation { requirer, dep } => {
                write!(f, "run: library validation: {requirer} may not load {dep}")
            }
            RunError::BadSegment { segname } => {
                write!(f, "run: segment {segname} file range exceeds mapping")
            }
        }
    }
}

impl std::error::Error for RunError {}

impl From<mol_macho::ParseError> for RunError {
    fn from(e: mol_macho::ParseError) -> Self {
        RunError::Parse(e)
    }
}

impl From<mol_dyld::LinkError> for RunError {
    fn from(e: mol_dyld::LinkError) -> Self {
        RunError::Link(e)
    }
}

pub type Result<T> = std::result::Result<T, RunError>;

// ---------------------------------------------------------------------------
// host facts
// ---------------------------------------------------------------------------

/// Host CPU type in Mach-O terms.
pub fn host_cputype() -> u32 {
    #[cfg(target_arch = "aarch64")]
    {
        mol_macho::header::CPU_TYPE_ARM64
    }
    #[cfg(target_arch = "x86_64")]
    {
        mol_macho::header::CPU_TYPE_X86_64
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        0
    }
}

pub fn host_arch_name() -> &'static str {
    #[cfg(target_arch = "aarch64")]
    {
        "arm64"
    }
    #[cfg(target_arch = "x86_64")]
    {
        "x86_64"
    }
    #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
    {
        "unknown"
    }
}

/// Whether the host signs/verifies arm64e pointers (P1.1.5 audit).
pub fn pac_available() -> bool {
    #[cfg(target_arch = "aarch64")]
    {
        std::arch::is_aarch64_feature_detected!("paca")
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        false
    }
}

/// Kernel page size via `AT_PAGESZ` (fallback 4K).
pub fn page_size() -> usize {
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

// ---------------------------------------------------------------------------
// codesign gating (P1.1.3)
// ---------------------------------------------------------------------------

/// Whether loading `dep` satisfies `requirer`'s library validation.
/// Unsigned images pair freely (ad-hoc carve-out); a requiring image
/// otherwise needs equal team ids or a platform dep. Unsigned deps
/// under the prefix system dirs count as platform (they are mol's own
/// ABI surface, documented in doc 42).
pub fn edge_allowed(
    req_flags: u32,
    req_team: Option<&str>,
    dep_flags: u32,
    dep_team: Option<&str>,
    dep_path: &str,
    sysroot: &str,
) -> bool {
    use mol_macho::codesign::{CS_PLATFORM_BINARY, CS_REQUIRE_LV};
    if req_flags & CS_REQUIRE_LV == 0 {
        return true;
    }
    if dep_flags & CS_PLATFORM_BINARY != 0 {
        return true;
    }
    let in_system = |dir: &str| {
        dep_path == dir
            || dep_path.starts_with(&format!("{dir}/"))
            || (!sysroot.is_empty()
                && (dep_path == &format!("{sysroot}{dir}")
                    || dep_path.starts_with(&format!("{sysroot}{dir}/"))))
    };
    if dep_flags & (mol_macho::codesign::CS_ADHOC | mol_macho::codesign::CS_VALID) == 0
        && (in_system("/usr/lib") || in_system("/System"))
    {
        return true;
    }
    req_team == dep_team
}

/// Whether the image may create JIT mappings: non-hardened images
/// always; hardened-runtime images need an allow-JIT entitlement.
/// (Enforced when JIT mappings arrive in P1.2+; recorded here.)
pub fn jit_allowed(flags: u32, gating: &[(String, bool)]) -> bool {
    if flags & mol_macho::codesign::CS_RUNTIME == 0 {
        return true;
    }
    gating.iter().any(|(k, v)| {
        *v && (k == "com.apple.security.cs.allow-jit"
            || k == "com.apple.security.cs.allow-unsigned-executable-memory")
    })
}

/// Enforce library validation over every linked edge.
fn check_library_validation(set: &LinkedSet, sysroot: &str) -> Result<()> {
    for img in set.images.iter() {
        let (req_flags, req_team) = match &img.map.codesign {
            Some(cs) => (cs.flags, cs.team_id.as_deref()),
            None => (0, None),
        };
        // Ordinal table is 1-based over non-Id dylibs; walk the same
        // order mol-dyld used.
        let mut ord = 0usize;
        for d in img.map.dylibs.iter() {
            use mol_macho::load::LC_ID_DYLIB;
            if d.cmd == LC_ID_DYLIB {
                continue;
            }
            let slot = img.ordinals.get(ord);
            ord += 1;
            let dep_idx = match slot.and_then(|o| *o) {
                Some(i) => i,
                None => continue, // missing weak: nothing loaded
            };
            let dep = &set.images[dep_idx];
            let (dep_flags, dep_team) = match &dep.map.codesign {
                Some(cs) => (cs.flags, cs.team_id.as_deref()),
                None => (0, None),
            };
            if !edge_allowed(req_flags, req_team, dep_flags, dep_team, &dep.path, sysroot) {
                return Err(RunError::LibraryValidation {
                    requirer: img.path.clone(),
                    dep: dep.path.clone(),
                });
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// store + slides
// ---------------------------------------------------------------------------

/// Owning file store. Image bytes are intentionally never freed:
/// they back the process image for its whole lifetime.
pub struct FsStore {
    sysroot: String,
    cache: HashMap<String, &'static [u8]>,
}

impl FsStore {
    pub fn new(sysroot: &str) -> Self {
        FsStore {
            sysroot: sysroot.to_string(),
            cache: HashMap::new(),
        }
    }

    fn read_candidate(&self, path: &str) -> Option<Vec<u8>> {
        if let Ok(b) = std::fs::read(path) {
            return Some(b);
        }
        if !self.sysroot.is_empty() && !path.starts_with('/') {
            std::fs::read(format!("{}/{}", self.sysroot.trim_end_matches('/'), path)).ok()
        } else {
            None
        }
    }
}

impl<'a> ImageStore<'a> for FsStore {
    fn bytes(&mut self, path: &str) -> Option<&'a [u8]> {
        if let Some(b) = self.cache.get(path) {
            return Some(*b);
        }
        let data = self.read_candidate(path)?;
        let leaked: &'static [u8] = Box::leak(data.into_boxed_slice());
        self.cache.insert(path.to_string(), leaked);
        Some(leaked)
    }
}

/// Whether `path` names the libSystem shim (bare or sysroot-joined).
fn is_shim_path(path: &str) -> bool {
    path == mol_libsystem::shim::SHIM_PATH || path.ends_with(mol_libsystem::shim::SHIM_PATH)
}

/// File store with the generated `libSystem.B.dylib` shim overlaid:
/// guests link the shim path and always get mol's generated image
/// (a real Apple libSystem could never run on Linux — it traps — so
/// the overlay takes precedence over any same-path file).
pub struct ShimStore {
    inner: FsStore,
    shim: Option<&'static [u8]>,
}

impl ShimStore {
    pub fn new(sysroot: &str) -> Self {
        ShimStore {
            inner: FsStore::new(sysroot),
            shim: None,
        }
    }
}

impl<'a> ImageStore<'a> for ShimStore {
    fn bytes(&mut self, path: &str) -> Option<&'a [u8]> {
        if is_shim_path(path) {
            if self.shim.is_none() {
                let fat = mol_libsystem::shim::build_shim(&mol_libsystem::exports());
                self.shim = Some(Box::leak(fat.into_boxed_slice()));
            }
            return self.shim;
        }
        self.inner.bytes(path)
    }
}

/// Random per-image ASLR slides (1MB granularity). Mains slide in
/// `[1MB, 255MB]`; dylibs slide in `[5GB+1MB, 5GB+255MB]`, mirroring
/// Apple's ~6GB shared-cache placement. The split is load-bearing:
/// the main image's 4GB `__PAGEZERO` reservation covers low VA, so
/// low-slid dylibs would collide with it on every attempt
/// (`SlideExhausted`). Slides are never zero: zero-base images
/// (dylibs link at 0, as does the mol shim) would otherwise map
/// below `vm.mmap_min_addr` (64K) and fail with EPERM for non-root
/// users.
pub struct RandomSlide;

impl SlidePolicy for RandomSlide {
    fn slide_for(&mut self, _path: &str, is_main: bool, is_pie: bool) -> u64 {
        if is_main && !is_pie {
            return 0;
        }
        let mut buf = [0u8; 8];
        let rand1_255 = if sys::getrandom(&mut buf).is_err() {
            1
        } else {
            u64::from_ne_bytes(buf) % 255 + 1
        };
        if is_main {
            rand1_255 * 0x10_0000
        } else {
            0x1_4000_0000 + rand1_255 * 0x10_0000
        }
    }
}

/// Fixed slide for the main image, random slides for dylibs (used
/// for `--slide`). Dylibs link at 0 and can never map at a low fixed
/// slide on Linux (`mmap_min_addr`); macOS likewise always slides
/// dylibs even when the main image is fixed.
struct FixedMainSlide {
    main_slide: u64,
    random: RandomSlide,
}

impl SlidePolicy for FixedMainSlide {
    fn slide_for(&mut self, path: &str, is_main: bool, is_pie: bool) -> u64 {
        if is_main {
            if is_pie {
                self.main_slide
            } else {
                0
            }
        } else {
            self.random.slide_for(path, is_main, is_pie)
        }
    }
}

// ---------------------------------------------------------------------------
// options + record
// ---------------------------------------------------------------------------

/// Guest launch options.
pub struct RunOptions {
    /// Guest argv including argv[0].
    pub argv: Vec<String>,
    /// Guest environment as `K=V` strings.
    pub envp: Vec<String>,
    /// Prefix merged dir guest absolute paths resolve under.
    pub sysroot: String,
    /// Fixed slide for the main image (`None` = random ASLR).
    /// Dylib slides are always random.
    pub fixed_slide: Option<u64>,
    /// Guest stack size in bytes.
    pub stack_size: u64,
    /// Install the raw-syscall fault catcher (SIGSEGV/SIGBUS/SIGILL
    /// report to stderr) before mapping the guest.
    pub debug_catch_faults: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        RunOptions {
            argv: Vec::new(),
            envp: Vec::new(),
            sysroot: String::new(),
            fixed_slide: None,
            stack_size: 8 * 1024 * 1024,
            debug_catch_faults: false,
        }
    }
}

fn vm_to_host_prot(initprot: u32) -> u64 {
    use mol_macho::load::{VM_PROT_EXECUTE, VM_PROT_READ, VM_PROT_WRITE};
    let mut p = sys::PROT_NONE;
    if initprot & VM_PROT_READ != 0 {
        p |= sys::PROT_READ;
    }
    if initprot & VM_PROT_WRITE != 0 {
        p |= sys::PROT_WRITE;
    }
    if initprot & VM_PROT_EXECUTE != 0 {
        p |= sys::PROT_EXEC;
    }
    p
}

// ---------------------------------------------------------------------------
// mapping
// ---------------------------------------------------------------------------

/// Reservation holes: zero-filesize segments minus the image's own
/// content ranges (slid). A reservation (e.g. `__PAGEZERO`) covers
/// VA the image's own later segments may overlap (zero-linked
/// mains); mapping the full reservation with `NOREPLACE` would
/// self-collide, so only the holes are reserved.
fn reservation_holes(map: &mol_macho::map::LoadMap, slide: u64) -> Vec<(u64, u64)> {
    let mut content: Vec<(u64, u64)> = Vec::new();
    for seg in &map.segments {
        if seg.vmsize != 0 && seg.filesize != 0 {
            content.push((seg.vmaddr.wrapping_add(slide), seg.vmsize));
        }
    }
    let mut holes = Vec::new();
    for seg in &map.segments {
        if seg.vmsize == 0 || seg.filesize != 0 {
            continue;
        }
        let mut cur = seg.vmaddr.wrapping_add(slide);
        let end = cur.wrapping_add(seg.vmsize);
        // Subtract content ranges (segments are few; O(n*m) is fine).
        let mut spans = vec![(cur, end)];
        for (caddr, clen) in &content {
            let cend = caddr.wrapping_add(*clen);
            let mut next = Vec::new();
            for (s, e) in spans {
                if *caddr >= e || cend <= s {
                    next.push((s, e));
                    continue;
                }
                if *caddr > s {
                    next.push((s, *caddr));
                }
                if cend < e {
                    next.push((cend, e));
                }
            }
            spans = next;
        }
        for (s, e) in spans {
            if e > s {
                cur = s;
                holes.push((cur, e - s));
            }
        }
    }
    holes
}

/// Map one image's segments at `vmaddr + slide` with read/write so
/// file bytes and fixups can be applied. Returns mapped ranges for
/// cleanup on retry.
fn map_image(bytes: &[u8], map: &mol_macho::map::LoadMap, slide: u64) -> Result<Vec<(u64, u64)>> {
    let mut mapped = Vec::new();
    // Pass 1: content segments.
    for seg in &map.segments {
        if seg.vmsize == 0 || seg.filesize == 0 {
            continue;
        }
        if seg.filesize > seg.vmsize {
            return Err(RunError::BadSegment {
                segname: seg.segname.clone(),
            });
        }
        let addr = seg.vmaddr.wrapping_add(slide);
        sys::mmap_fixed(addr, seg.vmsize, sys::PROT_READ | sys::PROT_WRITE, true).map_err(
            |errno| RunError::Mmap {
                addr,
                len: seg.vmsize,
                errno,
            },
        )?;
        mapped.push((addr, seg.vmsize));
        // Copy file bytes; the anonymous tail past filesize is
        // already zero-filled.
        let src_off = seg.fileoff as usize;
        let src_len = seg.filesize as usize;
        let src = bytes
            .get(src_off..src_off + src_len)
            .ok_or(RunError::BadSegment {
                segname: seg.segname.clone(),
            })?;
        // SAFETY: destination is our fresh mapping, sized vmsize >=
        // filesize; source is bounds-checked above.
        unsafe {
            std::ptr::copy_nonoverlapping(src.as_ptr(), addr as *mut u8, src_len);
        }
    }
    // Pass 2: reservation holes (no access, no bytes).
    for (addr, len) in reservation_holes(map, slide) {
        sys::mmap_fixed(addr, len, sys::PROT_NONE, true).map_err(|errno| RunError::Mmap {
            addr,
            len,
            errno,
        })?;
        mapped.push((addr, len));
    }
    Ok(mapped)
}

fn unmap_all(mapped: &[(u64, u64)]) {
    for (addr, len) in mapped {
        let _ = sys::munmap(*addr, *len);
    }
}

/// Apply planned fixup writes to the mappings.
fn apply_fixups(plan: &[fixups::FixupApply]) -> Result<()> {
    for a in plan {
        // SAFETY: addresses come from the engine's segment math over
        // mappings this process owns; widths are 4 or 8 by
        // construction.
        unsafe {
            match a.size {
                8 => std::ptr::write_unaligned(a.addr as *mut u64, a.value.to_le()),
                4 => std::ptr::write_unaligned(a.addr as *mut u32, (a.value as u32).to_le()),
                s => {
                    return Err(RunError::BadSegment {
                        segname: format!("fixup width {s} at {:#x}", a.addr),
                    })
                }
            }
        }
    }
    Ok(())
}

/// Fill the shim's `__data` slots with handler addresses (and the
/// stack-guard value). Runs after fixups, before `mprotect`, while
/// every mapping is still writable. Images that link no shim are
/// unaffected.
fn fill_shim_slots(set: &LinkedSet) -> Result<()> {
    use mol_libsystem::shim::slot_addr;
    let exports = mol_libsystem::exports();
    mol_syscall::chk::init_guard();
    for img in &set.images {
        if !is_shim_path(&img.path) {
            continue;
        }
        let data = img
            .map
            .segments
            .iter()
            .find(|s| s.segname == "__DATA")
            .ok_or(RunError::BadSegment {
                segname: format!("{} has no __DATA", img.path),
            })?;
        for (i, e) in exports.iter().enumerate() {
            let value = mol_libsystem::resolve(e.name).ok_or(RunError::Link(
                mol_dyld::LinkError::SymbolNotFound {
                    needed_by: img.path.clone(),
                    symbol: e.name.to_string(),
                },
            ))?;
            // Slot addresses derive from the segment (robust to
            // generator layout changes); the index order is ABI.
            let at = data
                .vmaddr
                .wrapping_add(img.slide)
                .wrapping_add(slot_addr(i) - mol_libsystem::shim::DATA_VMADDR);
            // SAFETY: our fresh mapping, sized for the slot run.
            unsafe {
                std::ptr::write_unaligned(at as *mut u64, value.to_le());
            }
        }
    }
    Ok(())
}

/// Drop each segment to its `initprot` and flush executable caches.
/// Reservations go first so page-rounding overlap resolves in favor
/// of content (a hole sharing a host page with a segment must not
/// strip that segment's protections).
fn protect_image(map: &mol_macho::map::LoadMap, slide: u64, page: u64) -> Result<()> {
    for (addr, len) in reservation_holes(map, slide) {
        let start = addr & !(page - 1);
        let end = (addr + len + page - 1) & !(page - 1);
        sys::mprotect(start, end - start, sys::PROT_NONE).map_err(|errno| RunError::Mprotect {
            addr: start,
            len: end - start,
            errno,
        })?;
    }
    for seg in &map.segments {
        if seg.vmsize == 0 || seg.filesize == 0 {
            continue;
        }
        let addr = seg.vmaddr.wrapping_add(slide);
        let prot = vm_to_host_prot(seg.initprot);
        // Round the range out to host pages.
        let start = addr & !(page - 1);
        let end = (addr + seg.vmsize + page - 1) & !(page - 1);
        sys::mprotect(start, end - start, prot).map_err(|errno| RunError::Mprotect {
            addr: start,
            len: end - start,
            errno,
        })?;
        if prot & sys::PROT_EXEC != 0 {
            // SAFETY: range is mapped readable+writable here... but
            // mprotect already dropped write. clear_cache only reads
            // (data clean) and issues icache maintenance by address.
            let slice =
                unsafe { std::slice::from_raw_parts_mut(addr as *mut u8, seg.vmsize as usize) };
            sys::clear_cache(slice);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// transfer
// ---------------------------------------------------------------------------

/// Switch to the guest stack, call each initializer, call the entry
/// point as `entry(argc, argv, envp, apple)`, and exit with its return
/// value. Diverges via `exit_group`.
#[cfg(target_arch = "aarch64")]
unsafe fn transfer(
    stack: u64,
    fns: *const u64,
    n: usize,
    entry: u64,
    argc: u64,
    argv: u64,
    envp: u64,
    apple: u64,
) -> ! {
    use core::arch::asm;
    // SAFETY: all addresses are live guest mappings; the block
    // diverges (noreturn) so clobbering sp is sound.
    unsafe {
        asm!(
            "mov sp, {stack}",
            "sub sp, sp, #64",
            "str {fns}, [sp, #0]",
            "str {n}, [sp, #8]",
            "str {entry}, [sp, #16]",
            "str {argc}, [sp, #24]",
            "str {argv}, [sp, #32]",
            "str {envp}, [sp, #40]",
            "str {apple}, [sp, #48]",
            "b 3f",
            "2:",
            "ldr x9, [sp, #0]",
            "ldr x10, [x9], #8",
            "str x9, [sp, #0]",
            "blr x10",
            "ldr x9, [sp, #8]",
            "sub x9, x9, #1",
            "str x9, [sp, #8]",
            "3:",
            "ldr x9, [sp, #8]",
            "cbnz x9, 2b",
            "ldr x0, [sp, #24]",
            "ldr x1, [sp, #32]",
            "ldr x2, [sp, #40]",
            "ldr x3, [sp, #48]",
            "ldr x9, [sp, #16]",
            "add sp, sp, #64",
            "blr x9",
            "mov x8, #94",
            "svc #0",
            "brk #0",
            stack = in(reg) stack,
            fns = in(reg) fns,
            n = in(reg) n,
            entry = in(reg) entry,
            argc = in(reg) argc,
            argv = in(reg) argv,
            envp = in(reg) envp,
            apple = in(reg) apple,
            options(noreturn)
        );
    }
}

/// Switch to the guest stack, call each initializer, call the entry
/// point as `entry(argc, argv, envp, apple)`, and exit with its return
/// value. Diverges via `exit_group`.
#[cfg(target_arch = "x86_64")]
unsafe fn transfer(
    stack: u64,
    fns: *const u64,
    n: usize,
    entry: u64,
    argc: u64,
    argv: u64,
    envp: u64,
    apple: u64,
) -> ! {
    use core::arch::asm;
    // SAFETY: as above.
    unsafe {
        asm!(
            "mov rsp, {stack}",
            "sub rsp, 64",
            "mov [rsp + 0], {fns}",
            "mov [rsp + 8], {n}",
            "mov [rsp + 16], {entry}",
            "mov [rsp + 24], {argc}",
            "mov [rsp + 32], {argv}",
            "mov [rsp + 40], {envp}",
            "mov [rsp + 48], {apple}",
            "jmp 3f",
            "2:",
            "mov r10, [rsp + 0]",
            "mov r11, [r10]",
            "add r10, 8",
            "mov [rsp + 0], r10",
            "call r11",
            "mov r10, [rsp + 8]",
            "dec r10",
            "mov [rsp + 8], r10",
            "3:",
            "cmp qword ptr [rsp + 8], 0",
            "jne 2b",
            "mov rdi, [rsp + 24]",
            "mov rsi, [rsp + 32]",
            "mov rdx, [rsp + 40]",
            "mov rcx, [rsp + 48]",
            "mov r10, [rsp + 16]",
            "add rsp, 64",
            "call r10",
            "mov rdi, rax",
            "mov rax, 231",
            "syscall",
            "ud2",
            stack = in(reg) stack,
            fns = in(reg) fns,
            n = in(reg) n,
            entry = in(reg) entry,
            argc = in(reg) argc,
            argv = in(reg) argv,
            envp = in(reg) envp,
            apple = in(reg) apple,
            options(noreturn)
        );
    }
}

// ---------------------------------------------------------------------------
// entry
// ---------------------------------------------------------------------------

const SLIDE_ATTEMPTS: u32 = 8;

/// Verify the main file carries a host-ISA slice before linking,
/// so cross-ISA launches fail with [`RunError::ArchMismatch`] (P6)
/// instead of a bare parse error.
fn check_host_slice(bytes: &[u8]) -> Result<()> {
    let host = host_cputype();
    let file = mol_macho::parse(bytes)?;
    let ok = match &file {
        mol_macho::MachFile::Thin(img) => img.header.cputype == host,
        mol_macho::MachFile::Fat(f) => f.fat.arches.iter().any(|a| a.cputype == host),
    };
    if ok {
        return Ok(());
    }
    let want = match &file {
        mol_macho::MachFile::Thin(img) => {
            mol_macho::header::cputype_name(img.header.cputype).to_string()
        }
        mol_macho::MachFile::Fat(f) => {
            let names: Vec<&str> = f
                .fat
                .arches
                .iter()
                .map(|a| mol_macho::header::cputype_name(a.cputype))
                .collect();
            format!("fat[{}]", names.join(","))
        }
    };
    Err(RunError::ArchMismatch {
        want,
        have: host_arch_name().to_string(),
    })
}

/// Link, map, and run the guest at `path`. Diverges into the guest
/// on success (guest exit becomes this process's exit); returns
/// `Err` only when the launch itself fails. (`Ok` is uninhabited but
/// stable Rust cannot spell `Result<!>`.)
pub fn run_guest(path: &str, opts: &RunOptions) -> Result<()> {
    let host = host_cputype();
    if host == 0 {
        return Err(RunError::ArchMismatch {
            want: "arm64 or x86_64".to_string(),
            have: host_arch_name().to_string(),
        });
    }
    let main_bytes =
        std::fs::read(path).map_err(|e| RunError::Io(format!("cannot read {path}: {e}")))?;
    check_host_slice(&main_bytes)?;
    let mut store = ShimStore::new(&opts.sysroot);
    // Closure inputs from the launch environment (doc 12 L3).
    let mut overrides = Vec::new();
    let mut interposing = false;
    let mut force_closures = false;
    for kv in &opts.envp {
        if let Some((k, _)) = kv.split_once('=') {
            if k.starts_with("DYLD_") {
                overrides.push(k);
            }
            if k == "DYLD_INSERT_LIBRARIES" {
                interposing = true;
            }
            if k == "DYLD_USE_CLOSURES" {
                force_closures = true;
            }
        }
    }
    let decision = policy::decide_closure(&policy::ClosureInputs {
        // No shared cache until the P1.1.4 strategy lands.
        cache_present: false,
        cache_stale: false,
        dyld_overrides: overrides,
        interposing,
        force_closures,
    });
    let search = SearchEnv {
        executable_path: path.to_string(),
        loader_path: path.to_string(),
        rpaths: Vec::new(),
        dyld_library_path: Vec::new(),
        dyld_fallback_library_path: Vec::new(),
        sysroot: opts.sysroot.clone(),
    };
    let req = LinkRequest {
        main_path: path.to_string(),
        cputype: host,
        cpusubtype: 0,
        search,
    };
    // Map with slide retries on occupied address space.
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        let mut fixed = opts.fixed_slide.map(|s| FixedMainSlide {
            main_slide: s,
            random: RandomSlide,
        });
        let mut random = RandomSlide;
        let slides: &mut dyn SlidePolicy = match &mut fixed {
            Some(f) => f,
            None => &mut random,
        };
        let set: LinkedSet = load::link(&req, &mut store, slides).map_err(|e| match e {
            mol_dyld::LinkError::Parse(p) => RunError::Parse(p),
            mol_dyld::LinkError::MissingFile { path } => {
                RunError::Io(format!("no such file: {path}"))
            }
            other => RunError::Link(other),
        })?;
        // Slice selection already guarantees the host ISA here.
        if set.main().image.header.filetype != MH_EXECUTE {
            return Err(RunError::NoEntry {
                path: path.to_string(),
            });
        }
        check_library_validation(&set, &opts.sysroot)?;
        // Map + copy + fix up.
        let mut mapped: Vec<(u64, u64)> = Vec::new();
        let map_result: Result<Vec<fixups::FixupApply>> = (|| {
            for img in &set.images {
                let ranges = map_image(img.image.bytes, &img.map, img.slide)?;
                mapped.extend(ranges);
            }
            let plan = fixups::plan_fixups(
                &set,
                &fixups::FixupInputs {
                    pac_available: pac_available(),
                },
            )?;
            apply_fixups(&plan)?;
            fill_shim_slots(&set)?;
            let page = page_size() as u64;
            for img in &set.images {
                protect_image(&img.map, img.slide, page)?;
            }
            Ok(plan)
        })();
        match map_result {
            Ok(plan) => {
                let entry = set.main().map.entry.as_ref().ok_or(RunError::NoEntry {
                    path: path.to_string(),
                })?;
                let entry_addr = entry.vmaddr.wrapping_add(set.main().slide);
                // Guest stack.
                let stack_top = {
                    let at = sys::mmap_any(opts.stack_size, sys::PROT_READ | sys::PROT_WRITE)
                        .map_err(|errno| RunError::Mmap {
                            addr: 0,
                            len: opts.stack_size,
                            errno,
                        })?;
                    at + opts.stack_size
                };
                let argv: Vec<&str> = opts.argv.iter().map(String::as_str).collect();
                let envp: Vec<&str> = opts.envp.iter().map(String::as_str).collect();
                let apple = [path];
                let image = stack::build_stack(stack_top, &argv, &envp, &apple);
                // SAFETY: stack mapping is ours, sized stack_size.
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        image.bytes.as_ptr(),
                        image.sp as *mut u8,
                        image.bytes.len(),
                    );
                }
                let inits = init::order_initializers(&set);
                let fns: Vec<u64> = inits.iter().map(|i| i.address).collect();
                // libSystem bring-up before guest initializers (P1.2.1).
                let mut ictx = mol_libsystem::init::InitCtx::default();
                mol_libsystem::init::run_init_order(&mut ictx);
                let record = RunRecord {
                    cache_mode: decision.mode,
                    reason: decision.reason,
                    images: set.images.iter().map(|i| i.path.clone()).collect(),
                    fixups_applied: plan.len(),
                    pac_stripped: plan.iter().filter(|a| a.stripped_pac).count(),
                    pac_available: pac_available(),
                    init_trace: ictx.trace.iter().map(|s| s.to_string()).collect(),
                };
                eprint!("{}", record.render());
                // Entry register values: argc count + pointers to
                // the argv/envp/apple vectors (NOT the words' own
                // addresses for argc).
                let (argc_val, argv_ptr, envp_ptr, apple_ptr) =
                    entry_args(image.sp, argv.len(), envp.len());
                // After libSystem init, which resets SIGBUS/SIGSEGV
                // to default: install the fault catcher last so it
                // survives to guest entry.
                if opts.debug_catch_faults {
                    crate::faults::install();
                }
                // SAFETY: transfer diverges into mapped guest code.
                unsafe {
                    transfer(
                        image.sp,
                        fns.as_ptr(),
                        fns.len(),
                        entry_addr,
                        argc_val,
                        argv_ptr,
                        envp_ptr,
                        apple_ptr,
                    )
                }
            }
            Err(RunError::Mmap { errno: 17, .. }) if attempt < SLIDE_ATTEMPTS => {
                // EEXIST: occupied range; retry with fresh slides.
                // Dylib slides are always random (even under
                // `--slide`), so a dylib collision can clear on
                // retry; a fixed-main collision fails deterministically
                // after SLIDE_ATTEMPTS.
                unmap_all(&mapped);
                continue;
            }
            Err(e) => {
                unmap_all(&mapped);
                return Err(e);
            }
        }
    }
}

/// Entry register values for a stack built by [`stack::build_stack`]:
/// argc count plus pointers to the three vectors.
fn entry_args(sp: u64, argc: usize, envc: usize) -> (u64, u64, u64, u64) {
    let argv_ptr = sp + 8;
    let envp_ptr = sp + 8 + ((argc + 1) * 8) as u64;
    let apple_ptr = envp_ptr + ((envc + 1) * 8) as u64;
    (argc as u64, argv_ptr, envp_ptr, apple_ptr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mol_macho::codesign::{CS_ADHOC, CS_PLATFORM_BINARY, CS_REQUIRE_LV, CS_RUNTIME, CS_VALID};

    #[test]
    fn fixed_main_slide_pins_main_only() {
        let mut p = FixedMainSlide {
            main_slide: 0x2000,
            random: RandomSlide,
        };
        assert_eq!(p.slide_for("/m", true, true), 0x2000);
        assert_eq!(p.slide_for("/m", true, false), 0);
        let d = p.slide_for("/d", false, false);
        assert!(d >= 0x1_4000_0000 + 0x10_0000 && d <= 0x1_4000_0000 + 255 * 0x10_0000);
    }

    #[test]
    fn entry_args_match_stack_layout() {
        // sp, argc=2, envc=1: argv@sp+8, envp after 3 words, apple after 2.
        assert_eq!(entry_args(0x1000, 2, 1), (2, 0x1008, 0x1020, 0x1030));
        // Cross-check against the real builder: pointers stored at
        // those addresses resolve to the right strings.
        let img = stack::build_stack(0x2000, &["p", "q"], &["E=1"], &["p"]);
        let (_, argv, envp, apple) = entry_args(img.sp, 2, 1);
        let r64 = |a: u64| {
            u64::from_le_bytes(
                img.bytes[(a - img.sp) as usize..(a - img.sp) as usize + 8]
                    .try_into()
                    .unwrap(),
            )
        };
        let s = |a: u64| {
            let b = &img.bytes[(a - img.sp) as usize..];
            b[..b.iter().position(|&c| c == 0).unwrap()].to_vec()
        };
        assert_eq!(r64(img.sp), 2);
        assert_eq!(s(r64(argv)), b"p");
        assert_eq!(s(r64(argv + 8)), b"q");
        assert_eq!(s(r64(envp)), b"E=1");
        assert_eq!(s(r64(apple)), b"p");
    }

    #[test]
    fn library_validation_matrix() {
        // No requirement: everything loads.
        assert!(edge_allowed(0, None, 0, Some("other"), "/x", ""));
        // Same team (including ad-hoc pair): allowed.
        assert!(edge_allowed(
            CS_REQUIRE_LV,
            Some("T"),
            CS_VALID,
            Some("T"),
            "/x",
            ""
        ));
        assert!(edge_allowed(CS_REQUIRE_LV, None, CS_ADHOC, None, "/x", ""));
        // Different teams: denied.
        assert!(!edge_allowed(
            CS_REQUIRE_LV,
            Some("A"),
            CS_VALID,
            Some("B"),
            "/x",
            ""
        ));
        // Platform deps always load.
        assert!(edge_allowed(
            CS_REQUIRE_LV,
            Some("A"),
            CS_PLATFORM_BINARY,
            Some("APPLE"),
            "/x",
            ""
        ));
        // Unsigned prefix system libs count as platform.
        assert!(edge_allowed(
            CS_REQUIRE_LV,
            Some("A"),
            0,
            None,
            "/p/merged/usr/lib/libSystem.B.dylib",
            "/p/merged"
        ));
        // ...but unsigned libs elsewhere fall back to team compare.
        assert!(!edge_allowed(
            CS_REQUIRE_LV,
            Some("A"),
            0,
            None,
            "/tmp/evil.dylib",
            ""
        ));
    }

    #[test]
    fn jit_policy_matrix() {
        // Non-hardened: always allowed.
        assert!(jit_allowed(0, &[]));
        // Hardened without entitlement: denied.
        assert!(!jit_allowed(CS_RUNTIME, &[]));
        assert!(!jit_allowed(
            CS_RUNTIME,
            &[("com.apple.security.cs.allow-jit".to_string(), false)]
        ));
        // Either JIT entitlement grants.
        assert!(jit_allowed(
            CS_RUNTIME,
            &[("com.apple.security.cs.allow-jit".to_string(), true)]
        ));
        assert!(jit_allowed(
            CS_RUNTIME,
            &[(
                "com.apple.security.cs.allow-unsigned-executable-memory".to_string(),
                true
            )]
        ));
        // Unrelated entitlements don't grant.
        assert!(!jit_allowed(
            CS_RUNTIME,
            &[("com.apple.security.get-task-allow".to_string(), true)]
        ));
    }

    #[test]
    fn page_size_sane() {
        let p = page_size();
        assert!(p >= 0x1000 && p.is_power_of_two());
    }

    #[test]
    fn reservation_holes_subtract_own_content() {
        use mol_macho::map::{LoadMap, MapSegment};
        fn seg(name: &str, vmaddr: u64, vmsize: u64, filesize: u64) -> MapSegment {
            MapSegment {
                segname: name.to_string(),
                vmaddr,
                vmsize,
                fileoff: 0,
                filesize,
                maxprot: 0,
                initprot: 0,
                flags: 0,
                sections: Vec::new(),
            }
        }
        fn map_with(segments: Vec<MapSegment>) -> LoadMap {
            LoadMap {
                cputype: 0,
                cpusubtype: 0,
                filetype: 0,
                flags: 0,
                slide: 0,
                segments,
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
            }
        }
        // Zero-linked main: PAGEZERO [0, 4GB) with TEXT/DATA inside.
        let map = map_with(vec![
            seg("__PAGEZERO", 0, 0x1_0000_0000, 0),
            seg("__TEXT", 0x1000, 0x3000, 0x3000),
            seg("__DATA", 0x5000, 0x1000, 0x1000),
        ]);
        let holes = reservation_holes(&map, 0x10_0000);
        assert_eq!(
            holes,
            vec![
                (0x10_0000, 0x1000),                 // [slide, TEXT)
                (0x10_4000, 0x1000),                 // (TEXT, DATA)
                (0x10_6000, 0x1_0000_0000 - 0x6000), // [DATA end, 4GB)
            ]
        );
        // No reservations: no holes.
        let map = map_with(vec![seg("__TEXT", 0x1000, 0x3000, 0x3000)]);
        assert!(reservation_holes(&map, 0).is_empty());
    }
}
