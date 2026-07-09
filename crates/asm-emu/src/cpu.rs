//! The interpreter itself: fetch, decode, execute one instruction at a time,
//! recording every effect.
//!
//! The loop is intentionally boring — fetch bytes, hand them to `asm-core`'s
//! decoder, dispatch on the mnemonic. All the interesting behaviour lives in
//! the per-instruction handlers and in the ALU. Nothing here is allowed to
//! panic on hostile input: every memory touch and every divide returns a
//! [`Fault`].

use crate::alu::{self, Af, ShiftOp};
use crate::effects::{Effects, MemRead, MemWrite, RegWrite, Run, Stop, SyscallEvent};
use crate::fault::Fault;
use crate::flags::Flags;
use crate::mem::{Memory, Perms};
use crate::regs::Regs;
use asm_core::{decode, Insn, Mnemonic, Operand, Reg, Size};

/// The architectural instruction-length ceiling. We never fetch more than this
/// for a single instruction.
const MAX_INSN_LEN: usize = 15;

/// Top of the initial stack. The stack grows down from just below here. The
/// exact value is cosmetic, but a high canonical address makes stack pointers
/// easy to recognise in the UI.
const STACK_TOP: u64 = 0x7fff_0000_0000;
const STACK_SIZE: usize = 64 * 1024;

/// A whole machine: registers, instruction pointer, flags, memory, and the I/O
/// and halt bookkeeping the run loop needs.
#[derive(Debug)]
pub struct Cpu {
    pub regs: Regs,
    pub rip: u64,
    pub flags: Flags,
    pub mem: Memory,
    pub steps: u64,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    /// Set once a terminating instruction runs (`hlt`, `exit`, `int3`). The run
    /// loop checks it; `step` on its own never blocks on it, so a caller
    /// single-stepping past a `hlt` simply sees no further effects.
    halt: Option<Stop>,
}

impl Cpu {
    /// Build a CPU over a pre-populated memory map. `rip`, the registers and the
    /// flags all start cleared; map a stack and set `rsp` yourself, or use
    /// [`Cpu::with_code`].
    pub fn new(mem: Memory) -> Cpu {
        Cpu {
            regs: Regs::new(),
            rip: 0,
            flags: Flags::default(),
            mem,
            steps: 0,
            stdout: Vec::new(),
            stderr: Vec::new(),
            halt: None,
        }
    }

    /// The common case: drop `code` at `base` as an `r-x` "text" region, give it
    /// a 64 KiB `rw-` "stack", point `rsp` just below the top and `rip` at the
    /// first byte of code.
    pub fn with_code(code: &[u8], base: u64) -> Cpu {
        let mut mem = Memory::new();
        mem.map_with(base, code.to_vec(), Perms::RX, "text");
        let stack_base = STACK_TOP - STACK_SIZE as u64;
        mem.map(stack_base, STACK_SIZE, Perms::RW, "stack");
        let mut cpu = Cpu::new(mem);
        cpu.rip = base;
        // Leave one qword of headroom below the top so the very first push has a
        // valid slot; no sentinel return address is planted.
        cpu.regs.write(Reg::RSP, STACK_TOP - 8);
        cpu
    }

    /// Bytes written to fd 1.
    /// The reason the machine stopped, if it has.
    ///
    /// `run` consults this internally, but a caller driving the CPU one
    /// instruction at a time needs it too: `hlt`, `int3` and the `exit`
    /// syscall all set it, and only the last of those is visible from the
    /// instruction alone. Re-deriving the stop condition from the mnemonic
    /// outside this crate would silently miss `exit`.
    pub fn stopped(&self) -> Option<&Stop> {
        self.halt.as_ref()
    }

    pub fn stdout(&self) -> &[u8] {
        &self.stdout
    }

    /// Bytes written to fd 2.
    pub fn stderr(&self) -> &[u8] {
        &self.stderr
    }

    /// Execute a single instruction, returning the record of what it changed.
    pub fn step(&mut self) -> Result<Effects, Fault> {
        let rip = self.rip;
        let window = self.mem.fetch_slice(rip, MAX_INSN_LEN)?;
        let insn = decode(&window, rip).map_err(|e| Fault::Decode(e.to_string()))?;

        let mut eff = Effects::new(insn.clone(), rip, self.flags);
        // Advance rip to the fall-through address up front; branch handlers
        // overwrite it when they redirect control flow.
        self.rip = insn.next_ip();
        self.execute(&insn, &mut eff)?;
        eff.rip_after = self.rip;
        eff.flags_after = self.flags;
        self.steps += 1;
        Ok(eff)
    }

    /// Run until the program stops, faults, or `max_steps` instructions have
    /// executed. The trace holds at most `max_steps` effect records.
    pub fn run(&mut self, max_steps: u64) -> Run {
        let mut trace = Vec::new();
        let mut steps = 0u64;
        loop {
            if let Some(stop) = self.halt.clone() {
                return Run { stop, steps, trace };
            }
            if steps >= max_steps {
                return Run { stop: Stop::StepLimit, steps, trace };
            }
            match self.step() {
                Ok(eff) => {
                    steps += 1;
                    if (trace.len() as u64) < max_steps {
                        trace.push(eff);
                    }
                }
                Err(f) => return Run { stop: Stop::Fault(f), steps, trace },
            }
        }
    }

    // -- operand plumbing ---------------------------------------------------

    /// The natural width of this instruction when the operands do not pin it
    /// down (e.g. an immediate-only push).
    fn op_size(insn: &Insn) -> Size {
        insn.op_size.unwrap_or(Size::Qword)
    }

    /// The width of a specific operand, falling back to the instruction width.
    fn width_of(op: &Operand, fallback: Size) -> Size {
        op.size().unwrap_or(fallback)
    }

    /// Compute a memory operand's effective address. Register reads for the
    /// base/index are not themselves recorded as effects — only the resulting
    /// memory access is.
    fn ea(&self, m: &asm_core::Mem, next_ip: u64) -> u64 {
        m.effective_address(next_ip, |r| self.regs.read(r))
    }

