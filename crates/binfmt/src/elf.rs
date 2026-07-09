//! ELF64 parser, little-endian only.
//!
//! ELF (Executable and Linkable Format) is built from three tables that point
//! at each other by *file offset*:
//!
//! * the **ELF header** (`Elf64_Ehdr`) at offset 0, which locates the other two;
//! * the **program header table** — the loader's view: which byte ranges become
//!   which memory ranges with which permissions;
//! * the **section header table** — the linker's view: `.text`, `.data`,
//!   `.symtab`, and so on, plus the string tables that name everything.
//!
//! A running program only needs the program headers; almost everything a
//! reverse engineer wants (names, symbols) lives in the section/dynamic tables.
//!
//! Every field access here goes through [`crate::reader`], so the parser cannot
//! panic no matter how corrupt the input is.

use crate::error::BinError;
use crate::reader::{bytes_at, cstr_in, i64_at, u16_at, u32_at, u64_at, u8_at};
use crate::types::*;

// --- e_ident indices & constants ---------------------------------------------
const EI_CLASS: usize = 4;
const EI_DATA: usize = 5;
const EI_VERSION: usize = 6;
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EV_CURRENT: u8 = 1;
const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

// --- e_type ------------------------------------------------------------------
// (ET_REL = 1 relocatable object, ET_EXEC = 2 fixed-address executable are the
// other values; only ET_DYN drives our PIE/shared-object logic.)
const ET_DYN: u16 = 3;

// --- e_machine ---------------------------------------------------------------
const EM_386: u16 = 3;
const EM_X86_64: u16 = 62;
const EM_AARCH64: u16 = 183;

// --- program header p_type ---------------------------------------------------
const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const PT_INTERP: u32 = 3;
const PT_NOTE: u32 = 4;
const PT_GNU_EH_FRAME: u32 = 0x6474_e550;
const PT_GNU_STACK: u32 = 0x6474_e551;
const PT_GNU_RELRO: u32 = 0x6474_e552;
const PT_GNU_PROPERTY: u32 = 0x6474_e553;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;

// --- section header sh_type / sh_flags ---------------------------------------
const SHT_SYMTAB: u32 = 2;
const SHT_RELA: u32 = 4;
const SHT_NOBITS: u32 = 8;
const SHT_REL: u32 = 9;
const SHT_DYNSYM: u32 = 11;
const SHF_WRITE: u64 = 1;
const SHF_ALLOC: u64 = 2;
const SHF_EXECINSTR: u64 = 4;

// --- special section indices (st_shndx) --------------------------------------
const SHN_UNDEF: u16 = 0;
const SHN_LORESERVE: u16 = 0xff00;

// --- symbol st_info ----------------------------------------------------------
const STB_LOCAL: u8 = 0;
const STB_GLOBAL: u8 = 1;
const STB_WEAK: u8 = 2;
const STT_NOTYPE: u8 = 0;
const STT_OBJECT: u8 = 1;
const STT_FUNC: u8 = 2;
const STT_SECTION: u8 = 3;
const STT_FILE: u8 = 4;

// --- dynamic tags ------------------------------------------------------------
const DT_NULL: i64 = 0;
const DT_NEEDED: i64 = 1;
const DT_STRTAB: i64 = 5;
const DT_STRSZ: i64 = 10;
const DT_SONAME: i64 = 14;
const DT_RPATH: i64 = 15;
const DT_RUNPATH: i64 = 29;
const DT_BIND_NOW: i64 = 24;
const DT_FLAGS: i64 = 30;
const DT_FLAGS_1: i64 = 0x6fff_fffb;
const DF_BIND_NOW: u64 = 0x08;
const DF_1_NOW: u64 = 0x0000_0001;
const DF_1_PIE: u64 = 0x0800_0000;

// --- structure sizes ---------------------------------------------------------
const EHDR_SIZE: usize = 64;
const PHDR_SIZE: usize = 56;
const SHDR_SIZE: usize = 64;
const SYM_SIZE: usize = 24;
const RELA_SIZE: usize = 24;
const REL_SIZE: usize = 16;
const DYN_SIZE: usize = 16;

// A defensive ceiling: no legitimate object we care about has millions of
// program/section headers, and refusing to even allocate for an absurd count
// is a cheap way to shrug off `e_shnum = 0xffff` style attacks early.
const MAX_HEADERS: usize = 1 << 20;

/// Decoded copy of the fields of `Elf64_Ehdr` we care about.
struct Ehdr {
    e_type: u16,
    e_machine: u16,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_phnum: usize,
    e_shnum: usize,
    e_shstrndx: usize,
}

