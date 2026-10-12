// SPDX-License-Identifier: MIT OR Apache-2.0
//! Synthetic import-calling guests for the syscall E2E suites.
//!
//! Portable (no `cfg(linux)`): Linux runners execute the guests,
//! macOS runners differential-check the emission against
//! `dyld_info`. Layout is fixed so tests can precompute addresses:
//! `__TEXT` at `0x1000_0000` (code at file `0x1000`),
//! `__DATA` at `0x1000_4000` (GOT first, then guest data).
//! Chained fixups use format 6 (what real `ld` emits on both
//! arches) with imports format 1.

pub const TEXT_VMADDR: u64 = 0x1000_0000;
pub const DATA_VMADDR: u64 = 0x1000_4000;
pub const CODE_OFF: u64 = 0x1000;
pub const CHAIN_OFF: u64 = 0x400;
pub const SHIM_PATH: &str = "/usr/lib/libSystem.B.dylib";

// Cross-arch surface: each runner uses its own pair.
#[allow(dead_code)]
pub const CPU_TYPE_ARM64: u32 = 0x0100_000c;
#[allow(dead_code)]
pub const CPU_TYPE_X86_64: u32 = 0x0100_0007;
#[allow(dead_code)]
pub const CPU_SUBTYPE_ARM64_ALL: u32 = 0;
#[allow(dead_code)]
pub const CPU_SUBTYPE_X86_64_ALL: u32 = 3;

/// Code address for a buffer offset.
pub fn code_at(off: usize) -> u64 {
    TEXT_VMADDR + CODE_OFF + off as u64
}

/// GOT slot address for import `i`.
pub fn got_slot(i: usize) -> u64 {
    DATA_VMADDR + i as u64 * 8
}

/// Guest-data address for a data offset (after the GOT).
pub fn data_at(off: usize, nimports: usize) -> u64 {
    DATA_VMADDR + nimports as u64 * 8 + off as u64
}

#[cfg(target_arch = "aarch64")]
fn w32(v: u32, out: &mut Vec<u8>) {
    out.extend_from_slice(&v.to_le_bytes());
}

// --- arm64 emitters (site = this instruction's vmaddr) ---

#[cfg(target_arch = "aarch64")]
pub fn a_movz(out: &mut Vec<u8>, rd: u32, imm16: u16, hw: u32, is64: bool) {
    let sf = if is64 { 1u32 } else { 0 };
    w32(
        sf << 31 | 0x5280_0000 | hw << 21 | (imm16 as u32) << 5 | rd,
        out,
    );
}

#[cfg(target_arch = "aarch64")]
pub fn a_movk_x(out: &mut Vec<u8>, rd: u32, imm16: u16, hw: u32) {
    w32(0xf280_0000 | hw << 21 | (imm16 as u32) << 5 | rd, out);
}

#[cfg(target_arch = "aarch64")]
pub fn a_movk_w(out: &mut Vec<u8>, rd: u32, imm16: u16, hw: u32) {
    w32(0x7280_0000 | hw << 21 | (imm16 as u32) << 5 | rd, out);
}

/// movn xd, #0 → all ones (-1).
#[cfg(target_arch = "aarch64")]
pub fn a_movn_x0(out: &mut Vec<u8>, rd: u32) {
    w32(0x9280_0000 | rd, out);
}

#[cfg(target_arch = "aarch64")]
pub fn a_mov_abs(out: &mut Vec<u8>, reg: u32, val: u64) {
    let parts = [
        (val & 0xffff) as u16,
        ((val >> 16) & 0xffff) as u16,
        ((val >> 32) & 0xffff) as u16,
        ((val >> 48) & 0xffff) as u16,
    ];
    let first = parts.iter().position(|&p| p != 0).unwrap_or(0);
    a_movz(out, reg, parts[first], first as u32, true);
    for (hw, &p) in parts.iter().enumerate() {
        if hw != first && p != 0 {
            a_movk_x(out, reg, p, hw as u32);
        }
    }
}

#[cfg(target_arch = "aarch64")]
pub fn a_adr(out: &mut Vec<u8>, rd: u32, site: u64, target: u64) {
    let off = target.wrapping_sub(site) as i64;
    assert!((-(1 << 20)..(1 << 20)).contains(&off), "adr range");
    let u = (off as u32) & 0x1f_ffff;
    w32(
        0x1000_0000 | ((u & 3) << 29) | (((u >> 2) & 0x7_ffff) << 5) | rd,
        out,
    );
}

#[cfg(target_arch = "aarch64")]
pub fn a_adrp(out: &mut Vec<u8>, rd: u32, site: u64, target: u64) {
    let delta = ((target & !0xfff).wrapping_sub(site & !0xfff) >> 12) as i64 as u32;
    w32(
        0x9000_0000 | ((delta & 3) << 29) | (((delta >> 2) & 0x7_ffff) << 5) | rd,
        out,
    );
}

#[cfg(target_arch = "aarch64")]
pub fn a_add_imm12(out: &mut Vec<u8>, rd: u32, rn: u32, imm12: u32) {
    assert!(imm12 < 0x1000);
    w32(0x9100_0000 | imm12 << 10 | rn << 5 | rd, out);
}

/// Call through a GOT slot (clobbers x16).
#[cfg(target_arch = "aarch64")]
pub fn a_call_got(out: &mut Vec<u8>, site: u64, slot: u64) {
    a_adrp(out, 16, site, slot);
    let off = (slot & 0xfff) as u32;
    assert!(off.is_multiple_of(8));
    w32(0xf940_0000 | ((off >> 3) << 10) | (16 << 5) | 16, out); // ldr x16, [x16, #off]
    w32(0xd63f_0000 | (16 << 5), out); // blr x16
}

