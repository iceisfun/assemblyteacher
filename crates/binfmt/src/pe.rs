//! PE32/PE32+ parser (Portable Executable — Windows).
//!
//! The on-disk layout is a stack of headers you reach by hopping pointers:
//!
//! ```text
//! offset 0x00  DOS header ("MZ")  ── e_lfanew (u32 @ 0x3C) ──▶
//! offset e_lfanew   "PE\0\0" signature
//!                   COFF file header (20 bytes)
//!                   optional header  (PE32 or PE32+; magic says which)
//!                     └─ data directories (export/import/reloc/TLS/…)
//!                   section table (one 40-byte row per section)
//! ```
//!
//! Almost everything interesting is addressed by **RVA** (Relative Virtual
//! Address = offset from the image base once loaded).  To read those bytes out
//! of the *file* we must translate each RVA back to a file offset through the
//! section table — [`rva_to_off`] is that translation, and it rejects any RVA
//! that lands outside every section, which is the single most important
//! bounds-safety check in this file.

use crate::error::BinError;
use crate::reader::{bytes_at, cstr_at, u16_at, u32_at, u64_at};
use crate::types::*;

const DOS_MAGIC: u16 = 0x5a4d; // 'MZ'
const PE_SIG: [u8; 4] = [b'P', b'E', 0, 0];
const PE32_MAGIC: u16 = 0x10b;
const PE32PLUS_MAGIC: u16 = 0x20b;

// COFF machine types.
const IMAGE_FILE_MACHINE_I386: u16 = 0x014c;
const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;
const IMAGE_FILE_MACHINE_ARM64: u16 = 0xaa64;

// Section characteristics.
const SCN_CNT_CODE: u32 = 0x0000_0020;
const SCN_MEM_EXECUTE: u32 = 0x2000_0000;
const SCN_MEM_READ: u32 = 0x4000_0000;
const SCN_MEM_WRITE: u32 = 0x8000_0000;

// DllCharacteristics bits (mitigations).
const DLL_HIGH_ENTROPY_VA: u16 = 0x0020;
const DLL_DYNAMIC_BASE: u16 = 0x0040;
const DLL_NX_COMPAT: u16 = 0x0100;
const DLL_GUARD_CF: u16 = 0x4000;

// Data directory indices.
const DIR_EXPORT: usize = 0;
const DIR_IMPORT: usize = 1;
const DIR_BASERELOC: usize = 5;
const DIR_LOAD_CONFIG: usize = 10;
const DIR_DELAY_IMPORT: usize = 13;

// Structure sizes.
const COFF_SIZE: usize = 20;
const SECTION_ROW: usize = 40;
const IMPORT_DESC_SIZE: usize = 20;
const DELAY_DESC_SIZE: usize = 32;
const EXPORT_DIR_SIZE: usize = 40;

const MAX_SECTIONS: usize = 96; // the Windows loader's own hard limit.
const MAX_THUNKS: usize = 1 << 16; // guard runaway/looping thunk arrays.
const MAX_DESCRIPTORS: usize = 4096;

/// One data-directory entry (RVA + size).
#[derive(Clone, Copy, Default)]
struct DataDir {
    rva: u32,
    size: u32,
}

/// A decoded section-table row.
struct SecRow {
    name: String,
    virtual_size: u32,
    virtual_address: u32,
    size_of_raw_data: u32,
    pointer_to_raw_data: u32,
    characteristics: u32,
}

/// Everything the header phase produces.
struct PeHeaders {
    machine: u16,
    is_plus: bool,
    entry_rva: u32,
    image_base: u64,
    dll_characteristics: u16,
    dirs: Vec<DataDir>,
    sections: Vec<SecRow>,
}

/// Cheap format sniff: `MZ`, then a plausible `PE\0\0` at `e_lfanew`.
pub(crate) fn is_pe(bytes: &[u8]) -> bool {
    if u16_at(bytes, 0).unwrap_or(0) != DOS_MAGIC {
        return false;
    }
    let Ok(lfanew) = u32_at(bytes, 0x3c) else {
        return false;
    };
    let off = lfanew as usize;
    bytes_at(bytes, off, 4).map(|s| s == PE_SIG).unwrap_or(false)
}

