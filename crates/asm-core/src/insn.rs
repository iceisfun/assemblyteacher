//! The instruction model, and the raw encoding that produced it.
//!
//! An [`Insn`] carries both the *semantic* view (a mnemonic and its operands)
//! and the *syntactic* view ([`Encoding`], the exact bytes and which field each
//! byte belongs to). Keeping both is the whole point of this crate: a student
//! should be able to look at `48 8b 44 24 08` and see, simultaneously, that it
//! means `mov rax, qword [rsp+0x8]` and that `48` is REX.W, `8b` is the opcode,
//! `44` is a ModRM byte selecting `rax` and a SIB-with-disp8 memory operand,
//! `24` is that SIB byte, and `08` is the displacement.

use crate::operand::Operand;
use crate::reg::Size;
use core::fmt;

/// The sixteen condition codes, in encoding order. The low bit of the encoding
/// inverts the condition, which is why every condition here comes in a pair.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum Cond {
    /// Overflow. `OF = 1`
    O = 0x0,
    /// Not overflow. `OF = 0`
    No = 0x1,
    /// Below / carry / not above-or-equal. `CF = 1`. **Unsigned.**
    B = 0x2,
    /// Above-or-equal / not carry. `CF = 0`. **Unsigned.**
    Ae = 0x3,
    /// Equal / zero. `ZF = 1`
    E = 0x4,
    /// Not equal / not zero. `ZF = 0`
    Ne = 0x5,
    /// Below-or-equal / not above. `CF = 1 or ZF = 1`. **Unsigned.**
    Be = 0x6,
    /// Above. `CF = 0 and ZF = 0`. **Unsigned.**
    A = 0x7,
    /// Sign. `SF = 1`
    S = 0x8,
    /// Not sign. `SF = 0`
    Ns = 0x9,
    /// Parity even. `PF = 1`
    P = 0xa,
    /// Parity odd. `PF = 0`
    Np = 0xb,
    /// Less. `SF != OF`. **Signed.**
    L = 0xc,
    /// Greater-or-equal. `SF = OF`. **Signed.**
    Ge = 0xd,
    /// Less-or-equal. `ZF = 1 or SF != OF`. **Signed.**
    Le = 0xe,
    /// Greater. `ZF = 0 and SF = OF`. **Signed.**
    G = 0xf,
}

impl Cond {
    pub const ALL: [Cond; 16] = [
        Cond::O,
        Cond::No,
        Cond::B,
        Cond::Ae,
        Cond::E,
        Cond::Ne,
        Cond::Be,
        Cond::A,
        Cond::S,
        Cond::Ns,
        Cond::P,
        Cond::Np,
        Cond::L,
        Cond::Ge,
        Cond::Le,
        Cond::G,
    ];

    pub const fn from_bits(b: u8) -> Cond {
        Cond::ALL[(b & 0xf) as usize]
    }

    pub const fn bits(self) -> u8 {
        self as u8
    }

