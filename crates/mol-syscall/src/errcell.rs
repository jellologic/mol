// SPDX-License-Identifier: MIT OR Apache-2.0
//! Guest-visible `errno` cell behind `___error`.
//!
//! Thread-local (P1.3 threads each get their own); zeroed by
//! `__libkernel_init` in the init order (doc 12 L2). Handlers set it
//! on failure and leave it untouched on success, matching libc
//! wrapper semantics.

use core::cell::Cell;

thread_local! {
    static ERRNO: Cell<i32> = const { Cell::new(0) };
}

/// Set the calling thread's guest errno.
pub fn set(no: i32) {
    ERRNO.with(|c| c.set(no));
}

/// Read the calling thread's guest errno.
pub fn get() -> i32 {
    ERRNO.with(|c| c.get())
}

/// `___error` implementation: pointer to the calling thread's cell.
pub fn error_ptr() -> *mut i32 {
    ERRNO.with(|c| c.as_ptr())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_roundtrip() {
        set(2);
        assert_eq!(get(), 2);
        set(0);
        assert_eq!(get(), 0);
    }

    #[test]
    fn pointer_aliases_cell() {
        set(34);
        // SAFETY: our own thread-local cell.
        unsafe {
            assert_eq!(*error_ptr(), 34);
            *error_ptr() = 0;
        }
        assert_eq!(get(), 0);
    }
}