    /// Read an operand's value as a width-masked integer, recording a memory
    /// read if the operand is in memory.
    fn read_op(
        &mut self,
        eff: &mut Effects,
        op: Operand,
        next_ip: u64,
        size: Size,
    ) -> Result<u64, Fault> {
        match op {
            Operand::Reg(r) => Ok(self.regs.read(r)),
            Operand::Imm(v) => Ok(v as u64 & size.mask()),
            Operand::Mem(m) => {
                let addr = self.ea(&m, next_ip);
                self.load(eff, addr, size.bytes() as usize)
            }
            Operand::Rel(_) => {
                Err(Fault::UnsupportedInstruction("relative operand used as a value".into()))
            }
        }
    }

    /// Write a value to an operand, recording the effect.
    fn write_op(
        &mut self,
        eff: &mut Effects,
        op: Operand,
        next_ip: u64,
        size: Size,
        val: u64,
    ) -> Result<(), Fault> {
        match op {
            Operand::Reg(r) => {
                self.put_reg(eff, r, val);
                Ok(())
            }
            Operand::Mem(m) => {
                let addr = self.ea(&m, next_ip);
                let bytes = u64_to_le(val, size.bytes() as usize);
                self.store(eff, addr, &bytes)
            }
            _ => Err(Fault::UnsupportedInstruction("write to a non-writable operand".into())),
        }
    }

    /// Write a register and log the full-width before/after so the UI can show
    /// zero-extension and high-byte aliasing effects.
    fn put_reg(&mut self, eff: &mut Effects, r: Reg, val: u64) {
        let before = self.regs.read_full(r.num);
        self.regs.write(r, val);
        let after = self.regs.read_full(r.num);
        eff.reg_writes.push(RegWrite { reg: Reg::new(r.num, Size::Qword).name(), before, after });
    }

    /// A recorded memory load.
    fn load(&mut self, eff: &mut Effects, addr: u64, n: usize) -> Result<u64, Fault> {
        let bytes = self.mem.read(addr, n)?;
        eff.mem_reads.push(MemRead { addr, bytes: bytes.clone() });
        Ok(le_to_u64(&bytes))
    }

    /// A recorded memory load returning the raw bytes (for sizes > 8 or when the
    /// caller wants the vector).
    fn load_bytes(&mut self, eff: &mut Effects, addr: u64, n: usize) -> Result<Vec<u8>, Fault> {
        let bytes = self.mem.read(addr, n)?;
        eff.mem_reads.push(MemRead { addr, bytes: bytes.clone() });
        Ok(bytes)
    }

    /// A recorded memory store.
    fn store(&mut self, eff: &mut Effects, addr: u64, bytes: &[u8]) -> Result<(), Fault> {
        let before = self.mem.write_capturing(addr, bytes)?;
        eff.mem_writes.push(MemWrite { addr, before, after: bytes.to_vec() });
        Ok(())
    }

    // -- stack helpers ------------------------------------------------------

    /// Push `val` as `size` bytes: decrement rsp *then* store. Order matters —
    /// the new value lands at the new, lower rsp.
    fn push(&mut self, eff: &mut Effects, val: u64, size: Size) -> Result<(), Fault> {
        let n = size.bytes() as u64;
        let sp = self.regs.read(Reg::RSP).wrapping_sub(n);
        self.put_reg(eff, Reg::RSP, sp);
        let bytes = u64_to_le(val, n as usize);
        self.store(eff, sp, &bytes)
    }

    /// Pop `size` bytes: load at rsp, then increment rsp past them.
    fn pop(&mut self, eff: &mut Effects, size: Size) -> Result<u64, Fault> {
        let n = size.bytes() as u64;
        let sp = self.regs.read(Reg::RSP);
        let val = self.load(eff, sp, n as usize)?;
        self.put_reg(eff, Reg::RSP, sp.wrapping_add(n));
        Ok(val)
    }

    // -- the dispatcher -----------------------------------------------------