    /// The canonical suffix, e.g. `"ne"` for `jne`/`setne`/`cmovne`.
    pub const fn suffix(self) -> &'static str {
        match self {
            Cond::O => "o",
            Cond::No => "no",
            Cond::B => "b",
            Cond::Ae => "ae",
            Cond::E => "e",
            Cond::Ne => "ne",
            Cond::Be => "be",
            Cond::A => "a",
            Cond::S => "s",
            Cond::Ns => "ns",
            Cond::P => "p",
            Cond::Np => "np",
            Cond::L => "l",
            Cond::Ge => "ge",
            Cond::Le => "le",
            Cond::G => "g",
        }
    }

    /// The condition this one is the negation of.
    pub const fn negate(self) -> Cond {
        Cond::from_bits(self.bits() ^ 1)
    }

    /// Whether this condition interprets its operands as signed. `jl`/`jg` are
    /// signed; `jb`/`ja` are unsigned. Choosing the wrong one is among the most
    /// common bugs in hand-written assembly, and the reason `cmp` alone tells
    /// you nothing about signedness — the *branch* decides.
    pub const fn is_signed(self) -> bool {
        matches!(self, Cond::L | Cond::Ge | Cond::Le | Cond::G)
    }

    pub const fn is_unsigned(self) -> bool {
        matches!(self, Cond::B | Cond::Ae | Cond::Be | Cond::A)
    }

    /// Accepted spellings, including the common aliases. `parse("jz")`-style
    /// callers should strip the mnemonic prefix first.
    pub fn parse(s: &str) -> Option<Cond> {
        Some(match s {
            "o" => Cond::O,
            "no" => Cond::No,
            "b" | "c" | "nae" => Cond::B,
            "ae" | "nb" | "nc" => Cond::Ae,
            "e" | "z" => Cond::E,
            "ne" | "nz" => Cond::Ne,
            "be" | "na" => Cond::Be,
            "a" | "nbe" => Cond::A,
            "s" => Cond::S,
            "ns" => Cond::Ns,
            "p" | "pe" => Cond::P,
            "np" | "po" => Cond::Np,
            "l" | "nge" => Cond::L,
            "ge" | "nl" => Cond::Ge,
            "le" | "ng" => Cond::Le,
            "g" | "nle" => Cond::G,
            _ => return None,
        })
    }
}

/// Instruction mnemonics covered by this decoder.
///
/// This is deliberately a subset of x86_64: the integer core that compilers
/// actually emit for straight-line code, branches, and calls. It is enough to
/// disassemble the hot path of most real functions, and small enough to read.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum Mnemonic {
    Add,
    Or,
    Adc,
    Sbb,
    And,
    Sub,
    Xor,
    Cmp,
    Test,
    Not,
    Neg,
    Inc,
    Dec,
    Mul,
    Imul,
    Div,
    Idiv,
    Mov,
    Movzx,
    Movsx,
    /// `movsxd` — sign-extend 32 to 64. Spelled separately from `movsx`
    /// because it has its own opcode (`0x63`) rather than a `0x0f` escape.
    Movsxd,
    Lea,
    Push,
    Pop,
    Xchg,
    Shl,
    Shr,
    Sar,
    Rol,
    Ror,
    /// Rotate through carry. No compiler emits these for ordinary code; they
    /// show up in hand-written bignum arithmetic and in obfuscators.
    Rcl,
    Rcr,
    Jmp,
    Jcc(Cond),
    Setcc(Cond),
    Cmovcc(Cond),
    Call,
    Ret,
    Leave,
    Nop,
    Hlt,
    Int3,
    Int,
    Syscall,
    Cdq,
    Cqo,
    Cwd,
    Cdqe,
    Cbw,
    Cwde,
    Bswap,
    Endbr64,
    /// `ud2` — guaranteed to raise #UD. Rust emits it after a `panic!` that
    /// the optimiser proved diverges, and LLVM uses it to fill unreachable
    /// blocks. Finding one usually means you have reached the end of a
    /// function's real code.
    Ud2,
    /// A byte sequence we could decode the length of but not the meaning.
    Unknown,
}

impl Mnemonic {
    /// The lowercase text form, e.g. `"jne"`. Allocates only for the
    /// condition-carrying forms.
    pub fn name(self) -> String {
        match self {
            Mnemonic::Jcc(c) => format!("j{}", c.suffix()),
            Mnemonic::Setcc(c) => format!("set{}", c.suffix()),
            Mnemonic::Cmovcc(c) => format!("cmov{}", c.suffix()),
            other => other.static_name().to_string(),
        }
    }

