// SPDX-License-Identifier: MIT OR Apache-2.0
//! Differential test: our parse vs `otool -l` + `dyld_info` + `codesign`
//! on the committed corpus (P1.1.V1). macOS-only; Linux CI covers the
//! same fixtures via byte-exact goldens (`golden.rs`).
#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::process::Command;

use mol_macho::header::{CPU_TYPE_ARM64, CPU_TYPE_X86_64};
use mol_macho::load::LoadCommand;

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("corpus")
        .join("macho")
}

fn run(cmd: &str, args: &[&str]) -> String {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("cannot run {cmd}: {e}"));
    assert!(
        out.status.success(),
        "{cmd} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("non-utf8 oracle output")
}

fn num(s: &str) -> u64 {
    let s = s.trim();
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(h, 16).unwrap()
    } else {
        s.parse().unwrap()
    }
}

#[derive(Debug, Default)]
struct OSection {
    name: String,
    seg: String,
    addr: u64,
    size: u64,
    offset: u64,
    flags: u64,
}

#[derive(Debug, Default)]
struct OSegment {
    name: String,
    vmaddr: u64,
    vmsize: u64,
    fileoff: u64,
    filesize: u64,
    maxprot: u64,
    initprot: u64,
    flags: u64,
    sections: Vec<OSection>,
}

/// Parse `otool -l` into per-command key/value blocks.
fn otool_blocks(path: &Path, arch: Option<&str>) -> Vec<(String, Vec<(String, String)>)> {
    let mut args = vec!["-l"];
    if let Some(a) = arch {
        args.push("-arch");
        args.push(a);
    }
    args.push(path.to_str().unwrap());
    let text = run("otool", &args);
    let mut blocks = Vec::new();
    let mut cur: Option<(String, Vec<(String, String)>)> = None;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("Load command") {
            if let Some(b) = cur.take() {
                blocks.push(b);
            }
            cur = Some((String::new(), Vec::new()));
        } else if t == "Section" {
            if let Some((_, pairs)) = &mut cur {
                pairs.push(("Section".to_string(), String::new()));
            }
        } else if let Some(block) = &mut cur {
            if let Some((k, v)) = t.split_once(char::is_whitespace) {
                if k == "cmd" {
                    block.0 = v.trim().to_string();
                }
                block.1.push((k.to_string(), v.trim().to_string()));
            }
        }
    }
    if let Some(b) = cur.take() {
        blocks.push(b);
    }
    assert!(!blocks.is_empty(), "no load commands parsed for {path:?}");
    blocks
}

fn otool_segments(blocks: &[(String, Vec<(String, String)>)]) -> Vec<OSegment> {
    let mut out = Vec::new();
    for (cmd, pairs) in blocks {
        if cmd != "LC_SEGMENT_64" {
            continue;
        }
        let mut seg = OSegment::default();
        let mut cur_sect: Option<OSection> = None;
        let flush = |seg: &mut OSegment, cur: &mut Option<OSection>| {
            if let Some(s) = cur.take() {
                seg.sections.push(s);
            }
        };
        for (k, v) in pairs {
            match k.as_str() {
                "Section" => flush(&mut seg, &mut cur_sect),
                "segname" => {
                    if let Some(s) = cur_sect.as_mut() {
                        s.seg = v.clone();
                    } else {
                        seg.name = v.clone();
                    }
                }
                "sectname" => {
                    cur_sect = Some(OSection {
                        name: v.clone(),
                        ..OSection::default()
                    });
                }
                "vmaddr" => seg.vmaddr = num(v),
                "vmsize" => seg.vmsize = num(v),
                "fileoff" => seg.fileoff = num(v),
                "filesize" => seg.filesize = num(v),
                "maxprot" => seg.maxprot = num(v),
                "initprot" => seg.initprot = num(v),
                "flags" => {
                    if let Some(s) = cur_sect.as_mut() {
                        s.flags = num(v);
                    } else {
                        seg.flags = num(v);
                    }
                }
                "addr" => cur_sect.as_mut().unwrap().addr = num(v),
                "size" => cur_sect.as_mut().unwrap().size = num(v),
                "offset" => cur_sect.as_mut().unwrap().offset = num(v),
                _ => {}
            }
        }
        flush(&mut seg, &mut cur_sect);
        out.push(seg);
    }
    out
}

fn block_field(blocks: &[(String, Vec<(String, String)>)], cmd: &str, key: &str) -> Option<String> {
    blocks
        .iter()
        .find(|(c, _)| c == cmd)
        .and_then(|(_, pairs)| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()))
}

fn dylib_name(field: &str) -> &str {
    field.split_whitespace().next().unwrap()
}

