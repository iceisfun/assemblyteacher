//! Bounds-checked, little-endian primitive reads over a `&[u8]`.
//!
//! Both ELF (in this crate: little-endian only) and PE32+ store their integers
//! little-endian, so every multi-byte read here is LE.  The whole point of this
//! module is that *no other file in the crate is allowed to index a slice
//! directly for header data* — they all go through these functions, which means
//! bounds checking lives in exactly one place and can be audited in one sitting.

use crate::error::BinError;

/// A byte in the buffer, or a precise `Truncated` error.
#[inline]
pub(crate) fn u8_at(b: &[u8], off: usize) -> Result<u8, BinError> {
    b.get(off).copied().ok_or_else(|| BinError::truncated(off, 1, b.len()))
}

/// `len` bytes starting at `off`, or `Truncated`.  Uses `checked_add` so a
/// hostile `off + len` cannot wrap around `usize::MAX`.
#[inline]
pub(crate) fn bytes_at(b: &[u8], off: usize, len: usize) -> Result<&[u8], BinError> {
    let end = off.checked_add(len).ok_or(BinError::Overflow("slice end"))?;
    b.get(off..end).ok_or_else(|| BinError::truncated(off, len, b.len()))
}

macro_rules! le_reader {
    ($name:ident, $ty:ty, $n:literal) => {
        #[doc = concat!("Read a little-endian `", stringify!($ty), "` at `off`.")]
        #[inline]
        pub(crate) fn $name(b: &[u8], off: usize) -> Result<$ty, BinError> {
            let raw = bytes_at(b, off, $n)?;
            // `raw` is exactly $n bytes, so the array conversion cannot fail.
            let arr: [u8; $n] = raw.try_into().map_err(|_| BinError::Overflow("array"))?;
            Ok(<$ty>::from_le_bytes(arr))
        }
    };
}

le_reader!(u16_at, u16, 2);
le_reader!(u32_at, u32, 4);
le_reader!(u64_at, u64, 8);
le_reader!(i64_at, i64, 8);

/// Read a NUL-terminated string starting at `off` within `b`.
///
/// This is deliberately forgiving, because real string tables are full of
/// surprises:
/// * The search stops at the first `0x00` **or** at the end of the buffer, so
///   an unterminated string yields whatever bytes remain rather than reading
///   past the end.
/// * Bytes are decoded with `from_utf8_lossy`, so invalid UTF-8 (which is legal
///   in ELF/PE string tables) becomes replacement characters instead of an
///   error or a panic.
pub(crate) fn cstr_at(b: &[u8], off: usize) -> String {
    if off >= b.len() {
        return String::new();
    }
    let rest = &b[off..];
    let end = rest.iter().position(|&c| c == 0).unwrap_or(rest.len());
    String::from_utf8_lossy(&rest[..end]).into_owned()
}

/// Read a NUL-terminated string that lives inside a sub-slice (a string table)
/// at a table-relative index.  Returns an empty string if the index is out of
/// range rather than erroring — a bad name should never sink a whole parse.
pub(crate) fn cstr_in(table: &[u8], index: u32) -> String {
    cstr_at(table, index as usize)
}