/// Parse a PE32/PE32+ image into the neutral [`Image`] model.
pub(crate) fn parse(bytes: &[u8]) -> Result<Image, BinError> {
    let h = parse_headers(bytes)?;

    let arch = match h.machine {
        IMAGE_FILE_MACHINE_AMD64 => Arch::X86_64,
        IMAGE_FILE_MACHINE_I386 => Arch::X86,
        IMAGE_FILE_MACHINE_ARM64 => Arch::Aarch64,
        other => Arch::Other(other),
    };

    let sections = build_sections(&h);
    let segments = build_segments(&h);

    let entry =
        if h.entry_rva == 0 { 0 } else { h.image_base.wrapping_add(u64::from(h.entry_rva)) };

    let is_pie = h.dll_characteristics & DLL_DYNAMIC_BASE != 0;

    let imports = parse_imports(bytes, &h);
    let exports = parse_exports(bytes, &h);
    let relocations = parse_base_relocs(bytes, &h);
    let mitigations = build_mitigations(bytes, &h);

    Ok(Image {
        format: Format::Pe,
        arch,
        entry,
        image_base: h.image_base,
        is_pie,
        sections,
        segments,
        // PE has no symbol table in the ELF sense (the COFF symbol table is
        // usually stripped from release images); exports/imports carry the
        // names a reverse engineer needs.
        symbols: Vec::new(),
        imports,
        exports,
        relocations,
        mitigations,
    })
}

fn parse_headers(bytes: &[u8]) -> Result<PeHeaders, BinError> {
    if u16_at(bytes, 0)? != DOS_MAGIC {
        return Err(BinError::BadDosMagic);
    }
    let lfanew = u32_at(bytes, 0x3c)? as usize;
    if bytes_at(bytes, lfanew, 4)? != PE_SIG {
        return Err(BinError::BadPeSignature);
    }

    let coff = lfanew.checked_add(4).ok_or(BinError::Overflow("coff offset"))?;
    // Ensure the whole COFF header is present before trusting its fields.
    bytes_at(bytes, coff, COFF_SIZE)?;
    let machine = u16_at(bytes, coff)?;
    let num_sections = usize::from(u16_at(bytes, coff + 2)?);
    let size_opt = usize::from(u16_at(bytes, coff + 16)?);
    if num_sections > MAX_SECTIONS {
        return Err(BinError::Malformed("too many sections"));
    }

    let opt = coff.checked_add(COFF_SIZE).ok_or(BinError::Overflow("opt offset"))?;
    let magic = u16_at(bytes, opt)?;
    let is_plus = match magic {
        PE32PLUS_MAGIC => true,
        PE32_MAGIC => false,
        other => return Err(BinError::UnsupportedPeMagic(other)),
    };

    let entry_rva = u32_at(bytes, opt + 16)?;

    // The Windows-specific fields differ between PE32 and PE32+ only in the
    // width/position of ImageBase and hence where the directory count sits.
    let (image_base, num_rva_off, dirs_off) = if is_plus {
        (u64_at(bytes, opt + 24)?, opt + 108, opt + 112)
    } else {
        (u64::from(u32_at(bytes, opt + 28)?), opt + 92, opt + 96)
    };

    let dll_characteristics = u16_at(bytes, opt + 70)?;

    let num_rva = u32_at(bytes, num_rva_off)?;
    // Clamp to a sane count and to what the optional header could hold.
    let num_dirs = (num_rva as usize).min(16);
    let mut dirs = Vec::with_capacity(num_dirs);
    for i in 0..num_dirs {
        let base = dirs_off
            .checked_add(i.checked_mul(8).ok_or(BinError::Overflow("dir index"))?)
            .ok_or(BinError::Overflow("dir offset"))?;
        // A truncated directory array just means fewer directories.
        let Ok(rva) = u32_at(bytes, base) else { break };
        let size = u32_at(bytes, base + 4).unwrap_or(0);
        dirs.push(DataDir { rva, size });
    }

    // The section table starts right after the optional header.
    let sec_base = opt.checked_add(size_opt).ok_or(BinError::Overflow("section table offset"))?;
    let mut sections = Vec::with_capacity(num_sections.min(MAX_SECTIONS));
    for i in 0..num_sections {
        let off = sec_base
            .checked_add(i.checked_mul(SECTION_ROW).ok_or(BinError::Overflow("section index"))?)
            .ok_or(BinError::Overflow("section offset"))?;
        if bytes_at(bytes, off, SECTION_ROW).is_err() {
            break;
        }
        let raw_name = bytes_at(bytes, off, 8)?;
        // Section names are a fixed 8-byte field, NUL-padded (long names via
        // "/nnn" into the string table are ignored here — rare in real images).
        let name = cstr_at(raw_name, 0);
        sections.push(SecRow {
            name,
            virtual_size: u32_at(bytes, off + 8)?,
            virtual_address: u32_at(bytes, off + 12)?,
            size_of_raw_data: u32_at(bytes, off + 16)?,
            pointer_to_raw_data: u32_at(bytes, off + 20)?,
            characteristics: u32_at(bytes, off + 36)?,
        });
    }

    Ok(PeHeaders { machine, is_plus, entry_rva, image_base, dll_characteristics, dirs, sections })
}

