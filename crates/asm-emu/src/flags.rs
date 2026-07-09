//! The condition flags, and how a condition code reads them.
//!
//! Six of the seven flags here are set as a side effect of arithmetic and
//! logic; the seventh, `df`, is a mode bit for string instructions. The whole
//! reason branches work is that a `cmp` records *how* two numbers compared into
//! these bits, and a later `jcc` interprets them — and crucially, the same
//! flags support both a signed and an unsigned reading, chosen by the branch.

use asm_core::Cond;
use serde::Serialize;

/// The x86 condition flags this interpreter models.
///
/// Ordinary integer code only ever observes these six plus `df`; the other
/// architectural flags (IF, TF, IOPL, ...) are privileged or irrelevant to a
/// user-mode teaching sandbox and are omitted.
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug, Serialize)]
pub struct Flags {
    /// Carry: unsigned overflow out of the most significant bit.
    pub cf: bool,
    /// Parity: set when the low 8 bits of the result have an even number of set
    /// bits. Only the low byte, always — a historical quirk, not a bug.
    pub pf: bool,
    /// Auxiliary carry: carry out of bit 3. Exists for BCD arithmetic and is
    /// almost never read directly, but it is defined, so we compute it.
    pub af: bool,
    /// Zero: the result was zero.
    pub zf: bool,
    /// Sign: a copy of the result's most significant bit.
    pub sf: bool,
    /// Overflow: signed overflow — the result's sign is wrong for the operands'
    /// signs. This is the flag that distinguishes `jl` from `jb`.
    pub of: bool,
    /// Direction: when set, string operations step downward. Set by `std`,
    /// cleared by `cld`. Untouched by everything the arithmetic paths do.
    pub df: bool,
}

impl Flags {
    /// Evaluate a condition code against the current flags.
    ///
    /// The signed conditions (`l`/`ge`/`le`/`g`) compare `sf` against `of`; the
    /// unsigned ones (`b`/`ae`/`be`/`a`) read `cf`. That single distinction is
    /// why `cmp` alone tells you nothing about signedness — the branch decides,
    /// and choosing the wrong one is a classic bug.
    pub fn eval(&self, cond: Cond) -> bool {
        match cond {
            Cond::O => self.of,
            Cond::No => !self.of,
            Cond::B => self.cf,
            Cond::Ae => !self.cf,
            Cond::E => self.zf,
            Cond::Ne => !self.zf,
            Cond::Be => self.cf || self.zf,
            Cond::A => !self.cf && !self.zf,
            Cond::S => self.sf,
            Cond::Ns => !self.sf,
            Cond::P => self.pf,
            Cond::Np => !self.pf,
            Cond::L => self.sf != self.of,
            Cond::Ge => self.sf == self.of,
            Cond::Le => self.zf || (self.sf != self.of),
            Cond::G => !self.zf && (self.sf == self.of),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_and_unsigned_less_read_different_flags() {
        // Result of comparing 1 with -1: as unsigned, 1 < 0xff.. so CF is not
        // set (1 is not below); as signed, 1 > -1 so `l` is false, `g` true.
        // Set up the flags that `cmp 1, -1` (i.e. 1 - (-1) = 2) leaves.
        let f = Flags { cf: true, of: false, sf: false, zf: false, ..Default::default() };
        // cmp 1, 0xffffffff: 1 - 0xffffffff borrows -> CF=1 -> jb taken.
        assert!(f.eval(Cond::B), "unsigned: 1 is below 0xffffffff");
        // Signed 1 vs -1: 1 is greater, so jl not taken, jg taken.
        assert!(!f.eval(Cond::L));
        assert!(f.eval(Cond::G));
    }

    #[test]
    fn equality_and_its_negation_agree() {
        let f = Flags { zf: true, ..Default::default() };
        assert!(f.eval(Cond::E));
        assert!(!f.eval(Cond::Ne));
    }
}
