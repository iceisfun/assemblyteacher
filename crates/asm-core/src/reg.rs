//! Register model.
//!
//! x86_64 has sixteen general purpose registers. Each one can be addressed at
//! four widths, and the width you name determines both what the instruction
//! encodes and what happens to the bits you *didn't* name:
//!
//! ```text
//!  63                             31              15      7      0
//! +--------------------------------+---------------+-------+------+
//! |                              rax                               |
//! |                                |             eax               |
//! |                                |               |      ax       |
//! |                                |               |  ah   |  al   |
//! +--------------------------------+---------------+-------+------+
//! ```
//!
//! Writing `eax` zero-extends into `rax`. Writing `ax` or `al` does not.
//! That asymmetry is a real hardware behaviour, not an emulator quirk, and it
//! is modelled faithfully in [`crate::reg::Reg::writeback_mask`].

use core::fmt;

/// Operand width, in bytes. The discriminants *are* the byte counts.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(u8)]
pub enum Size {
    Byte = 1,
    Word = 2,
    Dword = 4,
    Qword = 8,
}

impl Size {
    /// Width in bytes.
    pub const fn bytes(self) -> u8 {
        self as u8
    }

    /// Width in bits.
    pub const fn bits(self) -> u32 {
        self as u32 * 8
    }

    /// A mask with the low `bits()` bits set. `Qword` saturates to all ones.
    pub const fn mask(self) -> u64 {
        match self {
            Size::Byte => 0xff,
            Size::Word => 0xffff,
            Size::Dword => 0xffff_ffff,
            Size::Qword => u64::MAX,
        }
    }

    /// The NASM/Intel size keyword used when an operand's width is ambiguous.
    pub const fn keyword(self) -> &'static str {
        match self {
            Size::Byte => "byte",
            Size::Word => "word",
            Size::Dword => "dword",
            Size::Qword => "qword",
        }
    }

    pub const fn from_bytes(n: u8) -> Option<Size> {
        match n {
            1 => Some(Size::Byte),
            2 => Some(Size::Word),
            4 => Some(Size::Dword),
            8 => Some(Size::Qword),
            _ => None,
        }
    }
}

/// A general purpose register reference at a specific width.
///
/// `num` is the 4-bit architectural register number (0..=15) exactly as it
/// appears in the encoding once REX extension bits are folded in.
///
/// `high_byte` distinguishes the legacy `ah`/`ch`/`dh`/`bh` registers, which
/// alias bits 8..16 of the first four registers, from `spl`/`bpl`/`sil`/`dil`,
/// which occupy the *same* encoding but are only reachable when a REX prefix
/// is present. This is why `mov ah, 1` and `mov spl, 1` differ only by a REX
/// byte, and why you cannot encode `ah` in the same instruction as `r8b`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Reg {
    pub num: u8,
    pub size: Size,
    pub high_byte: bool,
}

const NAMES64: [&str; 16] = [
    "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13",
    "r14", "r15",
];
const NAMES32: [&str; 16] = [
    "eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi", "r8d", "r9d", "r10d", "r11d", "r12d",
    "r13d", "r14d", "r15d",
];
const NAMES16: [&str; 16] = [
    "ax", "cx", "dx", "bx", "sp", "bp", "si", "di", "r8w", "r9w", "r10w", "r11w", "r12w", "r13w",
    "r14w", "r15w",
];
/// Byte registers as seen *with* a REX prefix present.
const NAMES8_REX: [&str; 16] = [
    "al", "cl", "dl", "bl", "spl", "bpl", "sil", "dil", "r8b", "r9b", "r10b", "r11b", "r12b",
    "r13b", "r14b", "r15b",
];
/// Byte registers as seen *without* a REX prefix.
const NAMES8_LEGACY: [&str; 8] = ["al", "cl", "dl", "bl", "ah", "ch", "dh", "bh"];

impl Reg {
    pub const fn new(num: u8, size: Size) -> Reg {
        Reg { num, size, high_byte: false }
    }

    /// `ah`, `ch`, `dh`, `bh` — `num` must be 0..=3, naming the *low* register
    /// whose second byte is aliased. `Reg::high(0)` is `ah`.
    pub const fn high(num: u8) -> Reg {
        Reg { num, size: Size::Byte, high_byte: true }
    }

    pub const RAX: Reg = Reg::new(0, Size::Qword);
    pub const RCX: Reg = Reg::new(1, Size::Qword);
    pub const RDX: Reg = Reg::new(2, Size::Qword);
    pub const RBX: Reg = Reg::new(3, Size::Qword);
    pub const RSP: Reg = Reg::new(4, Size::Qword);
    pub const RBP: Reg = Reg::new(5, Size::Qword);
    pub const RSI: Reg = Reg::new(6, Size::Qword);
    pub const RDI: Reg = Reg::new(7, Size::Qword);

