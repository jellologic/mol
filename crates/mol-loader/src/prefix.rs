// SPDX-License-Identifier: MIT OR Apache-2.0
//! Prefix boot: mount+PID namespace container with an overlayfs merged
//! view (doc 12 L0).
//!
//! Split: pure spec builders (unit-tested everywhere) + a Linux-only
//! exec layer. Outer `boot` wraps `unshare(1)`; the hidden
//! `__inner-boot` mounts and `exec`s the init process so it lands
//! as PID 1. No pivot_root in P0 — the merged view is mounted at
//! `<prefix>/merged` and the guest still sees the host fs (hardened
//! in P1).
//!
//! Layout: `<prefix>/{merged,lower,upper,work,dyld-cache}`. Upper,
//! work, and cache are *siblings* of the merged view, never beneath
//! it — anything under the mountpoint would be shadowed post-mount.
//! Upper/work must live on a filesystem the kernel accepts as an
//! overlay upperdir (ext4, tmpfs — notably NOT overlayfs itself,
//! i.e. not on a docker container rootfs).

use std::path::{Path, PathBuf};

/// Resolved prefix layout. All paths absolute (overlayfs requirement).
/// `root` is the merged view; the rest are its siblings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrefixSpec {
    pub root: PathBuf,
    pub lowerdir: PathBuf,
    pub upperdir: PathBuf,
    pub workdir: PathBuf,
    pub cache_dir: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PrefixError {
    RelativePath {
        which: &'static str,
        path: PathBuf,
    },
    UpperEqualsWork,
    CrossDeviceUpperWork,
}

impl std::fmt::Display for PrefixError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PrefixError::RelativePath { which, path } => {
                write!(f, "{which} must be absolute: {}", path.display())
            }
            PrefixError::UpperEqualsWork => {
                write!(f, "upperdir and workdir must differ")
            }
            PrefixError::CrossDeviceUpperWork => {
                write!(f, "workdir must share a filesystem with upperdir")
            }
        }
    }
}

impl std::error::Error for PrefixError {}

impl PrefixSpec {
    /// Layout for a prefix dir: merged view + sibling layers and the
    /// dyld cache override.
    pub fn with_defaults(prefix: &Path) -> Self {
        PrefixSpec {
            root: prefix.join("merged"),
            lowerdir: prefix.join("lower"),
            upperdir: prefix.join("upper"),
            workdir: prefix.join("work"),
            cache_dir: prefix.join("dyld-cache"),
        }
    }

    /// Pure shape validation (no filesystem access).
    pub fn validate_shape(&self) -> Result<(), PrefixError> {
        for (which, path) in [
            ("root", &self.root),
            ("lowerdir", &self.lowerdir),
            ("upperdir", &self.upperdir),
            ("workdir", &self.workdir),
            ("cache_dir", &self.cache_dir),
        ] {
            if !path.is_absolute() {
                return Err(PrefixError::RelativePath {
                    which,
                    path: path.clone(),
                });
            }
        }
        if self.upperdir == self.workdir {
            return Err(PrefixError::UpperEqualsWork);
        }
        Ok(())
    }

    /// Overlay mount options (doc 12: lowerdir,upperdir,workdir).
    pub fn overlay_options(&self) -> String {
        format!(
            "lowerdir={},upperdir={},workdir={}",
            self.lowerdir.display(),
            self.upperdir.display(),
            self.workdir.display()
        )
    }

