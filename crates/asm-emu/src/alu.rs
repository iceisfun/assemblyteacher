//! The arithmetic and logic core: pure functions from operands to a result and
//! the flags it produces.
//!
//! Keeping these free of any CPU state makes them directly testable and makes
//! the flag rules — which are the entire subject of several lessons — readable
//! in one place. Every function works at an arbitrary [`Size`] by masking to
//! that width; the sign bit and the "carry out" position both move with it.

use crate::flags::Flags;
use asm_core::Size;

/// The six arithmetic flags an ALU op produces. `df` is never touched by
/// arithmetic, so it is not part of this struct — the caller merges these in
/// and leaves `df` alone.
#[derive(Clone, Copy, Debug)]
pub struct Af {
    pub cf: bool,
    pub pf: bool,
    pub af: bool,
    pub zf: bool,
    pub sf: bool,
    pub of: bool,
}

impl Af {
    /// Copy these six flags into `flags`, preserving `df`.
    pub fn apply(self, flags: &mut Flags) {
        flags.cf = self.cf;
        flags.pf = self.pf;
        flags.af = self.af;
        flags.zf = self.zf;
        flags.sf = self.sf;
        flags.of = self.of;
    }
}

/// Parity of the low 8 bits: true when an even number of them are set. x86
/// computes PF from the low byte only, at every width — a quirk worth stating
/// out loud because it surprises people who expect it to reflect the whole
/// result.
fn parity(v: u64) -> bool {
    (v as u8).count_ones() % 2 == 0
}

fn sign_bit(size: Size) -> u64 {
    1u64 << (size.bits() - 1)
}

/// The common tail: zero, sign and parity of a width-masked result.
fn zsp(res: u64, size: Size) -> (bool, bool, bool) {
    (res == 0, res & sign_bit(size) != 0, parity(res))
}

/// `a + b`. CF is carry out of the top bit; OF is signed overflow, which
/// happens exactly when the operands share a sign that the result does not.
pub fn add(a: u64, b: u64, size: Size) -> (u64, Af) {
    adc(a, b, false, size)
}

/// `a + b + carry`. Folding ADC and ADD together keeps the carry-propagation
/// rule in one place.
pub fn adc(a: u64, b: u64, carry: bool, size: Size) -> (u64, Af) {
    let m = size.mask();
    let a = a & m;
    let b = b & m;
    let c = carry as u64;
    let wide = a as u128 + b as u128 + c as u128;
    let res = (wide as u64) & m;
    let sb = sign_bit(size);
    let (zf, sf, pf) = zsp(res, size);
    let of = (!(a ^ b) & (a ^ res) & sb) != 0;
    let af = ((a & 0xf) + (b & 0xf) + c) > 0xf;
    (res, Af { cf: wide > m as u128, pf, af, zf, sf, of })
}

/// `a - b`. CF is a borrow (unsigned `a < b`); OF is signed overflow.
pub fn sub(a: u64, b: u64, size: Size) -> (u64, Af) {
    sbb(a, b, false, size)
}

/// `a - b - borrow`.
pub fn sbb(a: u64, b: u64, borrow: bool, size: Size) -> (u64, Af) {
    let m = size.mask();
    let a = a & m;
    let b = b & m;
    let c = borrow as u64;
    let res = a.wrapping_sub(b).wrapping_sub(c) & m;
    let sb = sign_bit(size);
    let (zf, sf, pf) = zsp(res, size);
    // Overflow for subtraction: operands differ in sign and the result's sign
    // matches the subtrahend's.
    let of = ((a ^ b) & (a ^ res) & sb) != 0;
    let cf = (a as u128) < (b as u128 + c as u128);
    let af = (a & 0xf) < (b & 0xf) + c;
    (res, Af { cf, pf, af, zf, sf, of })
}