fn dir(h: &PeHeaders, idx: usize) -> DataDir {
    h.dirs.get(idx).copied().unwrap_or_default()
}

fn sec_flags(ch: u32) -> SectionFlags {
    SectionFlags {
        alloc: ch & SCN_MEM_READ != 0,
        write: ch & SCN_MEM_WRITE != 0,
        execute: ch & (SCN_MEM_EXECUTE | SCN_CNT_CODE) != 0,
    }
}

fn build_sections(h: &PeHeaders) -> Vec<Section> {
    h.sections
        .iter()
        .map(|s| Section {
            name: s.name.clone(),
            address: h.image_base.wrapping_add(u64::from(s.virtual_address)),
            size: u64::from(s.virtual_size),
            file_offset: u64::from(s.pointer_to_raw_data),
            // A section maps at most SizeOfRawData bytes from the file; the rest
            // of its virtual size is zero-filled (like .bss).
            file_size: u64::from(s.size_of_raw_data),
            flags: sec_flags(s.characteristics),
        })
        .collect()
}

/// PE has no program headers, so we synthesise one segment per section to keep
/// the loader's-eye-view uniform with ELF.
fn build_segments(h: &PeHeaders) -> Vec<Segment> {
    h.sections
        .iter()
        .map(|s| Segment {
            kind: s.name.clone(),
            vaddr: h.image_base.wrapping_add(u64::from(s.virtual_address)),
            filesz: u64::from(s.size_of_raw_data),
            memsz: u64::from(s.virtual_size.max(s.size_of_raw_data)),
            perms: sec_flags(s.characteristics),
            offset: u64::from(s.pointer_to_raw_data),
        })
        .collect()
}

/// Translate an RVA to a file offset via the section table.
///
/// Returns `None` (never panics, never guesses) for an RVA that falls outside
/// every section's *raw* extent — that's the loop-and-overrun guard the SOW
/// calls out.
fn rva_to_off(h: &PeHeaders, rva: u32) -> Option<usize> {
    for s in &h.sections {
        let start = s.virtual_address;
        // Use the raw size for file mapping: bytes beyond it are uninitialised
        // and simply not present in the file.
        let raw = s.size_of_raw_data;
        let end = start.checked_add(raw)?;
        if rva >= start && rva < end {
            let delta = rva - start;
            let off = s.pointer_to_raw_data.checked_add(delta)?;
            return usize::try_from(off).ok();
        }
    }
    None
}

/// Read a NUL-terminated string that lives at an RVA.
fn cstr_at_rva(bytes: &[u8], h: &PeHeaders, rva: u32) -> Option<String> {
    let off = rva_to_off(h, rva)?;
    Some(cstr_at(bytes, off))
}

/// Parse the import directory *and* the delay-load import directory.
fn parse_imports(bytes: &[u8], h: &PeHeaders) -> Vec<Import> {
    let mut out = Vec::new();
    parse_import_dir(bytes, h, false, &mut out);
    parse_import_dir(bytes, h, true, &mut out);
    out
}

