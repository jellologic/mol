// SPDX-License-Identifier: MIT OR Apache-2.0
//! `posix_spawn` handler: fork + exec with an error pipe.
//!
//! Semantics: `clone(SIGCHLD)` then the child `execve`s. Mach-O
//! targets re-exec through the loader (`/proc/self/exe run <file>
//! <args...>`); everything else execs directly. An `O_CLOEXEC` pipe
//! reports child-side `exec` failures back as the return value (real
//! `posix_spawn` returns the errno instead of failing the child).
//! `file_actions`/`attrp` must be NULL in the smoke subset. Returns
//! the errno number directly (POSIX: `posix_spawn` does not set
//! `errno`).

use core::ffi::{c_char, CStr};

use crate::errno::{from_linux, EFAULT, EINVAL, ENAMETOOLONG};
use crate::io::PATH_MAX;

const SIGCHLD: u64 = 17;
const O_CLOEXEC: u64 = 0o2000000;

/// Max argv/envp entries walked (bounded guest scan).
const MAX_VEC: usize = 4096;

/// Mach-O + fat magics (both endianships).
fn is_macho_magic(m: u32) -> bool {
    matches!(
        m,
        0xfeed_face | 0xcefa_edfe | 0xfeed_facf | 0xcffa_edfe | 0xcafe_babe | 0xbeba_feca
    )
}

/// Walk a guest `char**` vector into owned NUL-terminated byte
/// strings. NULL vector → `Ok(None)` (inherit for envp, error for
/// argv — decided by the caller).
fn walk_vec(vec: *const *const c_char, cap: usize) -> Result<Option<Vec<Vec<u8>>>, i32> {
    if vec.is_null() {
        return Ok(None);
    }
    let mut out = Vec::new();
    // SAFETY: bounded walk of guest pointers; wild entries fault
    // (documented divergence, doc 43 OQ).
    unsafe {
        for i in 0..cap {
            let p = *vec.add(i) as *const u8;
            if p.is_null() {
                return Ok(Some(out));
            }
            let mut len = 0usize;
            while len <= PATH_MAX {
                if *p.add(len) == 0 {
                    break;
                }
                len += 1;
            }
            if len > PATH_MAX {
                return Err(ENAMETOOLONG);
            }
            out.push(core::slice::from_raw_parts(p, len + 1).to_vec());
        }
    }
    Err(crate::errno::E2BIG)
}

/// Probe the target: `Ok(true)` for Mach-O (re-exec via loader).
fn is_macho(path: &CStr) -> Result<bool, i32> {
    use crate::raw;
    let fd = raw::openat(raw::AT_FDCWD, path.as_ptr() as u64, 0, 0).map_err(from_linux)? as i64;
    let mut magic = [0u8; 4];
    let n = raw::read(fd, magic.as_mut_ptr() as u64, 4).map_err(from_linux)?;
    let _ = raw::close(fd);
    if n < 4 {
        return Ok(false);
    }
    Ok(is_macho_magic(u32::from_le_bytes(magic)))
}

/// Current process environment as `K=V` NUL-terminated strings (NULL
/// `envp` inherits).
fn inherit_env() -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for (k, v) in std::env::vars_os() {
        let mut s = k.into_encoded_bytes();
        s.push(b'=');
        s.extend_from_slice(&v.into_encoded_bytes());
        s.push(0);
        out.push(s);
    }
    out
}

/// Build a null-terminated argv pointer vector over owned strings.
/// Returns (pointer vector, owned strings) — both must outlive the
/// clone.
fn build_exec_vec(strs: &[Vec<u8>]) -> Vec<u64> {
    let mut v: Vec<u64> = strs.iter().map(|s| s.as_ptr() as u64).collect();
    v.push(0);
    v
}

#[cfg(target_os = "linux")]
fn write_all(fd: i64, buf: &[u8]) {
    let mut off = 0usize;
    while off < buf.len() {
        match crate::raw::write(fd, buf[off..].as_ptr() as u64, (buf.len() - off) as u64) {
            Ok(n) if n > 0 => off += n as usize,
            _ => break,
        }
    }
}