/// A decoded program header.
struct Phdr {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_filesz: u64,
    p_memsz: u64,
}

/// A decoded section header (plus its resolved name).  Only the fields the
/// parser consumes are kept — a real `Elf64_Shdr` also has `sh_name` (raw),
/// `sh_info`, `sh_addralign` and `sh_entsize`.
struct Shdr {
    name: String,
    sh_type: u32,
    sh_flags: u64,
    sh_addr: u64,
    sh_offset: u64,
    sh_size: u64,
    sh_link: u32,
}

/// Cheap format sniff: does the buffer start with the ELF magic?
pub(crate) fn is_elf(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[..4] == ELF_MAGIC
}

/// Parse a little-endian ELF64 image into the neutral [`Image`] model.
pub(crate) fn parse(bytes: &[u8]) -> Result<Image, BinError> {
    let eh = parse_ehdr(bytes)?;

    let arch = match eh.e_machine {
        EM_X86_64 => Arch::X86_64,
        EM_386 => Arch::X86,
        EM_AARCH64 => Arch::Aarch64,
        other => Arch::Other(other),
    };

    let phdrs = parse_phdrs(bytes, &eh)?;
    let shdrs = parse_shdrs(bytes, &eh)?;

    let segments = build_segments(&phdrs);
    let sections = build_sections(&shdrs);

    // Dynamic info drives imports, needed-libraries and several mitigations.
    let dynamic = parse_dynamic(bytes, &phdrs, &shdrs);

    // Symbols from both static (.symtab) and dynamic (.dynsym) tables.
    let mut symbols = Vec::new();
    collect_symtab(bytes, &shdrs, SHT_SYMTAB, &mut symbols);
    let dynsym = collect_symtab(bytes, &shdrs, SHT_DYNSYM, &mut symbols);

    let relocations = collect_relocations(bytes, &shdrs);

    let is_pie = eh.e_type == ET_DYN;
    let image_base =
        phdrs.iter().filter(|p| p.p_type == PT_LOAD).map(|p| p.p_vaddr).min().unwrap_or(0);

    let imports = build_imports(bytes, &shdrs, &dynsym, &dynamic);
    let exports = build_exports(&dynsym);

    let mitigations = build_mitigations(bytes, &eh, &phdrs, &shdrs, &dynamic, &symbols, &imports);

    Ok(Image {
        format: Format::Elf,
        arch,
        entry: eh.e_entry,
        image_base,
        is_pie,
        sections,
        segments,
        symbols,
        imports,
        exports,
        relocations,
        mitigations,
    })
}

fn parse_ehdr(bytes: &[u8]) -> Result<Ehdr, BinError> {
    // Need the whole 64-byte header before trusting any field.
    let ident = bytes_at(bytes, 0, EHDR_SIZE)?;
    if ident[0..4] != ELF_MAGIC {
        return Err(BinError::BadElfMagic);
    }
    let class = u8_at(bytes, EI_CLASS)?;
    if class != ELFCLASS64 {
        return Err(BinError::UnsupportedElfClass(class));
    }
    let data = u8_at(bytes, EI_DATA)?;
    if data != ELFDATA2LSB {
        return Err(BinError::UnsupportedElfData(data));
    }
    let iversion = u8_at(bytes, EI_VERSION)?;
    if iversion != EV_CURRENT {
        return Err(BinError::UnsupportedElfVersion(u32::from(iversion)));
    }

    let e_phnum = usize::from(u16_at(bytes, 56)?);
    let e_shnum = usize::from(u16_at(bytes, 60)?);
    if e_phnum > MAX_HEADERS || e_shnum > MAX_HEADERS {
        return Err(BinError::Malformed("absurd program/section header count"));
    }
    // A header table can never be larger than the file that contains it: if the
    // declared count times the entry size already exceeds the buffer, the header
    // is lying. Reject early rather than limp along reading zeros.
    let ph_bytes = e_phnum.checked_mul(PHDR_SIZE).ok_or(BinError::Overflow("phdr table size"))?;
    let sh_bytes = e_shnum.checked_mul(SHDR_SIZE).ok_or(BinError::Overflow("shdr table size"))?;
    if ph_bytes > bytes.len() || sh_bytes > bytes.len() {
        return Err(BinError::Malformed("header table larger than file"));
    }

    Ok(Ehdr {
        e_type: u16_at(bytes, 16)?,
        e_machine: u16_at(bytes, 18)?,
        e_entry: u64_at(bytes, 24)?,
        e_phoff: u64_at(bytes, 32)?,
        e_shoff: u64_at(bytes, 40)?,
        e_phnum,
        e_shnum,
        e_shstrndx: usize::from(u16_at(bytes, 62)?),
    })
}

