// SPDX-License-Identifier: MIT OR Apache-2.0
//! Darwin→Linux flag translation for `open` and `mmap`.
//!
//! Darwin values from the macOS SDK (`sys/fcntl.h`, `sys/mman.h`);
//! Linux values from `asm-generic/fcntl.h`, `linux/mman.h`,
//! `asm-generic/mman-common.h`. Pure and portable: conformance tests
//! run the translators on any host, handlers on Linux.

use crate::errno::EINVAL;

// --- Darwin open flags (sys/fcntl.h) ---

pub const O_RDONLY: i32 = 0x0000;
pub const O_WRONLY: i32 = 0x0001;
pub const O_RDWR: i32 = 0x0002;
pub const O_ACCMODE: i32 = 0x0003;
pub const O_NONBLOCK: i32 = 0x0004;
pub const O_APPEND: i32 = 0x0008;
pub const O_SHLOCK: i32 = 0x0010;
pub const O_EXLOCK: i32 = 0x0020;
pub const O_ASYNC: i32 = 0x0040;
pub const O_SYNC: i32 = 0x0080;
pub const O_NOFOLLOW: i32 = 0x0100;
pub const O_CREAT: i32 = 0x0200;
pub const O_TRUNC: i32 = 0x0400;
pub const O_EXCL: i32 = 0x0800;
pub const O_RESOLVE_BENEATH: i32 = 0x1000;
pub const O_UNIQUE: i32 = 0x2000;
pub const O_EVTONLY: i32 = 0x8000;
pub const O_NOCTTY: i32 = 0x0002_0000;
pub const O_DIRECTORY: i32 = 0x0010_0000;
pub const O_SYMLINK: i32 = 0x0020_0000;
pub const O_DSYNC: i32 = 0x0040_0000;
pub const O_CLOEXEC: i32 = 0x0100_0000;
pub const O_CLOFORK: i32 = 0x0800_0000;
pub const O_NOFOLLOW_ANY: i32 = 0x2000_0000;
pub const O_EXEC: i32 = 0x4000_0000;
pub const O_POPUP: i32 = i32::MIN as i32; // 0x8000_0000, Finder relic

// --- Linux open flags (asm-generic/fcntl.h, octal in the header) ---

const L_O_ACCMODE: u32 = 0o3;
const L_O_RDONLY: u32 = 0o0;
const L_O_WRONLY: u32 = 0o1;
const L_O_RDWR: u32 = 0o2;
const L_O_CREAT: u32 = 0o100;
const L_O_EXCL: u32 = 0o200;
const L_O_NOCTTY: u32 = 0o400;
const L_O_TRUNC: u32 = 0o1000;
const L_O_APPEND: u32 = 0o2000;
const L_O_NONBLOCK: u32 = 0o4000;
const L_O_DSYNC: u32 = 0o10000;
const L_O_DIRECTORY: u32 = 0o200000;
const L_O_NOFOLLOW: u32 = 0o400000;
const L_O_CLOEXEC: u32 = 0o2000000;
const L_O_SYNC: u32 = 0x101000; // __O_SYNC | O_DSYNC
const L_O_PATH: u32 = 0o10000000;

/// Post-open lock requested by `O_SHLOCK`/`O_EXLOCK`, applied with
/// `flock(2)` (same open-file-description semantics: released at
/// close, inherited across fork).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenLock {
    Shared,
    Exclusive,
}

/// Translated `open` flags: Linux `openat` flags plus an optional
/// post-open `flock`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenFlags {
    pub linux: u32,
    pub lock: Option<OpenLock>,
}