#[cfg(target_arch = "aarch64")]
pub fn a_ldr_x(out: &mut Vec<u8>, rd: u32, rn: u32) {
    w32(0xf940_0000 | rn << 5 | rd, out);
}

#[cfg(target_arch = "aarch64")]
pub fn a_ldr_w(out: &mut Vec<u8>, rd: u32, rn: u32) {
    w32(0xb940_0000 | rn << 5 | rd, out);
}

#[cfg(target_arch = "aarch64")]
pub fn a_str_x(out: &mut Vec<u8>, rd: u32, rn: u32) {
    w32(0xf900_0000 | rn << 5 | rd, out);
}

#[cfg(target_arch = "aarch64")]
pub fn a_str_w(out: &mut Vec<u8>, rd: u32, rn: u32) {
    w32(0xb900_0000 | rn << 5 | rd, out);
}

#[cfg(target_arch = "aarch64")]
pub fn a_ret(out: &mut Vec<u8>) {
    w32(0xd65f_03c0, out);
}

/// ldr dd, [xn].
#[cfg(target_arch = "aarch64")]
pub fn a_ldr_d(out: &mut Vec<u8>, rd: u32, rn: u32) {
    w32(0xfd40_0000 | rn << 5 | rd, out);
}

/// Compare wn with -1 (cmn wn, #1).
#[cfg(target_arch = "aarch64")]
pub fn a_cmn_w1(out: &mut Vec<u8>, rn: u32) {
    w32(0x3100_0000 | (1 << 10) | rn << 5 | 0x1f, out);
}

/// Compare xn with -1.
#[cfg(target_arch = "aarch64")]
pub fn a_cmn_x1(out: &mut Vec<u8>, rn: u32) {
    w32(0xb100_0000 | (1 << 10) | rn << 5 | 0x1f, out);
}

#[cfg(target_arch = "aarch64")]
pub fn a_cmp_w_imm(out: &mut Vec<u8>, rn: u32, imm: u32) {
    assert!(imm < 0x1000);
    w32(0x7100_0000 | imm << 10 | rn << 5 | 0x1f, out);
}

#[cfg(target_arch = "aarch64")]
pub fn a_cmp_w_reg(out: &mut Vec<u8>, rn: u32, rm: u32) {
    w32(0x6b00_001f | rm << 16 | rn << 5, out);
}

/// mov xd, xm (ORR xd, xzr, xm: source lives in Rm).
#[cfg(target_arch = "aarch64")]
pub fn a_mov_x(out: &mut Vec<u8>, rd: u32, rm: u32) {
    w32(0xaa00_03e0 | rm << 16 | rd, out);
}

#[cfg(target_arch = "aarch64")]
pub fn a_mov_w(out: &mut Vec<u8>, rd: u32, rn: u32) {
    w32(0x2a00_03e0 | rn << 16 | rd, out);
}

#[cfg(target_arch = "aarch64")]
#[allow(clippy::manual_is_multiple_of)] // i64 has no is_multiple_of
pub fn enc_b_cond(cond: u32, site: u64, target: u64) -> u32 {
    let off = target.wrapping_sub(site) as i64;
    assert!(off % 4 == 0 && (-(1 << 20)..(1 << 20)).contains(&off));
    0x5400_0000 | (((off >> 2) as u32 & 0x7_ffff) << 5) | cond
}

/// b.cond: eq=0 ne=1 lt=0xb ge=0xa.
#[cfg(target_arch = "aarch64")]
pub fn a_b_cond(out: &mut Vec<u8>, cond: u32, site: u64, target: u64) {
    w32(enc_b_cond(cond, site, target), out);
}

#[cfg(target_arch = "aarch64")]
#[allow(clippy::manual_is_multiple_of)] // i64 has no is_multiple_of
pub fn enc_b(site: u64, target: u64) -> u32 {
    let off = target.wrapping_sub(site) as i64;
    assert!(off % 4 == 0 && (-(1 << 27)..(1 << 27)).contains(&off));
    0x1400_0000 | ((off >> 2) as u32 & 0x3ff_ffff)
}

#[cfg(target_arch = "aarch64")]
pub fn a_b(out: &mut Vec<u8>, site: u64, target: u64) {
    w32(enc_b(site, target), out);
}

#[cfg(target_arch = "aarch64")]
#[allow(clippy::manual_is_multiple_of)] // i64 has no is_multiple_of
pub fn enc_cbnz_w(rt: u32, site: u64, target: u64) -> u32 {
    let off = target.wrapping_sub(site) as i64;
    assert!(off % 4 == 0 && (-(1 << 20)..(1 << 20)).contains(&off));
    0x3500_0000 | (((off >> 2) as u32 & 0x7_ffff) << 5) | rt
}

#[cfg(target_arch = "aarch64")]
pub fn a_cbnz_w(out: &mut Vec<u8>, rt: u32, site: u64, target: u64) {
    w32(enc_cbnz_w(rt, site, target), out);
}

/// Rewrite a previously emitted branch word (two-pass assembly).
#[cfg(target_arch = "aarch64")]
pub fn patch_u32(code: &mut [u8], pos: usize, word: u32) {
    code[pos..pos + 4].copy_from_slice(&word.to_le_bytes());
}

#[cfg(target_arch = "aarch64")]
pub fn a_subs_x_imm(out: &mut Vec<u8>, rd: u32, rn: u32, imm: u32) {
    assert!(imm < 0x1000);
    w32(0xf100_0000 | imm << 10 | rn << 5 | rd, out);
}