fn parse_phdrs(bytes: &[u8], eh: &Ehdr) -> Result<Vec<Phdr>, BinError> {
    let mut out = Vec::with_capacity(eh.e_phnum.min(64));
    if eh.e_phoff == 0 || eh.e_phnum == 0 {
        return Ok(out);
    }
    let base = usize::try_from(eh.e_phoff).map_err(|_| BinError::Overflow("e_phoff"))?;
    for i in 0..eh.e_phnum {
        let off = base
            .checked_add(i.checked_mul(PHDR_SIZE).ok_or(BinError::Overflow("phdr index"))?)
            .ok_or(BinError::Overflow("phdr offset"))?;
        // A truncated program header table is fatal-ish for the loader but we
        // simply stop; better a partial-but-sound parse than an error page.
        if bytes_at(bytes, off, PHDR_SIZE).is_err() {
            break;
        }
        out.push(Phdr {
            p_type: u32_at(bytes, off)?,
            p_flags: u32_at(bytes, off + 4)?,
            p_offset: u64_at(bytes, off + 8)?,
            p_vaddr: u64_at(bytes, off + 16)?,
            p_filesz: u64_at(bytes, off + 32)?,
            p_memsz: u64_at(bytes, off + 40)?,
        });
    }
    Ok(out)
}

fn parse_shdrs(bytes: &[u8], eh: &Ehdr) -> Result<Vec<Shdr>, BinError> {
    let mut out = Vec::with_capacity(eh.e_shnum.min(64));
    if eh.e_shoff == 0 || eh.e_shnum == 0 {
        return Ok(out);
    }
    let base = usize::try_from(eh.e_shoff).map_err(|_| BinError::Overflow("e_shoff"))?;

    // First pass: read the raw headers (no names yet).
    struct Raw {
        sh_name: u32,
        sh_type: u32,
        sh_flags: u64,
        sh_addr: u64,
        sh_offset: u64,
        sh_size: u64,
        sh_link: u32,
    }
    let mut raws: Vec<Raw> = Vec::with_capacity(eh.e_shnum.min(64));
    for i in 0..eh.e_shnum {
        let off = base
            .checked_add(i.checked_mul(SHDR_SIZE).ok_or(BinError::Overflow("shdr index"))?)
            .ok_or(BinError::Overflow("shdr offset"))?;
        if bytes_at(bytes, off, SHDR_SIZE).is_err() {
            break;
        }
        raws.push(Raw {
            sh_name: u32_at(bytes, off)?,
            sh_type: u32_at(bytes, off + 4)?,
            sh_flags: u64_at(bytes, off + 8)?,
            sh_addr: u64_at(bytes, off + 16)?,
            sh_offset: u64_at(bytes, off + 24)?,
            sh_size: u64_at(bytes, off + 32)?,
            sh_link: u32_at(bytes, off + 40)?,
        });
    }

    // The section-name string table is itself just a section, pointed to by
    // e_shstrndx.  A bogus index simply yields empty names rather than an error.
    let shstr: &[u8] = raws
        .get(eh.e_shstrndx)
        .filter(|_| eh.e_shstrndx < raws.len() && eh.e_shstrndx != 0)
        .and_then(|s| slice_of(bytes, s.sh_offset, s.sh_size))
        .unwrap_or(&[]);

    for r in raws {
        let name = cstr_in(shstr, r.sh_name);
        out.push(Shdr {
            name,
            sh_type: r.sh_type,
            sh_flags: r.sh_flags,
            sh_addr: r.sh_addr,
            sh_offset: r.sh_offset,
            sh_size: r.sh_size,
            sh_link: r.sh_link,
        });
    }
    Ok(out)
}

/// A bounds-checked view of `[off, off+size)` in the file, or `None`.
fn slice_of(bytes: &[u8], off: u64, size: u64) -> Option<&[u8]> {
    let off = usize::try_from(off).ok()?;
    let size = usize::try_from(size).ok()?;
    let end = off.checked_add(size)?;
    bytes.get(off..end)
}

fn perms_from_pflags(f: u32) -> SectionFlags {
    SectionFlags { alloc: f & PF_R != 0, write: f & PF_W != 0, execute: f & PF_X != 0 }
}