/// Shared walker for both the normal (`IMAGE_IMPORT_DESCRIPTOR`) and delay-load
/// (`IMAGE_DELAYLOAD_DESCRIPTOR`) tables — they differ only in record layout and
/// which fields hold the name/thunk RVAs.
fn parse_import_dir(bytes: &[u8], h: &PeHeaders, delay: bool, out: &mut Vec<Import>) {
    let d = dir(h, if delay { DIR_DELAY_IMPORT } else { DIR_IMPORT });
    if d.rva == 0 {
        return;
    }
    let Some(mut off) = rva_to_off(h, d.rva) else {
        return;
    };
    let desc_size = if delay { DELAY_DESC_SIZE } else { IMPORT_DESC_SIZE };

    for _ in 0..MAX_DESCRIPTORS {
        let Ok(row) = bytes_at(bytes, off, desc_size) else {
            break;
        };
        // A record of all zeroes terminates the array.
        if row.iter().all(|&b| b == 0) {
            break;
        }

        let (name_rva, ilt_rva, iat_rva) = if delay {
            // Delay descriptor: DllNameRVA@4, ImportNameTableRVA@16, IAT@12.
            // Modern linkers emit RVA-based tables (Attributes bit0 = 1);
            // pre-VS2015 used absolute VAs, which we do not attempt to rebase.
            (
                u32_at(bytes, off + 4).unwrap_or(0),
                u32_at(bytes, off + 16).unwrap_or(0),
                u32_at(bytes, off + 12).unwrap_or(0),
            )
        } else {
            // Import descriptor: Name@12, OriginalFirstThunk(ILT)@0, IAT@16.
            (
                u32_at(bytes, off + 12).unwrap_or(0),
                u32_at(bytes, off).unwrap_or(0),
                u32_at(bytes, off + 16).unwrap_or(0),
            )
        };

        let library = cstr_at_rva(bytes, h, name_rva).filter(|s| !s.is_empty());

        // The Import Lookup Table (a.k.a. OriginalFirstThunk / hint-name table)
        // holds the *names*; the IAT holds the addresses to be filled in.  Some
        // bound imports null the ILT and keep names only in the IAT, so fall
        // back to the IAT when the ILT is absent.
        let thunk_rva = if ilt_rva != 0 { ilt_rva } else { iat_rva };
        walk_thunks(bytes, h, thunk_rva, iat_rva, delay, library.as_deref(), out);

        off = match off.checked_add(desc_size) {
            Some(v) => v,
            None => break,
        };
    }
}

/// Walk a thunk array, emitting one [`Import`] per non-terminator entry.
fn walk_thunks(
    bytes: &[u8],
    h: &PeHeaders,
    thunk_rva: u32,
    iat_rva: u32,
    delay: bool,
    library: Option<&str>,
    out: &mut Vec<Import>,
) {
    if thunk_rva == 0 {
        return;
    }
    let Some(mut toff) = rva_to_off(h, thunk_rva) else {
        return;
    };
    let width = if h.is_plus { 8 } else { 4 };
    // High bit of the thunk marks an ordinal import.
    let ord_flag: u64 = if h.is_plus { 0x8000_0000_0000_0000 } else { 0x8000_0000 };

    for i in 0..MAX_THUNKS {
        let value = if h.is_plus {
            match u64_at(bytes, toff) {
                Ok(v) => v,
                Err(_) => break,
            }
        } else {
            match u32_at(bytes, toff) {
                Ok(v) => u64::from(v),
                Err(_) => break,
            }
        };
        if value == 0 {
            break; // null terminator
        }

        // The IAT slot address is imagebase + iat_rva + i*width, regardless of
        // whether we read names from the ILT or the IAT.
        let iat_address = if iat_rva != 0 {
            u64::from(iat_rva)
                .checked_add((i as u64).checked_mul(width as u64).unwrap_or(0))
                .and_then(|r| h.image_base.checked_add(r))
        } else {
            None
        };

        if value & ord_flag != 0 {
            // Ordinal import: low 16 bits are the ordinal, no name.
            let ordinal = (value & 0xffff) as u16;
            out.push(Import {
                name: String::new(),
                library: library.map(str::to_string),
                kind: ImportKind::Function,
                ordinal: Some(ordinal),
                iat_address,
            });
        } else {
            // Name import: the value is an RVA to IMAGE_IMPORT_BY_NAME =
            // { Hint(u16), Name(cstr) }.
            let by_name_rva = (value & 0xffff_ffff) as u32;
            let name = rva_to_off(h, by_name_rva)
                .map(|o| cstr_at(bytes, o.saturating_add(2)))
                .unwrap_or_default();
            out.push(Import {
                name,
                library: library.map(str::to_string),
                // We cannot tell code from data at the import table alone; most
                // imports are functions, but be honest and say Unknown for
                // delay imports where intent is murkier.
                kind: if delay { ImportKind::Unknown } else { ImportKind::Function },
                ordinal: None,
                iat_address,
            });
        }

        toff = match toff.checked_add(width) {
            Some(v) => v,
            None => break,
        };
    }
}