    fn execute(&mut self, insn: &Insn, eff: &mut Effects) -> Result<(), Fault> {
        let next_ip = insn.next_ip();
        let osz = Self::op_size(insn);
        let ops = &insn.operands;

        use Mnemonic as M;
        match insn.mnemonic {
            // ---- binary arithmetic / logic --------------------------------
            M::Add | M::Or | M::Adc | M::Sbb | M::And | M::Sub | M::Xor => {
                let dst = op(ops, 0)?;
                let src = op(ops, 1)?;
                let size = Self::width_of(&dst, osz);
                let a = self.read_op(eff, dst, next_ip, size)?;
                let b = self.read_op(eff, src, next_ip, size)?;
                let (res, af) = self.binop(insn.mnemonic, a, b, size);
                af.apply(&mut self.flags);
                self.write_op(eff, dst, next_ip, size, res)?;
            }
            // cmp and test compute the same arithmetic but discard the result,
            // keeping only the flags. That is their entire purpose.
            M::Cmp => {
                let dst = op(ops, 0)?;
                let src = op(ops, 1)?;
                let size = Self::width_of(&dst, osz);
                let a = self.read_op(eff, dst, next_ip, size)?;
                let b = self.read_op(eff, src, next_ip, size)?;
                let (_, af) = alu::sub(a, b, size);
                af.apply(&mut self.flags);
            }
            M::Test => {
                let dst = op(ops, 0)?;
                let src = op(ops, 1)?;
                let size = Self::width_of(&dst, osz);
                let a = self.read_op(eff, dst, next_ip, size)?;
                let b = self.read_op(eff, src, next_ip, size)?;
                alu::logic(a & b, size).apply(&mut self.flags);
            }

            // ---- unary ----------------------------------------------------
            M::Not => {
                let dst = op(ops, 0)?;
                let size = Self::width_of(&dst, osz);
                let a = self.read_op(eff, dst, next_ip, size)?;
                self.write_op(eff, dst, next_ip, size, !a & size.mask())?;
            }
            M::Neg => {
                let dst = op(ops, 0)?;
                let size = Self::width_of(&dst, osz);
                let a = self.read_op(eff, dst, next_ip, size)?;
                let (res, af) = alu::neg(a, size);
                af.apply(&mut self.flags);
                self.write_op(eff, dst, next_ip, size, res)?;
            }
            M::Inc => {
                let dst = op(ops, 0)?;
                let size = Self::width_of(&dst, osz);
                let a = self.read_op(eff, dst, next_ip, size)?;
                let (res, af) = alu::inc(a, size, self.flags.cf);
                af.apply(&mut self.flags);
                self.write_op(eff, dst, next_ip, size, res)?;
            }
            M::Dec => {
                let dst = op(ops, 0)?;
                let size = Self::width_of(&dst, osz);
                let a = self.read_op(eff, dst, next_ip, size)?;
                let (res, af) = alu::dec(a, size, self.flags.cf);
                af.apply(&mut self.flags);
                self.write_op(eff, dst, next_ip, size, res)?;
            }

            // ---- multiply / divide ----------------------------------------
            M::Mul => self.exec_mul(eff, ops, next_ip, osz)?,
            M::Imul => self.exec_imul(eff, ops, next_ip, osz)?,
            M::Div => self.exec_div(eff, ops, next_ip, osz, false)?,
            M::Idiv => self.exec_div(eff, ops, next_ip, osz, true)?,

            // ---- data movement --------------------------------------------
            M::Mov => {
                let dst = op(ops, 0)?;
                let src = op(ops, 1)?;
                let size = Self::width_of(&dst, osz);
                let v = self.read_op(eff, src, next_ip, size)?;
                self.write_op(eff, dst, next_ip, size, v)?;
            }
            M::Movzx => {
                let dst = op(ops, 0)?;
                let src = op(ops, 1)?;
                let ssz = Self::width_of(&src, osz);
                let v = self.read_op(eff, src, next_ip, ssz)?;
                // Zero-extend is just the masked value; put_reg handles the
                // destination width (incl. the 32-bit clear-upper rule).
                self.write_op(eff, dst, next_ip, Self::width_of(&dst, osz), v)?;
            }
            M::Movsx | M::Movsxd => {
                let dst = op(ops, 0)?;
                let src = op(ops, 1)?;
                let ssz = Self::width_of(&src, osz);
                let v = self.read_op(eff, src, next_ip, ssz)?;
                let sext = alu::sign_extend(v, ssz) as u64;
                self.write_op(eff, dst, next_ip, Self::width_of(&dst, osz), sext)?;
            }
            M::Lea => {
                // lea computes the address and stores it, without ever touching
                // memory — so it cannot fault on an unmapped operand.
                let dst = op(ops, 0)?;
                let src = op(ops, 1)?;
                let m = src.as_mem().ok_or_else(|| {
                    Fault::UnsupportedInstruction("lea without a memory operand".into())
                })?;
                let addr = self.ea(&m, next_ip);
                let size = Self::width_of(&dst, osz);
                self.write_op(eff, dst, next_ip, size, addr & size.mask())?;
            }
            M::Xchg => {
                let a = op(ops, 0)?;
                let b = op(ops, 1)?;
                let size = Self::width_of(&a, osz);
                let va = self.read_op(eff, a, next_ip, size)?;
                let vb = self.read_op(eff, b, next_ip, size)?;
                self.write_op(eff, a, next_ip, size, vb)?;
                self.write_op(eff, b, next_ip, size, va)?;
            }

            // ---- shifts and rotates ---------------------------------------
            M::Shl | M::Shr | M::Sar | M::Rol | M::Ror => {
                self.exec_shift(eff, insn, next_ip, osz)?
            }
            M::Rcl | M::Rcr => {
                return Err(Fault::UnsupportedInstruction(insn.mnemonic.name()));
            }

            // ---- stack ----------------------------------------------------
            M::Push => {
                let src = op(ops, 0)?;
                // push is always at the stack width; the decoder sets op_size
                // accordingly (8 in long mode, or 2 with a 0x66 prefix).
                let v = self.read_op(eff, src, next_ip, osz)?;
                self.push(eff, v, osz)?;
            }
            M::Pop => {
                let dst = op(ops, 0)?;
                let v = self.pop(eff, osz)?;
                self.write_op(eff, dst, next_ip, osz, v)?;
            }
            M::Leave => {
                // leave == `mov rsp, rbp; pop rbp`. It tears down the frame the
                // matching `push rbp; mov rbp, rsp` prologue built.
                let rbp = self.regs.read(Reg::RBP);
                self.put_reg(eff, Reg::RSP, rbp);
                let v = self.pop(eff, Size::Qword)?;
                self.put_reg(eff, Reg::RBP, v);
            }

            // ---- control flow ---------------------------------------------
            M::Jmp => {
                let target = self.branch_dest(eff, insn, next_ip)?;
                self.rip = target;
            }
            M::Jcc(cond) => {
                if self.flags.eval(cond) {
                    let target = insn.branch_target().ok_or_else(|| {
                        Fault::UnsupportedInstruction("jcc without a relative target".into())
                    })?;
                    self.rip = target;
                }
                // Not taken: rip already points at the fall-through.
            }
            M::Call => {
                let target = self.branch_dest(eff, insn, next_ip)?;
                // Push the return address — the address of the *next*
                // instruction, which is exactly where `ret` will come back to.
                self.push(eff, next_ip, Size::Qword)?;
                self.rip = target;
            }
            M::Ret => {
                let ret = self.pop(eff, Size::Qword)?;
                // `ret imm16` additionally pops `imm` bytes of arguments.
                if let Some(Operand::Imm(extra)) = ops.first().copied() {
                    let sp = self.regs.read(Reg::RSP).wrapping_add(extra as u64);
                    self.put_reg(eff, Reg::RSP, sp);
                }
                self.rip = ret;
            }

            // ---- conditional set / move -----------------------------------
            M::Setcc(cond) => {
                let dst = op(ops, 0)?;
                let v = self.flags.eval(cond) as u64;
                self.write_op(eff, dst, next_ip, Size::Byte, v)?;
            }
            M::Cmovcc(cond) => {
                let dst = op(ops, 0)?;
                let src = op(ops, 1)?;
                let size = Self::width_of(&dst, osz);
                // The source is read unconditionally (matching hardware, which
                // will fault on a bad memory source even when not taken); only
                // the write is conditional.
                let v = self.read_op(eff, src, next_ip, size)?;
                if self.flags.eval(cond) {
                    self.write_op(eff, dst, next_ip, size, v)?;
                }
            }

            // ---- sign-extension of the accumulator ------------------------
            M::Cbw => {
                let v = alu::sign_extend(self.regs.read(Reg::new(0, Size::Byte)), Size::Byte);
                self.put_reg(eff, Reg::new(0, Size::Word), v as u64);
            }
            M::Cwde => {
                let v = alu::sign_extend(self.regs.read(Reg::new(0, Size::Word)), Size::Word);
                self.put_reg(eff, Reg::new(0, Size::Dword), v as u64);
            }
            M::Cdqe => {
                let v = alu::sign_extend(self.regs.read(Reg::new(0, Size::Dword)), Size::Dword);
                self.put_reg(eff, Reg::new(0, Size::Qword), v as u64);
            }
            // cwd/cdq/cqo broadcast the accumulator's sign bit across the
            // matching D register — the setup every signed `idiv` needs.
            M::Cwd => self.sign_into_d(eff, Size::Word),
            M::Cdq => self.sign_into_d(eff, Size::Dword),
            M::Cqo => self.sign_into_d(eff, Size::Qword),

            M::Bswap => {
                let dst = op(ops, 0)?;
                let size = Self::width_of(&dst, osz);
                let v = self.read_op(eff, dst, next_ip, size)?;
                let swapped = match size {
                    Size::Qword => v.swap_bytes(),
                    Size::Dword => (v as u32).swap_bytes() as u64,
                    // bswap on a 16-bit register is architecturally undefined;
                    // real CPUs zero it. We reproduce that.
                    _ => 0,
                };
                self.write_op(eff, dst, next_ip, size, swapped)?;
            }

            // ---- syscalls and terminators ---------------------------------
            M::Syscall => self.exec_syscall(eff)?,
            M::Nop | M::Endbr64 => {}
            M::Hlt => self.halt = Some(Stop::Halted),
            M::Int3 => self.halt = Some(Stop::Breakpoint(eff.rip_before)),
            M::Ud2 => {
                return Err(Fault::UnsupportedInstruction("ud2 (guaranteed #UD)".into()));
            }
            M::Int => {
                return Err(Fault::UnsupportedInstruction("int (software interrupt)".into()));
            }
            M::Unknown => return Err(Fault::UnsupportedInstruction("unknown opcode".into())),
            // `Mnemonic` is `#[non_exhaustive]`; anything asm-core adds later
            // that we have not taught the interpreter about faults cleanly.
            other => return Err(Fault::UnsupportedInstruction(other.name())),
        }
        Ok(())
    }