fn parse_version(s: &str) -> (u16, u8, u8) {
    let mut parts = s.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    (
        parts.next().unwrap_or(0) as u16,
        parts.next().unwrap_or(0) as u8,
        parts.next().unwrap_or(0) as u8,
    )
}

type DyldSections = (Vec<(u64, String)>, Vec<(u64, String, String)>);
type VersionPair = ((u16, u8, u8), (u16, u8, u8));

fn dyld_sections(path: &Path, arch: Option<&str>) -> DyldSections {
    let mut args = Vec::new();
    if let Some(a) = arch {
        args.push("-arch");
        args.push(a);
    }
    args.push("-fixups");
    args.push("-exports");
    args.push(path.to_str().unwrap());
    let text = run("dyld_info", &args);
    let mut exports = Vec::new();
    let mut fixups = Vec::new();
    let mut section = "";
    for line in text.lines() {
        let t = line.trim();
        if t == "-exports:" {
            section = "exports";
        } else if t == "-fixups:" {
            section = "fixups";
        } else if t.starts_with("offset") || t.starts_with("segment") || t.is_empty() {
            continue;
        } else if section == "exports" {
            let mut cols = t.split_whitespace();
            let off = num(cols.next().unwrap());
            let sym = cols.next().unwrap().to_string();
            exports.push((off, sym));
        } else if section == "fixups" {
            // segment section address type target...
            let cols: Vec<&str> = t.split_whitespace().collect();
            assert!(cols.len() >= 5, "short fixup row: {t}");
            fixups.push((num(cols[2]), cols[3].to_string(), cols[4..].join(" ")));
        }
    }
    (exports, fixups)
}

fn codesign_id(path: &Path) -> (String, Option<String>) {
    let out = Command::new("codesign")
        .args(["-dvvv", path.to_str().unwrap()])
        .output()
        .expect("cannot run codesign");
    let text =
        String::from_utf8_lossy(&out.stderr).into_owned() + &String::from_utf8_lossy(&out.stdout);
    let mut ident = None;
    let mut team = None;
    for line in text.lines() {
        if let Some(v) = line.trim().strip_prefix("Identifier=") {
            ident = Some(v.to_string());
        }
        if let Some(v) = line.trim().strip_prefix("TeamIdentifier=") {
            if v != "not set" {
                team = Some(v.to_string());
            }
        }
    }
    (ident.expect("no Identifier in codesign output"), team)
}