    /// The text form for mnemonics that do not carry a condition code.
    /// Returns `"jcc"`/`"setcc"`/`"cmovcc"` for those that do.
    pub const fn static_name(self) -> &'static str {
        match self {
            Mnemonic::Add => "add",
            Mnemonic::Or => "or",
            Mnemonic::Adc => "adc",
            Mnemonic::Sbb => "sbb",
            Mnemonic::And => "and",
            Mnemonic::Sub => "sub",
            Mnemonic::Xor => "xor",
            Mnemonic::Cmp => "cmp",
            Mnemonic::Test => "test",
            Mnemonic::Not => "not",
            Mnemonic::Neg => "neg",
            Mnemonic::Inc => "inc",
            Mnemonic::Dec => "dec",
            Mnemonic::Mul => "mul",
            Mnemonic::Imul => "imul",
            Mnemonic::Div => "div",
            Mnemonic::Idiv => "idiv",
            Mnemonic::Mov => "mov",
            Mnemonic::Movzx => "movzx",
            Mnemonic::Movsx => "movsx",
            Mnemonic::Movsxd => "movsxd",
            Mnemonic::Lea => "lea",
            Mnemonic::Push => "push",
            Mnemonic::Pop => "pop",
            Mnemonic::Xchg => "xchg",
            Mnemonic::Shl => "shl",
            Mnemonic::Shr => "shr",
            Mnemonic::Sar => "sar",
            Mnemonic::Rol => "rol",
            Mnemonic::Ror => "ror",
            Mnemonic::Rcl => "rcl",
            Mnemonic::Rcr => "rcr",
            Mnemonic::Jmp => "jmp",
            Mnemonic::Jcc(_) => "jcc",
            Mnemonic::Setcc(_) => "setcc",
            Mnemonic::Cmovcc(_) => "cmovcc",
            Mnemonic::Call => "call",
            Mnemonic::Ret => "ret",
            Mnemonic::Leave => "leave",
            Mnemonic::Nop => "nop",
            Mnemonic::Hlt => "hlt",
            Mnemonic::Int3 => "int3",
            Mnemonic::Int => "int",
            Mnemonic::Syscall => "syscall",
            Mnemonic::Cdq => "cdq",
            Mnemonic::Cqo => "cqo",
            Mnemonic::Cwd => "cwd",
            Mnemonic::Cdqe => "cdqe",
            Mnemonic::Cbw => "cbw",
            Mnemonic::Cwde => "cwde",
            Mnemonic::Bswap => "bswap",
            Mnemonic::Endbr64 => "endbr64",
            Mnemonic::Ud2 => "ud2",
            Mnemonic::Unknown => "(bad)",
        }
    }

    /// Does control flow continue at the following instruction?
    ///
    /// `false` for unconditional transfers and `ret`. Conditional jumps and
    /// `call` both fall through (a `call` is expected to return), so both are
    /// `true`. Function-discovery and CFG-building code depends on this.
    pub const fn falls_through(self) -> bool {
        !matches!(self, Mnemonic::Jmp | Mnemonic::Ret | Mnemonic::Hlt | Mnemonic::Ud2)
    }

    /// Does this instruction transfer control somewhere other than the next
    /// instruction?
    pub const fn is_branch(self) -> bool {
        matches!(self, Mnemonic::Jmp | Mnemonic::Jcc(_) | Mnemonic::Call | Mnemonic::Ret)
    }
}

impl fmt::Display for Mnemonic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

/// A `rep`-family prefix. Retained because it changes the meaning of the
/// instruction it precedes rather than merely decorating it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RepPrefix {
    /// `f3`
    Rep,
    /// `f2`
    Repnz,
}

/// The raw bytes of an instruction, split into the fields the CPU's decoder
/// splits them into.
///
/// Reconstructing `bytes()` from these fields in order yields exactly the
/// original input — this is asserted in the decoder's tests.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Encoding {
    /// Legacy prefixes, in the order they appeared.
    pub legacy: Vec<u8>,
    /// The REX byte, `0x40..=0x4f`, if present. Must immediately precede the
    /// opcode.
    pub rex: Option<u8>,
    /// One to three opcode bytes, including any `0x0f` escape.
    pub opcode: Vec<u8>,
    pub modrm: Option<u8>,
    pub sib: Option<u8>,
    /// Displacement bytes, little-endian, as encoded.
    pub disp: Vec<u8>,
    /// Immediate bytes, little-endian, as encoded.
    pub imm: Vec<u8>,
}

