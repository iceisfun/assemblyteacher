//! A tiny, permission-checked, non-overlapping memory map.
//!
//! The model is deliberately close to how an operating system presents virtual
//! memory to a process: a handful of regions ("pages", loosely), each with its
//! own read/write/execute permissions, and nothing at all in the gaps between
//! them. Touching a gap is a page fault; writing a read-only page is a
//! protection fault. Those two distinctions are the whole point — they are what
//! the memory-safety and W^X lessons demonstrate.

use crate::fault::{Access, Fault};
use serde::Serialize;

/// Read / write / execute permission bits for a region.
///
/// Stored as one byte, `R=4 W=2 X=1`, so the numeric value reads like the octal
/// mode a Unix user already knows. The individual bits and the common
/// combinations are exposed as associated constants.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Perms(pub u8);

impl Perms {
    pub const NONE: Perms = Perms(0);
    pub const R: Perms = Perms(0b100);
    pub const W: Perms = Perms(0b010);
    pub const X: Perms = Perms(0b001);
    pub const RW: Perms = Perms(0b110);
    pub const RX: Perms = Perms(0b101);
    pub const RWX: Perms = Perms(0b111);

    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Does this permission set include every bit in `needed`?
    pub const fn contains(self, needed: Perms) -> bool {
        self.0 & needed.0 == needed.0
    }
}

impl core::fmt::Display for Perms {
    /// The familiar `rwx` triad, with `-` for absent bits.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let c = |bit: u8, ch: char| if self.0 & bit != 0 { ch } else { '-' };
        write!(f, "{}{}{}", c(0b100, 'r'), c(0b010, 'w'), c(0b001, 'x'))
    }
}

impl core::fmt::Debug for Perms {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Perms({})", self)
    }
}

impl Serialize for Perms {
    /// Serialised as the human string `"r-x"`, which is what the UI wants to
    /// print, rather than the raw bit value.
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

/// One contiguous mapping. `data.len()` is the region's size; the region covers
/// `base .. base + data.len()`.
#[derive(Clone, Debug, Serialize)]
pub struct Region {
    pub base: u64,
    pub name: String,
    pub perms: Perms,
    pub data: Vec<u8>,
}

impl Region {
    fn end(&self) -> u64 {
        self.base.wrapping_add(self.data.len() as u64)
    }
}

/// The process address space: a set of regions kept sorted by base address and
/// guaranteed not to overlap.
#[derive(Clone, Debug, Default)]
pub struct Memory {
    regions: Vec<Region>,
}

impl Memory {
    pub fn new() -> Memory {
        Memory { regions: Vec::new() }
    }

    /// Map `size` zero-filled bytes at `base`.
    pub fn map(&mut self, base: u64, size: usize, perms: Perms, name: &str) {
        self.map_with(base, vec![0u8; size], perms, name);
    }

    /// Map an existing byte vector at `base` (e.g. a program's code image).
    pub fn map_with(&mut self, base: u64, data: Vec<u8>, perms: Perms, name: &str) {
        let region = Region { base, name: name.to_string(), perms, data };
        // Keep the vector sorted by base so lookups and the UI's region list are
        // both in address order. Overlaps are a caller bug; the last writer of a
        // given base wins after the sort, which is good enough for a teaching
        // sandbox that only ever maps a handful of disjoint regions.
        let pos = self.regions.partition_point(|r| r.base < region.base);
        self.regions.insert(pos, region);
    }

    pub fn regions(&self) -> &[Region] {
        &self.regions
    }

    /// Find the region wholly containing `addr .. addr+len` and confirm it
    /// grants `needed`. Returns the region index and the offset of `addr`
    /// within it. An access that straddles two regions is treated as unmapped:
    /// the second region might not exist, and even if it does the two need not
    /// be adjacent in the backing store.
    fn locate(
        &self,
        addr: u64,
        len: usize,
        needed: Perms,
        access: Access,
    ) -> Result<(usize, usize), Fault> {
        let end = addr.checked_add(len as u64).ok_or(Fault::NotMapped { addr, len, access })?;
        for (i, r) in self.regions.iter().enumerate() {
            if addr >= r.base && end <= r.end() {
                if !r.perms.contains(needed) {
                    return Err(Fault::Permission { addr, len, needed, have: r.perms, access });
                }
                return Ok((i, (addr - r.base) as usize));
            }
        }
        Err(Fault::NotMapped { addr, len, access })
    }