/// Differential-check one (file, arch) pair against all three oracles.
fn check_fixture(file: &str, arch: Option<(&str, u32, u32)>) {
    let path = corpus().join(file);
    let bytes = std::fs::read(&path).unwrap();
    let parsed = mol_macho::parse(&bytes).unwrap();
    let (arch_name, cputype, cpusubtype) = arch.unwrap_or(("arm64", CPU_TYPE_ARM64, 0));
    // Thin fixtures ignore the arch request when it mismatches (the
    // dump helper does the same); fat fixtures must select exactly.
    let img = match parsed.select(cputype, cpusubtype) {
        Ok(img) => img,
        Err(_) => match &parsed {
            mol_macho::MachFile::Thin(img) => *img,
            mol_macho::MachFile::Fat(_) => panic!("{file}: no slice for {arch_name}"),
        },
    };
    let oracle_arch = match &parsed {
        mol_macho::MachFile::Fat(_) => Some(arch_name),
        mol_macho::MachFile::Thin(_) => None,
    };
    let decoded = img.decoded().unwrap();
    let map = img.load_map(0).unwrap();
    let tag = format!("{file}[{arch_name}]");

    // --- segments + sections vs otool ---
    let blocks = otool_blocks(&path, oracle_arch);
    let osegs = otool_segments(&blocks);
    assert_eq!(map.segments.len(), osegs.len(), "{tag}: segment count");
    for (ours, oracle) in map.segments.iter().zip(osegs.iter()) {
        assert_eq!(ours.segname, oracle.name, "{tag}: segname");
        for (field, a, b) in [
            ("vmaddr", ours.vmaddr, oracle.vmaddr),
            ("vmsize", ours.vmsize, oracle.vmsize),
            ("fileoff", ours.fileoff, oracle.fileoff),
            ("filesize", ours.filesize, oracle.filesize),
            ("maxprot", ours.maxprot as u64, oracle.maxprot),
            ("initprot", ours.initprot as u64, oracle.initprot),
            ("flags", ours.flags as u64, oracle.flags),
        ] {
            assert_eq!(a, b, "{tag}: {} {}", ours.segname, field);
        }
        assert_eq!(
            ours.sections.len(),
            oracle.sections.len(),
            "{tag}: {} section count",
            ours.segname
        );
        for (os, rs) in ours.sections.iter().zip(oracle.sections.iter()) {
            assert_eq!(os.sectname, rs.name, "{tag}: sectname");
            assert_eq!(ours.segname, rs.seg, "{tag}: section seg");
            assert_eq!(os.addr, rs.addr, "{tag}: {} addr", os.sectname);
            assert_eq!(os.size, rs.size, "{tag}: {} size", os.sectname);
            assert_eq!(os.offset as u64, rs.offset, "{tag}: {} offset", os.sectname);
            assert_eq!(os.flags as u64, rs.flags, "{tag}: {} flags", os.sectname);
        }
    }

    // --- LC_MAIN vs otool ---
    let o_entry = block_field(&blocks, "LC_MAIN", "entryoff").map(|s| num(&s));
    let ours_main = decoded.iter().find_map(|c| match c {
        LoadCommand::EntryPoint(e) => Some(e),
        _ => None,
    });
    match (o_entry, ours_main, &map.entry) {
        (Some(off), Some(e), Some(entry)) => {
            assert_eq!(e.entryoff, off, "{tag}: entryoff");
            // Resolved vmaddr must land inside a segment at entryoff.
            let seg = map
                .segments
                .iter()
                .find(|s| off >= s.fileoff && off < s.fileoff + s.filesize)
                .unwrap();
            assert_eq!(
                entry.vmaddr,
                seg.vmaddr + (off - seg.fileoff),
                "{tag}: entry vmaddr"
            );
        }
        (None, None, None) => {}
        other => panic!("{tag}: entry mismatch: {other:?}"),
    }

    // --- dylibs + dylinker vs otool (names in file order) ---
    let otool_text = {
        let mut args = vec!["-l"];
        if let Some(a) = oracle_arch {
            args.push("-arch");
            args.push(a);
        }
        args.push(path.to_str().unwrap());
        run("otool", &args)
    };
    let mut o_names = Vec::new();
    for line in otool_text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("name ") {
            o_names.push(dylib_name(rest).to_string());
        }
    }
    // Dylibs carry no LC_LOAD_DYLINKER: only split off the linker
    // when otool saw one.
    let has_linker = blocks.iter().any(|(c, _)| c == "LC_LOAD_DYLINKER");
    let (o_linker, o_libs): (Option<&String>, &[String]) = if has_linker {
        let (first, rest) = o_names.split_first().expect("linker name");
        (Some(first), rest)
    } else {
        (None, &o_names)
    };
    assert_eq!(map.dylinker.as_ref(), o_linker, "{tag}: dylinker");
    let our_libs: Vec<&str> = map.dylibs.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(our_libs, *o_libs, "{tag}: dylib order");
    // Versions: otool splits "current version 1.2.3" into
    // ("current", "version 1.2.3").
    let o_vers: Vec<VersionPair> = blocks
        .iter()
        .filter(|(c, _)| c == "LC_LOAD_DYLIB" || c == "LC_ID_DYLIB")
        .map(|(_, pairs)| {
            let ver = |k: &str| {
                pairs
                    .iter()
                    .find(|(kk, _)| kk == k)
                    .and_then(|(_, v)| v.strip_prefix("version "))
                    .map(parse_version)
                    .unwrap()
            };
            (ver("current"), ver("compatibility"))
        })
        .collect();
    let our_vers: Vec<VersionPair> = map.dylibs.iter().map(|d| (d.current, d.compat)).collect();
    assert_eq!(our_vers, o_vers, "{tag}: dylib versions");

    // --- uuid vs otool ---
    let o_uuid = block_field(&blocks, "LC_UUID", "uuid").map(|s| s.to_lowercase());
    assert_eq!(
        map.uuid
            .as_ref()
            .map(|u| { u.iter().map(|b| format!("{b:02x}")).collect::<String>() }),
        o_uuid.map(|s| s.replace('-', "")),
        "{tag}: uuid"
    );

    // --- build vs otool ---
    let o_build = blocks
        .iter()
        .find(|(c, _)| c == "LC_BUILD_VERSION")
        .map(|(_, pairs)| {
            let get = |k: &str| pairs.iter().find(|(kk, _)| kk == k).unwrap().1.clone();
            (
                get("platform").parse::<u32>().unwrap(),
                parse_version(&get("minos")),
                parse_version(&get("sdk")),
            )
        });
    assert_eq!(
        map.build.map(|(p, minos, sdk)| (
            p,
            mol_macho::load::decode_version(minos),
            mol_macho::load::decode_version(sdk)
        )),
        o_build,
        "{tag}: build"
    );

    // --- codesign ident vs codesign(1) ---
    // Cross-compiled x86_64 slices ship unsigned (no LC_CODE_SIGNATURE).
    let o_signed = blocks.iter().any(|(c, _)| c == "LC_CODE_SIGNATURE");
    if o_signed {
        let (ident, team) = codesign_id(&path);
        let cs = map
            .codesign
            .as_ref()
            .unwrap_or_else(|| panic!("{tag}: no codesign parsed"));
        assert_eq!(cs.ident, ident, "{tag}: codesign ident");
        assert_eq!(cs.team_id, team, "{tag}: team id");
    } else {
        assert!(
            map.codesign.is_none(),
            "{tag}: unsigned but parsed codesign"
        );
    }

    // --- exports + fixups vs dyld_info ---
    let (o_exports, o_fixups) = dyld_sections(&path, oracle_arch);
    let mut our_exports: Vec<(u64, String)> = map
        .exports
        .iter()
        .map(|e| (e.address, e.name.clone()))
        .collect();
    our_exports.sort();
    let mut o_sorted = o_exports.clone();
    o_sorted.sort();
    assert_eq!(our_exports, o_sorted, "{tag}: exports");
    assert_eq!(map.fixups.len(), o_fixups.len(), "{tag}: fixup count");
    for (addr, kind, target) in &o_fixups {
        let ours = map
            .fixups
            .iter()
            .find(|f| f.vmaddr == *addr)
            .unwrap_or_else(|| panic!("{tag}: missing fixup at {addr:#x}"));
        if kind == "bind" {
            assert!(
                ours.detail().contains("bind"),
                "{tag}: {addr:#x} kind: {}",
                ours.detail()
            );
            let sym = target.rsplit('/').next().unwrap();
            let lib_tok = target.rsplit('/').nth(1).unwrap_or("");
            assert!(
                ours.detail().contains(sym),
                "{tag}: {addr:#x} symbol: {}",
                ours.detail()
            );
            // Ordinal must index the dylib whose file stem matches.
            let ord: usize = ours
                .detail()
                .split_whitespace()
                .find_map(|w| w.strip_prefix("ord=").and_then(|n| n.parse().ok()))
                .expect("ord= in bind detail");
            let _ = ord;
            let lib_ord: usize = ours
                .detail()
                .split_whitespace()
                .find_map(|w| w.strip_prefix("lib=").and_then(|n| n.parse().ok()))
                .expect("lib= in bind detail");
            assert!(
                lib_ord >= 1 && lib_ord <= map.dylibs.len(),
                "{tag}: lib ordinal range"
            );
            let stem = Path::new(&map.dylibs[lib_ord - 1].name)
                .file_stem()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string();
            assert!(
                stem == lib_tok || stem.starts_with(lib_tok) || lib_tok.starts_with(&stem),
                "{tag}: {addr:#x} lib token {lib_tok} vs {stem}"
            );
        } else {
            assert!(
                ours.detail().contains("rebase"),
                "{tag}: {addr:#x} kind: {}",
                ours.detail()
            );
        }
    }
}

