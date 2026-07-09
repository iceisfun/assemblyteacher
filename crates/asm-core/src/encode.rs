//! The x86_64 instruction encoder — the decoder run backwards.
//!
//! Encoding is harder than decoding, because the encoder must *choose*. There
//! are usually several byte sequences that mean the same thing, and picking
//! among them is where assemblers differ:
//!
//! * `add rax, 1` can be `48 05 01 00 00 00` (accumulator form, 6 bytes) or
//!   `48 83 c0 01` (sign-extended imm8, 4 bytes). We pick the short one.
//! * `mov rax, 1` can be a 10-byte `movabs`, or a 7-byte `mov r/m64, imm32`
//!   whose immediate is sign-extended. We pick the short one, and fall back to
//!   `movabs` only when the value genuinely needs 64 bits.
//! * `xor eax, eax` and `xor rax, rax` clear the same 64 bits, because a 32-bit
//!   write zero-extends. The 32-bit form is one byte shorter. We honour whatever
//!   the caller asked for rather than second-guessing it.
//!
//! Choices that change *meaning* are never made silently: an ambiguous operand
//! (`mov [rax], 1` — how wide?) is an error, not a guess.

use crate::error::EncodeError;
use crate::insn::{Cond, Encoding, Insn, Mnemonic};
use crate::operand::{Mem, Operand};
use crate::reg::{Reg, Size};

/// Encode one instruction. Relative branches use whatever displacement the
/// caller supplies, choosing the short form when it fits.
pub fn encode(mnemonic: Mnemonic, operands: &[Operand]) -> Result<Vec<u8>, EncodeError> {
    let mut b = Builder::default();
    b.emit(mnemonic, operands)?;
    b.finish()
}

/// Re-encode a decoded instruction. The result is not guaranteed to be
/// byte-identical to the input — the input may have used a longer encoding than
/// the one this encoder chooses — but it is guaranteed to decode to an
/// equivalent instruction.
pub fn encode_insn(insn: &Insn) -> Result<Vec<u8>, EncodeError> {
    encode(insn.mnemonic, &insn.operands)
}

/// The number of bytes a relative branch to `disp` occupies, including whether
/// the short form is available. Used by the assembler's branch-relaxation pass.
pub fn rel_branch_len(mnemonic: Mnemonic, disp: i64) -> usize {
    match mnemonic {
        Mnemonic::Jmp if fits_i8(disp) => 2,
        Mnemonic::Jmp => 5,
        Mnemonic::Jcc(_) if fits_i8(disp) => 2,
        Mnemonic::Jcc(_) => 6,
        _ => 5, // call rel32
    }
}

fn fits_i8(v: i64) -> bool {
    (i8::MIN as i64..=i8::MAX as i64).contains(&v)
}

/// An immediate fits `n` bytes if it fits either the signed or the unsigned
/// range of that width. `mov al, 0xff` and `mov al, -1` encode identically;
/// rejecting one of them would be pedantry.
fn fits(v: i64, n: u8) -> bool {
    match n {
        1 => (-128..=255).contains(&v),
        2 => (-32768..=65535).contains(&v),
        4 => (-(1i64 << 31)..=((1i64 << 32) - 1)).contains(&v),
        8 => true,
        _ => false,
    }
}

/// The register number as it appears in an encoding field.
///
/// `ah`/`ch`/`dh`/`bh` are modelled as `Reg::high(0..3)` but encode as 4..7.
fn enc_num(r: Reg) -> u8 {
    if r.high_byte {
        r.num + 4
    } else {
        r.num
    }
}

/// Is this `al`/`ax`/`eax`/`rax` — the register with the special short forms?
fn is_acc(r: Reg) -> bool {
    r.num == 0 && !r.high_byte
}

#[derive(Default)]
struct Builder {
    enc: Encoding,
    w: bool,
    r: bool,
    x: bool,
    bb: bool,
    /// A `spl`/`bpl`/`sil`/`dil` or `r8`..`r15` operand forces a REX prefix.
    need_rex: bool,
    /// An `ah`/`ch`/`dh`/`bh` operand forbids one.
    no_rex: bool,
    /// The `0x66` operand-size override.
    o16: bool,
    /// The register that forced REX, and the one that forbade it — for the
    /// error message when both happen.
    rex_forcer: Option<&'static str>,
    rex_forbidder: Option<&'static str>,
}

impl Builder {
    fn note_reg(&mut self, reg: Reg) {
        if reg.requires_rex() {
            self.need_rex = true;
            self.rex_forcer.get_or_insert(reg.name());
        }
        if reg.forbids_rex() {
            self.no_rex = true;
            self.rex_forbidder.get_or_insert(reg.name());
        }
    }

    fn set_size(&mut self, s: Size) {
        match s {
            Size::Word => self.o16 = true,
            Size::Qword => self.w = true,
            _ => {}
        }
    }

    /// Stack/branch operands default to 64-bit and must not set REX.W.
    fn set_stack_size(&mut self, s: Size) {
        if s == Size::Word {
            self.o16 = true;
        }
    }