/// Bitwise AND/OR/XOR share flag behaviour: they *clear* CF and OF outright —
/// there is no such thing as a logical carry — and set ZF/SF/PF from the
/// result. AF is left undefined by the manual; we clear it for determinism.
pub fn logic(res: u64, size: Size) -> Af {
    let m = size.mask();
    let res = res & m;
    let (zf, sf, pf) = zsp(res, size);
    Af { cf: false, pf, af: false, zf, sf, of: false }
}

/// `inc` is `add ..., 1` in every respect *except* that it leaves CF alone.
/// That exception is deliberate: it lets a loop use `inc` as its counter while
/// carrying an unrelated multi-word add across iterations in CF.
pub fn inc(a: u64, size: Size, old_cf: bool) -> (u64, Af) {
    let (res, mut f) = add(a, 1, size);
    f.cf = old_cf;
    (res, f)
}

/// `dec` mirrors `inc`: `sub ..., 1` but CF is preserved for the same reason.
pub fn dec(a: u64, size: Size, old_cf: bool) -> (u64, Af) {
    let (res, mut f) = sub(a, 1, size);
    f.cf = old_cf;
    (res, f)
}

/// `neg a` is `0 - a`, and it sets flags exactly like that subtraction: CF is
/// set unless `a` was zero.
pub fn neg(a: u64, size: Size) -> (u64, Af) {
    sub(0, a, size)
}

/// The direction of a shift/rotate, used only to keep the five variants from
/// duplicating the count-masking and flag-suppression logic.
#[derive(Clone, Copy)]
pub enum ShiftOp {
    Shl,
    Shr,
    Sar,
    Rol,
    Ror,
}

/// Result of a shift/rotate: the value, and the flag changes (if any).
///
/// A shift whose masked count is zero changes *nothing*, not even the flags —
/// so the flag update is optional. This is the rule that makes `shl reg, cl`
/// with `cl == 0` a true no-op, and forgetting it is a real hardware-accurate
/// gotcha.
pub struct ShiftResult {
    pub value: u64,
    pub flags: Option<ShiftFlags>,
}

/// Shifts only touch CF and OF meaningfully (plus SF/ZF/PF for the non-rotate
/// forms); the fields left `None` are not written.
pub struct ShiftFlags {
    pub cf: bool,
    /// OF is architecturally defined *only* for a shift count of 1. For any
    /// other count it is undefined, so we do not report it and the caller
    /// leaves the flag as-is. Stating this in the type keeps a lesson honest.
    pub of: Option<bool>,
    /// Present for shl/shr/sar (which update SF/ZF/PF), absent for rotates
    /// (which do not).
    pub szp: Option<(bool, bool, bool)>,
}

/// Perform a shift or rotate. `raw_count` is the untruncated count; it is
/// masked to 6 bits for 64-bit operands and 5 bits otherwise, matching the
/// hardware, before anything else happens.
pub fn shift(op: ShiftOp, a: u64, raw_count: u64, size: Size) -> ShiftResult {
    let m = size.mask();
    let bits = size.bits() as u64;
    let a = a & m;
    let mask = if size == Size::Qword { 0x3f } else { 0x1f };
    let count = raw_count & mask;
    if count == 0 {
        return ShiftResult { value: a, flags: None };
    }
    let sb = sign_bit(size);
    let (value, cf, of) = match op {
        ShiftOp::Shl => {
            let value = (a << count) & m;
            // CF is the last bit shifted off the top.
            let cf = (a >> (bits - count)) & 1 != 0;
            // OF (count==1) is "did the sign bit change": MSB(result) xor CF.
            let of = (value & sb != 0) ^ cf;
            (value, cf, of)
        }
        ShiftOp::Shr => {
            let value = a >> count;
            let cf = (a >> (count - 1)) & 1 != 0;
            // OF (count==1) for a logical right shift is the old MSB.
            let of = a & sb != 0;
            (value, cf, of)
        }
        ShiftOp::Sar => {
            // Arithmetic shift: replicate the sign bit. Do it on the sign-
            // extended value so the vacated high bits fill correctly.
            let signed = sign_extend(a, size) >> count;
            let value = (signed as u64) & m;
            let cf = (a >> (count - 1)) & 1 != 0;
            // A sign-preserving shift can never overflow, so OF(count==1)=0.
            (value, cf, false)
        }
        ShiftOp::Rol => {
            let c = count % bits;
            let value = if c == 0 { a } else { ((a << c) | (a >> (bits - c))) & m };
            // After a left rotate CF is the bit that wrapped into bit 0.
            let cf = value & 1 != 0;
            let of = (value & sb != 0) ^ cf;
            return ShiftResult {
                value,
                flags: Some(ShiftFlags { cf, of: opt_of(count, of), szp: None }),
            };
        }
        ShiftOp::Ror => {
            let c = count % bits;
            let value = if c == 0 { a } else { ((a >> c) | (a << (bits - c))) & m };
            // After a right rotate CF is the new most-significant bit.
            let cf = value & sb != 0;
            // OF (count==1) is the xor of the top two bits of the result.
            let second = value & (sb >> 1) != 0;
            let of = (value & sb != 0) ^ second;
            return ShiftResult {
                value,
                flags: Some(ShiftFlags { cf, of: opt_of(count, of), szp: None }),
            };
        }
    };
    let (zf, sf, pf) = zsp(value, size);
    ShiftResult {
        value,
        flags: Some(ShiftFlags { cf, of: opt_of(count, of), szp: Some((sf, zf, pf)) }),
    }
}