#[test]
fn oracle_hello_arm64() {
    check_fixture("hello-arm64", None);
}

#[test]
fn oracle_hello_x86_64() {
    check_fixture("hello-x86_64", Some(("x86_64", CPU_TYPE_X86_64, 3)));
}

#[test]
fn oracle_hello_fat_arm64() {
    check_fixture("hello-fat", Some(("arm64", CPU_TYPE_ARM64, 0)));
}

#[test]
fn oracle_hello_fat_x86_64() {
    check_fixture("hello-fat", Some(("x86_64", CPU_TYPE_X86_64, 3)));
}

#[test]
fn oracle_rich_arm64() {
    check_fixture("rich-arm64", None);
}

#[test]
fn oracle_rich_x86_64() {
    check_fixture("rich-x86_64", Some(("x86_64", CPU_TYPE_X86_64, 3)));
}

#[test]
fn oracle_rich_fat_arm64() {
    check_fixture("rich-fat", Some(("arm64", CPU_TYPE_ARM64, 0)));
}

#[test]
fn oracle_rich_fat_x86_64() {
    check_fixture("rich-fat", Some(("x86_64", CPU_TYPE_X86_64, 3)));
}

#[test]
fn oracle_librich_arm64() {
    check_fixture("librich-arm64.dylib", None);
}

#[test]
fn oracle_librich_x86_64() {
    check_fixture("librich-x86_64.dylib", Some(("x86_64", CPU_TYPE_X86_64, 3)));
}

#[test]
fn oracle_librich_fat_arm64() {
    check_fixture("librich-fat.dylib", Some(("arm64", CPU_TYPE_ARM64, 0)));
}

#[test]
fn oracle_librich_fat_x86_64() {
    check_fixture("librich-fat.dylib", Some(("x86_64", CPU_TYPE_X86_64, 3)));
}