fn pt_name(t: u32) -> String {
    match t {
        PT_LOAD => "LOAD",
        PT_DYNAMIC => "DYNAMIC",
        PT_INTERP => "INTERP",
        PT_NOTE => "NOTE",
        PT_GNU_EH_FRAME => "GNU_EH_FRAME",
        PT_GNU_STACK => "GNU_STACK",
        PT_GNU_RELRO => "GNU_RELRO",
        PT_GNU_PROPERTY => "GNU_PROPERTY",
        6 => "PHDR",
        7 => "TLS",
        _ => return format!("PT_{t:#x}"),
    }
    .to_string()
}

fn build_segments(phdrs: &[Phdr]) -> Vec<Segment> {
    phdrs
        .iter()
        .map(|p| Segment {
            kind: pt_name(p.p_type),
            vaddr: p.p_vaddr,
            filesz: p.p_filesz,
            memsz: p.p_memsz,
            perms: perms_from_pflags(p.p_flags),
            offset: p.p_offset,
        })
        .collect()
}

fn build_sections(shdrs: &[Shdr]) -> Vec<Section> {
    shdrs
        .iter()
        // The first section header (index 0) is always the reserved SHN_UNDEF
        // null entry; skip it so the list matches `readelf -S`'s named rows.
        .filter(|s| !(s.sh_type == 0 && s.name.is_empty() && s.sh_addr == 0 && s.sh_size == 0))
        .map(|s| {
            let file_size = if s.sh_type == SHT_NOBITS { 0 } else { s.sh_size };
            Section {
                name: s.name.clone(),
                address: s.sh_addr,
                size: s.sh_size,
                file_offset: s.sh_offset,
                file_size,
                flags: SectionFlags {
                    alloc: s.sh_flags & SHF_ALLOC != 0,
                    write: s.sh_flags & SHF_WRITE != 0,
                    execute: s.sh_flags & SHF_EXECINSTR != 0,
                },
            }
        })
        .collect()
}

/// A decoded dynamic symbol, retained so relocations/imports/exports can refer
/// back to it by index.
struct DynSymEntry {
    name: String,
    value: u64,
    bind: u8,
    stype: u8,
    shndx: u16,
}

/// Everything we pull out of the `PT_DYNAMIC` / `.dynamic` array.
#[derive(Default)]
struct DynamicInfo {
    needed: Vec<String>,
    /// `DT_RPATH`/`DT_RUNPATH` search paths.  Parsed for completeness and as a
    /// teaching point (they are an oft-abused library-hijack vector), though the
    /// neutral `Image` model does not currently surface them.
    #[allow(dead_code)]
    runpath: Vec<String>,
    bind_now: bool,
    pie_flag: bool,
}

/// Collect one symbol table (`SHT_SYMTAB` or `SHT_DYNSYM`) into `out`, and also
/// return the decoded entries (used by the dynamic-symbol consumers).
fn collect_symtab(
    bytes: &[u8],
    shdrs: &[Shdr],
    want_type: u32,
    out: &mut Vec<Symbol>,
) -> Vec<DynSymEntry> {
    let mut entries = Vec::new();
    let Some(symsec) = shdrs.iter().find(|s| s.sh_type == want_type) else {
        return entries;
    };
    // sh_link on a symbol table names its string table.
    let strtab: &[u8] = shdrs
        .get(symsec.sh_link as usize)
        .and_then(|s| slice_of(bytes, s.sh_offset, s.sh_size))
        .unwrap_or(&[]);
    let Some(symdata) = slice_of(bytes, symsec.sh_offset, symsec.sh_size) else {
        return entries;
    };

    let count = symdata.len() / SYM_SIZE;
    for i in 0..count {
        let base = i * SYM_SIZE;
        // Reads are within `symdata`, which is already bounds-checked.
        let st_name = u32_at(symdata, base).unwrap_or(0);
        let st_info = u8_at(symdata, base + 4).unwrap_or(0);
        let st_shndx = u16_at(symdata, base + 6).unwrap_or(0);
        let st_value = u64_at(symdata, base + 8).unwrap_or(0);
        let st_size = u64_at(symdata, base + 16).unwrap_or(0);

        let bind = st_info >> 4;
        let stype = st_info & 0xf;
        let name = cstr_in(strtab, st_name);

        let section = section_name_for_index(shdrs, st_shndx);
        out.push(Symbol {
            name: name.clone(),
            address: st_value,
            size: st_size,
            kind: sym_kind(stype),
            binding: sym_binding(bind),
            section,
        });
        entries.push(DynSymEntry { name, value: st_value, bind, stype, shndx: st_shndx });
    }
    entries
}

