//! The format-neutral data model.
//!
//! ELF and PE are very different on disk, but a *reverse engineer* asks the same
//! questions of both: where does execution start, what's mapped where and with
//! what permissions, what symbols/imports/exports are there, and which
//! exploit-mitigations are enabled.  These types are that shared vocabulary.
//! Both parsers lower their native structures into an [`Image`].

use core::fmt;
use serde::Serialize;

/// Which container format an image is in.
///
/// Serializes and `Display`s as the lowercase string `"elf"` / `"pe"` so the
/// web UI and JSON API can use it verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// 64-bit ELF (Executable and Linkable Format) — Unix-like systems.
    Elf,
    /// PE32/PE32+ (Portable Executable) — Windows.
    Pe,
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Format::Elf => "elf",
            Format::Pe => "pe",
        })
    }
}

/// Target machine architecture, as recorded in the file header.
///
/// We name only the architectures a student of this project is likely to meet;
/// anything else is preserved verbatim in [`Arch::Other`] so no information is
/// silently lost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Arch {
    /// x86-64 / AMD64 (`EM_X86_64` / `IMAGE_FILE_MACHINE_AMD64`).
    X86_64,
    /// 32-bit x86 (`EM_386` / `IMAGE_FILE_MACHINE_I386`).
    X86,
    /// 64-bit ARM (`EM_AARCH64` / `IMAGE_FILE_MACHINE_ARM64`).
    Aarch64,
    /// Any other machine value, kept as the raw header field.
    Other(u16),
}

impl fmt::Display for Arch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Arch::X86_64 => f.write_str("x86_64"),
            Arch::X86 => f.write_str("x86"),
            Arch::Aarch64 => f.write_str("aarch64"),
            Arch::Other(v) => write!(f, "other({v:#x})"),
        }
    }
}

/// Read/Write/Execute permission bits for a section or segment.
///
/// `Display`s as a classic `rwx` triad with dashes for missing bits, e.g. a
/// read-only executable region is `r-x` and a writable data region is `rw-`.
/// Note the field name is `alloc` (mapped into memory), not `read`: in ELF a
/// section is either allocated or it isn't, and an allocated section is always
/// readable — so `alloc` fills the `r` slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SectionFlags {
    /// The region occupies memory at run time (ELF `SHF_ALLOC` / any PE
    /// section is mapped).  Drives the `r` slot in `Display`.
    pub alloc: bool,
    /// Writable at run time.
    pub write: bool,
    /// Executable at run time.
    pub execute: bool,
}

impl fmt::Display for SectionFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let r = if self.alloc { 'r' } else { '-' };
        let w = if self.write { 'w' } else { '-' };
        let x = if self.execute { 'x' } else { '-' };
        write!(f, "{r}{w}{x}")
    }
}

/// One entry from the section table.
///
/// A *section* is a link-time view of the file (`.text`, `.data`, `.symtab`…).
/// Contrast with [`Segment`], the load-time view.  `file_size` is 0 for
/// `SHT_NOBITS` sections such as `.bss` that occupy memory but no file bytes.
#[derive(Debug, Clone, Serialize)]
pub struct Section {
    /// Section name (from `.shstrtab` on ELF, the 8-byte name field on PE).
    pub name: String,
    /// Virtual address the section loads at (0 if not allocated).
    pub address: u64,
    /// Size in memory.
    pub size: u64,
    /// Offset of the section's bytes within the file.
    pub file_offset: u64,
    /// Number of bytes present in the file (0 for `.bss`-like sections).
    pub file_size: u64,
    /// Run-time permissions.
    pub flags: SectionFlags,
}

/// One loadable region — the *loader's* view of the file.
///
/// On ELF these are the `PT_LOAD` (and friends) program headers.  PE has no
/// real program-header table, so the PE parser synthesises one segment per
/// section; this keeps the disassembler-facing API uniform.
#[derive(Debug, Clone, Serialize)]
pub struct Segment {
    /// Human-readable kind, e.g. `"LOAD"`, `"DYNAMIC"`, `"GNU_RELRO"`, or on PE
    /// the originating section name.
    pub kind: String,
    /// Virtual address.
    pub vaddr: u64,
    /// Bytes present in the file for this segment.
    pub filesz: u64,
    /// Bytes occupied in memory (>= `filesz`; the difference is zero-filled).
    pub memsz: u64,
    /// Run-time permissions.
    pub perms: SectionFlags,
    /// File offset of the segment's bytes.
    pub offset: u64,
}

