//! The x86_64 instruction decoder.
//!
//! # How an x86_64 instruction is laid out
//!
//! ```text
//! [legacy prefixes] [REX] [opcode 1-3] [ModRM] [SIB] [displacement] [immediate]
//!    0..4 bytes    0..1     1..3        0..1    0..1     0/1/4        0/1/2/4/8
//! ```
//!
//! Nothing in the byte stream says which fields are present. The decoder learns
//! that as it goes: the opcode determines whether a ModRM byte follows, the
//! ModRM byte determines whether a SIB byte follows, and the two of them
//! together determine the displacement size. This is why x86 cannot be decoded
//! in parallel from an arbitrary offset, and why disassemblers that guess a
//! starting point can be led astray. It is also why the architectural maximum
//! instruction length — 15 bytes — has to be enforced explicitly rather than
//! falling out of the encoding.
//!
//! The decoder below walks those stages in order. Every byte it consumes is
//! recorded into an [`Encoding`], so a caller can show a student exactly which
//! field each byte belonged to.

use crate::error::DecodeError;
use crate::insn::{Cond, Encoding, Insn, Mnemonic, RepPrefix};
use crate::operand::{Mem, Operand};
use crate::reg::{Reg, Seg, Size};

/// Decode one instruction from the start of `code`, as if it lived at `ip`.
///
/// `ip` only affects [`Insn::ip`] and therefore the resolution of RIP-relative
/// operands and branch targets. Pass `0` if you do not care.
pub fn decode(code: &[u8], ip: u64) -> Result<Insn, DecodeError> {
    Dec::new(code, ip).decode()
}

/// Linear-sweep disassembly: decode instructions back to back until the buffer
/// is exhausted or an instruction fails to decode.
///
/// This is the naive strategy, and it is wrong in the presence of data mixed
/// into the text section or of jumps into the middle of an instruction. The
/// `Err` it yields tells you where it lost the thread.
#[derive(Debug)]
pub struct Decoder<'a> {
    code: &'a [u8],
    base_ip: u64,
    offset: usize,
    done: bool,
}

impl<'a> Decoder<'a> {
    pub fn new(code: &'a [u8], base_ip: u64) -> Decoder<'a> {
        Decoder { code, base_ip, offset: 0, done: false }
    }

    /// Byte offset of the next instruction to be decoded.
    pub fn offset(&self) -> usize {
        self.offset
    }
}

impl Iterator for Decoder<'_> {
    type Item = Result<Insn, DecodeError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done || self.offset >= self.code.len() {
            return None;
        }
        let ip = self.base_ip.wrapping_add(self.offset as u64);
        match decode(&self.code[self.offset..], ip) {
            Ok(insn) => {
                self.offset += insn.len();
                Some(Ok(insn))
            }
            Err(e) => {
                self.done = true;
                Some(Err(e))
            }
        }
    }
}

/// Architectural maximum instruction length. The hardware raises #GP on
/// anything longer, so a decoder that keeps consuming prefixes forever would
/// disagree with the CPU.
const MAX_INSN_LEN: usize = 15;

struct Dec<'a> {
    b: &'a [u8],
    pos: usize,
    ip: u64,
    enc: Encoding,
    /// Operand size after prefixes are applied.
    opsize: Size,
    /// Address size for memory operands: 64-bit unless `0x67` says otherwise.
    addrsize: Size,
    seg: Option<Seg>,
    lock: bool,
    rep: Option<RepPrefix>,
    /// The `0x66` prefix was seen. Kept separately from `opsize` because for
    /// some opcodes `0x66` selects an opcode rather than an operand size.
    o66: bool,
}