fn section_name_for_index(shdrs: &[Shdr], shndx: u16) -> Option<String> {
    if shndx == SHN_UNDEF || shndx >= SHN_LORESERVE {
        return None;
    }
    shdrs.get(shndx as usize).map(|s| s.name.clone()).filter(|n| !n.is_empty())
}

fn sym_kind(t: u8) -> SymbolKind {
    match t {
        STT_FUNC => SymbolKind::Func,
        STT_OBJECT => SymbolKind::Object,
        STT_SECTION => SymbolKind::Section,
        STT_FILE => SymbolKind::File,
        STT_NOTYPE => SymbolKind::Notype,
        other => SymbolKind::Other(other),
    }
}

fn sym_binding(b: u8) -> SymbolBinding {
    match b {
        STB_LOCAL => SymbolBinding::Local,
        STB_GLOBAL => SymbolBinding::Global,
        STB_WEAK => SymbolBinding::Weak,
        other => SymbolBinding::Other(other),
    }
}

/// Name the common x86_64 relocation types; leave the rest as their number.
fn reloc_type_name(machine_x86_64: bool, ty: u32) -> String {
    if machine_x86_64 {
        let n = match ty {
            0 => "R_X86_64_NONE",
            1 => "R_X86_64_64",
            2 => "R_X86_64_PC32",
            3 => "R_X86_64_GOT32",
            4 => "R_X86_64_PLT32",
            5 => "R_X86_64_COPY",
            6 => "R_X86_64_GLOB_DAT",
            7 => "R_X86_64_JUMP_SLOT",
            8 => "R_X86_64_RELATIVE",
            9 => "R_X86_64_GOTPCREL",
            10 => "R_X86_64_32",
            11 => "R_X86_64_32S",
            16 => "R_X86_64_DTPMOD64",
            17 => "R_X86_64_DTPOFF64",
            18 => "R_X86_64_TPOFF64",
            19 => "R_X86_64_TLSGD",
            20 => "R_X86_64_TLSLD",
            22 => "R_X86_64_GOTTPOFF",
            23 => "R_X86_64_TPOFF32",
            24 => "R_X86_64_PC64",
            37 => "R_X86_64_IRELATIVE",
            _ => return format!("R_X86_64_{ty}"),
        };
        n.to_string()
    } else {
        format!("R_{ty}")
    }
}

/// Walk every `SHT_RELA`/`SHT_REL` section and decode its entries.
fn collect_relocations(bytes: &[u8], shdrs: &[Shdr]) -> Vec<Reloc> {
    let mut out = Vec::new();
    for sec in shdrs {
        let is_rela = sec.sh_type == SHT_RELA;
        let is_rel = sec.sh_type == SHT_REL;
        if !is_rela && !is_rel {
            continue;
        }
        let ent = if is_rela { RELA_SIZE } else { REL_SIZE };
        let Some(data) = slice_of(bytes, sec.sh_offset, sec.sh_size) else {
            continue;
        };
        // The relocation section's sh_link names the symbol table it indexes;
        // that table's sh_link names the matching string table.
        let symsec = shdrs.get(sec.sh_link as usize);
        let strtab: &[u8] = symsec
            .and_then(|ss| shdrs.get(ss.sh_link as usize))
            .and_then(|s| slice_of(bytes, s.sh_offset, s.sh_size))
            .unwrap_or(&[]);
        let symdata: &[u8] =
            symsec.and_then(|ss| slice_of(bytes, ss.sh_offset, ss.sh_size)).unwrap_or(&[]);

        let count = data.len() / ent;
        for i in 0..count {
            let base = i * ent;
            let r_offset = u64_at(data, base).unwrap_or(0);
            let r_info = u64_at(data, base + 8).unwrap_or(0);
            // For ELF64: high 32 bits = symbol index, low 32 = type.
            let sym_idx = (r_info >> 32) as u32;
            let r_type = (r_info & 0xffff_ffff) as u32;
            let addend = if is_rela { i64_at(data, base + 16).unwrap_or(0) } else { 0 };
            let symbol = sym_name_at_index(symdata, strtab, sym_idx);
            out.push(Reloc {
                offset: r_offset,
                kind: reloc_type_name(true, r_type),
                symbol,
                addend,
            });
        }
    }
    out
}