/// Spawn `path` with `argv`/`envp`. Returns the child pid; failures
/// return the Darwin errno number (not via the errno cell).
#[cfg(target_os = "linux")]
pub fn posix_spawn(
    pid_out: *mut i32,
    path: *const c_char,
    file_actions: *const u8,
    attrp: *const u8,
    argv: *const *const c_char,
    envp: *const *const c_char,
) -> Result<i32, i32> {
    use crate::raw;
    if !file_actions.is_null() || !attrp.is_null() {
        return Err(EINVAL); // smoke: NULL only
    }
    if path.is_null() {
        return Err(EFAULT);
    }
    // SAFETY: bounded scan (see io::guest_str contract).
    let cpath = unsafe {
        let bytes = path as *const u8;
        let mut len = 0usize;
        while len <= PATH_MAX {
            if *bytes.add(len) == 0 {
                break;
            }
            len += 1;
        }
        if len > PATH_MAX {
            return Err(ENAMETOOLONG);
        }
        CStr::from_bytes_with_nul_unchecked(core::slice::from_raw_parts(bytes, len + 1))
    };
    let args = match walk_vec(argv, MAX_VEC)? {
        Some(a) if !a.is_empty() => a,
        _ => return Err(EINVAL), // NULL/empty argv
    };
    let env = match walk_vec(envp, MAX_VEC)? {
        Some(e) => e,
        None => inherit_env(),
    };
    let macho = is_macho(cpath)?;
    // Pre-build both exec vectors before the clone: the child must
    // not allocate. Loader argv: ["mol-loader", "run", path, args...]
    // with the guest argv preserved after the image (guest argv[0]
    // handling is the loader's contract, doc 43).
    let (exec_path, exec_args): (Vec<u8>, Vec<Vec<u8>>) = if macho {
        let mut v = vec![
            b"mol-loader\0".to_vec(),
            b"run\0".to_vec(),
            cpath.to_bytes_with_nul().to_vec(),
        ];
        v.extend(args.iter().skip(1).cloned());
        // MOL_SPAWN_LOADER overrides the loader executable for
        // hermetic tests (parent side, pre-clone).
        let exe = std::env::var_os("MOL_SPAWN_LOADER")
            .map(|p| {
                let mut b = p.into_encoded_bytes();
                b.push(0);
                b
            })
            .unwrap_or_else(|| b"/proc/self/exe\0".to_vec());
        (exe, v)
    } else {
        (cpath.to_bytes_with_nul().to_vec(), args)
    };
    let arg_ptrs = build_exec_vec(&exec_args);
    let env_ptrs = build_exec_vec(&env);

    let mut pipe = [0i32; 2];
    raw::pipe2(pipe.as_mut_ptr() as u64, O_CLOEXEC).map_err(from_linux)?;
    let snap = crate::atfork::prepare();
    let child = match raw::clone(SIGCHLD, 0, 0, 0, 0) {
        Ok(c) => c as i64,
        Err(linux) => {
            let _ = raw::close(pipe[0] as i64);
            let _ = raw::close(pipe[1] as i64);
            crate::atfork::run_parent(&snap);
            return Err(from_linux(linux));
        }
    };
    if child == 0 {
        // Child: async-signal-safe only from here to exec.
        crate::atfork::run_child(&snap);
        let _ = raw::close(pipe[0] as i64);
        let err = match raw::execve(
            exec_path.as_ptr() as u64,
            arg_ptrs.as_ptr() as u64,
            env_ptrs.as_ptr() as u64,
        ) {
            Ok(_) => 0,
            Err(linux) => from_linux(linux),
        };
        // execve returned: report through the pipe, then die.
        write_all(pipe[1] as i64, &(err as i32).to_ne_bytes());
        let _ = raw::close(pipe[1] as i64);
        raw::exit_group(127);
    }
    crate::atfork::run_parent(&snap);
    let _ = raw::close(pipe[1] as i64);
    // Parent: EOF means the write end died with a successful exec.
    let mut err_buf = [0u8; 4];
    let mut got = 0usize;
    loop {
        match raw::read(
            pipe[0] as i64,
            err_buf[got..].as_mut_ptr() as u64,
            (4 - got) as u64,
        ) {
            Ok(0) => break,
            Ok(n) => {
                got += n as usize;
                if got == 4 {
                    break;
                }
            }
            // EINTR (Linux 4) retries: a signal here must not turn a
            // failed spawn into a phantom success.
            Err(4) => continue,
            Err(_) => break,
        }
    }
    let _ = raw::close(pipe[0] as i64);
    if got == 4 {
        let err = i32::from_ne_bytes(err_buf);
        // Reap the stillborn child.
        let _ = raw::wait4(child, 0, 0);
        return Err(err);
    }
    if !pid_out.is_null() {
        // SAFETY: guest out-pointer for one pid; wild pointers
        // fault (documented divergence, doc 43 OQ).
        unsafe {
            *pid_out = child as i32;
        }
    }
    Ok(child as i32)
}