/// Translate Darwin `open(2)` flags. Unsupported-by-construction
/// flags fail with `EINVAL` (documented per flag below); unknown
/// bits are ignored the way XNU ignores them.
pub fn translate_open(darwin: i32) -> Result<OpenFlags, i32> {
    let d = darwin as u32;
    // ACCMODE value 3 is invalid on both OSes.
    if d & (O_ACCMODE as u32) == 3 {
        return Err(EINVAL);
    }
    let mut linux = match d & (O_ACCMODE as u32) {
        0 => L_O_RDONLY,
        1 => L_O_WRONLY,
        2 => L_O_RDWR,
        _ => unreachable!("value 3 rejected above"),
    };
    // O_EVTONLY / O_EXEC / O_SYMLINK request limited fds: O_PATH is
    // the Linux cousin (stat/poll-capable, no I/O), matching the
    // observable "no read/write" contract.
    if d & ((O_EVTONLY | O_EXEC) as u32) != 0 {
        linux |= L_O_PATH;
    }
    if d & (O_SYMLINK as u32) != 0 {
        linux |= L_O_PATH | L_O_NOFOLLOW;
    }
    if d & (O_NONBLOCK as u32) != 0 {
        linux |= L_O_NONBLOCK;
    }
    if d & (O_APPEND as u32) != 0 {
        linux |= L_O_APPEND;
    }
    if d & (O_SYNC as u32) != 0 {
        linux |= L_O_SYNC;
    }
    if d & (O_DSYNC as u32) != 0 {
        linux |= L_O_DSYNC;
    }
    if d & (O_NOFOLLOW as u32) != 0 {
        linux |= L_O_NOFOLLOW;
    }
    if d & (O_CREAT as u32) != 0 {
        linux |= L_O_CREAT;
    }
    if d & (O_TRUNC as u32) != 0 {
        linux |= L_O_TRUNC;
    }
    if d & (O_EXCL as u32) != 0 {
        linux |= L_O_EXCL;
    }
    if d & (O_NOCTTY as u32) != 0 {
        linux |= L_O_NOCTTY;
    }
    if d & (O_DIRECTORY as u32) != 0 {
        linux |= L_O_DIRECTORY;
    }
    if d & (O_CLOEXEC as u32) != 0 {
        linux |= L_O_CLOEXEC;
    }
    // O_PATH fds must be O_RDONLY on Linux; the access mode above is
    // already masked to O_PATH-compatible when these bits are set.
    if linux & L_O_PATH != 0 {
        linux &= !L_O_ACCMODE;
    }
    let lock = if d & (O_EXLOCK as u32) != 0 {
        Some(OpenLock::Exclusive)
    } else if d & (O_SHLOCK as u32) != 0 {
        Some(OpenLock::Shared)
    } else {
        None
    };
    // Rejected: no Linux cousin in the smoke subset.
    // - O_ASYNC: needs F_SETOWN + SIGIO routing (P1.4+).
    // - O_RESOLVE_BENEATH / O_NOFOLLOW_ANY: need openat2 resolve
    //   emulation (P1.4+).
    // - O_UNIQUE / O_CLOFORK / O_POPUP: Apple-only, no cousin.
    if d & ((O_ASYNC | O_RESOLVE_BENEATH | O_UNIQUE | O_CLOFORK | O_NOFOLLOW_ANY | O_POPUP) as u32)
        != 0
    {
        return Err(EINVAL);
    }
    Ok(OpenFlags { linux, lock })
}

// --- mmap: protection is identical, flags are not ---

pub const PROT_NONE: i32 = 0x0;
pub const PROT_READ: i32 = 0x1;
pub const PROT_WRITE: i32 = 0x2;
pub const PROT_EXEC: i32 = 0x4;

pub const MAP_SHARED: i32 = 0x0001;
pub const MAP_PRIVATE: i32 = 0x0002;
pub const MAP_FIXED: i32 = 0x0010;
const MAP_RENAME: i32 = 0x0020;
const MAP_NORESERVE: i32 = 0x0040;
const MAP_NOEXTEND: i32 = 0x0100;
const MAP_HASSEMAPHORE: i32 = 0x0200;
const MAP_NOCACHE: i32 = 0x0400;
const MAP_JIT: i32 = 0x0800;
pub const MAP_ANON: i32 = 0x1000;
const MAP_RESILIENT_CODESIGN: i32 = 0x2000;
const MAP_RESILIENT_MEDIA: i32 = 0x4000;
const MAP_32BIT: i32 = 0x8000;
const MAP_TRANSLATED_ALLOW_EXECUTE: i32 = 0x02_0000;
const MAP_UNIX03: i32 = 0x04_0000;
const MAP_TPRO: i32 = 0x08_0000;