/// Parse the export directory: named exports, ordinal-only exports, and
/// forwarders (where the function RVA points *back into* the export directory).
fn parse_exports(bytes: &[u8], h: &PeHeaders) -> Vec<Export> {
    let mut out = Vec::new();
    let d = dir(h, DIR_EXPORT);
    if d.rva == 0 || d.size == 0 {
        return out;
    }
    let Some(base) = rva_to_off(h, d.rva) else {
        return out;
    };
    if bytes_at(bytes, base, EXPORT_DIR_SIZE).is_err() {
        return out;
    }

    let ordinal_base = u32_at(bytes, base + 16).unwrap_or(0);
    let num_functions = u32_at(bytes, base + 20).unwrap_or(0) as usize;
    let num_names = u32_at(bytes, base + 24).unwrap_or(0) as usize;
    let addr_functions = u32_at(bytes, base + 28).unwrap_or(0); // EAT rva
    let addr_names = u32_at(bytes, base + 32).unwrap_or(0); // name-ptr rva
    let addr_ordinals = u32_at(bytes, base + 36).unwrap_or(0); // name-ordinal rva

    // Sanity ceilings.
    if num_functions > (1 << 20) || num_names > (1 << 20) {
        return out;
    }

    // The forwarder test: a function RVA inside [dir.rva, dir.rva+size) is not a
    // real address but a pointer to a "OTHERDLL.Symbol" string.
    let dir_start = d.rva;
    let dir_end = d.rva.saturating_add(d.size);

    // First, map name-index → (name, ordinal-index) so we can attach names to
    // EAT slots.
    let mut name_for_slot: std::collections::BTreeMap<usize, (String, u16)> =
        std::collections::BTreeMap::new();
    let Some(eat_off) = rva_to_off(h, addr_functions) else {
        return out;
    };

    for i in 0..num_names {
        let name_ptr_off = match rva_to_off(h, addr_names) {
            Some(o) => o + i * 4,
            None => break,
        };
        let name_rva = match u32_at(bytes, name_ptr_off) {
            Ok(v) => v,
            Err(_) => break,
        };
        let ord_off = match rva_to_off(h, addr_ordinals) {
            Some(o) => o + i * 2,
            None => break,
        };
        let slot = match u16_at(bytes, ord_off) {
            Ok(v) => v as usize,
            Err(_) => break,
        };
        let name = cstr_at_rva(bytes, h, name_rva).unwrap_or_default();
        name_for_slot.insert(slot, (name, slot as u16));
    }

    for slot in 0..num_functions {
        let ent_off = match eat_off.checked_add(slot * 4) {
            Some(o) => o,
            None => break,
        };
        let func_rva = match u32_at(bytes, ent_off) {
            Ok(v) => v,
            Err(_) => break,
        };
        if func_rva == 0 {
            continue; // empty EAT slot
        }
        let ordinal = (ordinal_base as u16).wrapping_add(slot as u16);
        let (name, forwarder, address) = if func_rva >= dir_start && func_rva < dir_end {
            // Forwarder string, e.g. "NTDLL.RtlAllocateHeap".
            let fwd = cstr_at_rva(bytes, h, func_rva).unwrap_or_default();
            let nm = name_for_slot.get(&slot).map(|(n, _)| n.clone());
            (nm.unwrap_or_default(), Some(fwd), 0)
        } else {
            let nm = name_for_slot.get(&slot).map(|(n, _)| n.clone());
            let addr = h.image_base.wrapping_add(u64::from(func_rva));
            (nm.unwrap_or_default(), None, addr)
        };
        out.push(Export { name, address, ordinal: Some(ordinal), forwarder });
    }

    out
}

/// Parse the base-relocation directory into typed entries.
fn parse_base_relocs(bytes: &[u8], h: &PeHeaders) -> Vec<Reloc> {
    let mut out = Vec::new();
    let d = dir(h, DIR_BASERELOC);
    if d.rva == 0 || d.size == 0 {
        return out;
    }
    let Some(base) = rva_to_off(h, d.rva) else {
        return out;
    };
    let total = d.size as usize;
    let mut pos = 0usize;

    // The directory is a run of blocks; each block relocates one 4 KiB page.
    while pos + 8 <= total {
        let page_rva = match u32_at(bytes, base + pos) {
            Ok(v) => v,
            Err(_) => break,
        };
        let block_size = match u32_at(bytes, base + pos + 4) {
            Ok(v) => v as usize,
            Err(_) => break,
        };
        if block_size < 8 || pos.checked_add(block_size).map(|e| e > total).unwrap_or(true) {
            break; // malformed / would overrun the directory
        }
        let entries = (block_size - 8) / 2;
        for i in 0..entries {
            let eoff = base + pos + 8 + i * 2;
            let entry = match u16_at(bytes, eoff) {
                Ok(v) => v,
                Err(_) => break,
            };
            let typ = entry >> 12;
            let offset = entry & 0x0fff;
            // Type 0 (ABSOLUTE) is padding; skip it.
            if typ == 0 {
                continue;
            }
            let rva = page_rva.wrapping_add(u32::from(offset));
            out.push(Reloc {
                offset: h.image_base.wrapping_add(u64::from(rva)),
                kind: base_reloc_name(typ).to_string(),
                symbol: None,
                addend: 0,
            });
        }
        pos += block_size;
    }
    out
}