    fn op(&mut self, bytes: &[u8]) {
        self.enc.opcode.extend_from_slice(bytes);
    }

    /// Emit an opcode and succeed. The zero-operand instructions are all of the
    /// form "write these bytes, nothing can go wrong".
    fn op_ok(&mut self, bytes: &[u8]) -> Result<(), EncodeError> {
        self.op(bytes);
        Ok(())
    }

    fn imm(&mut self, v: i64, n: u8) -> Result<(), EncodeError> {
        if !fits(v, n) {
            return Err(EncodeError::ImmediateOutOfRange { value: v, bytes: n });
        }
        self.enc.imm.extend_from_slice(&v.to_le_bytes()[..n as usize]);
        Ok(())
    }

    /// Emit the ModRM byte (plus SIB and displacement) for `rm`, with
    /// `reg_field` in the `reg` slot. `reg_field` may be a register number or
    /// an opcode extension digit.
    fn modrm(&mut self, reg_field: u8, rm: &Operand) -> Result<(), EncodeError> {
        self.r = reg_field >= 8;
        match rm {
            Operand::Reg(reg) => {
                self.note_reg(*reg);
                let n = enc_num(*reg);
                self.bb = n >= 8;
                self.enc.modrm = Some(0b1100_0000 | ((reg_field & 7) << 3) | (n & 7));
                Ok(())
            }
            Operand::Mem(m) => self.modrm_mem(reg_field, m),
            other => Err(EncodeError::BadOperands {
                mnemonic: "<modrm>".into(),
                operands: other.to_string(),
            }),
        }
    }

    fn modrm_mem(&mut self, reg_field: u8, m: &Mem) -> Result<(), EncodeError> {
        if let Some(seg) = m.seg {
            self.enc.legacy.push(seg.prefix_byte());
        }
        if let Some(base) = m.base {
            if base.size == Size::Dword {
                self.enc.legacy.push(0x67);
            }
        }

        let reg3 = (reg_field & 7) << 3;

        // [rip + disp32]
        if m.rip_relative {
            self.enc.modrm = Some(reg3 | 0b101);
            self.enc.disp.extend_from_slice(&(m.disp as i32).to_le_bytes());
            return Ok(());
        }

        // An index register needs a SIB byte. So does rsp/r12 as a base, since
        // rm == 100 is the escape that introduces SIB rather than naming rsp.
        let base_needs_sib = m.base.is_some_and(|b| enc_num(b) & 7 == 4);
        let need_sib = m.index.is_some() || base_needs_sib || m.is_absolute();

        if let Some(idx) = m.index {
            if idx.num == 4 && !idx.high_byte {
                // Encoding 100 in the index field means "no index". There is no
                // bit pattern left over to name rsp, so rsp can never be one.
                return Err(EncodeError::Unsupported("rsp cannot be an index register".into()));
            }
            if !matches!(m.scale, 1 | 2 | 4 | 8) {
                return Err(EncodeError::Unsupported(format!(
                    "scale {} is not 1, 2, 4 or 8",
                    m.scale
                )));
            }
            self.note_reg(idx);
            self.x = idx.num >= 8;
        }
        if let Some(base) = m.base {
            self.note_reg(base);
            self.bb = base.num >= 8;
        }

        // Choose mod: no displacement, disp8, or disp32.
        //
        // rbp/r13 as a base (rm == 101) with mod == 00 is stolen for the
        // RIP-relative form, so `[rbp]` must be encoded as `[rbp+0]` with an
        // explicit zero disp8. This is why `[rbp]` costs one more byte than
        // `[rax]` — a real, observable asymmetry in compiled code.
        let base_is_bp = m.base.is_some_and(|b| enc_num(b) & 7 == 5);
        let (md, disp_len) = if m.base.is_none() {
            // No base at all — whether that is `[0x404000]` or `[rcx*8]`. Both
            // are encoded as mod=00 with a SIB byte whose base field is 101,
            // which always carries a disp32. There is no "no base, no
            // displacement" form to fall back on.
            (0u8, 4u8)
        } else if m.disp == 0 && !base_is_bp {
            (0, 0)
        } else if fits_i8(m.disp) {
            (1, 1)
        } else {
            (2, 4)
        };

        if need_sib {
            let scale_bits = match m.scale {
                2 => 1u8,
                4 => 2,
                8 => 3,
                _ => 0,
            };
            let index_bits = m.index.map(|i| enc_num(i) & 7).unwrap_or(0b100);
            let base_bits = m.base.map(|b| enc_num(b) & 7).unwrap_or(0b101);
            self.enc.modrm = Some((md << 6) | reg3 | 0b100);
            self.enc.sib = Some((scale_bits << 6) | (index_bits << 3) | base_bits);
        } else {
            let rm_bits = enc_num(m.base.expect("no base implies SIB")) & 7;
            self.enc.modrm = Some((md << 6) | reg3 | rm_bits);
        }

        match disp_len {
            1 => self.enc.disp.push(m.disp as i8 as u8),
            4 => {
                if !fits(m.disp, 4) {
                    return Err(EncodeError::ImmediateOutOfRange { value: m.disp, bytes: 4 });
                }
                self.enc.disp.extend_from_slice(&(m.disp as i32).to_le_bytes());
            }
            _ => {}
        }
        Ok(())
    }