/// UBFM wd, wn, #immr, #imms (lsr/uxtb identities).
#[cfg(target_arch = "aarch64")]
pub fn a_ubfm_w(out: &mut Vec<u8>, rd: u32, rn: u32, immr: u32, imms: u32) {
    w32(0x5300_0000 | immr << 16 | imms << 10 | rn << 5 | rd, out);
}

#[cfg(target_arch = "aarch64")]
pub fn a_lsr_w8(out: &mut Vec<u8>, rd: u32, rn: u32) {
    a_ubfm_w(out, rd, rn, 8, 31);
}

#[cfg(target_arch = "aarch64")]
pub fn a_uxtb(out: &mut Vec<u8>, rd: u32, rn: u32) {
    a_ubfm_w(out, rd, rn, 0, 7);
}

/// fmov dd, xzr (zero a double).
#[cfg(target_arch = "aarch64")]
pub fn a_fmov_d_xzr(out: &mut Vec<u8>, rd: u32) {
    w32(0x9e67_0000 | (31 << 5) | rd, out);
}

/// fcmp dn, #0.0.
#[cfg(target_arch = "aarch64")]
pub fn a_fcmp_d0(out: &mut Vec<u8>, rn: u32) {
    w32(0x1e60_2008 | rn << 5, out);
}

/// fcmp dn, dm.
#[cfg(target_arch = "aarch64")]
pub fn a_fcmp_d(out: &mut Vec<u8>, rn: u32, rm: u32) {
    w32(0x1e60_2000 | rm << 16 | rn << 5, out);
}

/// cset wd, eq.
#[cfg(target_arch = "aarch64")]
pub fn a_cset_eq(out: &mut Vec<u8>, rd: u32) {
    w32(0x1a9f_17e0 | rd, out);
}

// --- x86_64 emitters (site = this instruction's vmaddr) ---

#[cfg(target_arch = "x86_64")]
fn rip_disp(site: u64, insn_len: u64, target: u64) -> u32 {
    target.wrapping_sub(site + insn_len) as u32
}

#[cfg(target_arch = "x86_64")]
pub fn x_mov_r32_imm(out: &mut Vec<u8>, rd: u8, v: u32) {
    // eax=0 ecx=1 edx=2 ebx=3.
    assert!(rd < 4);
    out.push(0xb8 + rd);
    out.extend_from_slice(&v.to_le_bytes());
}

#[cfg(target_arch = "x86_64")]
pub fn x_mov_rdi_imm32(out: &mut Vec<u8>, v: u32) {
    out.push(0xbf);
    out.extend_from_slice(&v.to_le_bytes());
}

#[cfg(target_arch = "x86_64")]
pub fn x_mov_esi_imm32(out: &mut Vec<u8>, v: u32) {
    out.push(0xbe);
    out.extend_from_slice(&v.to_le_bytes());
}

#[cfg(target_arch = "x86_64")]
pub fn x_mov_ecx_imm32(out: &mut Vec<u8>, v: u32) {
    out.push(0xb9);
    out.extend_from_slice(&v.to_le_bytes());
}

#[cfg(target_arch = "x86_64")]
pub fn x_mov_r8d_imm32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&[0x41, 0xb8]);
    out.extend_from_slice(&v.to_le_bytes());
}

#[cfg(target_arch = "x86_64")]
pub fn x_xor_esi(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x31, 0xf6]);
}

#[cfg(target_arch = "x86_64")]
pub fn x_xor_edx(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x31, 0xd2]);
}

#[cfg(target_arch = "x86_64")]
pub fn x_xor_ecx(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x31, 0xc9]);
}

#[cfg(target_arch = "x86_64")]
pub fn x_xor_edi(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x31, 0xff]);
}

#[cfg(target_arch = "x86_64")]
pub fn x_xor_r8d(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x45, 0x31, 0xc0]);
}

#[cfg(target_arch = "x86_64")]
pub fn x_xor_r9d(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x45, 0x31, 0xc9]);
}

/// lea r64, [rip+disp]: rax=0 rcx=1 rdx=2 rbx=3 rsi=6 rdi=7 r8=8 ...
/// r15=15 (REX.R for the high half).
#[cfg(target_arch = "x86_64")]
pub fn x_lea_rip(out: &mut Vec<u8>, reg: u8, site: u64, target: u64) {
    assert!(reg < 16);
    let rex = 0x48 | ((reg >> 3) << 2);
    out.extend_from_slice(&[rex, 0x8d, 0x05 | ((reg & 7) << 3)]);
    out.extend_from_slice(&rip_disp(site, 7, target).to_le_bytes());
}

/// call *[rip+disp].
#[cfg(target_arch = "x86_64")]
pub fn x_call_got(out: &mut Vec<u8>, site: u64, slot: u64) {
    out.extend_from_slice(&[0xff, 0x15]);
    out.extend_from_slice(&rip_disp(site, 6, slot).to_le_bytes());
}

/// mov [rip+disp], rax.
#[cfg(target_arch = "x86_64")]
pub fn x_store_rip_rax(out: &mut Vec<u8>, site: u64, target: u64) {
    out.extend_from_slice(&[0x48, 0x89, 0x05]);
    out.extend_from_slice(&rip_disp(site, 7, target).to_le_bytes());
}

/// mov eax, [rip+disp].
#[cfg(target_arch = "x86_64")]
pub fn x_load_eax_rip(out: &mut Vec<u8>, site: u64, target: u64) {
    out.extend_from_slice(&[0x8b, 0x05]);
    out.extend_from_slice(&rip_disp(site, 6, target).to_le_bytes());
}

/// mov edi, [rip+disp].
#[cfg(target_arch = "x86_64")]
pub fn x_load_edi_rip(out: &mut Vec<u8>, site: u64, target: u64) {
    out.extend_from_slice(&[0x8b, 0x3d]);
    out.extend_from_slice(&rip_disp(site, 6, target).to_le_bytes());
}