const L_MAP_SHARED: u32 = 0x01;
const L_MAP_PRIVATE: u32 = 0x02;
const L_MAP_FIXED: u32 = 0x10;
const L_MAP_ANONYMOUS: u32 = 0x20;
#[cfg(target_arch = "x86_64")]
const L_MAP_32BIT: u32 = 0x40;
const L_MAP_NORESERVE: u32 = 0x4000;

/// Translated `mmap` flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MmapFlags {
    pub linux: u32,
}

/// Translate Darwin `mmap(2)` flags. Advisory/Apple-only bits
/// (`MAP_HASSEMAPHORE`, `MAP_NOCACHE`, `MAP_RESILIENT_*`,
/// `MAP_RENAME`, `MAP_NOEXTEND`, `MAP_TRANSLATED_ALLOW_EXECUTE`,
/// `MAP_UNIX03`, `MAP_TPRO`) are accepted and ignored: they change no
/// guest-observable mapping property on Linux. `MAP_JIT` maps as a
/// plain mapping (Linux has no MAP_JIT gate; W^X policy stays with
/// the caller); `MAP_32BIT` needs the x86_64 knob and fails with
/// `EINVAL` on arm64.
pub fn translate_mmap(darwin: i32) -> Result<MmapFlags, i32> {
    let d = darwin as u32;
    let ty = d & 0x0f;
    if ty != (MAP_SHARED as u32) && ty != (MAP_PRIVATE as u32) {
        return Err(EINVAL); // neither or both: EINVAL on both OSes
    }
    let mut linux = if ty == (MAP_SHARED as u32) {
        L_MAP_SHARED
    } else {
        L_MAP_PRIVATE
    };
    if d & (MAP_FIXED as u32) != 0 {
        linux |= L_MAP_FIXED;
    }
    if d & (MAP_ANON as u32) != 0 {
        linux |= L_MAP_ANONYMOUS;
    }
    if d & (MAP_NORESERVE as u32) != 0 {
        linux |= L_MAP_NORESERVE;
    }
    if d & (MAP_32BIT as u32) != 0 {
        #[cfg(target_arch = "x86_64")]
        {
            linux |= L_MAP_32BIT;
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            return Err(EINVAL);
        }
    }
    // Accepted, ignored (see doc): advisory bits that change no
    // guest-observable mapping property on Linux.
    let _ = (
        MAP_JIT,
        MAP_HASSEMAPHORE,
        MAP_RENAME,
        MAP_NOEXTEND,
        MAP_NOCACHE,
        MAP_RESILIENT_CODESIGN,
        MAP_RESILIENT_MEDIA,
        MAP_TRANSLATED_ALLOW_EXECUTE,
        MAP_UNIX03,
        MAP_TPRO,
    );
    Ok(MmapFlags { linux })
}