    /// Dispatch the seven read-modify-write arithmetic/logic ops to the ALU.
    fn binop(&self, m: Mnemonic, a: u64, b: u64, size: Size) -> (u64, Af) {
        match m {
            Mnemonic::Add => alu::add(a, b, size),
            Mnemonic::Adc => alu::adc(a, b, self.flags.cf, size),
            Mnemonic::Sub => alu::sub(a, b, size),
            Mnemonic::Sbb => alu::sbb(a, b, self.flags.cf, size),
            Mnemonic::And => ((a & b) & size.mask(), alu::logic(a & b, size)),
            Mnemonic::Or => ((a | b) & size.mask(), alu::logic(a | b, size)),
            Mnemonic::Xor => ((a ^ b) & size.mask(), alu::logic(a ^ b, size)),
            // Unreachable: only the seven mnemonics above reach here.
            _ => (a, alu::logic(a, size)),
        }
    }

    /// The destination of a `jmp`/`call`: either a relative target resolved by
    /// asm-core, or an indirect one read from a register/memory operand.
    fn branch_dest(&mut self, eff: &mut Effects, insn: &Insn, next_ip: u64) -> Result<u64, Fault> {
        if let Some(t) = insn.branch_target() {
            return Ok(t);
        }
        let target = op(&insn.operands, 0)?;
        self.read_op(eff, target, next_ip, Size::Qword)
    }

    /// Broadcast the accumulator's sign bit into the D register at `size`.
    fn sign_into_d(&mut self, eff: &mut Effects, size: Size) {
        let a = self.regs.read(Reg::new(0, size));
        let fill = if alu::sign_extend(a, size) < 0 { size.mask() } else { 0 };
        self.put_reg(eff, Reg::new(2, size), fill);
    }

    // -- multiply / divide --------------------------------------------------

    fn exec_mul(
        &mut self,
        eff: &mut Effects,
        ops: &[Operand],
        next_ip: u64,
        osz: Size,
    ) -> Result<(), Fault> {
        let src = op(ops, 0)?;
        let size = Self::width_of(&src, osz);
        let a = self.regs.read(Reg::new(0, size));
        let b = self.read_op(eff, src, next_ip, size)?;
        let full = (a as u128) * (b as u128);
        let m = size.mask() as u128;
        let low = (full & m) as u64;
        let high = ((full >> size.bits()) & m) as u64;
        self.store_wide(eff, size, low, high);
        // CF and OF are set iff the high half is non-zero — i.e. the product did
        // not fit in the low half. The other flags are left undefined (here,
        // unchanged), which the manual explicitly permits.
        let overflow = high != 0;
        self.flags.cf = overflow;
        self.flags.of = overflow;
        Ok(())
    }

