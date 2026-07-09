//! The general-purpose register file, and the aliasing rules that make it
//! subtle.
//!
//! There are sixteen 64-bit registers. The interpreter stores exactly those
//! sixteen `u64`s; every narrower name (`eax`, `ax`, `al`, `ah`) is a *view*
//! onto one of them. Reads mask; writes are where the interesting asymmetry
//! lives, and it is enforced by [`asm_core::Reg`] rather than re-derived here.

use asm_core::{Reg, Size};
use serde::Serialize;

/// Sixteen 64-bit registers, indexed by architectural number 0..=15.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize)]
pub struct Regs(pub [u64; 16]);

impl Regs {
    pub fn new() -> Regs {
        Regs::default()
    }

    /// The full 64-bit value of register `num`, ignoring any width or high-byte
    /// view. Used to snapshot before/after values for the effects log.
    pub fn read_full(&self, num: u8) -> u64 {
        self.0[(num & 15) as usize]
    }

    /// Read `r` at its declared width. `al` returns the low byte; `ah` returns
    /// bits 8..16; `eax` returns the low 32 bits, and so on. The upper bits are
    /// masked away, never observed.
    pub fn read(&self, r: Reg) -> u64 {
        let full = self.read_full(r.num);
        if r.high_byte {
            (full >> 8) & 0xff
        } else {
            full & r.size.mask()
        }
    }

    /// Write `r`, obeying the one rule that trips up every beginner: a 32-bit
    /// write clears the upper 32 bits, but an 8- or 16-bit write leaves the
    /// bits it does not name untouched.
    ///
    /// This is why `xor eax, eax` zeroes all of `rax` while `mov al, 0` does
    /// not, and why compilers reach for the 32-bit form so often. The decision
    /// is delegated to [`Reg::writeback_zero_extends`]/[`Reg::writeback_mask`]
    /// so that the emulator and disassembler can never disagree about it.
    pub fn write(&mut self, r: Reg, v: u64) {
        let idx = (r.num & 15) as usize;
        if r.writeback_zero_extends() {
            // 32-bit write: the whole 64-bit register becomes the zero-extended
            // value. `writeback_mask()` here is 0xffff_ffff.
            self.0[idx] = v & r.writeback_mask();
            return;
        }
        let mask = r.writeback_mask();
        if r.high_byte {
            // ah/ch/dh/bh occupy bits 8..16; shift the byte into place.
            self.0[idx] = (self.0[idx] & !mask) | ((v << 8) & mask);
        } else {
            self.0[idx] = (self.0[idx] & !mask) | (v & mask);
        }
    }

    /// Iterate the sixteen registers as `(name, value)` pairs in architectural
    /// order, using the 64-bit names (`rax`, `rcx`, ...). Feeds the register
    /// panel in the UI.
    pub fn iter_named(&self) -> impl Iterator<Item = (&'static str, u64)> + '_ {
        (0..16u8).map(move |n| (Reg::new(n, Size::Qword).name(), self.0[n as usize]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asm_core::Reg;

    fn reg(name: &str) -> Reg {
        Reg::parse(name).unwrap()
    }

    #[test]
    fn dword_write_zero_extends() {
        let mut r = Regs::new();
        r.0[0] = 0xdead_beef_dead_beef;
        r.write(reg("eax"), 0x1234_5678);
        assert_eq!(r.read_full(0), 0x1234_5678);
    }

    #[test]
    fn byte_write_preserves_upper_bits() {
        let mut r = Regs::new();
        r.0[0] = 0xdead_beef_dead_beef;
        r.write(reg("al"), 0x00);
        assert_eq!(r.read_full(0), 0xdead_beef_dead_be00);
    }

    #[test]
    fn word_write_preserves_upper_bits() {
        let mut r = Regs::new();
        r.0[0] = 0xffff_ffff_ffff_ffff;
        r.write(reg("ax"), 0x1234);
        assert_eq!(r.read_full(0), 0xffff_ffff_ffff_1234);
    }

    #[test]
    fn high_byte_aliases_bits_8_through_15() {
        let mut r = Regs::new();
        r.0[0] = 0x0000_0000_0000_00ff; // al = 0xff
        r.write(reg("ah"), 0x12);
        assert_eq!(r.read_full(0), 0x0000_0000_0000_12ff);
        assert_eq!(r.read(reg("ah")), 0x12);
        assert_eq!(r.read(reg("al")), 0xff);
    }
}
