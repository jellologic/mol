// SPDX-License-Identifier: MIT OR Apache-2.0
//! Generate the shim dylib: `cargo run -p mol-libsystem --example
//! gen_shim -- <out>`. Used for the checked-in golden
//! (`corpus/shim/libSystem.B.dylib`) and Apple-tool validation.

fn main() {
    let out = std::env::args().nth(1).expect("usage: gen_shim <out>");
    let reg = mol_libsystem::exports();
    let fat = mol_libsystem::shim::build_shim(&reg);
    std::fs::write(&out, &fat).expect("write shim");
    eprintln!("wrote {} ({} exports)", out, reg.len());
}