impl Encoding {
    /// Reassemble the instruction's bytes in encoding order.
    pub fn bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(16);
        v.extend_from_slice(&self.legacy);
        if let Some(rex) = self.rex {
            v.push(rex);
        }
        v.extend_from_slice(&self.opcode);
        if let Some(m) = self.modrm {
            v.push(m);
        }
        if let Some(s) = self.sib {
            v.push(s);
        }
        v.extend_from_slice(&self.disp);
        v.extend_from_slice(&self.imm);
        v
    }

    pub fn len(&self) -> usize {
        self.legacy.len()
            + self.rex.is_some() as usize
            + self.opcode.len()
            + self.modrm.is_some() as usize
            + self.sib.is_some() as usize
            + self.disp.len()
            + self.imm.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// REX.W — promotes the operand size to 64 bits.
    pub fn rex_w(&self) -> bool {
        self.rex.is_some_and(|r| r & 0b1000 != 0)
    }
    /// REX.R — extends the ModRM `reg` field to 4 bits.
    pub fn rex_r(&self) -> bool {
        self.rex.is_some_and(|r| r & 0b0100 != 0)
    }
    /// REX.X — extends the SIB `index` field to 4 bits.
    pub fn rex_x(&self) -> bool {
        self.rex.is_some_and(|r| r & 0b0010 != 0)
    }
    /// REX.B — extends the ModRM `rm`, SIB `base`, or opcode register field.
    pub fn rex_b(&self) -> bool {
        self.rex.is_some_and(|r| r & 0b0001 != 0)
    }

    /// A human-readable breakdown of every field, for teaching UIs.
    /// Each entry is `(field name, bytes, explanation)`.
    pub fn explain(&self) -> Vec<(&'static str, Vec<u8>, String)> {
        let mut out = Vec::new();
        for &p in &self.legacy {
            let why = match p {
                0x66 => "operand-size override: 32-bit operands become 16-bit".to_string(),
                0x67 => "address-size override: 64-bit addressing becomes 32-bit".to_string(),
                0xf0 => "lock: make the read-modify-write atomic".to_string(),
                0xf3 => "rep / repe, or a mandatory prefix selecting an opcode".to_string(),
                0xf2 => "repne, or a mandatory prefix selecting an opcode".to_string(),
                b => match crate::reg::Seg::from_prefix(b) {
                    Some(s) => format!("segment override: address is relative to {}", s),
                    None => "prefix".to_string(),
                },
            };
            out.push(("legacy prefix", vec![p], why));
        }
        if let Some(rex) = self.rex {
            let why = format!(
                "REX: W={} (operand size {}), R={} (reg field +8), X={} (index +8), B={} (rm/base +8)",
                self.rex_w() as u8,
                if self.rex_w() { "64-bit" } else { "default" },
                self.rex_r() as u8,
                self.rex_x() as u8,
                self.rex_b() as u8,
            );
            out.push(("REX", vec![rex], why));
        }
        if !self.opcode.is_empty() {
            let why = if self.opcode.len() > 1 {
                "opcode, escaped via 0x0f into the two-byte map".to_string()
            } else {
                "opcode: selects the operation".to_string()
            };
            out.push(("opcode", self.opcode.clone(), why));
        }
        if let Some(m) = self.modrm {
            let why =
                format!("ModRM: mod={:02b} reg={:03b} rm={:03b}", m >> 6, (m >> 3) & 7, m & 7);
            out.push(("ModRM", vec![m], why));
        }
        if let Some(s) = self.sib {
            let why = format!(
                "SIB: scale={} index={:03b} base={:03b}",
                1u8 << (s >> 6),
                (s >> 3) & 7,
                s & 7
            );
            out.push(("SIB", vec![s], why));
        }
        if !self.disp.is_empty() {
            out.push((
                "displacement",
                self.disp.clone(),
                format!("{}-byte signed displacement, little-endian", self.disp.len()),
            ));
        }
        if !self.imm.is_empty() {
            out.push((
                "immediate",
                self.imm.clone(),
                format!("{}-byte immediate, little-endian", self.imm.len()),
            ));
        }
        out
    }
}