    fn finish(mut self) -> Result<Vec<u8>, EncodeError> {
        if self.o16 {
            self.enc.legacy.insert(0, 0x66);
        }
        let rex_needed = self.w || self.r || self.x || self.bb || self.need_rex;
        if rex_needed {
            if self.no_rex {
                return Err(EncodeError::RexConflict {
                    reg: self.rex_forcer.unwrap_or("r8..r15").to_string(),
                    other: self.rex_forbidder.unwrap_or("ah/ch/dh/bh").to_string(),
                });
            }
            self.enc.rex = Some(
                0x40 | ((self.w as u8) << 3)
                    | ((self.r as u8) << 2)
                    | ((self.x as u8) << 1)
                    | self.bb as u8,
            );
        }
        Ok(self.enc.bytes())
    }
}

/// The operand size implied by the operands, or an error if nothing implies one.
fn size_of(mnemonic: Mnemonic, ops: &[Operand]) -> Result<Size, EncodeError> {
    ops.iter().find_map(|o| o.size()).ok_or_else(|| {
        EncodeError::SizeMismatch(format!(
            "`{}` with these operands has no width; write `byte`, `word`, `dword` or `qword` \
             before the memory operand",
            mnemonic
        ))
    })
}

fn bad(mnemonic: Mnemonic, ops: &[Operand]) -> EncodeError {
    EncodeError::BadOperands {
        mnemonic: mnemonic.name(),
        operands: ops.iter().map(|o| o.to_string()).collect::<Vec<_>>().join(", "),
    }
}

/// Index of an arithmetic op within the regular `0x00..0x3f` block.
fn arith_index(m: Mnemonic) -> Option<u8> {
    Some(match m {
        Mnemonic::Add => 0,
        Mnemonic::Or => 1,
        Mnemonic::Adc => 2,
        Mnemonic::Sbb => 3,
        Mnemonic::And => 4,
        Mnemonic::Sub => 5,
        Mnemonic::Xor => 6,
        Mnemonic::Cmp => 7,
        _ => return None,
    })
}

/// ModRM `reg` digit for the shift/rotate group.
fn shift_digit(m: Mnemonic) -> Option<u8> {
    Some(match m {
        Mnemonic::Rol => 0,
        Mnemonic::Ror => 1,
        Mnemonic::Rcl => 2,
        Mnemonic::Rcr => 3,
        Mnemonic::Shl => 4,
        Mnemonic::Shr => 5,
        Mnemonic::Sar => 7,
        _ => return None,
    })
}

/// ModRM `reg` digit for the `0xf6`/`0xf7` unary group.
fn unary_digit(m: Mnemonic) -> Option<u8> {
    Some(match m {
        Mnemonic::Not => 2,
        Mnemonic::Neg => 3,
        Mnemonic::Mul => 4,
        Mnemonic::Imul => 5,
        Mnemonic::Div => 6,
        Mnemonic::Idiv => 7,
        _ => return None,
    })
}