impl<'a> Dec<'a> {
    fn new(b: &'a [u8], ip: u64) -> Dec<'a> {
        Dec {
            b,
            pos: 0,
            ip,
            enc: Encoding::default(),
            opsize: Size::Dword,
            addrsize: Size::Qword,
            seg: None,
            lock: false,
            rep: None,
            o66: false,
        }
    }

    // ---- byte-level reads -------------------------------------------------

    fn peek(&self) -> Result<u8, DecodeError> {
        self.b.get(self.pos).copied().ok_or(DecodeError::Truncated { at: self.pos, need: 1 })
    }

    fn u8(&mut self) -> Result<u8, DecodeError> {
        let v = self.peek()?;
        self.pos += 1;
        if self.pos > MAX_INSN_LEN {
            return Err(DecodeError::TooLong);
        }
        Ok(v)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        if self.pos + n > self.b.len() {
            return Err(DecodeError::Truncated { at: self.pos, need: self.pos + n - self.b.len() });
        }
        let s = &self.b[self.pos..self.pos + n];
        self.pos += n;
        if self.pos > MAX_INSN_LEN {
            return Err(DecodeError::TooLong);
        }
        Ok(s)
    }

    fn disp(&mut self, n: usize) -> Result<i64, DecodeError> {
        let s = self.take(n)?;
        self.enc.disp.extend_from_slice(s);
        Ok(sign_extend(s))
    }

    fn imm(&mut self, n: usize) -> Result<i64, DecodeError> {
        let s = self.take(n)?;
        self.enc.imm.extend_from_slice(s);
        Ok(sign_extend(s))
    }

    /// An immediate whose width follows the operand size, capped at 4 bytes and
    /// sign-extended. This is the `imm16/32` of the manual: a 64-bit `add` still
    /// only carries a 32-bit immediate, sign-extended at execution time. It is
    /// why you cannot `add rax, 0x100000000` in one instruction.
    fn imm_z(&mut self) -> Result<i64, DecodeError> {
        match self.opsize {
            Size::Word => self.imm(2),
            _ => self.imm(4),
        }
    }

    // ---- register helpers -------------------------------------------------

    /// Select an 8-bit register. Without a REX prefix, encodings 4..=7 name the
    /// *high* bytes `ah`,`ch`,`dh`,`bh`. With any REX prefix at all they name
    /// `spl`,`bpl`,`sil`,`dil`. The prefix does not extend the register number
    /// here — it changes which register file the number indexes.
    fn gpr8(&self, num: u8) -> Reg {
        if self.enc.rex.is_none() && (4..8).contains(&num) {
            Reg::high(num - 4)
        } else {
            Reg::new(num, Size::Byte)
        }
    }

    fn gpr(&self, num: u8, size: Size) -> Reg {
        if size == Size::Byte {
            self.gpr8(num)
        } else {
            Reg::new(num, size)
        }
    }

    // ---- ModRM ------------------------------------------------------------

    /// Consume a ModRM byte (and any SIB / displacement it implies).
    ///
    /// Returns the `reg` field (already extended by REX.R) and the operand
    /// named by the `mod`+`rm` fields, sized as `rm_size`.
    fn modrm(&mut self, rm_size: Size) -> Result<(u8, Operand), DecodeError> {
        let m = self.u8()?;
        self.enc.modrm = Some(m);

        let md = m >> 6;
        let reg = ((m >> 3) & 7) | ((self.enc.rex_r() as u8) << 3);
        let rm = m & 7;

        // mod == 11: the rm field names a register, not memory.
        if md == 3 {
            let num = rm | ((self.enc.rex_b() as u8) << 3);
            return Ok((reg, Operand::Reg(self.gpr(num, rm_size))));
        }

        let mut mem = Mem::new();
        mem.seg = self.seg;
        mem.size = Some(rm_size);

        if rm == 4 {
            // rm == 100 is the escape that says "a SIB byte follows".
            // It is why you cannot use rsp as a base without one.
            let sib = self.u8()?;
            self.enc.sib = Some(sib);

            let idx = ((sib >> 3) & 7) | ((self.enc.rex_x() as u8) << 3);
            // index == 100 with REX.X clear means "no index register".
            // With REX.X set the same bits mean r12, which is why r12 *can* be
            // an index but rsp never can.
            if idx != 4 {
                mem.index = Some(Reg::new(idx, self.addrsize));
                mem.scale = 1 << (sib >> 6);
            }

            if (sib & 7) == 5 && md == 0 {
                // base == 101 with mod == 00 means "no base, disp32 instead".
                mem.disp = self.disp(4)?;
            } else {
                let base = (sib & 7) | ((self.enc.rex_b() as u8) << 3);
                mem.base = Some(Reg::new(base, self.addrsize));
            }
        } else if rm == 5 && md == 0 {
            // The 32-bit meaning of this slot was "absolute disp32". In 64-bit
            // mode it was repurposed as RIP-relative, and absolute addressing
            // now costs a SIB byte. Position-independent code got cheaper;
            // absolute addressing got more expensive. That trade is deliberate.
            mem.rip_relative = true;
            mem.disp = self.disp(4)?;
        } else {
            let base = rm | ((self.enc.rex_b() as u8) << 3);
            mem.base = Some(Reg::new(base, self.addrsize));
        }

        match md {
            1 => mem.disp = self.disp(1)?,
            2 => mem.disp = self.disp(4)?,
            _ => {}
        }

        Ok((reg, Operand::Mem(mem)))
    }

    /// Look at the ModRM byte's `reg` field without consuming it. Needed for
    /// the opcode groups, where `reg` selects the operation and therefore the
    /// operand size.
    fn peek_modrm_reg(&self) -> Result<u8, DecodeError> {
        Ok((self.peek()? >> 3) & 7)
    }

    // ---- top level --------------------------------------------------------

    fn decode(mut self) -> Result<Insn, DecodeError> {
        self.prefixes()?;

        let op = self.u8()?;
        self.enc.opcode.push(op);

        let (mnemonic, operands) = if op == 0x0f {
            let op2 = self.u8()?;
            self.enc.opcode.push(op2);
            self.two_byte(op2)?
        } else {
            self.one_byte(op)?
        };

        Ok(Insn {
            ip: self.ip,
            mnemonic,
            operands,
            lock: self.lock,
            rep: self.rep,
            op_size: Some(self.opsize),
            encoding: self.enc,
        })
    }

    /// Consume legacy prefixes, then the REX byte if one is present.
    ///
    /// Only a REX byte *immediately* before the opcode has any effect; a REX
    /// followed by another legacy prefix is silently ignored by the hardware.
    /// Handling the prefixes in this order reproduces that.
    fn prefixes(&mut self) -> Result<(), DecodeError> {
        loop {
            let b = self.peek()?;
            match b {
                0xf0 => self.lock = true,
                0xf2 => self.rep = Some(RepPrefix::Repnz),
                0xf3 => self.rep = Some(RepPrefix::Rep),
                0x66 => self.o66 = true,
                0x67 => self.addrsize = Size::Dword,
                0x26 | 0x2e | 0x36 | 0x3e | 0x64 | 0x65 => {
                    self.seg = Seg::from_prefix(b);
                }
                _ => break,
            }
            self.pos += 1;
            self.enc.legacy.push(b);
            if self.pos > MAX_INSN_LEN {
                return Err(DecodeError::TooLong);
            }
        }

        let b = self.peek()?;
        if (0x40..=0x4f).contains(&b) {
            self.enc.rex = Some(b);
            self.pos += 1;
        }

        // Operand size: REX.W wins over 0x66, which wins over the 32-bit default.
        self.opsize = if self.enc.rex_w() {
            Size::Qword
        } else if self.o66 {
            Size::Word
        } else {
            Size::Dword
        };
        Ok(())
    }

    /// Stack operations and near branches default to 64-bit operands in long
    /// mode, and there is no way to ask for 32. `0x66` still gets you 16.
    fn stack_size(&self) -> Size {
        if self.o66 {
            Size::Word
        } else {
            Size::Qword
        }
    }

    fn bad(&self, op: u8) -> DecodeError {
        DecodeError::BadOpcode { at: self.pos.saturating_sub(1), opcode: op }
    }

    fn invalid_long(&self, op: u8) -> DecodeError {
        DecodeError::InvalidInLongMode { at: self.pos.saturating_sub(1), opcode: op }
    }

    fn one_byte(&mut self, op: u8) -> Result<(Mnemonic, Vec<Operand>), DecodeError> {
        const ARITH: [Mnemonic; 8] = [
            Mnemonic::Add,
            Mnemonic::Or,
            Mnemonic::Adc,
            Mnemonic::Sbb,
            Mnemonic::And,
            Mnemonic::Sub,
            Mnemonic::Xor,
            Mnemonic::Cmp,
        ];

        match op {
            // ---- 0x00..0x3f: the eight arithmetic/logic operations ---------
            //
            // The opcode is `00 ooo ff` where `ooo` picks the operation and
            // `ff` picks the operand form. Recognising this regularity turns a
            // 64-entry table into six lines. The same shape reappears in the
            // `0x80` group, where `ooo` moves into the ModRM `reg` field.
            0x00..=0x3f if op & 7 <= 5 => {
                let m = ARITH[((op >> 3) & 7) as usize];
                let ops = self.arith_form(op & 7)?;
                Ok((m, ops))
            }
            0x06 | 0x07 | 0x0e | 0x16 | 0x17 | 0x1e | 0x1f | 0x27 | 0x2f | 0x37 | 0x3f => {
                Err(self.invalid_long(op))
            }

            // ---- 0x50..0x5f: push/pop a register ---------------------------
            0x50..=0x57 => {
                let n = (op & 7) | ((self.enc.rex_b() as u8) << 3);
                self.opsize = self.stack_size();
                Ok((Mnemonic::Push, vec![Operand::Reg(Reg::new(n, self.opsize))]))
            }
            0x58..=0x5f => {
                let n = (op & 7) | ((self.enc.rex_b() as u8) << 3);
                self.opsize = self.stack_size();
                Ok((Mnemonic::Pop, vec![Operand::Reg(Reg::new(n, self.opsize))]))
            }

            0x60 | 0x61 | 0x62 | 0x82 | 0x9a | 0xc4 | 0xc5 | 0xce | 0xd4 | 0xd5 | 0xd6 | 0xea => {
                Err(self.invalid_long(op))
            }

            // movsxd: the only sign-extending move with its own one-byte opcode.
            0x63 => {
                let (reg, rm) = self.modrm(Size::Dword)?;
                Ok((Mnemonic::Movsxd, vec![Operand::Reg(Reg::new(reg, self.opsize)), rm]))
            }

            0x68 => {
                let v = self.imm_z()?;
                self.opsize = self.stack_size();
                Ok((Mnemonic::Push, vec![Operand::Imm(v)]))
            }
            0x6a => {
                let v = self.imm(1)?;
                self.opsize = self.stack_size();
                Ok((Mnemonic::Push, vec![Operand::Imm(v)]))
            }

            // Three-operand imul. Note the destination is a register only — the
            // form with a memory destination does not exist.
            0x69 | 0x6b => {
                let (reg, rm) = self.modrm(self.opsize)?;
                let v = if op == 0x6b { self.imm(1)? } else { self.imm_z()? };
                Ok((
                    Mnemonic::Imul,
                    vec![Operand::Reg(Reg::new(reg, self.opsize)), rm, Operand::Imm(v)],
                ))
            }

            0x70..=0x7f => {
                let d = self.imm(1)?;
                Ok((Mnemonic::Jcc(Cond::from_bits(op & 0xf)), vec![Operand::Rel(d)]))
            }

            // ---- Group 1: the same eight operations, immediate source -------
            0x80 | 0x81 | 0x83 => {
                let size = if op == 0x80 { Size::Byte } else { self.opsize };
                let (reg, rm) = self.modrm(size)?;
                // 0x83 sign-extends an 8-bit immediate to the operand size.
                // `add rax, 1` is 4 bytes because of it; without it, 7.
                let v = match op {
                    0x80 => self.imm(1)?,
                    0x83 => self.imm(1)?,
                    _ => self.imm_z()?,
                };
                Ok((ARITH[reg as usize & 7], vec![rm, Operand::Imm(v)]))
            }

            0x84 | 0x85 => {
                let size = if op == 0x84 { Size::Byte } else { self.opsize };
                let (reg, rm) = self.modrm(size)?;
                Ok((Mnemonic::Test, vec![rm, Operand::Reg(self.gpr(reg, size))]))
            }
            0x86 | 0x87 => {
                let size = if op == 0x86 { Size::Byte } else { self.opsize };
                let (reg, rm) = self.modrm(size)?;
                Ok((Mnemonic::Xchg, vec![rm, Operand::Reg(self.gpr(reg, size))]))
            }

            // ---- mov, in its four register/memory forms ---------------------
            0x88 | 0x89 => {
                let size = if op == 0x88 { Size::Byte } else { self.opsize };
                let (reg, rm) = self.modrm(size)?;
                Ok((Mnemonic::Mov, vec![rm, Operand::Reg(self.gpr(reg, size))]))
            }
            0x8a | 0x8b => {
                let size = if op == 0x8a { Size::Byte } else { self.opsize };
                let (reg, rm) = self.modrm(size)?;
                Ok((Mnemonic::Mov, vec![Operand::Reg(self.gpr(reg, size)), rm]))
            }

            // lea computes an address and never touches memory. The size on the
            // memory operand is meaningless here, so it is stripped.
            0x8d => {
                let (reg, rm) = self.modrm(self.opsize)?;
                let mem = match rm {
                    Operand::Mem(mut m) => {
                        m.size = None;
                        Operand::Mem(m)
                    }
                    // `lea reg, reg` is not encodable; mod==11 is illegal here.
                    _ => return Err(self.bad(op)),
                };
                Ok((Mnemonic::Lea, vec![Operand::Reg(Reg::new(reg, self.opsize)), mem]))
            }

            0x8f => {
                self.opsize = self.stack_size();
                let (_, rm) = self.modrm(self.opsize)?;
                Ok((Mnemonic::Pop, vec![rm]))
            }

            // 0x90 is `xchg eax, eax`, which does nothing — so it is spelled
            // `nop`. With REX.B it is `xchg r8, rax`, which does something.
            0x90 if !self.enc.rex_b() => Ok((Mnemonic::Nop, vec![])),
            0x90..=0x97 => {
                let n = (op & 7) | ((self.enc.rex_b() as u8) << 3);
                Ok((
                    Mnemonic::Xchg,
                    vec![
                        Operand::Reg(Reg::new(n, self.opsize)),
                        Operand::Reg(Reg::new(0, self.opsize)),
                    ],
                ))
            }

            0x98 => Ok((
                match self.opsize {
                    Size::Word => Mnemonic::Cbw,
                    Size::Dword => Mnemonic::Cwde,
                    _ => Mnemonic::Cdqe,
                },
                vec![],
            )),
            0x99 => Ok((
                match self.opsize {
                    Size::Word => Mnemonic::Cwd,
                    Size::Dword => Mnemonic::Cdq,
                    _ => Mnemonic::Cqo,
                },
                vec![],
            )),

            0xa8 => {
                let v = self.imm(1)?;
                Ok((Mnemonic::Test, vec![Operand::Reg(self.gpr8(0)), Operand::Imm(v)]))
            }
            0xa9 => {
                let v = self.imm_z()?;
                Ok((Mnemonic::Test, vec![Operand::Reg(Reg::new(0, self.opsize)), Operand::Imm(v)]))
            }

            0xb0..=0xb7 => {
                let n = (op & 7) | ((self.enc.rex_b() as u8) << 3);
                let v = self.imm(1)?;
                Ok((Mnemonic::Mov, vec![Operand::Reg(self.gpr8(n)), Operand::Imm(v)]))
            }
            // The only instruction that can carry a full 64-bit immediate.
            // Assemblers call the 64-bit form `movabs`.
            0xb8..=0xbf => {
                let n = (op & 7) | ((self.enc.rex_b() as u8) << 3);
                let v = match self.opsize {
                    Size::Word => self.imm(2)?,
                    Size::Dword => self.imm(4)?,
                    _ => self.imm(8)?,
                };
                Ok((Mnemonic::Mov, vec![Operand::Reg(Reg::new(n, self.opsize)), Operand::Imm(v)]))
            }

            0xc0 | 0xc1 => {
                let size = if op == 0xc0 { Size::Byte } else { self.opsize };
                let (reg, rm) = self.modrm(size)?;
                let v = self.imm(1)?;
                Ok((shift_op(reg), vec![rm, Operand::Imm(v)]))
            }
            0xd0 | 0xd1 => {
                let size = if op == 0xd0 { Size::Byte } else { self.opsize };
                let (reg, rm) = self.modrm(size)?;
                Ok((shift_op(reg), vec![rm, Operand::Imm(1)]))
            }
            0xd2 | 0xd3 => {
                let size = if op == 0xd2 { Size::Byte } else { self.opsize };
                let (reg, rm) = self.modrm(size)?;
                Ok((shift_op(reg), vec![rm, Operand::Reg(self.gpr8(1))]))
            }

            0xc2 => {
                let v = self.imm(2)?;
                Ok((Mnemonic::Ret, vec![Operand::Imm(v)]))
            }
            0xc3 => Ok((Mnemonic::Ret, vec![])),

            0xc6 | 0xc7 => {
                let size = if op == 0xc6 { Size::Byte } else { self.opsize };
                let (reg, rm) = self.modrm(size)?;
                if reg & 7 != 0 {
                    return Err(self.bad(op));
                }
                let v = if op == 0xc6 { self.imm(1)? } else { self.imm_z()? };
                Ok((Mnemonic::Mov, vec![rm, Operand::Imm(v)]))
            }

            0xc9 => Ok((Mnemonic::Leave, vec![])),

            // 0xcc is a one-byte instruction on purpose: a debugger can plant a
            // breakpoint by overwriting a single byte, whatever instruction was
            // there before. See the debugging chapter.
            0xcc => Ok((Mnemonic::Int3, vec![])),
            0xcd => {
                let v = self.imm(1)?;
                Ok((Mnemonic::Int, vec![Operand::Imm(v)]))
            }

            0xe8 => {
                let d = self.imm(4)?;
                Ok((Mnemonic::Call, vec![Operand::Rel(d)]))
            }
            0xe9 => {
                let d = self.imm(4)?;
                Ok((Mnemonic::Jmp, vec![Operand::Rel(d)]))
            }
            0xeb => {
                let d = self.imm(1)?;
                Ok((Mnemonic::Jmp, vec![Operand::Rel(d)]))
            }

            0xf4 => Ok((Mnemonic::Hlt, vec![])),

            // ---- Group 3 ----------------------------------------------------
            0xf6 | 0xf7 => {
                let size = if op == 0xf6 { Size::Byte } else { self.opsize };
                let ext = self.peek_modrm_reg()?;
                let (_, rm) = self.modrm(size)?;
                match ext {
                    // /0 and /1 both mean `test`, with an immediate.
                    0 | 1 => {
                        let v = if op == 0xf6 { self.imm(1)? } else { self.imm_z()? };
                        Ok((Mnemonic::Test, vec![rm, Operand::Imm(v)]))
                    }
                    2 => Ok((Mnemonic::Not, vec![rm])),
                    3 => Ok((Mnemonic::Neg, vec![rm])),
                    4 => Ok((Mnemonic::Mul, vec![rm])),
                    5 => Ok((Mnemonic::Imul, vec![rm])),
                    6 => Ok((Mnemonic::Div, vec![rm])),
                    _ => Ok((Mnemonic::Idiv, vec![rm])),
                }
            }

            // ---- Groups 4 and 5 ---------------------------------------------
            0xfe => {
                let ext = self.peek_modrm_reg()?;
                let (_, rm) = self.modrm(Size::Byte)?;
                match ext {
                    0 => Ok((Mnemonic::Inc, vec![rm])),
                    1 => Ok((Mnemonic::Dec, vec![rm])),
                    _ => Err(self.bad(op)),
                }
            }
            0xff => {
                let ext = self.peek_modrm_reg()?;
                // call/jmp/push through memory are 64-bit operations regardless
                // of REX.W; inc/dec follow the operand size like everything else.
                let size = match ext {
                    2 | 4 | 6 => self.stack_size(),
                    _ => self.opsize,
                };
                let (_, rm) = self.modrm(size)?;
                match ext {
                    0 => Ok((Mnemonic::Inc, vec![rm])),
                    1 => Ok((Mnemonic::Dec, vec![rm])),
                    2 => Ok((Mnemonic::Call, vec![rm])),
                    4 => Ok((Mnemonic::Jmp, vec![rm])),
                    6 => Ok((Mnemonic::Push, vec![rm])),
                    _ => Err(self.bad(op)),
                }
            }

            _ => Err(self.bad(op)),
        }
    }

    /// The six operand forms shared by every `0x00..0x3f` arithmetic opcode.
    fn arith_form(&mut self, form: u8) -> Result<Vec<Operand>, DecodeError> {
        Ok(match form {
            // op r/m8, r8
            0 => {
                let (reg, rm) = self.modrm(Size::Byte)?;
                vec![rm, Operand::Reg(self.gpr8(reg))]
            }
            // op r/m, r
            1 => {
                let (reg, rm) = self.modrm(self.opsize)?;
                vec![rm, Operand::Reg(Reg::new(reg, self.opsize))]
            }
            // op r8, r/m8
            2 => {
                let (reg, rm) = self.modrm(Size::Byte)?;
                vec![Operand::Reg(self.gpr8(reg)), rm]
            }
            // op r, r/m
            3 => {
                let (reg, rm) = self.modrm(self.opsize)?;
                vec![Operand::Reg(Reg::new(reg, self.opsize)), rm]
            }
            // op al, imm8
            4 => {
                let v = self.imm(1)?;
                vec![Operand::Reg(self.gpr8(0)), Operand::Imm(v)]
            }
            // op eAX, imm16/32
            _ => {
                let v = self.imm_z()?;
                vec![Operand::Reg(Reg::new(0, self.opsize)), Operand::Imm(v)]
            }
        })
    }

    fn two_byte(&mut self, op2: u8) -> Result<(Mnemonic, Vec<Operand>), DecodeError> {
        match op2 {
            0x05 => Ok((Mnemonic::Syscall, vec![])),
            0x0b => Ok((Mnemonic::Ud2, vec![])),

            // `f3 0f 1e fa` — the CET landing pad. Chosen to be a no-op on
            // older CPUs, which decode it as a `nop` with a redundant prefix.
            0x1e if self.rep == Some(RepPrefix::Rep) && self.peek()? == 0xfa => {
                let m = self.u8()?;
                self.enc.modrm = Some(m);
                Ok((Mnemonic::Endbr64, vec![]))
            }

            // Multi-byte nop. Compilers emit these to align branch targets;
            // one long nop retires faster than several short ones.
            0x1f => {
                let (_, rm) = self.modrm(self.opsize)?;
                Ok((Mnemonic::Nop, vec![rm]))
            }

            0x40..=0x4f => {
                let (reg, rm) = self.modrm(self.opsize)?;
                Ok((
                    Mnemonic::Cmovcc(Cond::from_bits(op2 & 0xf)),
                    vec![Operand::Reg(Reg::new(reg, self.opsize)), rm],
                ))
            }

            0x80..=0x8f => {
                let d = self.imm(4)?;
                Ok((Mnemonic::Jcc(Cond::from_bits(op2 & 0xf)), vec![Operand::Rel(d)]))
            }

            0x90..=0x9f => {
                let (_, rm) = self.modrm(Size::Byte)?;
                Ok((Mnemonic::Setcc(Cond::from_bits(op2 & 0xf)), vec![rm]))
            }

            0xaf => {
                let (reg, rm) = self.modrm(self.opsize)?;
                Ok((Mnemonic::Imul, vec![Operand::Reg(Reg::new(reg, self.opsize)), rm]))
            }

            // movzx/movsx: the destination width comes from the operand size,
            // the source width from the opcode. This is the only place in the
            // integer ISA where one instruction names two different widths.
            0xb6 | 0xb7 | 0xbe | 0xbf => {
                let src = if op2 & 1 == 0 { Size::Byte } else { Size::Word };
                let (reg, rm) = self.modrm(src)?;
                let m = if op2 < 0xbe { Mnemonic::Movzx } else { Mnemonic::Movsx };
                Ok((m, vec![Operand::Reg(Reg::new(reg, self.opsize)), rm]))
            }

            0xc8..=0xcf => {
                let n = (op2 & 7) | ((self.enc.rex_b() as u8) << 3);
                Ok((Mnemonic::Bswap, vec![Operand::Reg(Reg::new(n, self.opsize))]))
            }

            _ => Err(self.bad(op2)),
        }
    }
}

/// ModRM `reg` field to shift/rotate mnemonic. `/6` is `sal`, an alias for
/// `shl` — the two encodings are interchangeable and mean the same thing,
/// because arithmetic and logical *left* shifts are identical operations.
fn shift_op(reg: u8) -> Mnemonic {
    match reg & 7 {
        0 => Mnemonic::Rol,
        1 => Mnemonic::Ror,
        2 => Mnemonic::Rcl,
        3 => Mnemonic::Rcr,
        4 | 6 => Mnemonic::Shl,
        5 => Mnemonic::Shr,
        _ => Mnemonic::Sar,
    }
}

/// Read a little-endian signed integer of 1, 2, 4 or 8 bytes.
fn sign_extend(bytes: &[u8]) -> i64 {
    match bytes.len() {
        1 => bytes[0] as i8 as i64,
        2 => i16::from_le_bytes([bytes[0], bytes[1]]) as i64,
        4 => i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64,
        8 => i64::from_le_bytes(bytes.try_into().unwrap()),
        _ => unreachable!("displacement and immediate widths are 1, 2, 4 or 8"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dis(bytes: &[u8]) -> String {
        crate::format::to_string(&decode(bytes, 0).expect("decodes"))
    }

    #[test]
    fn every_decode_consumes_exactly_the_bytes_it_reports() {
        // Reassembling the recorded Encoding must reproduce the input.
        let cases: &[&[u8]] = &[
            &[0x48, 0x89, 0xe5],
            &[0x48, 0x8b, 0x44, 0x24, 0x08],
            &[0x48, 0x8d, 0x05, 0x10, 0x00, 0x00, 0x00],
            &[0x0f, 0xaf, 0xc1],
            &[0x48, 0xb8, 1, 2, 3, 4, 5, 6, 7, 8],
            &[0xf3, 0x0f, 0x1e, 0xfa],
        ];
        for c in cases {
            let insn = decode(c, 0).unwrap();
            assert_eq!(insn.bytes(), *c, "round-trip failed for {:02x?}", c);
            assert_eq!(insn.len(), c.len());
        }
    }

    #[test]
    fn rex_w_selects_64_bit_operands() {
        assert_eq!(dis(&[0x89, 0xd8]), "mov eax, ebx");
        assert_eq!(dis(&[0x48, 0x89, 0xd8]), "mov rax, rbx");
        assert_eq!(dis(&[0x66, 0x89, 0xd8]), "mov ax, bx");
    }

    #[test]
    fn rex_b_extends_the_register_number() {
        assert_eq!(dis(&[0x50]), "push rax");
        assert_eq!(dis(&[0x41, 0x50]), "push r8");
    }

    #[test]
    fn rex_presence_switches_ah_to_spl() {
        assert_eq!(dis(&[0x88, 0xe0]), "mov al, ah");
        // Same ModRM, but a REX prefix renames register 4 from ah to spl.
        assert_eq!(dis(&[0x40, 0x88, 0xe0]), "mov al, spl");
    }

    #[test]
    fn rsp_as_a_base_forces_a_sib_byte() {
        let insn = decode(&[0x48, 0x8b, 0x44, 0x24, 0x08], 0).unwrap();
        assert!(insn.encoding.sib.is_some());
        assert_eq!(crate::format::to_string(&insn), "mov rax, qword [rsp+0x8]");
    }

    #[test]
    fn r12_can_be_an_index_but_rsp_cannot() {
        // index field 100 with REX.X set names r12.
        let insn = decode(&[0x4a, 0x8b, 0x04, 0x20], 0).unwrap();
        assert_eq!(crate::format::to_string(&insn), "mov rax, qword [rax+r12]");
        // The same field with REX.X clear means "no index".
        let insn = decode(&[0x48, 0x8b, 0x04, 0x20], 0).unwrap();
        assert_eq!(crate::format::to_string(&insn), "mov rax, qword [rax]");
    }

    #[test]
    fn rip_relative_resolves_against_the_next_instruction() {
        // lea rax, [rip+0x10] at 0x1000; the instruction is 7 bytes long.
        let insn = decode(&[0x48, 0x8d, 0x05, 0x10, 0x00, 0x00, 0x00], 0x1000).unwrap();
        let mem = insn.operands[1].as_mem().unwrap();
        assert!(mem.rip_relative);
        assert_eq!(mem.effective_address(insn.next_ip(), |_| 0), 0x1017);
    }

    #[test]
    fn sib_base_101_with_mod_00_means_absolute() {
        // mov eax, [0x404000] — needs a SIB byte with no base and no index.
        let insn = decode(&[0x8b, 0x04, 0x25, 0x00, 0x40, 0x40, 0x00], 0).unwrap();
        let mem = insn.operands[1].as_mem().unwrap();
        assert!(mem.is_absolute());
        assert_eq!(mem.disp, 0x404000);
    }

    #[test]
    fn branch_targets_are_relative_to_the_end_of_the_instruction() {
        // eb fe is the classic two-byte infinite loop: jump back over yourself.
        let insn = decode(&[0xeb, 0xfe], 0x1000).unwrap();
        assert_eq!(insn.branch_target(), Some(0x1000));

        let insn = decode(&[0xe8, 0x00, 0x00, 0x00, 0x00], 0x1000).unwrap();
        assert_eq!(insn.branch_target(), Some(0x1005), "call +0 lands on the next instruction");
    }

    #[test]
    fn group1_0x83_sign_extends_its_immediate() {
        let insn = decode(&[0x48, 0x83, 0xc0, 0xff], 0).unwrap();
        assert_eq!(insn.operands[1], Operand::Imm(-1));
        assert_eq!(crate::format::to_string(&insn), "add rax, -0x1");
    }

    #[test]
    fn movabs_is_the_only_64_bit_immediate() {
        let insn =
            decode(&[0x48, 0xb8, 0xef, 0xcd, 0xab, 0x89, 0x67, 0x45, 0x23, 0x01], 0).unwrap();
        assert_eq!(insn.operands[1], Operand::Imm(0x0123_4567_89ab_cdef));
        assert_eq!(insn.encoding.imm.len(), 8);
    }

    #[test]
    fn condition_codes_come_from_the_low_opcode_nibble() {
        assert_eq!(dis(&[0x74, 0x00]), "je 0x2");
        assert_eq!(dis(&[0x75, 0x00]), "jne 0x2");
        assert_eq!(dis(&[0x0f, 0x8f, 0, 0, 0, 0]), "jg 0x6");
    }

    #[test]
    fn opcode_groups_dispatch_on_the_modrm_reg_field() {
        assert_eq!(dis(&[0xf7, 0xd0]), "not eax");
        assert_eq!(dis(&[0xf7, 0xd8]), "neg eax");
        assert_eq!(dis(&[0xf7, 0xe0]), "mul eax");
        assert_eq!(dis(&[0xf7, 0xf8]), "idiv eax");
        assert_eq!(dis(&[0xc1, 0xe0, 0x03]), "shl eax, 0x3");
        assert_eq!(dis(&[0xc1, 0xf8, 0x03]), "sar eax, 0x3");
    }

    #[test]
    fn ff_group_call_and_jmp_are_64_bit_without_rex() {
        assert_eq!(dis(&[0xff, 0xd0]), "call rax");
        assert_eq!(dis(&[0xff, 0xe0]), "jmp rax");
        assert_eq!(dis(&[0xff, 0xc0]), "inc eax");
    }

    #[test]
    fn nop_is_xchg_eax_eax_unless_rex_b_makes_it_real() {
        assert_eq!(dis(&[0x90]), "nop");
        assert_eq!(dis(&[0x41, 0x90]), "xchg r8d, eax");
    }

    #[test]
    fn endbr64_hides_behind_a_rep_prefixed_nop() {
        assert_eq!(dis(&[0xf3, 0x0f, 0x1e, 0xfa]), "endbr64");
    }

    #[test]
    fn truncated_input_reports_how_much_was_missing() {
        assert_eq!(decode(&[0x48, 0x8b], 0), Err(DecodeError::Truncated { at: 2, need: 1 }));
        assert!(matches!(decode(&[], 0), Err(DecodeError::Truncated { .. })));
    }

    #[test]
    fn thirty_two_bit_only_opcodes_are_rejected() {
        assert!(matches!(decode(&[0x06], 0), Err(DecodeError::InvalidInLongMode { .. })));
        assert!(matches!(decode(&[0x60], 0), Err(DecodeError::InvalidInLongMode { .. })));
    }

    #[test]
    fn a_long_run_of_prefixes_is_rejected_not_looped_on() {
        let bytes = [0x66u8; 20];
        assert_eq!(decode(&bytes, 0), Err(DecodeError::TooLong));
    }

    #[test]
    fn linear_sweep_walks_a_whole_function() {
        // push rbp; mov rbp, rsp; mov eax, edi; pop rbp; ret
        let code = [0x55, 0x48, 0x89, 0xe5, 0x89, 0xf8, 0x5d, 0xc3];
        let insns: Vec<_> = Decoder::new(&code, 0x1000).collect::<Result<_, _>>().unwrap();
        let text: Vec<String> = insns.iter().map(crate::format::to_string).collect();
        assert_eq!(text, ["push rbp", "mov rbp, rsp", "mov eax, edi", "pop rbp", "ret"]);
        assert_eq!(insns[0].ip, 0x1000);
        assert_eq!(insns[4].ip, 0x1007);
    }

    #[test]
    fn lea_strips_the_operand_size_because_it_never_loads() {
        let insn = decode(&[0x48, 0x8d, 0x04, 0x0b], 0).unwrap();
        assert_eq!(crate::format::to_string(&insn), "lea rax, [rbx+rcx]");
        assert_eq!(insn.operands[1].as_mem().unwrap().size, None);
    }

    #[test]
    fn lock_and_segment_prefixes_are_recorded() {
        let insn = decode(&[0xf0, 0x48, 0x01, 0x08], 0).unwrap();
        assert!(insn.lock);
        let insn = decode(&[0x64, 0x48, 0x8b, 0x04, 0x25, 0x28, 0, 0, 0], 0).unwrap();
        assert_eq!(insn.operands[1].as_mem().unwrap().seg, Some(Seg::Fs));
    }
}
