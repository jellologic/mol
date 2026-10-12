// SPDX-License-Identifier: MIT OR Apache-2.0
//! Closure/cache-mode policy and dylib path search (docs 03, 12 L3).
//!
//! Pure policy: no filesystem access. The loader supplies the inputs
//! (env snapshot, cache freshness) and resolves the candidates this
//! module returns in order.

/// Which loader-set strategy this run uses. Exactly one mode is
/// recorded per run ([`crate::RunRecord`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheMode {
    /// Prebuilt closure (fast path): cache is fresh, no overrides.
    Prebuilt,
    /// Just-in-time parse (slow path): overrides, interposing, or no
    /// usable cache.
    JustInTime,
}

/// Inputs to the closure decision (doc 12 L3).
#[derive(Debug, Clone, Default)]
pub struct ClosureInputs<'a> {
    /// A shared-cache file exists in the prefix.
    pub cache_present: bool,
    /// The cache predates boot (closures are invalid after reboot).
    pub cache_stale: bool,
    /// Names of `DYLD_*` overrides present in the environment.
    pub dyld_overrides: Vec<&'a str>,
    /// Interposing is active (`DYLD_INSERT_LIBRARIES` or tuples).
    pub interposing: bool,
    /// `DYLD_USE_CLOSURES` forces the fast path when usable.
    pub force_closures: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosureDecision {
    pub mode: CacheMode,
    pub reason: &'static str,
}

/// Decide the cache mode for one run. Policy: closures are never used
/// without a cache, with interposing, or with `DYLD_*` overrides,
/// and are invalid after reboot; `DYLD_USE_CLOSURES` forces them.
pub fn decide_closure(inp: &ClosureInputs<'_>) -> ClosureDecision {
    if !inp.cache_present {
        return ClosureDecision {
            mode: CacheMode::JustInTime,
            reason: "no shared cache in prefix",
        };
    }
    if inp.cache_stale {
        return ClosureDecision {
            mode: CacheMode::JustInTime,
            reason: "cache invalid after reboot",
        };
    }
    if inp.interposing {
        return ClosureDecision {
            mode: CacheMode::JustInTime,
            reason: "interposing active",
        };
    }
    if !inp.dyld_overrides.is_empty() {
        return ClosureDecision {
            mode: CacheMode::JustInTime,
            reason: "DYLD_* overrides present",
        };
    }
    if inp.force_closures {
        return ClosureDecision {
            mode: CacheMode::Prebuilt,
            reason: "DYLD_USE_CLOSURES forces fast path",
        };
    }
    ClosureDecision {
        mode: CacheMode::Prebuilt,
        reason: "clean launch on fresh cache",
    }
}

/// Path-search environment for one dependency edge (owned so the
/// linker can re-point `loader_path` per edge without borrowing).
#[derive(Debug, Clone, Default)]
pub struct SearchEnv {
    /// Absolute guest path of the main executable.
    pub executable_path: String,
    /// Absolute guest path of the image naming the dependency.
    pub loader_path: String,
    /// `LC_RPATH` entries accumulated from the main executable.
    pub rpaths: Vec<String>,
    pub dyld_library_path: Vec<String>,
    pub dyld_fallback_library_path: Vec<String>,
    /// Prefix merged dir guest absolute paths resolve under.
    /// Empty means "no sysroot" (unit tests, host tools).
    pub sysroot: String,
}

fn dirname(path: &str) -> &str {
    match path.rfind('/') {
        Some(0) => "/",
        Some(i) => &path[..i],
        None => ".",
    }
}

fn basename(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => &path[i + 1..],
        None => path,
    }
}

fn join_sysroot(sysroot: &str, abs: &str) -> String {
    if sysroot.is_empty() {
        return abs.to_string();
    }
    format!("{}{}", sysroot.trim_end_matches('/'), abs)
}

/// Expand `@executable_path` / `@loader_path` prefixes in `s`.
fn expand_loader_vars(s: &str, env: &SearchEnv) -> String {
    let mut out = s.to_string();
    if out.contains("@executable_path") {
        out = out.replace("@executable_path", dirname(&env.executable_path));
    }
    if out.contains("@loader_path") {
        out = out.replace("@loader_path", dirname(&env.loader_path));
    }
    out
}

/// Resolve a raw dylib name to candidate paths in search order. The
/// caller tries each until one loads.
pub fn resolve_dylib(raw: &str, env: &SearchEnv) -> Vec<String> {
    // `@rpath/` fans out over the rpath stack first.
    if let Some(rest) = raw.strip_prefix("@rpath/") {
        let mut out = Vec::new();
        for rp in &env.rpaths {
            let base = expand_loader_vars(rp, env);
            out.push(join_sysroot(&env.sysroot, &format!("{base}/{rest}")));
        }
        return out;
    }
    if raw.contains("@executable_path") || raw.contains("@loader_path") {
        let expanded = expand_loader_vars(raw, env);
        if expanded.starts_with('/') {
            return vec![join_sysroot(&env.sysroot, &expanded)];
        }
        return vec![expanded];
    }
    if raw.starts_with('/') {
        return vec![join_sysroot(&env.sysroot, raw)];
    }
    // Bare name: DYLD_LIBRARY_PATH, then the name itself, then the
    // fallback path, then the compiled-in defaults.
    let mut out = Vec::new();
    for dir in &env.dyld_library_path {
        out.push(format!("{dir}/{raw}"));
    }
    out.push(raw.to_string());
    for dir in &env.dyld_fallback_library_path {
        out.push(format!("{dir}/{raw}"));
    }
    for dir in ["/usr/lib", "/usr/local/lib"] {
        out.push(join_sysroot(&env.sysroot, &format!("{dir}/{raw}")));
    }
    // Also try the bare file name under the loader's own directory:
    // fixtures link sibling dylibs by relative name.
    out.push(format!("{}/{raw}", dirname(&env.loader_path)));
    out
}

