// SPDX-License-Identifier: MIT OR Apache-2.0
//! Generate the V2 ulp envelope: `cargo run -p mol-libsystem
//! --example gen_envelope -- <trace.csv> <envelope.json>`. Runs on
//! Linux; evaluates our wrappers over the Apple oracle trace and
//! records per-function max ulp distance. Exact-set functions must
//! be bit-identical and errno must stay untouched, or generation
//! fails. Re-run when the host libm changes (the checked-in
//! envelope pins the measured libm's behavior).

#[cfg(target_os = "linux")]
fn call(func: &str, in1: u64, in2: Option<u64>) -> f64 {
    use mol_libsystem::math;
    let x = f64::from_bits(in1);
    match func {
        "sin" => math::mol_sin(x),
        "cos" => math::mol_cos(x),
        "exp" => math::mol_exp(x),
        "log" => math::mol_log(x),
        "sqrt" => math::mol_sqrt(x),
        "floor" => math::mol_floor(x),
        "ceil" => math::mol_ceil(x),
        "fabs" => math::mol_fabs(x),
        "pow" => math::mol_pow(x, f64::from_bits(in2.unwrap())),
        "fmod" => math::mol_fmod(x, f64::from_bits(in2.unwrap())),
        f => panic!("unknown func {f}"),
    }
}

#[cfg(target_os = "linux")]
fn run(trace: &str, out: &str, margin: u64) {
    use mol_libsystem::ulp::{is_nan_bits, parse_trace, ulp_dist, EXACT_FNS};
    use std::collections::BTreeMap;

    let rows = parse_trace(std::path::Path::new(trace)).expect("parse trace");
    let mut max: BTreeMap<String, u64> = BTreeMap::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for r in &rows {
        let mine = call(&r.func, r.in1, r.in2);
        let mb = mine.to_bits();
        *counts.entry(r.func.clone()).or_default() += 1;
        if is_nan_bits(r.out) {
            assert!(mine.is_nan(), "{}: oracle NaN, ours {mine}", r.func);
            continue;
        }
        assert!(!is_nan_bits(mb), "{}: ours NaN, oracle {:x}", r.func, r.out);
        if EXACT_FNS.contains(&r.func.as_str()) {
            assert_eq!(
                mb, r.out,
                "{}: exact-set mismatch in {:x}: ours {mb:x}, oracle {:x}",
                r.func, r.in1, r.out
            );
            continue;
        }
        let d = ulp_dist(mb, r.out);
        let e = max.entry(r.func.clone()).or_default();
        *e = (*e).max(d);
    }
    // errno untouched across every call (oracle: Apple never sets it).
    mol_syscall::errcell::set(41);
    for r in &rows {
        let _ = call(&r.func, r.in1, r.in2);
        assert_eq!(
            mol_syscall::errcell::get(),
            41,
            "{} left errno touched",
            r.func
        );
    }
    mol_syscall::errcell::set(0);
    let mut js = String::from("{\n");
    for (i, (f, m)) in max.iter().enumerate() {
        js.push_str(&format!(
            "  \"{}\": {}{}\n",
            f,
            m + margin,
            if i + 1 < max.len() { "," } else { "" }
        ));
    }
    js.push_str(&format!(
        "  \"_meta\": {{\"trace\": \"{}\", \"rows\": {}, \"arch\": \"{}\", \"margin\": {margin}, \"exact\": [{}]}}\n}}\n",
        trace,
        rows.len(),
        std::env::consts::ARCH,
        EXACT_FNS
            .iter()
            .map(|f| format!("\"{f}\""))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    std::fs::write(out, &js).expect("write envelope");
    eprintln!("trace rows: {} in {trace}", rows.len());
    for (f, m) in &max {
        eprintln!(
            "  {f}: max {m} ulps over {} samples (bound {})",
            counts[f],
            m + margin
        );
    }
    for f in EXACT_FNS {
        eprintln!("  {f}: exact over {} samples", counts[*f]);
    }
    eprintln!("wrote {out}");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(trace), Some(out)) = (args.next(), args.next()) else {
        eprintln!("usage: gen_envelope <trace.csv> <envelope.json> [--margin N]");
        std::process::exit(2);
    };
    let mut margin = 1u64;
    while let Some(a) = args.next() {
        if a == "--margin" {
            margin = args
                .next()
                .expect("--margin needs a value")
                .parse()
                .expect("margin");
        } else {
            eprintln!("unknown arg '{a}'");
            std::process::exit(2);
        }
    }
    #[cfg(target_os = "linux")]
    run(&trace, &out, margin);
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (trace, out, margin);
        eprintln!("gen_envelope runs on Linux (it measures the Linux host libm)");
        std::process::exit(2);
    }
}