impl Builder {
    fn emit(&mut self, m: Mnemonic, ops: &[Operand]) -> Result<(), EncodeError> {
        use Mnemonic as M;
        use Operand as O;

        // ---- zero-operand instructions --------------------------------------
        match m {
            M::Ret if ops.is_empty() => return self.op_ok(&[0xc3]),
            M::Ret => {
                if let [O::Imm(v)] = ops {
                    self.op(&[0xc2]);
                    return self.imm(*v, 2);
                }
                return Err(bad(m, ops));
            }
            M::Leave => return self.op_ok(&[0xc9]),
            M::Nop if ops.is_empty() => return self.op_ok(&[0x90]),
            M::Hlt => return self.op_ok(&[0xf4]),
            M::Int3 => return self.op_ok(&[0xcc]),
            M::Int => {
                if let [O::Imm(v)] = ops {
                    self.op(&[0xcd]);
                    return self.imm(*v, 1);
                }
                return Err(bad(m, ops));
            }
            M::Syscall => return self.op_ok(&[0x0f, 0x05]),
            M::Ud2 => return self.op_ok(&[0x0f, 0x0b]),
            M::Endbr64 => {
                self.enc.legacy.push(0xf3);
                self.op(&[0x0f, 0x1e]);
                self.enc.modrm = Some(0xfa);
                return Ok(());
            }
            M::Cwde => return self.op_ok(&[0x98]),
            M::Cbw => {
                self.o16 = true;
                return self.op_ok(&[0x98]);
            }
            M::Cdqe => {
                self.w = true;
                return self.op_ok(&[0x98]);
            }
            M::Cdq => return self.op_ok(&[0x99]),
            M::Cwd => {
                self.o16 = true;
                return self.op_ok(&[0x99]);
            }
            M::Cqo => {
                self.w = true;
                return self.op_ok(&[0x99]);
            }
            _ => {}
        }

        // ---- the eight arithmetic/logic operations --------------------------
        if let Some(idx) = arith_index(m) {
            let size = size_of(m, ops)?;
            return match ops {
                // op r/m, imm
                [dst, O::Imm(v)] if dst.is_write_target() => {
                    self.set_size(size);
                    if size == Size::Byte {
                        // The accumulator form saves the ModRM byte.
                        if matches!(dst, O::Reg(r) if enc_num(*r) == 0 && !r.high_byte) {
                            self.op(&[0x04 | (idx << 3)]);
                            return self.imm(*v, 1);
                        }
                        self.op(&[0x80]);
                        self.modrm(idx, dst)?;
                        self.imm(*v, 1)
                    } else if fits_i8(*v) {
                        // 0x83: one immediate byte, sign-extended to the operand
                        // size. Almost every `add rsp, 8` in existence.
                        self.op(&[0x83]);
                        self.modrm(idx, dst)?;
                        self.imm(*v, 1)
                    } else if matches!(dst, O::Reg(r) if r.num == 0) {
                        self.op(&[0x05 | (idx << 3)]);
                        self.imm(*v, if size == Size::Word { 2 } else { 4 })
                    } else {
                        self.op(&[0x81]);
                        self.modrm(idx, dst)?;
                        self.imm(*v, if size == Size::Word { 2 } else { 4 })
                    }
                }
                // op r/m, r
                [dst, O::Reg(src)] => {
                    check_sizes(m, size, src.size)?;
                    self.set_size(size);
                    self.op(&[if size == Size::Byte { idx << 3 } else { 0x01 | (idx << 3) }]);
                    self.note_reg(*src);
                    self.modrm(enc_num(*src), dst)
                }
                // op r, m
                [O::Reg(dst), src @ O::Mem(_)] => {
                    self.set_size(size);
                    self.op(&[if size == Size::Byte {
                        0x02 | (idx << 3)
                    } else {
                        0x03 | (idx << 3)
                    }]);
                    self.note_reg(*dst);
                    self.modrm(enc_num(*dst), src)
                }
                _ => Err(bad(m, ops)),
            };
        }

        // ---- shifts and rotates ---------------------------------------------
        if let Some(digit) = shift_digit(m) {
            let size = ops.first().and_then(|o| o.size()).ok_or_else(|| {
                EncodeError::SizeMismatch(format!("`{}` needs a sized destination", m))
            })?;
            self.set_size(size);
            let byte = size == Size::Byte;
            return match ops {
                // A shift by one has its own opcode, saving the immediate byte.
                [dst, O::Imm(1)] => {
                    self.op(&[if byte { 0xd0 } else { 0xd1 }]);
                    self.modrm(digit, dst)
                }
                [dst, O::Imm(v)] => {
                    self.op(&[if byte { 0xc0 } else { 0xc1 }]);
                    self.modrm(digit, dst)?;
                    self.imm(*v, 1)
                }
                // The variable shift count lives in cl. Only cl. This is why
                // shift-heavy code keeps rcx free.
                [dst, O::Reg(c)] if c.num == 1 && c.size == Size::Byte && !c.high_byte => {
                    self.op(&[if byte { 0xd2 } else { 0xd3 }]);
                    self.modrm(digit, dst)
                }
                _ => Err(bad(m, ops)),
            };
        }

        // ---- the f6/f7 unary group ------------------------------------------
        if let Some(digit) = unary_digit(m) {
            if ops.len() == 1 {
                let size = size_of(m, ops)?;
                self.set_size(size);
                self.op(&[if size == Size::Byte { 0xf6 } else { 0xf7 }]);
                return self.modrm(digit, &ops[0]);
            }
            // imul has two- and three-operand forms that are not in this group.
            if m != M::Imul {
                return Err(bad(m, ops));
            }
        }

        match m {
            M::Mov => {
                let size = size_of(m, ops)?;
                match ops {
                    [O::Reg(dst), O::Imm(v)] => {
                        self.note_reg(*dst);
                        self.set_size(size);
                        // `mov r/m64, imm32` sign-extends its immediate, so it
                        // can only stand in for `movabs` when the value is
                        // exactly what a signed 32-bit field sign-extends to.
                        // `0xffffffff` is *not*: it would arrive as -1. Testing
                        // "fits in 4 bytes" instead of "fits in a signed 32"
                        // silently corrupts every value in 0x8000_0000..=0xffff_ffff.
                        if size == Size::Qword && i32::try_from(*v).is_ok() {
                            self.op(&[0xc7]);
                            self.modrm(0, &ops[0])?;
                            return self.imm(*v, 4);
                        }
                        let n = enc_num(*dst);
                        self.bb = n >= 8;
                        self.op(&[if size == Size::Byte { 0xb0 } else { 0xb8 } | (n & 7)]);
                        self.imm(*v, size.bytes())
                    }
                    [dst, O::Imm(v)] if dst.is_write_target() => {
                        self.set_size(size);
                        self.op(&[if size == Size::Byte { 0xc6 } else { 0xc7 }]);
                        self.modrm(0, dst)?;
                        self.imm(
                            *v,
                            if size == Size::Byte {
                                1
                            } else if size == Size::Word {
                                2
                            } else {
                                4
                            },
                        )
                    }
                    [dst, O::Reg(src)] => {
                        check_sizes(m, size, src.size)?;
                        self.set_size(size);
                        self.op(&[if size == Size::Byte { 0x88 } else { 0x89 }]);
                        self.note_reg(*src);
                        self.modrm(enc_num(*src), dst)
                    }
                    [O::Reg(dst), src @ O::Mem(_)] => {
                        self.set_size(size);
                        self.op(&[if size == Size::Byte { 0x8a } else { 0x8b }]);
                        self.note_reg(*dst);
                        self.modrm(enc_num(*dst), src)
                    }
                    _ => Err(bad(m, ops)),
                }
            }

            M::Movzx | M::Movsx => match ops {
                [O::Reg(dst), src] => {
                    let ssize = src.size().ok_or_else(|| {
                        EncodeError::SizeMismatch(format!("`{}` needs a sized source", m))
                    })?;
                    if ssize != Size::Byte && ssize != Size::Word {
                        return Err(EncodeError::SizeMismatch(format!(
                            "`{}` widens from a byte or a word, not from {}",
                            m,
                            ssize.keyword()
                        )));
                    }
                    if dst.size <= ssize {
                        return Err(EncodeError::SizeMismatch(format!(
                            "`{}` destination {} is not wider than source {}",
                            m,
                            dst.name(),
                            ssize.keyword()
                        )));
                    }
                    self.set_size(dst.size);
                    let base = if m == M::Movzx { 0xb6 } else { 0xbe };
                    self.op(&[0x0f, base | (ssize == Size::Word) as u8]);
                    self.note_reg(*dst);
                    self.modrm(enc_num(*dst), src)
                }
                _ => Err(bad(m, ops)),
            },

            M::Movsxd => match ops {
                [O::Reg(dst), src] => {
                    self.set_size(dst.size);
                    self.op(&[0x63]);
                    self.note_reg(*dst);
                    self.modrm(enc_num(*dst), src)
                }
                _ => Err(bad(m, ops)),
            },

            M::Lea => match ops {
                [O::Reg(dst), O::Mem(_)] => {
                    self.set_size(dst.size);
                    self.op(&[0x8d]);
                    self.note_reg(*dst);
                    self.modrm(enc_num(*dst), &ops[1])
                }
                _ => Err(bad(m, ops)),
            },

            M::Push => match ops {
                [O::Reg(r)] => {
                    self.set_stack_size(r.size);
                    self.note_reg(*r);
                    let n = enc_num(*r);
                    self.bb = n >= 8;
                    self.op_ok(&[0x50 | (n & 7)])
                }
                [O::Imm(v)] if fits_i8(*v) => {
                    self.op(&[0x6a]);
                    self.imm(*v, 1)
                }
                [O::Imm(v)] => {
                    self.op(&[0x68]);
                    self.imm(*v, 4)
                }
                [mem @ O::Mem(_)] => {
                    self.op(&[0xff]);
                    self.modrm(6, mem)
                }
                _ => Err(bad(m, ops)),
            },

            M::Pop => match ops {
                [O::Reg(r)] => {
                    self.set_stack_size(r.size);
                    self.note_reg(*r);
                    let n = enc_num(*r);
                    self.bb = n >= 8;
                    self.op_ok(&[0x58 | (n & 7)])
                }
                [mem @ O::Mem(_)] => {
                    self.op(&[0x8f]);
                    self.modrm(0, mem)
                }
                _ => Err(bad(m, ops)),
            },

            M::Test => {
                let size = size_of(m, ops)?;
                self.set_size(size);
                match ops {
                    [O::Reg(a), O::Imm(v)] if enc_num(*a) == 0 && !a.high_byte => {
                        self.op(&[if size == Size::Byte { 0xa8 } else { 0xa9 }]);
                        self.imm(
                            *v,
                            if size == Size::Byte {
                                1
                            } else if size == Size::Word {
                                2
                            } else {
                                4
                            },
                        )
                    }
                    [dst, O::Imm(v)] => {
                        self.op(&[if size == Size::Byte { 0xf6 } else { 0xf7 }]);
                        self.modrm(0, dst)?;
                        self.imm(
                            *v,
                            if size == Size::Byte {
                                1
                            } else if size == Size::Word {
                                2
                            } else {
                                4
                            },
                        )
                    }
                    [dst, O::Reg(src)] => {
                        self.op(&[if size == Size::Byte { 0x84 } else { 0x85 }]);
                        self.note_reg(*src);
                        self.modrm(enc_num(*src), dst)
                    }
                    _ => Err(bad(m, ops)),
                }
            }

            M::Xchg => match ops {
                // xchg with the accumulator has a one-byte form. 0x90 is
                // `xchg eax, eax`, which is where `nop` comes from.
                //
                // xchg is symmetric, so we take the short form whichever side
                // the accumulator is written on. The decoded operand order then
                // follows the encoding rather than the source text, which is
                // harmless and is what every other assembler does too.
                [O::Reg(x), O::Reg(y)]
                    if x.size == y.size && x.size != Size::Byte && (is_acc(*x) != is_acc(*y)) =>
                {
                    let other = if is_acc(*x) { *y } else { *x };
                    self.set_size(other.size);
                    self.note_reg(other);
                    let n = enc_num(other);
                    self.bb = n >= 8;
                    self.op_ok(&[0x90 | (n & 7)])
                }
                [dst, O::Reg(src)] => {
                    let size = size_of(m, ops)?;
                    self.set_size(size);
                    self.op(&[if size == Size::Byte { 0x86 } else { 0x87 }]);
                    self.note_reg(*src);
                    self.modrm(enc_num(*src), dst)
                }
                _ => Err(bad(m, ops)),
            },

            M::Inc | M::Dec => {
                let digit = if m == M::Inc { 0 } else { 1 };
                let size = size_of(m, ops)?;
                self.set_size(size);
                self.op(&[if size == Size::Byte { 0xfe } else { 0xff }]);
                self.modrm(digit, &ops[0])
            }

            M::Imul => match ops {
                [O::Reg(dst), src] => {
                    self.set_size(dst.size);
                    self.op(&[0x0f, 0xaf]);
                    self.note_reg(*dst);
                    self.modrm(enc_num(*dst), src)
                }
                [O::Reg(dst), src, O::Imm(v)] => {
                    self.set_size(dst.size);
                    let short = fits_i8(*v);
                    self.op(&[if short { 0x6b } else { 0x69 }]);
                    self.note_reg(*dst);
                    self.modrm(enc_num(*dst), src)?;
                    self.imm(
                        *v,
                        if short {
                            1
                        } else if dst.size == Size::Word {
                            2
                        } else {
                            4
                        },
                    )
                }
                _ => Err(bad(m, ops)),
            },

            M::Jmp => match ops {
                [O::Rel(d)] if fits_i8(*d) => {
                    self.op(&[0xeb]);
                    self.imm(*d, 1)
                }
                [O::Rel(d)] => {
                    self.op(&[0xe9]);
                    self.imm(*d, 4)
                }
                [target] => {
                    self.op(&[0xff]);
                    self.modrm(4, target)
                }
                _ => Err(bad(m, ops)),
            },

            M::Call => match ops {
                [O::Rel(d)] => {
                    self.op(&[0xe8]);
                    self.imm(*d, 4)
                }
                [target] => {
                    self.op(&[0xff]);
                    self.modrm(2, target)
                }
                _ => Err(bad(m, ops)),
            },

            M::Jcc(c) => match ops {
                [O::Rel(d)] if fits_i8(*d) => {
                    self.op(&[0x70 | c.bits()]);
                    self.imm(*d, 1)
                }
                [O::Rel(d)] => {
                    self.op(&[0x0f, 0x80 | c.bits()]);
                    self.imm(*d, 4)
                }
                _ => Err(bad(m, ops)),
            },

            M::Setcc(c) => match ops {
                [dst] if dst.size() == Some(Size::Byte) || dst.size().is_none() => {
                    self.op(&[0x0f, 0x90 | c.bits()]);
                    self.modrm(0, dst)
                }
                _ => Err(EncodeError::SizeMismatch(format!("`{}` writes one byte", m))),
            },

            M::Cmovcc(c) => match ops {
                [O::Reg(dst), src] => {
                    self.set_size(dst.size);
                    self.op(&[0x0f, 0x40 | c.bits()]);
                    self.note_reg(*dst);
                    self.modrm(enc_num(*dst), src)
                }
                _ => Err(bad(m, ops)),
            },

            M::Bswap => match ops {
                [O::Reg(r)] if r.size != Size::Byte => {
                    self.set_size(r.size);
                    self.note_reg(*r);
                    let n = enc_num(*r);
                    self.bb = n >= 8;
                    self.op_ok(&[0x0f, 0xc8 | (n & 7)])
                }
                _ => Err(bad(m, ops)),
            },

            M::Nop => match ops {
                [rm] => {
                    let size = rm.size().unwrap_or(Size::Dword);
                    self.set_size(size);
                    self.op(&[0x0f, 0x1f]);
                    self.modrm(0, rm)
                }
                _ => Err(bad(m, ops)),
            },

            other => Err(EncodeError::Unsupported(other.name())),
        }
    }
}