/// mov eax, [rax].
#[cfg(target_arch = "x86_64")]
pub fn x_load_eax_rax(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x8b, 0x00]);
}

/// mov ebx, eax / mov eax, ebx (callee-saved fd holder).
#[cfg(target_arch = "x86_64")]
pub fn x_mov_ebx_eax(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x89, 0xc3]);
}

#[cfg(target_arch = "x86_64")]
pub fn x_mov_eax_ebx(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x8b, 0xc3]);
}

/// mov rdi, rbx.
#[cfg(target_arch = "x86_64")]
pub fn x_mov_rdi_rbx(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x48, 0x89, 0xdf]);
}

/// mov edi, ebx.
#[cfg(target_arch = "x86_64")]
pub fn x_mov_edi_ebx(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x89, 0xdf]);
}

/// mov edi, eax.
#[cfg(target_arch = "x86_64")]
pub fn x_mov_edi_eax(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x89, 0xc7]);
}

/// mov r64, r64 (low regs: ax=0 cx=1 dx=2 bx=3 sp=4 bp=5 si=6 di=7).
#[cfg(target_arch = "x86_64")]
pub fn x_mov_r64_r64(out: &mut Vec<u8>, dst: u8, src: u8) {
    assert!(dst < 8 && src < 8);
    out.extend_from_slice(&[0x48, 0x89, 0xc0 | (src << 3) | dst]);
}

/// mov dword [rip+disp], imm32.
#[cfg(target_arch = "x86_64")]
pub fn x_store_rip_imm32(out: &mut Vec<u8>, site: u64, target: u64, v: u32) {
    out.extend_from_slice(&[0xc7, 0x05]);
    out.extend_from_slice(&rip_disp(site, 10, target).to_le_bytes());
    out.extend_from_slice(&v.to_le_bytes());
}

/// mov dword [rbx], imm32.
#[cfg(target_arch = "x86_64")]
pub fn x_store_rbx_imm32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&[0xc7, 0x03]);
    out.extend_from_slice(&v.to_le_bytes());
}

/// mov eax, [rbx].
#[cfg(target_arch = "x86_64")]
pub fn x_load_eax_rbx(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x8b, 0x03]);
}

/// cmp eax, imm32.
#[cfg(target_arch = "x86_64")]
pub fn x_cmp_eax_imm32(out: &mut Vec<u8>, v: u32) {
    out.push(0x3d);
    out.extend_from_slice(&v.to_le_bytes());
}

/// cmp rax, imm32 (sign-extended).
#[cfg(target_arch = "x86_64")]
pub fn x_cmp_rax_imm32(out: &mut Vec<u8>, v: u32) {
    out.push(0x48);
    x_cmp_eax_imm32(out, v);
}

/// cmp eax, 0 (83 f8 00).
#[cfg(target_arch = "x86_64")]
pub fn x_cmp_eax_0(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x83, 0xf8, 0x00]);
}

/// cmp dword [rip+disp], imm8.
#[cfg(target_arch = "x86_64")]
pub fn x_cmp_rip_imm8(out: &mut Vec<u8>, site: u64, target: u64, v: u8) {
    out.extend_from_slice(&[0x83, 0x3d]);
    out.extend_from_slice(&rip_disp(site, 7, target).to_le_bytes());
    out.push(v);
}

/// test eax, eax.
#[cfg(target_arch = "x86_64")]
pub fn x_test_eax(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x85, 0xc0]);
}

/// Near jcc: eq=0x84 ne=0x85 lt=0x8c ge=0x8d.
#[cfg(target_arch = "x86_64")]
pub fn x_jcc(out: &mut Vec<u8>, op: u8, site: u64, target: u64) {
    out.extend_from_slice(&[0x0f, op]);
    out.extend_from_slice(&rip_disp(site, 6, target).to_le_bytes());
}

#[cfg(target_arch = "x86_64")]
pub fn x_jmp(out: &mut Vec<u8>, site: u64, target: u64) {
    out.push(0xe9);
    out.extend_from_slice(&rip_disp(site, 5, target).to_le_bytes());
}

/// Rewrite a near-jump displacement (two-pass assembly).
#[cfg(target_arch = "x86_64")]
pub fn patch_jcc(code: &mut [u8], pos: usize, site: u64, target: u64) {
    code[pos + 2..pos + 6].copy_from_slice(&rip_disp(site, 6, target).to_le_bytes());
}

/// Rewrite an unconditional near-jump displacement.
#[cfg(target_arch = "x86_64")]
pub fn patch_jmp(code: &mut [u8], pos: usize, site: u64, target: u64) {
    code[pos + 1..pos + 5].copy_from_slice(&rip_disp(site, 5, target).to_le_bytes());
}

#[cfg(target_arch = "x86_64")]
pub fn x_dec_ecx(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0xff, 0xc9]);
}

#[cfg(target_arch = "x86_64")]
pub fn x_shr_eax_8(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0xc1, 0xe8, 0x08]);
}

#[cfg(target_arch = "x86_64")]
pub fn x_and_eax_ff(out: &mut Vec<u8>) {
    // AND EAX, imm32 (the 83 /4 ib form would sign-extend 0xff to
    // 0xffff_ffff — a no-op, not a mask).
    out.push(0x25);
    out.extend_from_slice(&0xffu32.to_le_bytes());
}

/// movsd xmm0, [rip+disp].
#[cfg(target_arch = "x86_64")]
pub fn x_movsd_xmm0_rip(out: &mut Vec<u8>, site: u64, target: u64) {
    out.extend_from_slice(&[0xf2, 0x0f, 0x10, 0x05]);
    out.extend_from_slice(&rip_disp(site, 8, target).to_le_bytes());
}