/// File stem used to attribute dyld-style `lib/symbol` targets.
pub fn dylib_stem(path: &str) -> &str {
    let base = basename(path);
    match base.rfind('.') {
        Some(i) => &base[..i],
        None => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env() -> SearchEnv {
        SearchEnv {
            executable_path: "/Applications/Q.app/Contents/MacOS/Q".to_string(),
            loader_path: "/Applications/Q.app/Contents/Frameworks/F.framework/F".to_string(),
            rpaths: ["@loader_path/../lib", "@executable_path/lib"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            dyld_library_path: vec!["/tmp/ov".to_string()],
            dyld_fallback_library_path: vec!["/tmp/fb".to_string()],
            sysroot: "/prefix/merged".to_string(),
        }
    }

    #[test]
    fn closure_rules_in_priority_order() {
        let clean = ClosureInputs {
            cache_present: true,
            ..ClosureInputs::default()
        };
        assert_eq!(decide_closure(&clean).mode, CacheMode::Prebuilt);
        let nocache = ClosureInputs::default();
        let d = decide_closure(&nocache);
        assert_eq!(d.mode, CacheMode::JustInTime);
        assert!(d.reason.contains("no shared cache"));
        let stale = ClosureInputs {
            cache_present: true,
            cache_stale: true,
            ..ClosureInputs::default()
        };
        assert_eq!(decide_closure(&stale).mode, CacheMode::JustInTime);
        let ov = ClosureInputs {
            cache_present: true,
            dyld_overrides: vec!["DYLD_LIBRARY_PATH"],
            ..ClosureInputs::default()
        };
        assert_eq!(decide_closure(&ov).mode, CacheMode::JustInTime);
        let ins = ClosureInputs {
            cache_present: true,
            interposing: true,
            ..ClosureInputs::default()
        };
        assert_eq!(decide_closure(&ins).mode, CacheMode::JustInTime);
        // Forcing still loses to staleness/overrides (checked above
        // only via priority), and wins on a clean launch.
        let forced = ClosureInputs {
            cache_present: true,
            force_closures: true,
            ..ClosureInputs::default()
        };
        let d = decide_closure(&forced);
        assert_eq!(d.mode, CacheMode::Prebuilt);
        assert!(d.reason.contains("forces"));
    }

    #[test]
    fn rpath_fans_out_in_order() {
        let c = resolve_dylib("@rpath/libX.dylib", &env());
        assert_eq!(
            c,
            vec![
                "/prefix/merged/Applications/Q.app/Contents/Frameworks/F.framework/../lib/libX.dylib",
                "/prefix/merged/Applications/Q.app/Contents/MacOS/lib/libX.dylib",
            ]
        );
    }

    #[test]
    fn loader_vars_expand_to_dirs() {
        let c = resolve_dylib("@loader_path/Plug.dylib", &env());
        assert_eq!(
            c,
            vec!["/prefix/merged/Applications/Q.app/Contents/Frameworks/F.framework/Plug.dylib"]
        );
        let c = resolve_dylib("@executable_path/Q.helper", &env());
        assert_eq!(
            c,
            vec!["/prefix/merged/Applications/Q.app/Contents/MacOS/Q.helper"]
        );
    }

    #[test]
    fn absolute_paths_join_sysroot() {
        let c = resolve_dylib("/usr/lib/libSystem.B.dylib", &env());
        assert_eq!(c, vec!["/prefix/merged/usr/lib/libSystem.B.dylib"]);
    }

    #[test]
    fn bare_names_search_library_path_first() {
        let c = resolve_dylib("libBare.dylib", &env());
        assert_eq!(c[0], "/tmp/ov/libBare.dylib");
        assert_eq!(c[1], "libBare.dylib");
        assert_eq!(c[2], "/tmp/fb/libBare.dylib");
        assert!(c.contains(&"/prefix/merged/usr/lib/libBare.dylib".to_string()));
        assert!(c.last().unwrap().ends_with("/F.framework/libBare.dylib"));
    }

    #[test]
    fn stem_strips_dirs_and_extension() {
        assert_eq!(dylib_stem("/usr/lib/libSystem.B.dylib"), "libSystem.B");
        assert_eq!(dylib_stem("librich-arm64.dylib"), "librich-arm64");
        assert_eq!(dylib_stem("noext"), "noext");
    }
}