    /// A data read. Requires the `R` permission.
    pub fn read(&self, addr: u64, n: usize) -> Result<Vec<u8>, Fault> {
        let (i, off) = self.locate(addr, n, Perms::R, Access::Read)?;
        Ok(self.regions[i].data[off..off + n].to_vec())
    }

    /// A data write. Requires `W`.
    pub fn write(&mut self, addr: u64, bytes: &[u8]) -> Result<(), Fault> {
        self.write_capturing(addr, bytes).map(|_| ())
    }

    /// Like [`Memory::write`], but returns the bytes that were there before, so
    /// the interpreter can record a before/after diff without a second read
    /// (which would spuriously require `R` on a write-only page).
    pub(crate) fn write_capturing(&mut self, addr: u64, bytes: &[u8]) -> Result<Vec<u8>, Fault> {
        let (i, off) = self.locate(addr, bytes.len(), Perms::W, Access::Write)?;
        let region = &mut self.regions[i];
        let old = region.data[off..off + bytes.len()].to_vec();
        region.data[off..off + bytes.len()].copy_from_slice(bytes);
        Ok(old)
    }

    /// An instruction fetch. Requires `X`.
    pub fn fetch(&self, addr: u64, n: usize) -> Result<Vec<u8>, Fault> {
        let (i, off) = self.locate(addr, n, Perms::X, Access::Fetch)?;
        Ok(self.regions[i].data[off..off + n].to_vec())
    }

    /// Fetch *up to* `max` executable bytes starting at `addr`, stopping at the
    /// end of the containing region. The decoder needs a window, not an exact
    /// count, because instruction length is not known until it is decoded; if
    /// the window is short the decoder reports a truncation.
    pub fn fetch_slice(&self, addr: u64, max: usize) -> Result<Vec<u8>, Fault> {
        // Zero-length probe just to classify the address (mapped? executable?).
        let (i, off) = self.locate(addr, 0, Perms::X, Access::Fetch)?;
        let region = &self.regions[i];
        let avail = region.data.len() - off;
        let take = avail.min(max);
        Ok(region.data[off..off + take].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perms_display_is_the_rwx_triad() {
        assert_eq!(Perms::RX.to_string(), "r-x");
        assert_eq!(Perms::RW.to_string(), "rw-");
        assert_eq!(Perms::RWX.to_string(), "rwx");
        assert_eq!(Perms::NONE.to_string(), "---");
    }

    #[test]
    fn contains_is_a_subset_test() {
        assert!(Perms::RWX.contains(Perms::R));
        assert!(Perms::RX.contains(Perms::X));
        assert!(!Perms::RX.contains(Perms::W));
    }

    #[test]
    fn unmapped_access_faults_rather_than_panics() {
        let m = Memory::new();
        assert!(matches!(m.read(0x1000, 4), Err(Fault::NotMapped { .. })));
    }

    #[test]
    fn write_to_readonly_page_is_a_permission_fault() {
        let mut m = Memory::new();
        m.map(0x1000, 16, Perms::RX, "text");
        let e = m.write(0x1000, &[1, 2, 3]).unwrap_err();
        assert!(matches!(e, Fault::Permission { needed: Perms::W, .. }));
    }

    #[test]
    fn write_capturing_returns_the_previous_bytes() {
        let mut m = Memory::new();
        m.map(0x2000, 8, Perms::RW, "data");
        let old = m.write_capturing(0x2000, &[0xaa, 0xbb]).unwrap();
        assert_eq!(old, vec![0, 0]);
        assert_eq!(m.read(0x2000, 2).unwrap(), vec![0xaa, 0xbb]);
    }

    #[test]
    fn access_spanning_two_regions_is_unmapped() {
        let mut m = Memory::new();
        m.map(0x1000, 0x10, Perms::RW, "a");
        m.map(0x1010, 0x10, Perms::RW, "b");
        // Read four bytes straddling the 0x1010 boundary.
        assert!(matches!(m.read(0x100e, 4), Err(Fault::NotMapped { .. })));
    }
}