/// movsd xmm1, [rip+disp].
#[cfg(target_arch = "x86_64")]
pub fn x_movsd_xmm1_rip(out: &mut Vec<u8>, site: u64, target: u64) {
    out.extend_from_slice(&[0xf2, 0x0f, 0x10, 0x0d]);
    out.extend_from_slice(&rip_disp(site, 8, target).to_le_bytes());
}

#[cfg(target_arch = "x86_64")]
pub fn x_xorpd_xmm0(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x66, 0x0f, 0x57, 0xc0]);
}

#[cfg(target_arch = "x86_64")]
pub fn x_xorpd_xmm1(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x66, 0x0f, 0x57, 0xc9]);
}

#[cfg(target_arch = "x86_64")]
pub fn x_ucomisd_01(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x66, 0x0f, 0x2e, 0xc1]);
}

#[cfg(target_arch = "x86_64")]
pub fn x_sete_al(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x0f, 0x94, 0xc0]);
}

#[cfg(target_arch = "x86_64")]
pub fn x_movzx_eax_al(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0x0f, 0xb6, 0xc0]);
}

#[cfg(target_arch = "x86_64")]
pub fn x_ret(out: &mut Vec<u8>) {
    out.push(0xc3);
}

// --- Mach-O builder ---

const MH_MAGIC_64: u32 = 0xfeed_facf;
const MH_EXECUTE: u32 = 2;
const MH_PIE: u32 = 0x20_0000;
const LC_SEGMENT_64: u32 = 0x19;
const LC_LOAD_DYLIB: u32 = 0xc;
const LC_DYLD_CHAINED_FIXUPS: u32 = 0x8000_0034;
const LC_MAIN: u32 = 0x8000_0028;

fn cmd(code: u32, payload: &[u8]) -> Vec<u8> {
    let size = ((8 + payload.len()).div_ceil(8) * 8) as u32;
    let mut v = Vec::new();
    v.extend_from_slice(&code.to_le_bytes());
    v.extend_from_slice(&size.to_le_bytes());
    v.extend_from_slice(payload);
    v.resize(size as usize, 0);
    v
}

fn segment(
    name: &str,
    vmaddr: u64,
    fileoff: u64,
    filesize: u64,
    vmsize: u64,
    prot: u32,
) -> Vec<u8> {
    let mut p = vec![0u8; 64];
    p[..name.len()].copy_from_slice(name.as_bytes());
    p[16..24].copy_from_slice(&vmaddr.to_le_bytes());
    p[24..32].copy_from_slice(&vmsize.to_le_bytes());
    p[32..40].copy_from_slice(&fileoff.to_le_bytes());
    p[40..48].copy_from_slice(&filesize.to_le_bytes());
    p[48..52].copy_from_slice(&7u32.to_le_bytes());
    p[52..56].copy_from_slice(&prot.to_le_bytes());
    cmd(LC_SEGMENT_64, &p)
}

fn dylib_cmd(name: &str) -> Vec<u8> {
    let mut payload = vec![0u8; 16];
    payload[0..4].copy_from_slice(&24u32.to_le_bytes());
    payload.extend_from_slice(name.as_bytes());
    payload.push(0);
    cmd(LC_LOAD_DYLIB, &payload)
}

/// Chained-fixups payload: format 6, imports format 1, one bind per
/// import at GOT slot `i` (stride-correct `next`, mirroring real
/// `ld` emission: consecutive 8-byte binds use next=2).
fn chained_payload(imports: &[&str]) -> (Vec<u8>, Vec<u8>) {
    let n = imports.len();
    // GOT chain entries.
    let mut got = Vec::new();
    for i in 0..n {
        let next = if i + 1 < n { 2u64 } else { 0 };
        got.extend_from_slice(&((i as u64) | (next << 51) | (1u64 << 63)).to_le_bytes());
    }
    // Symbols.
    let mut symbols = Vec::new();
    let mut name_offs = Vec::new();
    for name in imports {
        name_offs.push(symbols.len() as u32);
        symbols.extend_from_slice(name.as_bytes());
        symbols.push(0);
    }
    // Payload: header + starts + imports + symbols.
    let mut p = vec![0u8; 28];
    let starts_off = 28u32;
    let info_rel = 4 + 2 * 4;
    p.extend_from_slice(&2u32.to_le_bytes());
    p.extend_from_slice(&0u32.to_le_bytes());
    p.extend_from_slice(&(info_rel as u32).to_le_bytes());
    let first = if n > 0 { 0u16 } else { 0xffffu16 };
    let mut seg = vec![0u8; 24];
    seg[0..4].copy_from_slice(&24u32.to_le_bytes());
    seg[4..6].copy_from_slice(&0x4000u16.to_le_bytes());
    seg[6..8].copy_from_slice(&6u16.to_le_bytes()); // PTR_64_OFFSET
    seg[20..22].copy_from_slice(&1u16.to_le_bytes());
    seg[22..24].copy_from_slice(&first.to_le_bytes());
    p.extend_from_slice(&seg);
    let imports_off = p.len() as u32;
    for (i, _) in imports.iter().enumerate() {
        // lib 1 (first LC_LOAD_DYLIB), not weak.
        let w = 1u32 | (name_offs[i] << 9);
        p.extend_from_slice(&w.to_le_bytes());
    }
    let symbols_off = p.len() as u32;
    p.extend_from_slice(&symbols);
    p[0..4].copy_from_slice(&0u32.to_le_bytes());
    p[4..8].copy_from_slice(&starts_off.to_le_bytes());
    p[8..12].copy_from_slice(&imports_off.to_le_bytes());
    p[12..16].copy_from_slice(&symbols_off.to_le_bytes());
    p[16..20].copy_from_slice(&(n as u32).to_le_bytes());
    p[20..24].copy_from_slice(&1u32.to_le_bytes());
    (p, got)
}

