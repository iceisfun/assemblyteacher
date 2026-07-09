//! Operands: the things an instruction reads and writes.

use crate::reg::{Reg, Seg, Size};
use core::fmt;

/// A memory reference.
///
/// Every x86_64 memory operand is some subset of the same general form:
///
/// ```text
///     segment : [ base + index*scale + displacement ]
/// ```
///
/// The address is computed at execution time by summing whichever parts are
/// present. `scale` is restricted by the hardware to 1, 2, 4 or 8 — which is
/// exactly the set of primitive type sizes, and why `arr[i]` for an array of
/// 4-byte ints compiles to a single `[base + i*4]` operand with no separate
/// multiply.
///
/// `rip_relative` marks the 64-bit-only addressing mode where the base is the
/// address of the *next* instruction. Position-independent code leans on it
/// heavily: it lets a global be referenced without a relocation that depends on
/// where the image was loaded.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Mem {
    /// Segment override, if one was encoded. In long mode only `fs`/`gs` have
    /// any effect on the computed address.
    pub seg: Option<Seg>,
    pub base: Option<Reg>,
    pub index: Option<Reg>,
    /// 1, 2, 4 or 8. Meaningless (and stored as 1) when `index` is `None`.
    pub scale: u8,
    pub disp: i64,
    /// The width of the access. `None` when the instruction implies it and no
    /// size keyword is needed to disambiguate.
    pub size: Option<Size>,
    /// `[rip + disp]`. When set, `base` and `index` are `None`.
    pub rip_relative: bool,
}

impl Mem {
    pub fn new() -> Mem {
        Mem { scale: 1, ..Default::default() }
    }

    pub fn with_size(mut self, size: Size) -> Mem {
        self.size = Some(size);
        self
    }

    pub fn base(mut self, r: Reg) -> Mem {
        self.base = Some(r);
        self
    }

    pub fn index(mut self, r: Reg, scale: u8) -> Mem {
        self.index = Some(r);
        self.scale = scale;
        self
    }

    pub fn disp(mut self, d: i64) -> Mem {
        self.disp = d;
        self
    }

    pub fn rip(disp: i64) -> Mem {
        Mem { rip_relative: true, disp, scale: 1, ..Default::default() }
    }

    /// Resolve the effective address. `next_ip` is the address of the
    /// instruction *after* this one, needed only for RIP-relative operands.
    ///
    /// `read_reg` supplies the current value of a register.
    pub fn effective_address(&self, next_ip: u64, mut read_reg: impl FnMut(Reg) -> u64) -> u64 {
        if self.rip_relative {
            return next_ip.wrapping_add(self.disp as u64);
        }
        let mut addr = self.disp as u64;
        if let Some(b) = self.base {
            addr = addr.wrapping_add(read_reg(b));
        }
        if let Some(i) = self.index {
            addr = addr.wrapping_add(read_reg(i).wrapping_mul(self.scale as u64));
        }
        addr
    }

    /// True when the operand names no registers at all — an absolute address.
    pub fn is_absolute(&self) -> bool {
        self.base.is_none() && self.index.is_none() && !self.rip_relative
    }
}

impl fmt::Display for Mem {
    /// Intel syntax, matching what NASM accepts and objdump prints.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(sz) = self.size {
            write!(f, "{} ", sz.keyword())?;
        }
        if let Some(seg) = self.seg {
            write!(f, "{}:", seg)?;
        }
        f.write_str("[")?;
        let mut wrote = false;
        if self.rip_relative {
            f.write_str("rip")?;
            wrote = true;
        }
        if let Some(b) = self.base {
            write!(f, "{}", b)?;
            wrote = true;
        }
        if let Some(i) = self.index {
            if wrote {
                f.write_str("+")?;
            }
            write!(f, "{}", i)?;
            if self.scale != 1 {
                write!(f, "*{}", self.scale)?;
            }
            wrote = true;
        }
        if self.disp != 0 || !wrote {
            if wrote {
                if self.disp < 0 {
                    write!(f, "-0x{:x}", (self.disp as i128).unsigned_abs())?;
                } else {
                    write!(f, "+0x{:x}", self.disp)?;
                }
            } else if self.disp < 0 {
                write!(f, "-0x{:x}", (self.disp as i128).unsigned_abs())?;
            } else {
                write!(f, "0x{:x}", self.disp)?;
            }
        }
        f.write_str("]")
    }
}

/// One operand of an instruction.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Operand {
    Reg(Reg),
    Mem(Mem),
    /// An immediate, already sign-extended to 64 bits from its encoded width.
    Imm(i64),
    /// A branch displacement relative to the end of the instruction.
    /// Use [`crate::insn::Insn::branch_target`] to resolve it.
    Rel(i64),
}

impl Operand {
    /// The access width of this operand, when it is determined by the operand
    /// itself rather than by the instruction.
    pub fn size(&self) -> Option<Size> {
        match self {
            Operand::Reg(r) => Some(r.size),
            Operand::Mem(m) => m.size,
            Operand::Imm(_) | Operand::Rel(_) => None,
        }
    }

    pub fn as_reg(&self) -> Option<Reg> {
        match self {
            Operand::Reg(r) => Some(*r),
            _ => None,
        }
    }

    pub fn as_mem(&self) -> Option<Mem> {
        match self {
            Operand::Mem(m) => Some(*m),
            _ => None,
        }
    }

    pub fn is_write_target(&self) -> bool {
        matches!(self, Operand::Reg(_) | Operand::Mem(_))
    }
}

impl fmt::Display for Operand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Operand::Reg(r) => write!(f, "{}", r),
            Operand::Mem(m) => write!(f, "{}", m),
            Operand::Imm(v) => {
                if *v < 0 {
                    write!(f, "-0x{:x}", (*v as i128).unsigned_abs())
                } else {
                    write!(f, "0x{:x}", v)
                }
            }
            Operand::Rel(v) => write!(f, "{:+}", v),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_address_sums_the_parts() {
        let m = Mem::new().base(Reg::RAX).index(Reg::RCX, 4).disp(0x10);
        let ea = m.effective_address(0, |r| match r.num {
            0 => 0x1000,
            1 => 3,
            _ => 0,
        });
        assert_eq!(ea, 0x1000 + 3 * 4 + 0x10);
    }

    #[test]
    fn rip_relative_is_measured_from_the_next_instruction() {
        let m = Mem::rip(0x20);
        assert_eq!(m.effective_address(0x1007, |_| unreachable!()), 0x1027);
    }

    #[test]
    fn negative_displacement_formats_as_subtraction() {
        let m = Mem::new().base(Reg::RBP).disp(-8).with_size(Size::Qword);
        assert_eq!(m.to_string(), "qword [rbp-0x8]");
    }

    #[test]
    fn absolute_memory_prints_a_bare_address() {
        let m = Mem::new().disp(0x404000);
        assert_eq!(m.to_string(), "[0x404000]");
        assert!(m.is_absolute());
    }
}