/// What a symbol names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    /// A function / executable code (`STT_FUNC`).
    Func,
    /// A data object (`STT_OBJECT`).
    Object,
    /// Associated with a section (`STT_SECTION`).
    Section,
    /// A source file name (`STT_FILE`).
    File,
    /// Unspecified type (`STT_NOTYPE`).
    Notype,
    /// Any other `st_type` value.
    Other(u8),
}

/// A symbol's linkage/visibility class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolBinding {
    /// File-local (`STB_LOCAL`).
    Local,
    /// Globally visible (`STB_GLOBAL`).
    Global,
    /// Weak — overridable by a strong definition (`STB_WEAK`).
    Weak,
    /// Any other `st_bind` value.
    Other(u8),
}

/// A symbol-table entry (from `.symtab`/`.strtab` or `.dynsym`/`.dynstr`).
#[derive(Debug, Clone, Serialize)]
pub struct Symbol {
    /// Symbol name (may be empty for unnamed/section symbols).
    pub name: String,
    /// Value — usually the virtual address of the symbol.
    pub address: u64,
    /// Size in bytes (0 if unknown).
    pub size: u64,
    /// What the symbol names.
    pub kind: SymbolKind,
    /// Linkage class.
    pub binding: SymbolBinding,
    /// Name of the section the symbol is defined in, if resolvable.
    pub section: Option<String>,
}

/// How an imported name is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportKind {
    /// A called function.
    Function,
    /// An imported data object.
    Data,
    /// Could not be determined from the available metadata.
    Unknown,
}

/// A symbol this image needs from somewhere else.
///
/// On ELF, imports are reconstructed from dynamic relocations against undefined
/// symbols (`JUMP_SLOT`/`GLOB_DAT`).  On PE they come straight from the import
/// (and delay-import) directory.
#[derive(Debug, Clone, Serialize)]
pub struct Import {
    /// Imported symbol name (empty for pure-ordinal PE imports).
    pub name: String,
    /// Providing library, when known (a `DT_NEEDED` entry or a PE DLL name).
    pub library: Option<String>,
    /// Whether the import is code or data, when known.
    pub kind: ImportKind,
    /// Import ordinal (PE ordinal imports).
    pub ordinal: Option<u16>,
    /// Virtual address of the import's slot in the IAT/GOT, when known.
    pub iat_address: Option<u64>,
}

/// A symbol this image makes available to others.
#[derive(Debug, Clone, Serialize)]
pub struct Export {
    /// Exported name (empty for ordinal-only PE exports).
    pub name: String,
    /// Virtual address of the exported item.
    pub address: u64,
    /// Export ordinal (PE).
    pub ordinal: Option<u16>,
    /// If this export forwards to another DLL, the `"OTHERDLL.Symbol"` string.
    pub forwarder: Option<String>,
}

/// A relocation — a spot the loader/linker must patch.
#[derive(Debug, Clone, Serialize)]
pub struct Reloc {
    /// Address/offset being patched (`r_offset` on ELF, page RVA + offset on PE).
    pub offset: u64,
    /// Human-readable relocation type, e.g. `"R_X86_64_JUMP_SLOT"` or
    /// `"IMAGE_REL_BASED_DIR64"`.
    pub kind: String,
    /// Target symbol name, when the relocation references one.
    pub symbol: Option<String>,
    /// Addend (`RELA` relocations; 0 for `REL`/PE).
    pub addend: i64,
}

/// RELRO (RELocation Read-Only) hardening level for ELF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Relro {
    /// No `PT_GNU_RELRO` segment.
    None,
    /// `PT_GNU_RELRO` present: the GOT/relro region is made read-only after
    /// startup, but lazy PLT binding still leaves `.got.plt` writable.
    Partial,
    /// Partial RELRO **plus** eager binding (`BIND_NOW`): the whole GOT is
    /// resolved and locked read-only before `main`.
    Full,
}