/// Build an import-calling guest: `code` runs at the entry point,
/// `data` sits in `__DATA` after the GOT.
pub fn build_import_guest(
    cputype: u32,
    cpusubtype: u32,
    imports: &[&str],
    code: &[u8],
    data: &[u8],
) -> Vec<u8> {
    assert!(code.len() <= 0x3000, "code too large");
    let (chained, got) = chained_payload(imports);
    assert!(32 + 512 + chained.len() < CHAIN_OFF as usize + 0x1000);
    let data_len = got.len() + data.len();
    assert!(data_len <= 0x4000, "data too large");
    let mut cmds = vec![
        segment("__TEXT", TEXT_VMADDR, 0, 0x4000, 0x4000, 5),
        segment("__DATA", DATA_VMADDR, 0x4000, data_len as u64, 0x4000, 3),
        dylib_cmd(SHIM_PATH),
    ];
    let mut fix = vec![0u8; 8];
    fix[0..4].copy_from_slice(&(CHAIN_OFF as u32).to_le_bytes());
    fix[4..8].copy_from_slice(&(chained.len() as u32).to_le_bytes());
    cmds.push(cmd(LC_DYLD_CHAINED_FIXUPS, &fix));
    let mut main = vec![0u8; 16];
    main[0..8].copy_from_slice(&CODE_OFF.to_le_bytes());
    cmds.push(cmd(LC_MAIN, &main));
    let cmds_len: usize = cmds.iter().map(Vec::len).sum();
    assert!(32 + cmds_len <= CHAIN_OFF as usize, "cmds overflow chain");

    let mut file = vec![0u8; 32];
    file[..4].copy_from_slice(&MH_MAGIC_64.to_le_bytes());
    file[4..8].copy_from_slice(&cputype.to_le_bytes());
    file[8..12].copy_from_slice(&cpusubtype.to_le_bytes());
    file[12..16].copy_from_slice(&MH_EXECUTE.to_le_bytes());
    file[16..20].copy_from_slice(&(cmds.len() as u32).to_le_bytes());
    file[20..24].copy_from_slice(&(cmds_len as u32).to_le_bytes());
    file[24..28].copy_from_slice(&MH_PIE.to_le_bytes());
    for c in &cmds {
        file.extend_from_slice(c);
    }
    file.resize(CHAIN_OFF as usize, 0);
    file.extend_from_slice(&chained);
    file.resize(CODE_OFF as usize, 0);
    file.extend_from_slice(code);
    file.resize(0x4000, 0);
    file.extend_from_slice(&got);
    file.extend_from_slice(data);
    file.resize(0x4000 + data_len, 0);
    file
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_arch = "aarch64")]
    fn w(v: u32) -> Vec<u8> {
        v.to_le_bytes().to_vec()
    }

    /// Every encoding below was verified against clang-assembled
    /// output (`emit.s` battery) before baking in.
    #[cfg(target_arch = "aarch64")]
    #[test]
    fn arm64_encodings_match_clang() {
        let mut v = Vec::new();
        a_movz(&mut v, 0, 42, 0, false);
        assert_eq!(v, w(0x5280_0540));
        v.clear();
        a_movz(&mut v, 0, 0x4030, 0, true);
        assert_eq!(v, w(0xd288_0600));
        v.clear();
        a_movk_x(&mut v, 0, 1, 2);
        assert_eq!(v, w(0xf2c0_0020));
        v.clear();
        a_movk_w(&mut v, 1, 0x1234, 1);
        assert_eq!(v, w(0x72a2_4681));
        v.clear();
        a_movn_x0(&mut v, 4);
        assert_eq!(v, w(0x9280_0004));
        v.clear();
        a_adr(&mut v, 0, 0x14, 0x80);
        assert_eq!(v, w(0x1000_0360));
        v.clear();
        a_adrp(&mut v, 16, 0x18, 0x0);
        assert_eq!(v, w(0x9000_0010));
        v.clear();
        a_call_got(&mut v, 0x1000_1000, 0x1000_4078);
        // adrp x16, #0x10 pages; ldr x16, [x16, #0x78]; blr x16.
        assert_eq!(v.len(), 12);
        assert_eq!(&v[4..8], &0xf940_3e10u32.to_le_bytes());
        assert_eq!(&v[8..12], &0xd63f_0200u32.to_le_bytes());
        v.clear();
        a_ldr_x(&mut v, 0, 1);
        assert_eq!(v, w(0xf940_0020));
        v.clear();
        a_ldr_w(&mut v, 0, 1);
        assert_eq!(v, w(0xb940_0020));
        v.clear();
        a_str_x(&mut v, 0, 1);
        assert_eq!(v, w(0xf900_0020));
        v.clear();
        a_str_w(&mut v, 1, 0);
        assert_eq!(v, w(0xb900_0001));
        v.clear();
        a_ret(&mut v);
        assert_eq!(v, w(0xd65f_03c0));
        v.clear();
        a_cmn_w1(&mut v, 0);
        assert_eq!(v, w(0x3100_041f));
        v.clear();
        a_cmn_x1(&mut v, 0);
        assert_eq!(v, w(0xb100_041f));
        v.clear();
        a_cmp_w_imm(&mut v, 0, 4);
        assert_eq!(v, w(0x7100_101f));
        v.clear();
        a_cmp_w_reg(&mut v, 0, 1);
        assert_eq!(v, w(0x6b01_001f));
        v.clear();
        a_mov_w(&mut v, 19, 0);
        assert_eq!(v, w(0x2a00_03f3));
        v.clear();
        a_mov_w(&mut v, 0, 19);
        assert_eq!(v, w(0x2a13_03e0));
        v.clear();
        a_mov_x(&mut v, 1, 0);
        assert_eq!(v, w(0xaa00_03e1));
        v.clear();
        a_mov_x(&mut v, 19, 0);
        assert_eq!(v, w(0xaa00_03f3));
        // Nonzero sources (a wrong-field bug hid behind x0-only
        // cases here once).
        v.clear();
        a_mov_x(&mut v, 1, 19);
        assert_eq!(v, w(0xaa13_03e1));
        v.clear();
        a_mov_x(&mut v, 0, 19);
        assert_eq!(v, w(0xaa13_03e0));
        v.clear();
        a_b_cond(&mut v, 0, 0x4c, 0x80);
        assert_eq!(v, w(0x5400_01a0));
        v.clear();
        a_b_cond(&mut v, 1, 0x50, 0x80);
        assert_eq!(v, w(0x5400_0181));
        v.clear();
        a_b_cond(&mut v, 0xb, 0x54, 0x80);
        assert_eq!(v, w(0x5400_016b));
        v.clear();
        a_b_cond(&mut v, 0xa, 0x58, 0x80);
        assert_eq!(v, w(0x5400_014a));
        v.clear();
        a_b(&mut v, 0x5c, 0x80);
        assert_eq!(v, w(0x1400_0009));
        v.clear();
        a_cbnz_w(&mut v, 0, 0x60, 0x80);
        assert_eq!(v, w(0x3500_0100));
        v.clear();
        a_subs_x_imm(&mut v, 20, 20, 1);
        assert_eq!(v, w(0xf100_0694));
        v.clear();
        a_lsr_w8(&mut v, 0, 0);
        assert_eq!(v, w(0x5308_7c00));
        v.clear();
        a_uxtb(&mut v, 0, 0);
        assert_eq!(v, w(0x5300_1c00));
        v.clear();
        a_fmov_d_xzr(&mut v, 0);
        assert_eq!(v, w(0x9e67_03e0));
        v.clear();
        a_ldr_d(&mut v, 0, 0);
        assert_eq!(v, w(0xfd40_0000));
        v.clear();
        a_ldr_d(&mut v, 1, 0);
        assert_eq!(v, w(0xfd40_0001));
        v.clear();
        a_fcmp_d0(&mut v, 0);
        assert_eq!(v, w(0x1e60_2008));
        v.clear();
        a_fcmp_d(&mut v, 1, 0);
        assert_eq!(v, w(0x1e60_2020));
        v.clear();
        a_fcmp_d(&mut v, 0, 1);
        assert_eq!(v, w(0x1e61_2000));
        v.clear();
        a_cset_eq(&mut v, 0);
        assert_eq!(v, w(0x1a9f_17e0));
        v.clear();
        a_add_imm12(&mut v, 1, 1, 0x10);
        assert_eq!(v, w(0x9100_4021));
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn x64_encodings_match_clang() {
        let mut v = Vec::new();
        x_mov_r32_imm(&mut v, 0, 42);
        assert_eq!(v, vec![0xb8, 0x2a, 0, 0, 0]);
        v.clear();
        x_mov_rdi_imm32(&mut v, 1);
        assert_eq!(v, vec![0xbf, 1, 0, 0, 0]);
        v.clear();
        x_mov_esi_imm32(&mut v, 2);
        assert_eq!(v, vec![0xbe, 2, 0, 0, 0]);
        v.clear();
        x_mov_ecx_imm32(&mut v, 3);
        assert_eq!(v, vec![0xb9, 3, 0, 0, 0]);
        v.clear();
        x_mov_r8d_imm32(&mut v, 4);
        assert_eq!(v, vec![0x41, 0xb8, 4, 0, 0, 0]);
        v.clear();
        x_xor_esi(&mut v);
        assert_eq!(v, vec![0x31, 0xf6]);
        v.clear();
        x_xor_edx(&mut v);
        assert_eq!(v, vec![0x31, 0xd2]);
        v.clear();
        x_xor_ecx(&mut v);
        assert_eq!(v, vec![0x31, 0xc9]);
        v.clear();
        x_xor_edi(&mut v);
        assert_eq!(v, vec![0x31, 0xff]);
        v.clear();
        x_xor_r8d(&mut v);
        assert_eq!(v, vec![0x45, 0x31, 0xc0]);
        v.clear();
        x_xor_r9d(&mut v);
        assert_eq!(v, vec![0x45, 0x31, 0xc9]);
        v.clear();
        x_lea_rip(&mut v, 7, 0x28, 0xa8);
        assert_eq!(v[..3], vec![0x48, 0x8d, 0x3d]);
        assert_eq!(v.len(), 7);
        v.clear();
        x_lea_rip(&mut v, 0, 0x2f, 0xa8);
        assert_eq!(v[..3], vec![0x48, 0x8d, 0x05]);
        v.clear();
        x_lea_rip(&mut v, 8, 0, 0x100);
        assert_eq!(v, vec![0x4c, 0x8d, 0x05, 0xf9, 0x00, 0x00, 0x00]);
        v.clear();
        x_call_got(&mut v, 0x36, 0xa8);
        assert_eq!(v[..2], vec![0xff, 0x15]);
        v.clear();
        x_store_rip_rax(&mut v, 0x3c, 0xa8);
        assert_eq!(v[..3], vec![0x48, 0x89, 0x05]);
        v.clear();
        x_load_eax_rip(&mut v, 0x43, 0xa8);
        assert_eq!(v[..2], vec![0x8b, 0x05]);
        v.clear();
        x_load_edi_rip(&mut v, 0x49, 0xa8);
        assert_eq!(v[..2], vec![0x8b, 0x3d]);
        v.clear();
        x_load_eax_rax(&mut v);
        assert_eq!(v, vec![0x8b, 0x00]);
        v.clear();
        x_mov_ebx_eax(&mut v);
        assert_eq!(v, vec![0x89, 0xc3]);
        v.clear();
        x_mov_eax_ebx(&mut v);
        assert_eq!(v, vec![0x8b, 0xc3]);
        v.clear();
        x_mov_rdi_rbx(&mut v);
        assert_eq!(v, vec![0x48, 0x89, 0xdf]);
        v.clear();
        x_mov_edi_ebx(&mut v);
        assert_eq!(v, vec![0x89, 0xdf]);
        v.clear();
        x_mov_edi_eax(&mut v);
        assert_eq!(v, vec![0x89, 0xc7]);
        v.clear();
        x_mov_r64_r64(&mut v, 3, 0);
        assert_eq!(v, vec![0x48, 0x89, 0xc3]);
        v.clear();
        x_mov_r64_r64(&mut v, 6, 3);
        assert_eq!(v, vec![0x48, 0x89, 0xde]);
        v.clear();
        x_store_rip_imm32(&mut v, 0, 0x100, 7);
        assert_eq!(v.len(), 10);
        assert_eq!(v[..2], vec![0xc7, 0x05]);
        assert_eq!(&v[6..10], &7u32.to_le_bytes()[..]);
        v.clear();
        x_movsd_xmm0_rip(&mut v, 0, 0x100);
        assert_eq!(v, vec![0xf2, 0x0f, 0x10, 0x05, 0xf8, 0x00, 0x00, 0x00]);
        v.clear();
        x_movsd_xmm1_rip(&mut v, 0, 0x100);
        assert_eq!(v, vec![0xf2, 0x0f, 0x10, 0x0d, 0xf8, 0x00, 0x00, 0x00]);
        v.clear();
        x_store_rbx_imm32(&mut v, 0x1234_5678);
        assert_eq!(v, vec![0xc7, 0x03, 0x78, 0x56, 0x34, 0x12]);
        v.clear();
        x_load_eax_rbx(&mut v);
        assert_eq!(v, vec![0x8b, 0x03]);
        v.clear();
        x_cmp_eax_imm32(&mut v, 0xffff_ffff);
        assert_eq!(v, vec![0x3d, 0xff, 0xff, 0xff, 0xff]);
        v.clear();
        x_cmp_rax_imm32(&mut v, 0xffff_ffff);
        assert_eq!(v, vec![0x48, 0x3d, 0xff, 0xff, 0xff, 0xff]);
        v.clear();
        x_cmp_eax_0(&mut v);
        assert_eq!(v, vec![0x83, 0xf8, 0x00]);
        v.clear();
        x_cmp_rip_imm8(&mut v, 0x6a, 0xa8, 0);
        assert_eq!(v[..2], vec![0x83, 0x3d]);
        assert_eq!(v[6], 0x00);
        v.clear();
        x_test_eax(&mut v);
        assert_eq!(v, vec![0x85, 0xc0]);
        v.clear();
        x_jcc(&mut v, 0x84, 0x73, 0xa8);
        assert_eq!(v[..2], vec![0x0f, 0x84]);
        v.clear();
        x_jcc(&mut v, 0x85, 0x79, 0xa8);
        assert_eq!(v[..2], vec![0x0f, 0x85]);
        v.clear();
        x_jcc(&mut v, 0x8c, 0x7f, 0xa8);
        assert_eq!(v[..2], vec![0x0f, 0x8c]);
        v.clear();
        x_jmp(&mut v, 0x85, 0xa8);
        assert_eq!(v[0], 0xe9);
        v.clear();
        x_dec_ecx(&mut v);
        assert_eq!(v, vec![0xff, 0xc9]);
        v.clear();
        x_shr_eax_8(&mut v);
        assert_eq!(v, vec![0xc1, 0xe8, 0x08]);
        v.clear();
        x_and_eax_ff(&mut v);
        assert_eq!(v, vec![0x25, 0xff, 0x00, 0x00, 0x00]);
        v.clear();
        x_xorpd_xmm0(&mut v);
        assert_eq!(v, vec![0x66, 0x0f, 0x57, 0xc0]);
        v.clear();
        x_xorpd_xmm1(&mut v);
        assert_eq!(v, vec![0x66, 0x0f, 0x57, 0xc9]);
        v.clear();
        x_ucomisd_01(&mut v);
        assert_eq!(v, vec![0x66, 0x0f, 0x2e, 0xc1]);
        v.clear();
        x_sete_al(&mut v);
        assert_eq!(v, vec![0x0f, 0x94, 0xc0]);
        v.clear();
        x_movzx_eax_al(&mut v);
        assert_eq!(v, vec![0x0f, 0xb6, 0xc0]);
        v.clear();
        x_ret(&mut v);
        assert_eq!(v, vec![0xc3]);
    }

    #[test]
    fn builder_layout_sane() {
        let img = build_import_guest(0x0100_0007, 3, &["_write"], &[0xc3], b"hi");
        // Header + 5 commands + chained + code + got + data.
        assert_eq!(&img[..4], &[0xcf, 0xfa, 0xed, 0xfe]);
        assert_eq!(u32::from_le_bytes(img[16..20].try_into().unwrap()), 5);
        // GOT chain entry: bind ord 0, next 0.
        let got = u64::from_le_bytes(img[0x4000..0x4008].try_into().unwrap());
        assert_eq!(got, 1u64 << 63);
        assert_eq!(&img[0x4008..0x400a], b"hi");
        assert_eq!(img[0x1000], 0xc3);
    }
}