/// A decoded instruction.
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Insn {
    /// Virtual address this instruction was decoded at.
    pub ip: u64,
    pub mnemonic: Mnemonic,
    pub operands: Vec<Operand>,
    pub encoding: Encoding,
    /// Present when a `lock` prefix was encoded.
    pub lock: bool,
    pub rep: Option<RepPrefix>,
    /// The width the instruction operates at, when it has a single natural one.
    pub op_size: Option<Size>,
}

impl Insn {
    pub fn len(&self) -> usize {
        self.encoding.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The address of the instruction that follows this one in memory.
    pub fn next_ip(&self) -> u64 {
        self.ip.wrapping_add(self.len() as u64)
    }

    /// For a relative branch, the absolute address it targets.
    ///
    /// Relative displacements are measured from the *end* of the instruction,
    /// not its start. That is why a `jmp` to itself encodes as `eb fe`
    /// (`-2`, back over the two bytes just consumed) rather than `eb 00`.
    pub fn branch_target(&self) -> Option<u64> {
        self.operands.iter().find_map(|o| match o {
            Operand::Rel(d) => Some(self.next_ip().wrapping_add(*d as u64)),
            _ => None,
        })
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.encoding.bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn condition_low_bit_inverts() {
        for c in Cond::ALL {
            assert_eq!(c.negate().negate(), c);
            assert_eq!(c.negate().bits(), c.bits() ^ 1);
        }
    }

    #[test]
    fn condition_aliases_agree() {
        assert_eq!(Cond::parse("z"), Cond::parse("e"));
        assert_eq!(Cond::parse("nz"), Cond::parse("ne"));
        assert_eq!(Cond::parse("c"), Cond::parse("b"));
        assert_eq!(Cond::parse("nle"), Cond::parse("g"));
    }

    #[test]
    fn signed_and_unsigned_conditions_are_disjoint() {
        for c in Cond::ALL {
            assert!(!(c.is_signed() && c.is_unsigned()), "{:?}", c);
        }
    }

    #[test]
    fn mnemonic_names_include_the_condition() {
        assert_eq!(Mnemonic::Jcc(Cond::Ne).name(), "jne");
        assert_eq!(Mnemonic::Setcc(Cond::G).name(), "setg");
        assert_eq!(Mnemonic::Cmovcc(Cond::B).name(), "cmovb");
    }

    #[test]
    fn encoding_round_trips_to_the_original_bytes() {
        let e = Encoding {
            legacy: vec![0x66],
            rex: Some(0x48),
            opcode: vec![0x0f, 0xaf],
            modrm: Some(0x44),
            sib: Some(0x24),
            disp: vec![0x08],
            imm: vec![],
        };
        assert_eq!(e.bytes(), vec![0x66, 0x48, 0x0f, 0xaf, 0x44, 0x24, 0x08]);
        assert_eq!(e.len(), 7);
    }

    #[test]
    fn rex_bits_decode() {
        let e = Encoding { rex: Some(0x4d), ..Default::default() };
        assert!(e.rex_w() && e.rex_r() && !e.rex_x() && e.rex_b());
    }

    #[test]
    fn call_falls_through_but_jmp_does_not() {
        assert!(Mnemonic::Call.falls_through());
        assert!(Mnemonic::Jcc(Cond::E).falls_through());
        assert!(!Mnemonic::Jmp.falls_through());
        assert!(!Mnemonic::Ret.falls_through());
    }
}