    pub fn name(self) -> &'static str {
        match self.size {
            Size::Qword => NAMES64[self.num as usize & 15],
            Size::Dword => NAMES32[self.num as usize & 15],
            Size::Word => NAMES16[self.num as usize & 15],
            Size::Byte => {
                if self.high_byte {
                    NAMES8_LEGACY[(self.num as usize & 3) + 4]
                } else {
                    NAMES8_REX[self.num as usize & 15]
                }
            }
        }
    }

    /// Byte offset of this register's bits within the 64-bit architectural
    /// register. Only `ah`/`ch`/`dh`/`bh` are non-zero.
    pub const fn byte_offset(self) -> u32 {
        if self.high_byte {
            1
        } else {
            0
        }
    }

    /// Does a write at this width zero the upper bits of the 64-bit register?
    ///
    /// Only 32-bit writes do. This single rule is the reason `xor eax, eax`
    /// clears all of `rax`, and the reason so much compiler output uses the
    /// 32-bit form of an instruction where a 64-bit value is intended.
    pub const fn writeback_zero_extends(self) -> bool {
        matches!(self.size, Size::Dword)
    }

    /// Mask of the bits within the 64-bit register that a write to this
    /// register replaces.
    pub const fn writeback_mask(self) -> u64 {
        if self.high_byte {
            0xff00
        } else {
            self.size.mask()
        }
    }

    /// Whether encoding this register *requires* a REX prefix, even a bare
    /// `REX.W=0` one. True for `spl`/`bpl`/`sil`/`dil` and for `r8`..`r15`.
    pub const fn requires_rex(self) -> bool {
        self.num >= 8 || (matches!(self.size, Size::Byte) && !self.high_byte && self.num >= 4)
    }

    /// Whether encoding this register *forbids* a REX prefix. True only for
    /// `ah`/`ch`/`dh`/`bh`.
    pub const fn forbids_rex(self) -> bool {
        self.high_byte
    }

    /// Parse a register name. Case-insensitive.
    pub fn parse(name: &str) -> Option<Reg> {
        let lower = name.to_ascii_lowercase();
        let s = lower.as_str();
        if let Some(i) = NAMES64.iter().position(|&n| n == s) {
            return Some(Reg::new(i as u8, Size::Qword));
        }
        if let Some(i) = NAMES32.iter().position(|&n| n == s) {
            return Some(Reg::new(i as u8, Size::Dword));
        }
        if let Some(i) = NAMES16.iter().position(|&n| n == s) {
            return Some(Reg::new(i as u8, Size::Word));
        }
        // Check legacy high-byte names before the REX table, since "al".."bl"
        // appear in both and mean the same thing.
        if let Some(i) = NAMES8_LEGACY.iter().position(|&n| n == s) {
            return Some(if i >= 4 {
                Reg::high(i as u8 - 4)
            } else {
                Reg::new(i as u8, Size::Byte)
            });
        }
        if let Some(i) = NAMES8_REX.iter().position(|&n| n == s) {
            return Some(Reg::new(i as u8, Size::Byte));
        }
        None
    }
}

impl fmt::Display for Reg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Segment registers. In 64-bit mode only `fs` and `gs` still apply a base;
/// the others are forced to zero by the hardware. `fs` typically points at the
/// thread control block on Windows, `gs` on Linux userspace — which is why
/// thread-local reads show up as `mov rax, fs:[0x28]`-shaped instructions.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Seg {
    Es,
    Cs,
    Ss,
    Ds,
    Fs,
    Gs,
}

impl Seg {
    pub const fn name(self) -> &'static str {
        match self {
            Seg::Es => "es",
            Seg::Cs => "cs",
            Seg::Ss => "ss",
            Seg::Ds => "ds",
            Seg::Fs => "fs",
            Seg::Gs => "gs",
        }
    }

    /// The legacy prefix byte that selects this segment.
    pub const fn prefix_byte(self) -> u8 {
        match self {
            Seg::Es => 0x26,
            Seg::Cs => 0x2e,
            Seg::Ss => 0x36,
            Seg::Ds => 0x3e,
            Seg::Fs => 0x64,
            Seg::Gs => 0x65,
        }
    }

    pub const fn from_prefix(b: u8) -> Option<Seg> {
        match b {
            0x26 => Some(Seg::Es),
            0x2e => Some(Seg::Cs),
            0x36 => Some(Seg::Ss),
            0x3e => Some(Seg::Ds),
            0x64 => Some(Seg::Fs),
            0x65 => Some(Seg::Gs),
            _ => None,
        }
    }

    /// In long mode, does this segment contribute a non-zero base address?
    pub const fn has_base_in_long_mode(self) -> bool {
        matches!(self, Seg::Fs | Seg::Gs)
    }
}

impl fmt::Display for Seg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for size in [Size::Byte, Size::Word, Size::Dword, Size::Qword] {
            for num in 0..16u8 {
                let r = Reg::new(num, size);
                assert_eq!(Reg::parse(r.name()), Some(r), "{}", r.name());
            }
        }
    }

    #[test]
    fn high_byte_registers_are_distinct_from_rex_byte_registers() {
        assert_eq!(Reg::parse("ah"), Some(Reg::high(0)));
        assert_eq!(Reg::parse("spl"), Some(Reg::new(4, Size::Byte)));
        // ah and spl share encoding number 4 but are different registers.
        assert_eq!(Reg::high(0).num, 0);
        assert_ne!(Reg::high(0), Reg::new(4, Size::Byte));
        assert!(Reg::high(0).forbids_rex());
        assert!(Reg::new(4, Size::Byte).requires_rex());
    }

    #[test]
    fn only_dword_writes_zero_extend() {
        assert!(Reg::new(0, Size::Dword).writeback_zero_extends());
        assert!(!Reg::new(0, Size::Byte).writeback_zero_extends());
        assert!(!Reg::new(0, Size::Word).writeback_zero_extends());
        assert!(!Reg::new(0, Size::Qword).writeback_zero_extends());
    }

    #[test]
    fn high_byte_writeback_mask_targets_bits_8_through_15() {
        assert_eq!(Reg::high(0).writeback_mask(), 0xff00);
        assert_eq!(Reg::new(0, Size::Byte).writeback_mask(), 0x00ff);
    }

    #[test]
    fn parse_is_case_insensitive() {
        assert_eq!(Reg::parse("RAX"), Some(Reg::RAX));
        assert_eq!(Reg::parse("R8D"), Some(Reg::new(8, Size::Dword)));
    }
}