/// Look up the name of symbol #`idx` in a raw symbol-table slice.
fn sym_name_at_index(symdata: &[u8], strtab: &[u8], idx: u32) -> Option<String> {
    if idx == 0 {
        return None;
    }
    let base = (idx as usize).checked_mul(SYM_SIZE)?;
    let st_name = u32_at(symdata, base).ok()?;
    let name = cstr_in(strtab, st_name);
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

/// Parse the dynamic section from `PT_DYNAMIC` (falling back to a `.dynamic`
/// section).  The tricky part is that `DT_STRTAB` is a *virtual address*, so we
/// have to translate it back to a file offset via the `PT_LOAD` segments.
fn parse_dynamic(bytes: &[u8], phdrs: &[Phdr], shdrs: &[Shdr]) -> DynamicInfo {
    let mut info = DynamicInfo::default();

    // Locate the dynamic array (offset + size) in the file.
    let dyn_region =
        phdrs.iter().find(|p| p.p_type == PT_DYNAMIC).map(|p| (p.p_offset, p.p_filesz)).or_else(
            || shdrs.iter().find(|s| s.name == ".dynamic").map(|s| (s.sh_offset, s.sh_size)),
        );
    let Some((dyn_off, dyn_size)) = dyn_region else {
        return info;
    };
    let Some(dyn_data) = slice_of(bytes, dyn_off, dyn_size) else {
        return info;
    };

    // First scan: collect tags, plus the string-table VA and size so we can
    // resolve DT_NEEDED/DT_RUNPATH names in a second pass.
    let mut strtab_va = None;
    let mut strtab_sz = None;
    let mut raw: Vec<(i64, u64)> = Vec::new();
    let count = dyn_data.len() / DYN_SIZE;
    for i in 0..count {
        let base = i * DYN_SIZE;
        let tag = i64_at(dyn_data, base).unwrap_or(DT_NULL);
        let val = u64_at(dyn_data, base + 8).unwrap_or(0);
        if tag == DT_NULL {
            break;
        }
        match tag {
            DT_STRTAB => strtab_va = Some(val),
            DT_STRSZ => strtab_sz = Some(val),
            DT_FLAGS => {
                if val & DF_BIND_NOW != 0 {
                    info.bind_now = true;
                }
            }
            DT_FLAGS_1 => {
                if val & DF_1_NOW != 0 {
                    info.bind_now = true;
                }
                if val & DF_1_PIE != 0 {
                    info.pie_flag = true;
                }
            }
            DT_BIND_NOW => info.bind_now = true,
            _ => {}
        }
        raw.push((tag, val));
    }

    // Resolve the string table to a file slice.  Prefer the `.dynstr` section
    // (already a file offset); else translate the DT_STRTAB virtual address.
    let dynstr: &[u8] = shdrs
        .iter()
        .find(|s| s.name == ".dynstr")
        .and_then(|s| slice_of(bytes, s.sh_offset, s.sh_size))
        .or_else(|| {
            let va = strtab_va?;
            let off = vaddr_to_offset(phdrs, va)?;
            let sz = strtab_sz.unwrap_or(0);
            // If size is unknown, take the remainder of the file.
            if sz == 0 {
                bytes.get(off..)
            } else {
                slice_of(bytes, off as u64, sz)
            }
        })
        .unwrap_or(&[]);

    for (tag, val) in raw {
        match tag {
            DT_NEEDED => {
                let n = cstr_in(dynstr, val as u32);
                if !n.is_empty() {
                    info.needed.push(n);
                }
            }
            DT_RUNPATH | DT_RPATH => {
                let n = cstr_in(dynstr, val as u32);
                if !n.is_empty() {
                    info.runpath.push(n);
                }
            }
            DT_SONAME => { /* ignored for now, but parsed for completeness */ }
            _ => {}
        }
    }

    info
}

/// Translate a virtual address to a file offset using the `PT_LOAD` map.
fn vaddr_to_offset(phdrs: &[Phdr], va: u64) -> Option<usize> {
    for p in phdrs.iter().filter(|p| p.p_type == PT_LOAD) {
        let start = p.p_vaddr;
        let end = start.checked_add(p.p_filesz)?;
        if va >= start && va < end {
            let delta = va - start;
            let off = p.p_offset.checked_add(delta)?;
            return usize::try_from(off).ok();
        }
    }
    None
}

/// Reconstruct imports from dynamic relocations that reference *undefined*
/// dynamic symbols (the classic `JUMP_SLOT` for functions, `GLOB_DAT` for data).
fn build_imports(
    bytes: &[u8],
    shdrs: &[Shdr],
    dynsym: &[DynSymEntry],
    dynamic: &DynamicInfo,
) -> Vec<Import> {
    let mut out = Vec::new();
    if dynsym.is_empty() {
        return out;
    }

    // If exactly one library is NEEDED, attribute imports to it; otherwise we
    // cannot know which DLL/so provides a given symbol, so leave it None.  This
    // is a genuine ELF ambiguity worth a lesson: unlike PE, ELF dynamic symbols
    // are *not* tagged with their providing library.
    let library = if dynamic.needed.len() == 1 { Some(dynamic.needed[0].clone()) } else { None };

    // We only look at relocation sections whose symbol table is a DYNSYM, since
    // imports live in the dynamic symbol namespace.
    for sec in shdrs {
        let is_rela = sec.sh_type == SHT_RELA;
        let is_rel = sec.sh_type == SHT_REL;
        if !is_rela && !is_rel {
            continue;
        }
        let symsec = shdrs.get(sec.sh_link as usize);
        if symsec.map(|s| s.sh_type) != Some(SHT_DYNSYM) {
            continue;
        }
        let ent = if is_rela { RELA_SIZE } else { REL_SIZE };
        let Some(data) = slice_of(bytes, sec.sh_offset, sec.sh_size) else {
            continue;
        };
        let is_pltrel = sec.name == ".rela.plt" || sec.name == ".rel.plt";
        let count = data.len() / ent;
        for i in 0..count {
            let base = i * ent;
            let r_info = u64_at(data, base + 8).unwrap_or(0);
            let sym_idx = (r_info >> 32) as usize;
            let r_type = (r_info & 0xffff_ffff) as u32;
            // JUMP_SLOT (7) / GLOB_DAT (6) against an undefined symbol == import.
            let is_import_type = r_type == 6 || r_type == 7;
            if !is_import_type {
                continue;
            }
            let Some(sym) = dynsym.get(sym_idx) else {
                continue;
            };
            if sym.shndx != SHN_UNDEF || sym.name.is_empty() {
                continue;
            }
            let iat_address = {
                let off = u64_at(data, base).unwrap_or(0);
                if off == 0 {
                    None
                } else {
                    Some(off)
                }
            };
            out.push(Import {
                name: sym.name.clone(),
                library: library.clone(),
                // JUMP_SLOT is a PLT entry → a called function; GLOB_DAT is a
                // GOT data slot (which may still be a function pointer, hence
                // Unknown rather than Data when we can't be sure).
                kind: if is_pltrel || r_type == 7 || sym.stype == STT_FUNC {
                    // JUMP_SLOT (7) and PLT relocations are always calls.
                    ImportKind::Function
                } else if sym.stype == STT_OBJECT {
                    ImportKind::Data
                } else {
                    ImportKind::Unknown
                },
                ordinal: None,
                iat_address,
            });
        }
    }

    // De-duplicate by (name) keeping the first — a symbol can appear in both
    // .rela.dyn and .rela.plt.
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

/// Exports = defined (non-`SHN_UNDEF`) global/weak dynamic symbols with a name.
fn build_exports(dynsym: &[DynSymEntry]) -> Vec<Export> {
    let mut out = Vec::new();
    for s in dynsym {
        if s.shndx == SHN_UNDEF || s.name.is_empty() {
            continue;
        }
        if s.bind != STB_GLOBAL && s.bind != STB_WEAK {
            continue;
        }
        // Skip absolute/reserved indices that aren't real exports of code/data.
        out.push(Export { name: s.name.clone(), address: s.value, ordinal: None, forwarder: None });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out.dedup_by(|a, b| a.name == b.name && a.address == b.address);
    out
}

#[allow(clippy::too_many_arguments)]
fn build_mitigations(
    bytes: &[u8],
    eh: &Ehdr,
    phdrs: &[Phdr],
    shdrs: &[Shdr],
    dynamic: &DynamicInfo,
    symbols: &[Symbol],
    imports: &[Import],
) -> Mitigations {
    // NX: an explicit PT_GNU_STACK without the X bit means a non-exec stack.
    // (No PT_GNU_STACK at all is the old default of an *executable* stack.)
    let nx = phdrs
        .iter()
        .find(|p| p.p_type == PT_GNU_STACK)
        .map(|p| p.p_flags & PF_X == 0)
        .unwrap_or(false);

    // PIE: an ET_DYN object that is actually an executable (has an INTERP) or
    // is explicitly flagged DF_1_PIE.  A bare ET_DYN with no interpreter is a
    // shared library, not a PIE program.
    let has_interp = phdrs.iter().any(|p| p.p_type == PT_INTERP);
    let pie = (eh.e_type == ET_DYN && has_interp) || dynamic.pie_flag;

    // RELRO: PT_GNU_RELRO ⇒ at least Partial; + eager binding ⇒ Full.
    let has_relro = phdrs.iter().any(|p| p.p_type == PT_GNU_RELRO);
    let relro = if has_relro {
        if dynamic.bind_now {
            Some(Relro::Full)
        } else {
            Some(Relro::Partial)
        }
    } else {
        Some(Relro::None)
    };

    // Stack canary: the SSP runtime helper is referenced.
    let stack_canary = symbols.iter().any(|s| s.name == "__stack_chk_fail")
        || imports.iter().any(|i| i.name == "__stack_chk_fail");

    // CET: parse .note.gnu.property (or the PT_GNU_PROPERTY note) for the x86
    // feature-1 IBT/SHSTK bits.
    let cet = detect_cet(bytes, phdrs, shdrs);

    Mitigations {
        nx,
        pie,
        relro,
        bind_now: dynamic.bind_now,
        stack_canary,
        // On ELF, ASLR of the main image is exactly PIE; libraries are always
        // relocatable. There is no separate opt-in bit as on PE.
        aslr: pie,
        cfg: false, // ELF has no Control-Flow Guard equivalent in the PE sense.
        cet,
    }
}

// GNU property note constants.
const NT_GNU_PROPERTY_TYPE_0: u32 = 5;
const GNU_PROPERTY_X86_FEATURE_1_AND: u32 = 0xc000_0002;
const GNU_PROPERTY_X86_FEATURE_1_IBT: u32 = 1;
const GNU_PROPERTY_X86_FEATURE_1_SHSTK: u32 = 2;

/// Detect CET by parsing the `.note.gnu.property` note for the x86 feature bits.
fn detect_cet(bytes: &[u8], phdrs: &[Phdr], shdrs: &[Shdr]) -> bool {
    // Prefer the section (its offset/size are file-relative and exact).
    let note = shdrs
        .iter()
        .find(|s| s.name == ".note.gnu.property")
        .and_then(|s| slice_of(bytes, s.sh_offset, s.sh_size))
        .or_else(|| {
            phdrs
                .iter()
                .find(|p| p.p_type == PT_GNU_PROPERTY)
                .and_then(|p| slice_of(bytes, p.p_offset, p.p_filesz))
        });
    let Some(note) = note else { return false };
    parse_gnu_property_cet(note)
}

/// Parse an ELF note whose payload is `GNU` property array, returning true if
/// the IBT or SHSTK feature bit is set.
///
/// Note layout (`Elf64_Nhdr` + name + desc, each 4-byte aligned):
///   n_namesz(u32) n_descsz(u32) n_type(u32) name[namesz] desc[descsz]
/// The property array (desc) is a sequence of:
///   pr_type(u32) pr_datasz(u32) data[datasz]  — each entry 8-byte aligned.
fn parse_gnu_property_cet(note: &[u8]) -> bool {
    let namesz = u32_at(note, 0).unwrap_or(0) as usize; // n_namesz
    let descsz = u32_at(note, 4).unwrap_or(0) as usize; // n_descsz
    let ntype = u32_at(note, 8).unwrap_or(0); // n_type
    if ntype != NT_GNU_PROPERTY_TYPE_0 {
        return false;
    }
    // name follows the 12-byte header, padded to 4 bytes.
    let name_end = 12usize.saturating_add(align4(namesz));
    let desc_end = name_end.saturating_add(descsz);
    let Some(desc) = note.get(name_end..desc_end.min(note.len())) else {
        return false;
    };

    let mut pos = 0usize;
    while pos + 8 <= desc.len() {
        let pr_type = u32_at(desc, pos).unwrap_or(0);
        let pr_datasz = u32_at(desc, pos + 4).unwrap_or(0) as usize;
        let data_start = pos + 8;
        // Bail if the declared data would run past the descriptor slice.
        match data_start.checked_add(pr_datasz) {
            Some(e) if e <= desc.len() => {}
            _ => break,
        }
        if pr_type == GNU_PROPERTY_X86_FEATURE_1_AND && pr_datasz >= 4 {
            let bits = u32_at(desc, data_start).unwrap_or(0);
            if bits & (GNU_PROPERTY_X86_FEATURE_1_IBT | GNU_PROPERTY_X86_FEATURE_1_SHSTK) != 0 {
                return true;
            }
        }
        // Advance to the next property, 8-byte aligned (ELF64).
        let advance = match align8(pr_datasz).checked_add(8) {
            Some(a) if a > 0 => a,
            _ => break,
        };
        pos = match pos.checked_add(advance) {
            Some(p) => p,
            None => break,
        };
    }
    false
}

fn align4(n: usize) -> usize {
    n.wrapping_add(3) & !3
}
fn align8(n: usize) -> usize {
    n.wrapping_add(7) & !7
}