fn check_sizes(m: Mnemonic, a: Size, b: Size) -> Result<(), EncodeError> {
    if a != b {
        return Err(EncodeError::SizeMismatch(format!(
            "`{}` operands are {} and {}",
            m,
            a.keyword(),
            b.keyword()
        )));
    }
    Ok(())
}

/// Convenience for `Cond`-carrying mnemonics.
pub fn jcc(c: Cond, disp: i64) -> Result<Vec<u8>, EncodeError> {
    encode(Mnemonic::Jcc(c), &[Operand::Rel(disp)])
}

/// Encode a relative branch at an explicitly chosen width.
///
/// [`encode`] picks the short form whenever the displacement fits, which is
/// what you want in isolation but not what an assembler wants: the assembler's
/// relaxation pass needs to *commit* to a width and never shrink again, or the
/// layout can oscillate. `call` has no short form and ignores `short`.
pub fn encode_branch(m: Mnemonic, disp: i64, short: bool) -> Result<Vec<u8>, EncodeError> {
    let mut out = Vec::with_capacity(6);
    match m {
        Mnemonic::Jmp if short => {
            out.push(0xeb);
            out.push(check_i8(disp)? as u8);
        }
        Mnemonic::Jmp => {
            out.push(0xe9);
            out.extend_from_slice(&check_i32(disp)?.to_le_bytes());
        }
        Mnemonic::Jcc(c) if short => {
            out.push(0x70 | c.bits());
            out.push(check_i8(disp)? as u8);
        }
        Mnemonic::Jcc(c) => {
            out.push(0x0f);
            out.push(0x80 | c.bits());
            out.extend_from_slice(&check_i32(disp)?.to_le_bytes());
        }
        Mnemonic::Call => {
            out.push(0xe8);
            out.extend_from_slice(&check_i32(disp)?.to_le_bytes());
        }
        other => {
            return Err(EncodeError::Unsupported(format!("{} is not a relative branch", other)))
        }
    }
    Ok(out)
}