    fn exec_imul(
        &mut self,
        eff: &mut Effects,
        ops: &[Operand],
        next_ip: u64,
        osz: Size,
    ) -> Result<(), Fault> {
        match ops.len() {
            // One-operand form: signed full-width product into rDX:rAX.
            1 => {
                let src = op(ops, 0)?;
                let size = Self::width_of(&src, osz);
                let a = alu::sign_extend(self.regs.read(Reg::new(0, size)), size) as i128;
                let b = alu::sign_extend(self.read_op(eff, src, next_ip, size)?, size) as i128;
                let full = a * b;
                let m = size.mask() as u128;
                let low = (full as u128 & m) as u64;
                let high = ((full as u128 >> size.bits()) & m) as u64;
                self.store_wide(eff, size, low, high);
                let overflow = (alu::sign_extend(low, size) as i128) != full;
                self.flags.cf = overflow;
                self.flags.of = overflow;
            }
            // Two- and three-operand forms: truncated product into the named
            // register. CF/OF flag whether the true product overflowed.
            n => {
                let dst = op(ops, 0)?;
                let size = Self::width_of(&dst, osz);
                let (aop, bop) =
                    if n == 2 { (op(ops, 0)?, op(ops, 1)?) } else { (op(ops, 1)?, op(ops, 2)?) };
                let a = alu::sign_extend(self.read_op(eff, aop, next_ip, size)?, size) as i128;
                let b = alu::sign_extend(self.read_op(eff, bop, next_ip, size)?, size) as i128;
                let full = a * b;
                let low = (full as u128 & size.mask() as u128) as u64;
                let overflow = (alu::sign_extend(low, size) as i128) != full;
                self.write_op(eff, dst, next_ip, size, low)?;
                self.flags.cf = overflow;
                self.flags.of = overflow;
            }
        }
        Ok(())
    }

    /// Store a double-width product/quotient pair: the low half in the
    /// accumulator, the high half in rDX — except for 8-bit, where the whole
    /// 16-bit result lives in AX and there is no DX half.
    fn store_wide(&mut self, eff: &mut Effects, size: Size, low: u64, high: u64) {
        if size == Size::Byte {
            // 8-bit multiply: AL*src -> AX. low is AL's product byte, high is
            // AH's; write them together as AX.
            self.put_reg(eff, Reg::new(0, Size::Word), low | (high << 8));
        } else {
            self.put_reg(eff, Reg::new(0, size), low);
            self.put_reg(eff, Reg::new(2, size), high);
        }
    }

    fn exec_div(
        &mut self,
        eff: &mut Effects,
        ops: &[Operand],
        next_ip: u64,
        osz: Size,
        signed: bool,
    ) -> Result<(), Fault> {
        let src = op(ops, 0)?;
        let size = Self::width_of(&src, osz);
        let divisor = self.read_op(eff, src, next_ip, size)?;
        if divisor == 0 {
            return Err(Fault::DivideByZero);
        }

        // Assemble the (possibly double-width) dividend, do the division with an
        // overflow check, and write quotient/remainder back to their homes.
        // Flags after a divide are architecturally undefined, so we leave them.
        if size == Size::Byte {
            // 8-bit: dividend is AX; results are AL (quotient) and AH (rem).
            let dividend = self.regs.read(Reg::new(0, Size::Word));
            let (q, r) = divmod(dividend as u128, divisor, size, signed, 8)?;
            self.put_reg(eff, Reg::new(0, Size::Byte), q);
            self.put_reg(eff, Reg::high(0), r);
            return Ok(());
        }
        let hi = self.regs.read(Reg::new(2, size));
        let lo = self.regs.read(Reg::new(0, size));
        let dividend = ((hi as u128) << size.bits()) | lo as u128;
        let (q, r) = divmod(dividend, divisor, size, signed, size.bits())?;
        self.put_reg(eff, Reg::new(0, size), q);
        self.put_reg(eff, Reg::new(2, size), r);
        Ok(())
    }

    // -- shifts -------------------------------------------------------------

    fn exec_shift(
        &mut self,
        eff: &mut Effects,
        insn: &Insn,
        next_ip: u64,
        osz: Size,
    ) -> Result<(), Fault> {
        let dst = op(&insn.operands, 0)?;
        let cnt = op(&insn.operands, 1)?;
        let size = Self::width_of(&dst, osz);
        let a = self.read_op(eff, dst, next_ip, size)?;
        // The count comes from an immediate or from CL; only the low byte is
        // meaningful, and the ALU masks it further to 5 or 6 bits.
        let raw = self.read_op(eff, cnt, next_ip, Size::Byte)? & 0xff;
        let sop = match insn.mnemonic {
            Mnemonic::Shl => ShiftOp::Shl,
            Mnemonic::Shr => ShiftOp::Shr,
            Mnemonic::Sar => ShiftOp::Sar,
            Mnemonic::Rol => ShiftOp::Rol,
            _ => ShiftOp::Ror,
        };
        let r = alu::shift(sop, a, raw, size);
        self.write_op(eff, dst, next_ip, size, r.value)?;
        if let Some(f) = r.flags {
            self.flags.cf = f.cf;
            if let Some(of) = f.of {
                self.flags.of = of;
            }
            if let Some((sf, zf, pf)) = f.szp {
                self.flags.sf = sf;
                self.flags.zf = zf;
                self.flags.pf = pf;
            }
        }
        Ok(())
    }

    // -- syscalls -----------------------------------------------------------

    fn exec_syscall(&mut self, eff: &mut Effects) -> Result<(), Fault> {
        // Linux x86_64 ABI: number in rax, args in rdi, rsi, rdx, r10, r8, r9.
        let number = self.regs.read(Reg::RAX);
        let args = [
            self.regs.read(Reg::RDI),
            self.regs.read(Reg::RSI),
            self.regs.read(Reg::RDX),
            self.regs.read(Reg::new(10, Size::Qword)),
            self.regs.read(Reg::new(8, Size::Qword)),
            self.regs.read(Reg::new(9, Size::Qword)),
        ];

        let (name, result) = match number {
            1 => ("write", self.sys_write(eff, args)?),
            60 | 231 => {
                let name = if number == 60 { "exit" } else { "exit_group" };
                self.halt = Some(Stop::Exited(args[0] as i32));
                (name, 0)
            }
            other => return Err(Fault::UnknownSyscall(other)),
        };

        // The kernel returns through rax. Record it as a register write so the
        // UI shows the result landing.
        self.put_reg(eff, Reg::RAX, result);
        eff.syscall = Some(SyscallEvent { number, name: name.to_string(), args, result });
        Ok(())
    }