/// Validate a Darwin `mmap`/`mprotect` protection: values match Linux
/// bit-for-bit, so this only rejects unknown bits (EINVAL on both).
pub fn check_prot(darwin: i32) -> Result<u32, i32> {
    if darwin as u32 & !0x7 != 0 {
        return Err(EINVAL);
    }
    Ok(darwin as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_common_subset() {
        // O_RDONLY passes through empty.
        assert_eq!(
            translate_open(O_RDONLY).unwrap(),
            OpenFlags {
                linux: L_O_RDONLY,
                lock: None
            }
        );
        // O_WRONLY|O_CREAT|O_TRUNC: the classic create-truncate.
        let f = translate_open(O_WRONLY | O_CREAT | O_TRUNC).unwrap();
        assert_eq!(f.linux, L_O_WRONLY | L_O_CREAT | L_O_TRUNC);
        assert_eq!(f.lock, None);
        // O_RDWR|O_APPEND|O_NONBLOCK|O_CLOEXEC.
        let f = translate_open(O_RDWR | O_APPEND | O_NONBLOCK | O_CLOEXEC).unwrap();
        assert_eq!(f.linux, L_O_RDWR | L_O_APPEND | L_O_NONBLOCK | L_O_CLOEXEC);
    }

    #[test]
    fn open_access_mode_3_rejected() {
        assert_eq!(translate_open(3), Err(EINVAL));
    }

    #[test]
    fn open_locks_ride_out_of_band() {
        let f = translate_open(O_RDONLY | O_SHLOCK).unwrap();
        assert_eq!(f.lock, Some(OpenLock::Shared));
        let f = translate_open(O_RDONLY | O_EXLOCK).unwrap();
        assert_eq!(f.lock, Some(OpenLock::Exclusive));
        // EXLOCK wins when both are set (matches open+flock order).
        let f = translate_open(O_RDONLY | O_SHLOCK | O_EXLOCK).unwrap();
        assert_eq!(f.lock, Some(OpenLock::Exclusive));
    }

    #[test]
    fn open_limited_fds_become_o_path() {
        let f = translate_open(O_EVTONLY).unwrap();
        assert_eq!(f.linux & L_O_PATH, L_O_PATH);
        assert_eq!(f.linux & L_O_ACCMODE, L_O_RDONLY);
        let f = translate_open(O_RDONLY | O_SYMLINK).unwrap();
        assert_eq!(f.linux & (L_O_PATH | L_O_NOFOLLOW), L_O_PATH | L_O_NOFOLLOW);
        // O_SEARCH = O_EXEC|O_DIRECTORY.
        let f = translate_open(O_EXEC | O_DIRECTORY).unwrap();
        assert_eq!(
            f.linux & (L_O_PATH | L_O_DIRECTORY),
            L_O_PATH | L_O_DIRECTORY
        );
    }

    #[test]
    fn open_sync_bits_translate() {
        assert_eq!(
            translate_open(O_WRONLY | O_SYNC).unwrap().linux,
            L_O_WRONLY | L_O_SYNC
        );
        assert_eq!(
            translate_open(O_WRONLY | O_DSYNC).unwrap().linux,
            L_O_WRONLY | L_O_DSYNC
        );
    }

    #[test]
    fn open_exotic_flags_rejected() {
        for flag in [
            O_ASYNC,
            O_RESOLVE_BENEATH,
            O_UNIQUE,
            O_CLOFORK,
            O_NOFOLLOW_ANY,
            O_POPUP,
        ] {
            assert_eq!(
                translate_open(O_RDONLY | flag),
                Err(EINVAL),
                "flag {flag:#x}"
            );
        }
    }

    #[test]
    fn mmap_type_bits() {
        assert_eq!(
            translate_mmap(MAP_PRIVATE | MAP_ANON).unwrap().linux,
            L_MAP_PRIVATE | L_MAP_ANONYMOUS
        );
        assert_eq!(
            translate_mmap(MAP_SHARED | MAP_FIXED).unwrap().linux,
            L_MAP_SHARED | L_MAP_FIXED
        );
        assert_eq!(translate_mmap(0), Err(EINVAL));
        assert_eq!(translate_mmap(MAP_SHARED | MAP_PRIVATE), Err(EINVAL));
    }

    #[test]
    fn mmap_advisory_bits_ignored() {
        // MAP_JIT + the advisory set change no mapping property.
        let plain = translate_mmap(MAP_PRIVATE | MAP_ANON).unwrap().linux;
        let fancy = translate_mmap(
            MAP_PRIVATE
                | MAP_ANON
                | MAP_JIT
                | MAP_HASSEMAPHORE
                | MAP_NOCACHE
                | MAP_RESILIENT_CODESIGN
                | MAP_RESILIENT_MEDIA
                | MAP_UNIX03,
        )
        .unwrap()
        .linux;
        assert_eq!(plain, fancy);
        // NORESERVE translates for real.
        assert_eq!(
            translate_mmap(MAP_PRIVATE | MAP_ANON | MAP_NORESERVE)
                .unwrap()
                .linux,
            L_MAP_PRIVATE | L_MAP_ANONYMOUS | L_MAP_NORESERVE
        );
    }

    #[test]
    fn prot_validates_only() {
        assert_eq!(check_prot(0), Ok(0));
        assert_eq!(check_prot(PROT_READ | PROT_WRITE), Ok(3));
        assert_eq!(check_prot(8), Err(EINVAL));
    }
}