/// Exploit-mitigation summary.  Fields that a given format cannot express are
/// left at their conservative default (`false` / `None`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct Mitigations {
    /// Non-executable data pages (ELF `PT_GNU_STACK` without X; PE `NX_COMPAT`).
    pub nx: bool,
    /// Position-independent executable (ELF `ET_DYN` PIE; PE `DYNAMIC_BASE`).
    pub pie: bool,
    /// RELRO level (ELF only; `None` on PE).
    pub relro: Option<Relro>,
    /// Eager symbol binding at load (`BIND_NOW`/`DF_1_NOW`).
    pub bind_now: bool,
    /// Stack-smashing protector present (`__stack_chk_fail` referenced; PE `/GS`
    /// security cookie in the load config).
    pub stack_canary: bool,
    /// Address-space layout randomization compatible.
    pub aslr: bool,
    /// Control-Flow Guard (PE `GUARD_CF`).
    pub cfg: bool,
    /// CET (shadow stack / IBT): ELF `.note.gnu.property` IBT/SHSTK bits.
    pub cet: bool,
}

/// A fully parsed executable image.  This is the crate's central output type
/// and is what the server serialises straight to JSON.
#[derive(Debug, Clone, Serialize)]
pub struct Image {
    /// Container format.
    pub format: Format,
    /// Target architecture.
    pub arch: Arch,
    /// Entry-point virtual address.
    pub entry: u64,
    /// Preferred load address of the image.
    pub image_base: u64,
    /// True if the image is position-independent (ELF `ET_DYN`; PE
    /// `DYNAMIC_BASE`).
    pub is_pie: bool,
    /// Section table.
    pub sections: Vec<Section>,
    /// Loadable segments (ELF `PT_LOAD` &c.; synthesised per-section on PE).
    pub segments: Vec<Segment>,
    /// Symbols.
    pub symbols: Vec<Symbol>,
    /// Imports.
    pub imports: Vec<Import>,
    /// Exports.
    pub exports: Vec<Export>,
    /// Relocations.
    pub relocations: Vec<Reloc>,
    /// Mitigation summary.
    pub mitigations: Mitigations,
}

impl Image {
    /// The raw file bytes of a named section, if it has any file contents.
    ///
    /// Returns `None` for an unknown name or a section with no file bytes
    /// (`.bss`), and — crucially — also returns `None` rather than panicking if
    /// the section's recorded offset/size fall outside `bytes` (which can happen
    /// on a corrupt file even after a successful parse).
    pub fn section_data<'a>(&self, bytes: &'a [u8], name: &str) -> Option<&'a [u8]> {
        let s = self.sections.iter().find(|s| s.name == name)?;
        if s.file_size == 0 {
            return None;
        }
        let off = usize::try_from(s.file_offset).ok()?;
        let len = usize::try_from(s.file_size).ok()?;
        let end = off.checked_add(len)?;
        bytes.get(off..end)
    }

    /// The executable code and the address it loads at — exactly what a
    /// disassembler needs to get started.
    ///
    /// Prefers a section literally named `.text`; failing that, the first
    /// allocated, executable section that has file bytes.
    pub fn text<'a>(&self, bytes: &'a [u8]) -> Option<(u64, &'a [u8])> {
        let pick =
            self.sections.iter().find(|s| s.name == ".text" && s.file_size > 0).or_else(|| {
                self.sections.iter().find(|s| s.flags.execute && s.flags.alloc && s.file_size > 0)
            })?;
        let off = usize::try_from(pick.file_offset).ok()?;
        let len = usize::try_from(pick.file_size).ok()?;
        let end = off.checked_add(len)?;
        let data = bytes.get(off..end)?;
        Some((pick.address, data))
    }

    /// The symbol whose address range contains `addr`, or the nearest symbol
    /// defined at or below `addr` if none has a covering size.
    ///
    /// This is the "what function am I in?" lookup a disassembler uses to label
    /// call targets.
    pub fn symbol_at(&self, addr: u64) -> Option<&Symbol> {
        let mut best: Option<&Symbol> = None;
        for s in &self.symbols {
            if s.address > addr {
                continue;
            }
            // A sized symbol only counts if the address is within its extent.
            if s.size > 0 {
                let end = s.address.saturating_add(s.size);
                if addr >= end {
                    continue;
                }
            }
            match best {
                Some(b) if b.address >= s.address => {}
                _ => best = Some(s),
            }
        }
        best
    }
}