fn base_reloc_name(t: u16) -> &'static str {
    match t {
        0 => "IMAGE_REL_BASED_ABSOLUTE",
        1 => "IMAGE_REL_BASED_HIGH",
        2 => "IMAGE_REL_BASED_LOW",
        3 => "IMAGE_REL_BASED_HIGHLOW",
        4 => "IMAGE_REL_BASED_HIGHADJ",
        10 => "IMAGE_REL_BASED_DIR64",
        _ => "IMAGE_REL_BASED_UNKNOWN",
    }
}

fn build_mitigations(bytes: &[u8], h: &PeHeaders) -> Mitigations {
    let dc = h.dll_characteristics;
    let nx = dc & DLL_NX_COMPAT != 0;
    let aslr = dc & DLL_DYNAMIC_BASE != 0;
    let cfg = dc & DLL_GUARD_CF != 0;
    let _high_entropy = dc & DLL_HIGH_ENTROPY_VA != 0;

    // /GS stack cookie: the load-config directory carries a non-zero
    // SecurityCookie pointer when the image was built with stack protection.
    let (stack_canary, cet) = parse_load_config(bytes, h);

    Mitigations {
        nx,
        // On PE, "PIE" is expressed as ASLR-compatibility (DYNAMIC_BASE); there
        // is no separate flag, so pie mirrors aslr here.
        pie: aslr,
        relro: None,     // RELRO is an ELF/glibc concept; not applicable to PE.
        bind_now: false, // Likewise DT_BIND_NOW has no PE analogue.
        stack_canary,
        aslr,
        cfg,
        cet,
    }
}

/// Read the two mitigation facts we can get from the load-config directory:
/// the `/GS` security cookie (stack canary) and, best-effort, CET shadow-stack
/// compatibility.
///
/// Honest limitation: reliable CET detection needs the *extended* load-config
/// layout, whose size varies by toolchain version. We check the documented
/// `GuardFlags` CET bits when the directory is large enough to contain them and
/// otherwise report `false` rather than guessing.
fn parse_load_config(bytes: &[u8], h: &PeHeaders) -> (bool, bool) {
    let d = dir(h, DIR_LOAD_CONFIG);
    if d.rva == 0 {
        return (false, false);
    }
    let Some(base) = rva_to_off(h, d.rva) else {
        return (false, false);
    };
    // The first field is the structure's own Size; trust the smaller of it and
    // the directory size so we never read a field the image didn't actually
    // populate.
    let declared = u32_at(bytes, base).unwrap_or(0) as usize;
    let avail = declared.max(d.size as usize);

    // SecurityCookie sits at offset 0x58 in the 64-bit load-config struct
    // (0x40 in the 32-bit one).
    let cookie_off = if h.is_plus { 0x58 } else { 0x40 };
    let stack_canary = if avail > cookie_off {
        let val = if h.is_plus {
            u64_at(bytes, base + cookie_off).unwrap_or(0)
        } else {
            u64::from(u32_at(bytes, base + cookie_off).unwrap_or(0))
        };
        val != 0
    } else {
        false
    };

    // GuardFlags (64-bit layout) is at offset 0x90; the CET-relevant bits are
    // IMAGE_GUARD_CF_ENABLE_EXPORT_SUPPRESSION-adjacent shadow-stack flags.
    // IMAGE_GUARD_CF_INSTRUMENTED = 0x100 (that's CFG, reported separately);
    // the shadow-stack CET compat bit is 0x400000 in recent SDKs.
    const GUARD_FLAGS_OFF_64: usize = 0x90;
    const IMAGE_GUARD_CET_SHADOW_STACK: u32 = 0x0040_0000;
    let cet = if h.is_plus && avail > GUARD_FLAGS_OFF_64 + 4 {
        let gf = u32_at(bytes, base + GUARD_FLAGS_OFF_64).unwrap_or(0);
        gf & IMAGE_GUARD_CET_SHADOW_STACK != 0
    } else {
        false
    };

    (stack_canary, cet)
}