    /// argv for `unshare(1)`, excluding the program and inner command.
    /// Userns included so stock Ubuntu users can boot without root;
    /// harmless when already root.
    pub fn unshare_argv(&self) -> Vec<String> {
        ["--user", "--map-root-user", "--mount", "--pid", "--fork", "--mount-proc"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    /// Guest environment: prefix pointer (the merged view) + dyld
    /// cache override (P0.3.1).
    pub fn guest_env(&self) -> Vec<(String, String)> {
        vec![
            ("MOL_PREFIX".to_string(), self.root.display().to_string()),
            (
                "DYLD_SHARED_CACHE_DIR".to_string(),
                self.cache_dir.display().to_string(),
            ),
        ]
    }
}

/// Pure same-device predicate backing the workdir/upperdir rule.
pub fn same_device(upper_dev: u64, work_dev: u64) -> bool {
    upper_dev == work_dev
}

/// Linux-only exec layer (thin wrappers over mkdir/mount/exec).
#[cfg(target_os = "linux")]
pub mod linux {
    use super::{same_device, PrefixError, PrefixSpec};
    use std::fs;
    use std::os::unix::fs::MetadataExt;
    use std::process::Command;

    pub fn create_layout(spec: &PrefixSpec) -> std::io::Result<()> {
        // NOTE: <root>/proc is created in mount_all *after* the overlay
        // mount — anything pre-created under <root> would be shadowed by
        // the merged view.
        for dir in [
            &spec.root,
            &spec.lowerdir,
            &spec.upperdir,
            &spec.workdir,
            &spec.cache_dir,
        ] {
            fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    pub fn check_devices(spec: &PrefixSpec) -> std::io::Result<bool> {
        Ok(same_device(
            fs::metadata(&spec.upperdir)?.dev(),
            fs::metadata(&spec.workdir)?.dev(),
        ))
    }

    fn run_mount(args: &[&str]) -> std::io::Result<()> {
        let status = Command::new("mount").args(args).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(format!(
                "mount {args:?} failed: {status} \
                 (upperdir fs must support overlay; see dmesg)"
            )))
        }
    }

    pub fn mount_all(spec: &PrefixSpec) -> std::io::Result<()> {
        let opts = spec.overlay_options();
        let root = spec.root.to_string_lossy().into_owned();
        run_mount(&["-t", "overlay", "overlay", "-o", &opts, &root])?;
        // Created post-mount (lands in upperdir): pre-creating it would
        // be shadowed by the merged view.
        fs::create_dir_all(spec.root.join("proc"))?;
        let proc = spec.root.join("proc").to_string_lossy().into_owned();
        run_mount(&["-t", "proc", "proc", &proc])?;
        Ok(())
    }

    /// Map a [`PrefixError`] into an I/O error for the inner-boot path.
    pub fn shape_error(err: PrefixError) -> std::io::Error {
        std::io::Error::other(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> PrefixSpec {
        PrefixSpec::with_defaults(Path::new("/tmp/mol-prefix-test"))
    }

    #[test]
    fn defaults_are_merged_plus_siblings() {
        let s = spec();
        assert_eq!(s.root, Path::new("/tmp/mol-prefix-test/merged"));
        assert_eq!(s.lowerdir, Path::new("/tmp/mol-prefix-test/lower"));
        assert_eq!(s.upperdir, Path::new("/tmp/mol-prefix-test/upper"));
        assert_eq!(s.workdir, Path::new("/tmp/mol-prefix-test/work"));
        assert_eq!(s.cache_dir, Path::new("/tmp/mol-prefix-test/dyld-cache"));
    }

    #[test]
    fn shape_rejects_relative_paths() {
        let mut s = spec();
        s.root = PathBuf::from("relative/root");
        assert!(matches!(
            s.validate_shape(),
            Err(PrefixError::RelativePath { which: "root", .. })
        ));
    }

    #[test]
    fn shape_rejects_upper_equals_work() {
        let mut s = spec();
        s.workdir = s.upperdir.clone();
        assert_eq!(s.validate_shape(), Err(PrefixError::UpperEqualsWork));
    }

    #[test]
    fn good_shape_passes() {
        assert_eq!(spec().validate_shape(), Ok(()));
    }

    #[test]
    fn overlay_options_format() {
        assert_eq!(
            spec().overlay_options(),
            "lowerdir=/tmp/mol-prefix-test/lower,\
             upperdir=/tmp/mol-prefix-test/upper,\
             workdir=/tmp/mol-prefix-test/work"
                .replace(" ", "")
        );
    }

    #[test]
    fn unshare_argv_requests_user_mount_pid_fork() {
        let argv = spec().unshare_argv();
        for flag in ["--user", "--map-root-user", "--mount", "--pid", "--fork"] {
            assert!(argv.contains(&flag.to_string()), "missing {flag}");
        }
    }

    #[test]
    fn guest_env_points_at_merged_view_and_cache() {
        let env = spec().guest_env();
        assert!(env.contains(&(
            "MOL_PREFIX".to_string(),
            "/tmp/mol-prefix-test/merged".to_string()
        )));
        assert!(env.contains(&(
            "DYLD_SHARED_CACHE_DIR".to_string(),
            "/tmp/mol-prefix-test/dyld-cache".to_string()
        )));
    }

    #[test]
    fn same_device_predicate() {
        assert!(same_device(41, 41));
        assert!(!same_device(41, 42));
    }
}