fn check_i8(v: i64) -> Result<i8, EncodeError> {
    i8::try_from(v).map_err(|_| EncodeError::ImmediateOutOfRange { value: v, bytes: 1 })
}

fn check_i32(v: i64) -> Result<i32, EncodeError> {
    i32::try_from(v).map_err(|_| EncodeError::ImmediateOutOfRange { value: v, bytes: 4 })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::decode;
    use crate::reg::Reg;

    fn enc(m: Mnemonic, ops: &[Operand]) -> Vec<u8> {
        encode(m, ops).expect("encodes")
    }

    #[test]
    fn encoder_and_decoder_are_inverses() {
        let cases: Vec<(Mnemonic, Vec<Operand>)> = vec![
            (Mnemonic::Mov, vec![Operand::Reg(Reg::RAX), Operand::Reg(Reg::RBX)]),
            (Mnemonic::Mov, vec![Operand::Reg(Reg::RAX), Operand::Imm(1)]),
            (Mnemonic::Add, vec![Operand::Reg(Reg::RSP), Operand::Imm(8)]),
            (Mnemonic::Sub, vec![Operand::Reg(Reg::RSP), Operand::Imm(0x1000)]),
            (Mnemonic::Push, vec![Operand::Reg(Reg::RBP)]),
            (Mnemonic::Pop, vec![Operand::Reg(Reg::new(15, Size::Qword))]),
            (
                Mnemonic::Mov,
                vec![
                    Operand::Reg(Reg::RAX),
                    Operand::Mem(Mem::new().base(Reg::RSP).disp(8).with_size(Size::Qword)),
                ],
            ),
            (
                Mnemonic::Lea,
                vec![
                    Operand::Reg(Reg::RAX),
                    Operand::Mem(Mem::new().base(Reg::RBX).index(Reg::RCX, 4)),
                ],
            ),
            (Mnemonic::Imul, vec![Operand::Reg(Reg::RAX), Operand::Reg(Reg::RCX)]),
            (Mnemonic::Ret, vec![]),
            (Mnemonic::Syscall, vec![]),
            (Mnemonic::Endbr64, vec![]),
        ];

        for (m, ops) in cases {
            let bytes = enc(m, &ops);
            let back = decode(&bytes, 0).unwrap_or_else(|e| panic!("{} {:?}: {e}", m, ops));
            assert_eq!(back.mnemonic, m, "mnemonic for {:02x?}", bytes);
            assert_eq!(back.operands, ops, "operands for {:02x?}", bytes);
            assert_eq!(back.len(), bytes.len());
        }
    }

    #[test]
    fn short_forms_are_preferred() {
        // add rsp, 8 -> 0x83 with a sign-extended imm8, not 0x81 with imm32.
        assert_eq!(
            enc(Mnemonic::Add, &[Operand::Reg(Reg::RSP), Operand::Imm(8)]),
            [0x48, 0x83, 0xc4, 0x08]
        );
        // mov rax, 1 -> 7-byte c7, not 10-byte movabs.
        assert_eq!(
            enc(Mnemonic::Mov, &[Operand::Reg(Reg::RAX), Operand::Imm(1)]),
            [0x48, 0xc7, 0xc0, 0x01, 0x00, 0x00, 0x00]
        );
        // shl eax, 1 -> d1, no immediate byte at all.
        assert_eq!(
            enc(Mnemonic::Shl, &[Operand::Reg(Reg::new(0, Size::Dword)), Operand::Imm(1)]),
            [0xd1, 0xe0]
        );
    }

    #[test]
    fn movabs_is_used_only_when_the_value_needs_it() {
        let big = enc(Mnemonic::Mov, &[Operand::Reg(Reg::RAX), Operand::Imm(0x1234_5678_9abc)]);
        assert_eq!(big.len(), 10);
        assert_eq!(big[1], 0xb8);
        // A negative value that fits a signed 32 uses the short form.
        let small = enc(Mnemonic::Mov, &[Operand::Reg(Reg::RAX), Operand::Imm(-1)]);
        assert_eq!(small.len(), 7);
    }

    #[test]
    fn rbp_as_a_base_needs_an_explicit_zero_displacement() {
        // [rax] encodes with no displacement...
        let a = enc(
            Mnemonic::Mov,
            &[
                Operand::Reg(Reg::RAX),
                Operand::Mem(Mem::new().base(Reg::RAX).with_size(Size::Qword)),
            ],
        );
        // ...but [rbp] must carry a zero disp8, because mod=00 rm=101 is
        // RIP-relative.
        let b = enc(
            Mnemonic::Mov,
            &[
                Operand::Reg(Reg::RAX),
                Operand::Mem(Mem::new().base(Reg::RBP).with_size(Size::Qword)),
            ],
        );
        assert_eq!(b.len(), a.len() + 1);
        assert_eq!(decode(&b, 0).unwrap().operands[1].as_mem().unwrap().base, Some(Reg::RBP));
    }

    #[test]
    fn rsp_as_a_base_gets_a_sib_byte_automatically() {
        let bytes = enc(
            Mnemonic::Mov,
            &[
                Operand::Reg(Reg::RAX),
                Operand::Mem(Mem::new().base(Reg::RSP).with_size(Size::Qword)),
            ],
        );
        assert_eq!(bytes, [0x48, 0x8b, 0x04, 0x24]);
    }

    #[test]
    fn rsp_cannot_be_an_index() {
        let m = Mem::new().base(Reg::RAX).index(Reg::RSP, 1).with_size(Size::Qword);
        let e = encode(Mnemonic::Mov, &[Operand::Reg(Reg::RAX), Operand::Mem(m)]).unwrap_err();
        assert!(matches!(e, EncodeError::Unsupported(_)));
    }

    #[test]
    fn ah_and_r8b_cannot_coexist() {
        let e = encode(
            Mnemonic::Mov,
            &[Operand::Reg(Reg::high(0)), Operand::Reg(Reg::new(8, Size::Byte))],
        )
        .unwrap_err();
        assert!(matches!(e, EncodeError::RexConflict { .. }));
    }

    #[test]
    fn a_bare_rex_appears_when_spl_is_named() {
        // mov al, spl needs REX even though no bit in it is set.
        let bytes = enc(
            Mnemonic::Mov,
            &[Operand::Reg(Reg::new(0, Size::Byte)), Operand::Reg(Reg::new(4, Size::Byte))],
        );
        assert_eq!(bytes, [0x40, 0x88, 0xe0]);
        // Without REX the same ModRM means `mov al, ah`.
        let bytes = enc(
            Mnemonic::Mov,
            &[Operand::Reg(Reg::new(0, Size::Byte)), Operand::Reg(Reg::high(0))],
        );
        assert_eq!(bytes, [0x88, 0xe0]);
    }

    #[test]
    fn ambiguous_memory_width_is_an_error_not_a_guess() {
        let m = Mem::new().base(Reg::RAX); // no size
        let e = encode(Mnemonic::Mov, &[Operand::Mem(m), Operand::Imm(1)]).unwrap_err();
        assert!(matches!(e, EncodeError::SizeMismatch(_)));
    }

    #[test]
    fn size_mismatches_are_rejected() {
        let e = encode(
            Mnemonic::Mov,
            &[Operand::Reg(Reg::RAX), Operand::Reg(Reg::new(0, Size::Dword))],
        )
        .unwrap_err();
        assert!(matches!(e, EncodeError::SizeMismatch(_)));
    }

    #[test]
    fn short_branches_are_chosen_when_the_displacement_fits() {
        assert_eq!(enc(Mnemonic::Jmp, &[Operand::Rel(-2)]), [0xeb, 0xfe]);
        assert_eq!(enc(Mnemonic::Jmp, &[Operand::Rel(0x100)]).len(), 5);
        assert_eq!(enc(Mnemonic::Jcc(Cond::E), &[Operand::Rel(0)]), [0x74, 0x00]);
        assert_eq!(enc(Mnemonic::Jcc(Cond::E), &[Operand::Rel(0x100)]).len(), 6);
        // call is always rel32; there is no short form.
        assert_eq!(enc(Mnemonic::Call, &[Operand::Rel(0)]).len(), 5);
    }

    #[test]
    fn immediates_out_of_range_are_rejected() {
        let e = encode(Mnemonic::Mov, &[Operand::Reg(Reg::new(0, Size::Byte)), Operand::Imm(300)])
            .unwrap_err();
        assert!(matches!(e, EncodeError::ImmediateOutOfRange { .. }));
    }
}