/// OF is only meaningful for a count of exactly 1.
fn opt_of(count: u64, of: bool) -> Option<bool> {
    if count == 1 {
        Some(of)
    } else {
        None
    }
}

/// Sign-extend a width-masked value to a full 64-bit signed integer.
pub fn sign_extend(v: u64, size: Size) -> i64 {
    match size {
        Size::Byte => v as u8 as i8 as i64,
        Size::Word => v as u16 as i16 as i64,
        Size::Dword => v as u32 as i32 as i64,
        Size::Qword => v as i64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_overflow_on_add_sets_of_not_cf() {
        // 0x7fff_ffff_ffff_ffff + 1 wraps to the minimum negative: OF set,
        // CF clear (no unsigned carry out of the top).
        let (res, f) = add(0x7fff_ffff_ffff_ffff, 1, Size::Qword);
        assert_eq!(res, 0x8000_0000_0000_0000);
        assert!(f.of);
        assert!(!f.cf);
        assert!(f.sf);
        assert!(!f.zf);
    }

    #[test]
    fn unsigned_borrow_on_sub_sets_cf_not_of() {
        // 0 - 1: CF set (borrow), OF clear (no signed overflow).
        let (res, f) = sub(0, 1, Size::Qword);
        assert_eq!(res, u64::MAX);
        assert!(f.cf);
        assert!(!f.of);
        assert!(f.sf);
    }

    #[test]
    fn logic_clears_carry_and_overflow() {
        let f = logic(0xff, Size::Byte);
        assert!(!f.cf && !f.of);
        assert!(!f.zf);
        assert!(f.pf, "0xff has eight set bits, even parity");
    }

    #[test]
    fn shl_by_zero_count_touches_nothing() {
        let r = shift(ShiftOp::Shl, 0x1234, 0, Size::Word);
        assert_eq!(r.value, 0x1234);
        assert!(r.flags.is_none());
    }

    #[test]
    fn shl_carry_is_the_last_bit_out() {
        // 0x80 << 1 in a byte: the 1 falls off the top, CF=1, result 0.
        let r = shift(ShiftOp::Shl, 0x80, 1, Size::Byte);
        assert_eq!(r.value, 0);
        let f = r.flags.unwrap();
        assert!(f.cf);
        // The sign bit flipped from 1 to 0, so OF(count==1) is set.
        assert_eq!(f.of, Some(true));
    }

    #[test]
    fn sar_replicates_the_sign_bit() {
        // 0x80 (byte) arithmetic-shifted right by 1 stays negative: 0xc0.
        let r = shift(ShiftOp::Sar, 0x80, 1, Size::Byte);
        assert_eq!(r.value, 0xc0);
    }
}