    /// `write(fd, buf, count)`. Only fds 1 (stdout) and 2 (stderr) are backed;
    /// there is no host file access of any kind.
    fn sys_write(&mut self, eff: &mut Effects, args: [u64; 6]) -> Result<u64, Fault> {
        let (fd, buf, count) = (args[0], args[1], args[2] as usize);
        let bytes = self.load_bytes(eff, buf, count)?;
        match fd {
            1 => self.stdout.extend_from_slice(&bytes),
            2 => self.stderr.extend_from_slice(&bytes),
            // A real kernel would return -EBADF; we surface it as an unknown
            // syscall target rather than inventing errno semantics.
            _ => return Err(Fault::UnknownSyscall(1)),
        }
        Ok(count as u64)
    }
}

/// Fetch operand `i`, or report an internal inconsistency as an unsupported
/// instruction rather than panicking on an out-of-range index.
fn op(ops: &[Operand], i: usize) -> Result<Operand, Fault> {
    ops.get(i).copied().ok_or_else(|| Fault::UnsupportedInstruction(format!("missing operand {i}")))
}

/// Signed/unsigned division with the x86 overflow check. `q_bits` is the width
/// of the quotient's home register, used to detect a quotient that will not fit
/// — which raises #DE just like a zero divisor does.
fn divmod(
    dividend: u128,
    divisor: u64,
    size: Size,
    signed: bool,
    q_bits: u32,
) -> Result<(u64, u64), Fault> {
    if signed {
        // Sign-extend both to i128 for the division.
        let dvd = sign_extend_wide(dividend, size, q_bits);
        let dvs = alu::sign_extend(divisor, size) as i128;
        if dvs == 0 {
            return Err(Fault::DivideByZero);
        }
        let q = dvd.checked_div(dvs).ok_or(Fault::DivideByZero)?;
        let r = dvd % dvs;
        // The quotient must fit the signed range of the destination width.
        let (lo, hi) = signed_range(q_bits);
        if q < lo || q > hi {
            return Err(Fault::DivideByZero);
        }
        Ok((q as u64 & mask_bits(q_bits), r as u64 & mask_bits(q_bits)))
    } else {
        let q = dividend / divisor as u128;
        let r = dividend % divisor as u128;
        if q > mask_bits(q_bits) as u128 {
            return Err(Fault::DivideByZero);
        }
        Ok(((q as u64) & mask_bits(q_bits), (r as u64) & mask_bits(q_bits)))
    }
}

/// Sign-extend a `2*width`-bit dividend held in a u128 to i128. The dividend
/// occupies `2 * q_bits` bits (rDX:rAX), except for the 8-bit case where the
/// full 16-bit AX is already the value.
fn sign_extend_wide(dividend: u128, size: Size, q_bits: u32) -> i128 {
    let total_bits = if size == Size::Byte { 16 } else { q_bits * 2 };
    if total_bits >= 128 {
        return dividend as i128;
    }
    let sign = 1u128 << (total_bits - 1);
    if dividend & sign != 0 {
        // Set the high bits above `total_bits`.
        (dividend | !((1u128 << total_bits) - 1)) as i128
    } else {
        dividend as i128
    }
}

fn mask_bits(bits: u32) -> u64 {
    if bits >= 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    }
}

fn signed_range(bits: u32) -> (i128, i128) {
    if bits >= 64 {
        (i64::MIN as i128, i64::MAX as i128)
    } else {
        let half = 1i128 << (bits - 1);
        (-half, half - 1)
    }
}

/// Little-endian bytes to a u64 (up to 8 bytes; extra are ignored).
fn le_to_u64(bytes: &[u8]) -> u64 {
    let mut v = 0u64;
    for (i, &b) in bytes.iter().enumerate().take(8) {
        v |= (b as u64) << (i * 8);
    }
    v
}

/// A u64 to `n` little-endian bytes.
fn u64_to_le(val: u64, n: usize) -> Vec<u8> {
    (0..n).map(|i| (val >> (i * 8)) as u8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::Stop;
    use crate::fault::Access;
    use asm_core::asm::assemble_at;

    const BASE: u64 = 0x1000;

    /// Assemble Intel-syntax `src` for `BASE`, load it, and run it. The stack is
    /// provided by `with_code`; any data the program defines lives in the (also
    /// readable) text region.
    fn run_src(src: &str, max: u64) -> (Cpu, Run) {
        let asm = assemble_at(src, BASE).expect("assembles");
        let mut cpu = Cpu::with_code(&asm.bytes, BASE);
        let run = cpu.run(max);
        (cpu, run)
    }

    fn rax(cpu: &Cpu) -> u64 {
        cpu.regs.read_full(0)
    }

    #[test]
    fn xor_eax_eax_zeroes_all_of_rax() {
        // Preload rax with garbage, then clear it via the 32-bit xor.
        let (cpu, run) = run_src("mov rax, -1\nxor eax, eax\nhlt", 10);
        assert_eq!(run.stop, Stop::Halted);
        assert_eq!(rax(&cpu), 0, "the 32-bit xor cleared the whole 64-bit register");
    }

    #[test]
    fn mov_eax_minus_one_is_zero_extended() {
        let (cpu, _) = run_src("mov eax, -1\nhlt", 10);
        assert_eq!(rax(&cpu), 0xffff_ffff);
    }

    #[test]
    fn mov_al_leaves_upper_bits() {
        let (cpu, _) = run_src("mov rax, -1\nmov al, 0\nhlt", 10);
        assert_eq!(rax(&cpu), 0xffff_ffff_ffff_ff00);
    }

    #[test]
    fn add_signed_overflow_sets_of_clears_cf() {
        // 0x7fff... + 1 -> OF set, CF clear.
        let (cpu, _) = run_src("mov rax, 0x7fffffffffffffff\nadd rax, 1\nhlt", 10);
        assert_eq!(rax(&cpu), 0x8000_0000_0000_0000);
        assert!(cpu.flags.of);
        assert!(!cpu.flags.cf);
    }

    #[test]
    fn sub_unsigned_borrow_sets_cf_clears_of() {
        // 0 - 1 -> CF set (borrow), OF clear.
        let (cpu, _) = run_src("xor eax, eax\nsub rax, 1\nhlt", 10);
        assert_eq!(rax(&cpu), u64::MAX);
        assert!(cpu.flags.cf);
        assert!(!cpu.flags.of);
    }

    #[test]
    fn inc_does_not_disturb_carry() {
        // Set CF with a borrowing sub, then inc and confirm CF survives.
        let (cpu, _) = run_src("xor eax, eax\nsub rax, 1\nmov rbx, 5\ninc rbx\nhlt", 10);
        assert_eq!(cpu.regs.read_full(3), 6); // rbx
        assert!(cpu.flags.cf, "inc must leave CF untouched so loops can carry it");
    }

    #[test]
    fn cmp_one_and_minus_one_signed_vs_unsigned() {
        // cmp 1, -1: unsigned 1 is *below* 0xffff..ff, but signed 1 is *not*
        // less than -1. So jb is taken and jl is not — the opposite reflex trap.
        let src_jb = "mov rax, 1\ncmp rax, -1\njb taken\nmov rbx, 0\nhlt\ntaken:\nmov rbx, 1\nhlt";
        let (cpu, _) = run_src(src_jb, 20);
        assert_eq!(cpu.regs.read_full(3), 1, "jb taken (1 is unsigned-below -1)");

        let src_jl = "mov rax, 1\ncmp rax, -1\njl taken\nmov rbx, 0\nhlt\ntaken:\nmov rbx, 1\nhlt";
        let (cpu, _) = run_src(src_jl, 20);
        assert_eq!(cpu.regs.read_full(3), 0, "jl not taken (1 is signed-greater than -1)");

        let src_jg = "mov rax, 1\ncmp rax, -1\njg taken\nmov rbx, 0\nhlt\ntaken:\nmov rbx, 1\nhlt";
        let (cpu, _) = run_src(src_jg, 20);
        assert_eq!(cpu.regs.read_full(3), 1, "jg taken");
    }

    #[test]
    fn je_and_jne_take_the_right_branch() {
        let taken = "mov rax, 5\ncmp rax, 5\nje t\nmov rbx, 9\nhlt\nt:\nmov rbx, 1\nhlt";
        let (cpu, _) = run_src(taken, 20);
        assert_eq!(cpu.regs.read_full(3), 1);

        let nottaken = "mov rax, 5\ncmp rax, 6\nje t\nmov rbx, 9\nhlt\nt:\nmov rbx, 1\nhlt";
        let (cpu, _) = run_src(nottaken, 20);
        assert_eq!(cpu.regs.read_full(3), 9);
    }

    #[test]
    fn recursive_factorial_via_call_ret_push_pop() {
        let src = "\
            mov rdi, 5
            call fact
            hlt
        fact:
            cmp rdi, 1
            jg recurse
            mov rax, 1
            ret
        recurse:
            push rdi
            dec rdi
            call fact
            pop rdi
            imul rax, rdi
            ret";
        let (cpu, run) = run_src(src, 1000);
        assert_eq!(run.stop, Stop::Halted);
        assert_eq!(rax(&cpu), 120, "5! = 120");
    }

    #[test]
    fn loop_sums_an_array_from_memory() {
        let src = "\
            mov rsi, arr
            xor eax, eax
            xor rcx, rcx
        loop:
            cmp rcx, 5
            jge done
            add eax, [rsi + rcx*4]
            inc rcx
            jmp loop
        done:
            hlt
        arr:
            dd 1, 2, 3, 4, 5";
        let (cpu, run) = run_src(src, 1000);
        assert_eq!(run.stop, Stop::Halted);
        assert_eq!(rax(&cpu), 15);
        assert_eq!(cpu.regs.read_full(1), 5, "rcx counted to 5");
    }

    #[test]
    fn stack_grows_down_and_pop_restores_rsp() {
        let src = "mov rax, 0x1234\npush rax\npush rbx\npop rbx\npop rax\nhlt";
        let asm = assemble_at(src, BASE).unwrap();
        let mut cpu = Cpu::with_code(&asm.bytes, BASE);
        let sp0 = cpu.regs.read(Reg::RSP);
        // Step through the first push and confirm rsp dropped by 8.
        cpu.step().unwrap(); // mov
        cpu.step().unwrap(); // push rax
        assert_eq!(cpu.regs.read(Reg::RSP), sp0 - 8);
        let run = cpu.run(100);
        assert_eq!(run.stop, Stop::Halted);
        assert_eq!(cpu.regs.read(Reg::RSP), sp0, "balanced push/pop restored rsp");
        assert_eq!(rax(&cpu), 0x1234);
    }

    #[test]
    fn divide_by_zero_faults() {
        let (_, run) = run_src("mov eax, 10\nxor edx, edx\nmov ecx, 0\ndiv ecx\nhlt", 10);
        assert_eq!(run.stop, Stop::Fault(Fault::DivideByZero));
    }

    #[test]
    fn division_quotient_overflow_faults() {
        // edx:eax / 1 with edx != 0 overflows a 32-bit quotient.
        let (_, run) = run_src("mov eax, 0\nmov edx, 1\nmov ecx, 1\ndiv ecx\nhlt", 10);
        assert_eq!(run.stop, Stop::Fault(Fault::DivideByZero));
    }

    #[test]
    fn unsigned_divide_produces_quotient_and_remainder() {
        // 17 / 5 = 3 remainder 2.
        let (cpu, _) = run_src("mov eax, 17\nxor edx, edx\nmov ecx, 5\ndiv ecx\nhlt", 10);
        assert_eq!(rax(&cpu), 3);
        assert_eq!(cpu.regs.read_full(2), 2); // rdx = remainder
    }

    #[test]
    fn signed_divide_handles_negatives() {
        // -17 / 5 = -3 remainder -2 (truncation toward zero).
        let (cpu, _) = run_src("mov eax, -17\ncdq\nmov ecx, 5\nidiv ecx\nhlt", 10);
        assert_eq!(cpu.regs.read_full(0) as i32, -3);
        assert_eq!(cpu.regs.read_full(2) as i32, -2);
    }

    #[test]
    fn execute_on_non_executable_page_faults() {
        let mut mem = Memory::new();
        // Writable but not executable — a stack-like page.
        mem.map(0x2000, 16, Perms::RW, "data");
        let mut cpu = Cpu::new(mem);
        cpu.rip = 0x2000;
        let e = cpu.step().unwrap_err();
        assert!(matches!(e, Fault::Permission { access: Access::Fetch, .. }), "got {e:?}");
    }

    #[test]
    fn write_to_readonly_code_page_faults() {
        // The text region is r-x; storing into it is a protection fault.
        let (_, run) = run_src("mov rax, 0x1000\nmov byte [rax], 1\nhlt", 10);
        assert!(
            matches!(run.stop, Stop::Fault(Fault::Permission { access: Access::Write, .. })),
            "got {:?}",
            run.stop
        );
    }

    #[test]
    fn effects_record_exact_register_write() {
        let asm = assemble_at("mov rax, 5\nhlt", BASE).unwrap();
        let mut cpu = Cpu::with_code(&asm.bytes, BASE);
        let eff = cpu.step().unwrap();
        assert_eq!(eff.reg_writes, vec![RegWrite { reg: "rax", before: 0, after: 5 }]);
        assert!(eff.mem_writes.is_empty());
        assert!(eff.mem_reads.is_empty());
    }

    #[test]
    fn effects_record_memory_write_of_a_push() {
        let asm = assemble_at("mov rax, 0xdead\npush rax\nhlt", BASE).unwrap();
        let mut cpu = Cpu::with_code(&asm.bytes, BASE);
        cpu.step().unwrap(); // mov
        let sp_after = cpu.regs.read(Reg::RSP) - 8;
        let eff = cpu.step().unwrap(); // push
                                       // rsp update is a register write; the store is a memory write.
        assert!(eff.reg_writes.iter().any(|w| w.reg == "rsp"));
        assert_eq!(eff.mem_writes.len(), 1);
        let mw = &eff.mem_writes[0];
        assert_eq!(mw.addr, sp_after);
        assert_eq!(le_to_u64(&mw.after), 0xdead);
    }

    #[test]
    fn write_syscall_appends_to_stdout_and_exit_stops() {
        // write(1, msg, 5); exit(7)
        let src = "\
            mov rax, 1
            mov rdi, 1
            mov rsi, msg
            mov rdx, 5
            syscall
            mov rax, 60
            mov rdi, 7
            syscall
            hlt
        msg:
            db \"hello\"";
        let (cpu, run) = run_src(src, 100);
        assert_eq!(run.stop, Stop::Exited(7));
        assert_eq!(cpu.stdout(), b"hello");
    }

    #[test]
    fn movzx_and_movsx_extend_correctly() {
        // bl = 0xff. movzx -> 0x000000ff; movsx -> 0xffffffffffffffff.
        let (cpu, _) = run_src("mov bl, 0xff\nmovzx eax, bl\nhlt", 10);
        assert_eq!(rax(&cpu), 0xff);
        let (cpu, _) = run_src("mov bl, 0xff\nmovsx rax, bl\nhlt", 10);
        assert_eq!(rax(&cpu), u64::MAX);
    }

    #[test]
    fn lea_computes_address_without_faulting_on_unmapped_operand() {
        // [rax + 0x10] with rax pointing far outside any region: lea must not
        // touch memory, so it cannot fault.
        let (cpu, run) = run_src("mov rax, 0x4000000000\nlea rbx, [rax + 0x10]\nhlt", 10);
        assert_eq!(run.stop, Stop::Halted);
        assert_eq!(cpu.regs.read_full(3), 0x4000000010);
    }

    #[test]
    fn setcc_writes_zero_or_one() {
        let (cpu, _) = run_src("mov rax, 5\ncmp rax, 5\nsete bl\nhlt", 10);
        assert_eq!(cpu.regs.read_full(3) & 0xff, 1);
        let (cpu, _) = run_src("mov rax, 5\ncmp rax, 6\nsete bl\nhlt", 10);
        assert_eq!(cpu.regs.read_full(3) & 0xff, 0);
    }

    #[test]
    fn cmovcc_moves_only_when_taken() {
        // cmove copies when ZF is set.
        let (cpu, _) = run_src("mov rbx, 99\nmov rax, 5\ncmp rax, 5\ncmove rbx, rax\nhlt", 10);
        assert_eq!(cpu.regs.read_full(3), 5);
        let (cpu, _) = run_src("mov rbx, 99\nmov rax, 5\ncmp rax, 6\ncmove rbx, rax\nhlt", 10);
        assert_eq!(cpu.regs.read_full(3), 99);
    }

    #[test]
    fn shift_by_masked_zero_count_leaves_flags() {
        // shl rax, 64 masks the count (0x40 & 0x3f == 0): no change, no flags.
        let (cpu, _) =
            run_src("mov rax, 0x1234\nstc_via_sub:\nsub rbx, 1\nmov cl, 64\nshl rax, cl\nhlt", 20);
        // rbx-=1 from 0 set CF; the masked-zero shift must preserve it.
        assert_eq!(rax(&cpu), 0x1234, "count masked to zero leaves the value alone");
        assert!(cpu.flags.cf, "a zero-count shift does not touch flags");
    }

    #[test]
    fn step_limit_stops_an_infinite_loop() {
        // eb fe style loop, written as a self jump.
        let (_, run) = run_src("here:\njmp here", 50);
        assert_eq!(run.stop, Stop::StepLimit);
        assert_eq!(run.steps, 50);
        assert_eq!(run.trace.len(), 50);
    }

    #[test]
    fn multiply_sets_carry_when_result_overflows_low_half() {
        // 0x1_0000 * 0x1_0000 = 0x1_0000_0000 -> high half of a 32-bit mul is
        // non-zero, so CF/OF set.
        let (cpu, _) = run_src("mov eax, 0x10000\nmov ecx, 0x10000\nmul ecx\nhlt", 10);
        assert_eq!(cpu.regs.read_full(0), 0, "low 32 bits are zero");
        assert_eq!(cpu.regs.read_full(2), 1, "high 32 bits (edx) hold the 1");
        assert!(cpu.flags.cf && cpu.flags.of);
    }
}
