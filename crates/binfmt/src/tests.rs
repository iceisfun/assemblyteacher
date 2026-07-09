//! Unit tests over hand-built byte buffers.
//!
//! These deliberately avoid any external toolchain: every input is either
//! synthesised here or a hostile mutation of one. The recurring assertion is not
//! "we got the right answer" but "we did not panic" — a parser fed attacker
//! controlled bytes must always return `Ok`/`Err`, never abort.

use crate::*;

// --- little helpers to poke fields into a pre-sized buffer ---------------------

fn w16(buf: &mut [u8], off: usize, v: u16) {
    buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
}
fn w32(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
fn w64(buf: &mut [u8], off: usize, v: u64) {
    buf[off..off + 8].copy_from_slice(&v.to_le_bytes());
}
fn wstr(buf: &mut [u8], off: usize, s: &str) {
    buf[off..off + s.len()].copy_from_slice(s.as_bytes());
}

// -----------------------------------------------------------------------------
// A minimal but *valid* little-endian ELF64 PIE with a .text and .shstrtab.
// -----------------------------------------------------------------------------
fn build_min_elf() -> Vec<u8> {
    // Layout:
    //   0x00 ehdr(64)
    //   0x40 phdr(56)  PT_LOAD
    //   0x78 .text data (16 bytes)
    //   0x88 .shstrtab
    //   0xA0 section headers (3 * 64)
    let shstr: &[u8] = b"\0.text\0.shstrtab\0"; // .text@1, .shstrtab@7
    let text_off = 0x78usize;
    let shstr_off = 0x88usize;
    let shoff = 0xA0usize;
    let total = shoff + 3 * 64;
    let mut b = vec![0u8; total];

    // e_ident
    b[0..4].copy_from_slice(&[0x7f, b'E', b'L', b'F']);
    b[4] = 2; // ELFCLASS64
    b[5] = 1; // ELFDATA2LSB
    b[6] = 1; // EV_CURRENT
    w16(&mut b, 16, 3); // e_type = ET_DYN
    w16(&mut b, 18, 62); // e_machine = EM_X86_64
    w32(&mut b, 20, 1); // e_version
    w64(&mut b, 24, 0x1000); // e_entry
    w64(&mut b, 32, 0x40); // e_phoff
    w64(&mut b, 40, shoff as u64); // e_shoff
    w16(&mut b, 52, 64); // e_ehsize
    w16(&mut b, 54, 56); // e_phentsize
    w16(&mut b, 56, 1); // e_phnum
    w16(&mut b, 58, 64); // e_shentsize
    w16(&mut b, 60, 3); // e_shnum
    w16(&mut b, 62, 2); // e_shstrndx

    // program header: PT_LOAD R+X
    let p = 0x40;
    w32(&mut b, p, 1); // p_type = PT_LOAD
    w32(&mut b, p + 4, 5); // p_flags = R|X
    w64(&mut b, p + 8, 0); // p_offset
    w64(&mut b, p + 16, 0); // p_vaddr
    w64(&mut b, p + 32, 0x2000); // p_filesz
    w64(&mut b, p + 40, 0x2000); // p_memsz
    w64(&mut b, p + 48, 0x1000); // p_align

    // .text data (0x90 = NOP)
    for x in b.iter_mut().skip(text_off).take(16) {
        *x = 0x90;
    }
    // .shstrtab
    b[shstr_off..shstr_off + shstr.len()].copy_from_slice(shstr);

    // section headers. #0 is the null entry (already zero).
    // #1 .text
    let s1 = shoff + 64;
    w32(&mut b, s1, 1); // sh_name = ".text"
    w32(&mut b, s1 + 4, 1); // SHT_PROGBITS
    w64(&mut b, s1 + 8, 2 | 4); // SHF_ALLOC | SHF_EXECINSTR
    w64(&mut b, s1 + 16, 0x1000); // sh_addr
    w64(&mut b, s1 + 24, text_off as u64); // sh_offset
    w64(&mut b, s1 + 32, 16); // sh_size
                              // #2 .shstrtab
    let s2 = shoff + 128;
    w32(&mut b, s2, 7); // sh_name = ".shstrtab"
    w32(&mut b, s2 + 4, 3); // SHT_STRTAB
    w64(&mut b, s2 + 24, shstr_off as u64);
    w64(&mut b, s2 + 32, shstr.len() as u64);

    b
}

// -----------------------------------------------------------------------------
// A minimal PE32+ with one section carrying an import ("USER32.dll!MessageBoxA")
// and an export ("myexport"), plus mitigation flags set.
// -----------------------------------------------------------------------------
fn build_min_pe() -> Vec<u8> {
    let mut b = vec![0u8; 0x600];
    // DOS
    w16(&mut b, 0, 0x5a4d); // 'MZ'
    w32(&mut b, 0x3c, 0x80); // e_lfanew
    b[0x80..0x84].copy_from_slice(b"PE\0\0");

    // COFF @0x84
    w16(&mut b, 0x84, 0x8664); // machine AMD64
    w16(&mut b, 0x86, 1); // 1 section
    w16(&mut b, 0x94, 0xF0); // SizeOfOptionalHeader = 240
    w16(&mut b, 0x96, 0x22); // characteristics

    // Optional header @0x98
    let opt = 0x98;
    w16(&mut b, opt, 0x20b); // PE32+ magic
    w32(&mut b, opt + 16, 0x1000); // AddressOfEntryPoint
    w32(&mut b, opt + 20, 0x1000); // BaseOfCode
    w64(&mut b, opt + 24, 0x1_4000_0000); // ImageBase
    w32(&mut b, opt + 32, 0x1000); // SectionAlignment
    w32(&mut b, opt + 36, 0x200); // FileAlignment
    w32(&mut b, opt + 56, 0x2000); // SizeOfImage
    w32(&mut b, opt + 60, 0x200); // SizeOfHeaders
    w16(&mut b, opt + 68, 2); // Subsystem = GUI
    w16(&mut b, opt + 70, 0x4140); // DllCharacteristics: DYNAMIC_BASE|NX_COMPAT|GUARD_CF
    w32(&mut b, opt + 108, 16); // NumberOfRvaAndSizes

    // Data directories @ opt+112 (=0x108)
    let dd = opt + 112;
    w32(&mut b, dd, 0x1062); // export RVA
    w32(&mut b, dd + 4, 0x40); // export size
    w32(&mut b, dd + 8, 0x1000); // import RVA
    w32(&mut b, dd + 12, 40); // import size

    // Section table @0x188
    let sec = 0x188;
    wstr(&mut b, sec, ".text");
    w32(&mut b, sec + 8, 0x400); // VirtualSize
    w32(&mut b, sec + 12, 0x1000); // VirtualAddress
    w32(&mut b, sec + 16, 0x400); // SizeOfRawData
    w32(&mut b, sec + 20, 0x200); // PointerToRawData
    w32(&mut b, sec + 36, 0x6000_0020); // CODE|EXECUTE|READ

    // --- section content: file 0x200 == RVA 0x1000 ---
    let base = 0x200usize; // file offset of RVA 0x1000
    let rva = |r: u32| base + (r as usize - 0x1000);

    // Import descriptor @rva 0x1000
    let idesc = rva(0x1000);
    w32(&mut b, idesc, 0x1028); // OriginalFirstThunk (ILT)
    w32(&mut b, idesc + 12, 0x1056); // Name (DLL)
    w32(&mut b, idesc + 16, 0x1038); // FirstThunk (IAT)
                                     // second descriptor left zero (terminator) at idesc+20

    // ILT @0x1028
    w64(&mut b, rva(0x1028), 0x1048); // -> IMAGE_IMPORT_BY_NAME
    w64(&mut b, rva(0x1028) + 8, 0); // terminator
                                     // IAT @0x1038 (mirror before binding)
    w64(&mut b, rva(0x1038), 0x1048);
    w64(&mut b, rva(0x1038) + 8, 0);
    // IMAGE_IMPORT_BY_NAME @0x1048: hint(2) + name
    w16(&mut b, rva(0x1048), 0);
    wstr(&mut b, rva(0x1048) + 2, "MessageBoxA");
    // DLL name @0x1056
    wstr(&mut b, rva(0x1056), "USER32.dll");

    // Export directory @0x1062
    let ed = rva(0x1062);
    w32(&mut b, ed + 12, 0x109D); // Name -> "mydll.dll"
    w32(&mut b, ed + 16, 1); // ordinal base
    w32(&mut b, ed + 20, 1); // NumberOfFunctions
    w32(&mut b, ed + 24, 1); // NumberOfNames
    w32(&mut b, ed + 28, 0x108A); // AddressOfFunctions (EAT)
    w32(&mut b, ed + 32, 0x108E); // AddressOfNames
    w32(&mut b, ed + 36, 0x1092); // AddressOfNameOrdinals
                                  // EAT @0x108A: one function at RVA 0x1000 (outside export dir → real export)
    w32(&mut b, rva(0x108A), 0x1000);
    // AddressOfNames @0x108E -> name string at 0x1094
    w32(&mut b, rva(0x108E), 0x1094);
    // AddressOfNameOrdinals @0x1092 -> slot 0
    w16(&mut b, rva(0x1092), 0);
    // export name @0x1094
    wstr(&mut b, rva(0x1094), "myexport");
    // export dll name @0x109D
    wstr(&mut b, rva(0x109D), "mydll.dll");

    b
}

// --- detect / dispatch -------------------------------------------------------

#[test]
fn detect_basics() {
    assert_eq!(detect(&[]), None);
    assert_eq!(detect(b"not an executable"), None);
    assert_eq!(detect(&build_min_elf()), Some(Format::Elf));
    assert_eq!(detect(&build_min_pe()), Some(Format::Pe));
    assert!(matches!(parse(b"garbage"), Err(BinError::Unknown)));
}

#[test]
fn format_and_flags_display() {
    assert_eq!(Format::Elf.to_string(), "elf");
    assert_eq!(Format::Pe.to_string(), "pe");
    let rx = SectionFlags { alloc: true, write: false, execute: true };
    assert_eq!(rx.to_string(), "r-x");
    let rw = SectionFlags { alloc: true, write: true, execute: false };
    assert_eq!(rw.to_string(), "rw-");
}

// --- ELF happy path ----------------------------------------------------------

#[test]
fn elf_min_parses() {
    let bytes = build_min_elf();
    let img = parse(&bytes).expect("valid min ELF");
    assert_eq!(img.format, Format::Elf);
    assert_eq!(img.arch, Arch::X86_64);
    assert_eq!(img.entry, 0x1000);
    assert!(img.is_pie);
    // section list has .text and .shstrtab (null entry filtered out).
    assert!(img.sections.iter().any(|s| s.name == ".text"));
    let text = img.sections.iter().find(|s| s.name == ".text").unwrap();
    assert_eq!(text.address, 0x1000);
    assert_eq!(text.size, 16);
    assert!(text.flags.execute && text.flags.alloc);
    // one PT_LOAD segment.
    assert_eq!(img.segments.len(), 1);
    assert_eq!(img.segments[0].kind, "LOAD");
    // helpers
    let (addr, code) = img.text(&bytes).expect("has text");
    assert_eq!(addr, 0x1000);
    assert_eq!(code, &[0x90u8; 16]);
    assert_eq!(img.section_data(&bytes, ".text").unwrap().len(), 16);
    assert!(img.section_data(&bytes, ".nope").is_none());
}

// --- ELF error / robustness cases -------------------------------------------

#[test]
fn elf_truncated_header_errors() {
    let bytes = build_min_elf();
    for n in 0..64 {
        // Any prefix shorter than a full Ehdr must Err, never panic.
        let r = parse(&bytes[..n]);
        assert!(r.is_err(), "prefix {n} should error");
    }
}

#[test]
fn elf_bad_class_and_data() {
    let mut b = build_min_elf();
    b[4] = 1; // ELFCLASS32
    assert!(matches!(parse(&b), Err(BinError::UnsupportedElfClass(1))));
    let mut b = build_min_elf();
    b[5] = 2; // ELFDATA2MSB
    assert!(matches!(parse(&b), Err(BinError::UnsupportedElfData(2))));
    let mut b = build_min_elf();
    b[6] = 9; // bad version
    assert!(matches!(parse(&b), Err(BinError::UnsupportedElfVersion(9))));
}

#[test]
fn elf_absurd_counts_rejected() {
    let mut b = build_min_elf();
    w16(&mut b, 60, 0xffff); // e_shnum
    assert!(matches!(parse(&b), Err(BinError::Malformed(_))));
    let mut b = build_min_elf();
    w16(&mut b, 56, 0xffff); // e_phnum
    assert!(matches!(parse(&b), Err(BinError::Malformed(_))));
}

#[test]
fn elf_bad_shstrndx_is_survivable() {
    let mut b = build_min_elf();
    w16(&mut b, 62, 999); // e_shstrndx out of range
                          // Names become empty but the parse must still succeed without panic.
    let img = parse(&b).expect("bad shstrndx should not be fatal");
    assert!(img.sections.iter().all(|s| s.name.is_empty()));
}

#[test]
fn elf_offsets_past_eof_survivable() {
    let mut b = build_min_elf();
    w64(&mut b, 40, 0xffff_ffff_0000); // e_shoff way past EOF
                                       // No section headers can be read, but no panic and no error either.
    let img = parse(&b).expect("offsets past eof handled");
    assert!(img.sections.is_empty());
}

// --- PE happy path -----------------------------------------------------------

#[test]
fn pe_min_parses() {
    let bytes = build_min_pe();
    let img = parse(&bytes).expect("valid min PE");
    assert_eq!(img.format, Format::Pe);
    assert_eq!(img.arch, Arch::X86_64);
    assert_eq!(img.entry, 0x1_4000_1000);
    assert_eq!(img.image_base, 0x1_4000_0000);
    assert!(img.is_pie);
    // one synthesised segment from the single section.
    assert_eq!(img.segments.len(), 1);
    assert!(img.sections.iter().any(|s| s.name == ".text"));

    // import parsed
    let imp = img.imports.iter().find(|i| i.name == "MessageBoxA").expect("MessageBoxA imported");
    assert_eq!(imp.library.as_deref(), Some("USER32.dll"));
    assert!(imp.iat_address.is_some());

    // export parsed
    let exp = img.exports.iter().find(|e| e.name == "myexport").expect("myexport exported");
    assert_eq!(exp.address, 0x1_4000_1000);
    assert_eq!(exp.ordinal, Some(1));
    assert!(exp.forwarder.is_none());

    // mitigations from DllCharacteristics
    assert!(img.mitigations.nx);
    assert!(img.mitigations.aslr);
    assert!(img.mitigations.pie);
    assert!(img.mitigations.cfg);
    assert_eq!(img.mitigations.relro, None);
}

#[test]
fn pe_forwarder_export() {
    // Point the EAT slot *into* the export directory so it is read as a
    // forwarder string rather than an address.
    let mut b = build_min_pe();
    let base = 0x200usize;
    let rva = |r: u32| base + (r as usize - 0x1000);
    // Grow the export data-directory so the forwarder RVA lands inside it, and
    // place the string in free space past the export tables (not overlapping the
    // directory struct/arrays).
    w32(&mut b, 0x10c, 0xA0); // export dir size -> covers 0x1062..0x1102
    wstr(&mut b, rva(0x10B0), "NTDLL.RtlAllocateHeap");
    // EAT slot -> 0x10B0 (a forwarder RVA inside the export directory range)
    w32(&mut b, rva(0x108A), 0x10B0);
    let img = parse(&b).expect("parse with forwarder");
    let exp = img.exports.iter().find(|e| e.name == "myexport").unwrap();
    assert_eq!(exp.forwarder.as_deref(), Some("NTDLL.RtlAllocateHeap"));
    assert_eq!(exp.address, 0); // forwarders have no local address
}

#[test]
fn pe_ordinal_import() {
    // Set the ILT/IAT entry's high bit → an ordinal import (#7).
    let mut b = build_min_pe();
    let base = 0x200usize;
    let rva = |r: u32| base + (r as usize - 0x1000);
    w64(&mut b, rva(0x1028), 0x8000_0000_0000_0007);
    w64(&mut b, rva(0x1038), 0x8000_0000_0000_0007);
    let img = parse(&b).expect("parse ordinal import");
    let imp = img.imports.iter().find(|i| i.ordinal == Some(7)).expect("ordinal import present");
    assert!(imp.name.is_empty());
    assert_eq!(imp.library.as_deref(), Some("USER32.dll"));
}

// --- PE error cases ----------------------------------------------------------

#[test]
fn pe_bad_signatures() {
    let mut b = build_min_pe();
    w16(&mut b, 0, 0); // clobber 'MZ'
                       // Without MZ it is no longer detected as PE at all.
    assert!(matches!(parse(&b), Err(BinError::Unknown)));

    let mut b = build_min_pe();
    b[0x80] = b'X'; // clobber "PE\0\0"
    assert!(matches!(parse(&b), Err(BinError::Unknown)));

    // Detected as PE (sig intact) but unsupported optional magic.
    let mut b = build_min_pe();
    w16(&mut b, 0x98, 0x1234);
    assert!(matches!(parse(&b), Err(BinError::UnsupportedPeMagic(0x1234))));
}

#[test]
fn pe_truncated_prefixes() {
    let b = build_min_pe();
    for n in 0..b.len().min(0x200) {
        // Never panic on any truncation of the headers.
        let _ = parse(&b[..n]);
    }
}

// --- symbol_at ---------------------------------------------------------------

#[test]
fn symbol_at_lookup() {
    let img = Image {
        format: Format::Elf,
        arch: Arch::X86_64,
        entry: 0,
        image_base: 0,
        is_pie: false,
        sections: vec![],
        segments: vec![],
        symbols: vec![
            Symbol {
                name: "a".into(),
                address: 0x1000,
                size: 0x20,
                kind: SymbolKind::Func,
                binding: SymbolBinding::Global,
                section: None,
            },
            Symbol {
                name: "b".into(),
                address: 0x1040,
                size: 0,
                kind: SymbolKind::Func,
                binding: SymbolBinding::Global,
                section: None,
            },
        ],
        imports: vec![],
        exports: vec![],
        relocations: vec![],
        mitigations: Mitigations::default(),
    };
    assert_eq!(img.symbol_at(0x1008).map(|s| s.name.as_str()), Some("a"));
    assert!(img.symbol_at(0x1030).is_none()); // in a's gap, past a's size
    assert_eq!(img.symbol_at(0x1050).map(|s| s.name.as_str()), Some("b")); // nearest below
    assert!(img.symbol_at(0x0500).is_none()); // before everything
}

// -----------------------------------------------------------------------------
// Fuzz-ish robustness: deterministic mutations of real images must never panic.
// A fixed-seed LCG drives the mutations — no `rand` dependency, fully
// reproducible, and fast (a few thousand parses well under a second).
// -----------------------------------------------------------------------------

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0
    }
}

fn mutate_and_parse(seed_image: &[u8]) {
    let mut rng = Lcg(0x1234_5678_9abc_def0);
    for _ in 0..3000 {
        let mut m = seed_image.to_vec();
        if m.is_empty() {
            break;
        }
        match rng.next() % 4 {
            0 => {
                let i = (rng.next() as usize) % m.len();
                m[i] ^= (rng.next() & 0xff) as u8;
            }
            1 => {
                let n = (rng.next() as usize) % m.len();
                m.truncate(n);
            }
            2 => {
                let i = (rng.next() as usize) % m.len();
                for j in i..(i + 4).min(m.len()) {
                    m[j] = 0;
                }
            }
            _ => {
                let i = (rng.next() as usize) % m.len();
                m[i] = 0xff;
            }
        }
        // The whole contract in one line: this must return, not abort.
        let _ = parse(&m);
        let _ = detect(&m);
    }
}

#[test]
fn fuzz_elf_never_panics() {
    mutate_and_parse(&build_min_elf());
}

#[test]
fn fuzz_pe_never_panics() {
    mutate_and_parse(&build_min_pe());
}

#[test]
fn fuzz_self_exe_never_panics() {
    // Bonus: mutate this test binary itself (a real, large ELF) if we can read
    // it. Purely a smoke test; skipped silently if unreadable/unsupported.
    if let Ok(bytes) = std::fs::read("/proc/self/exe") {
        if detect(&bytes).is_some() {
            let mut rng = Lcg(0xdead_beef_0000_0001);
            // Kept modest: this binary is large, so a few hundred clone+parse
            // rounds already exercise every code path while staying fast.
            for _ in 0..150 {
                let mut m = bytes.clone();
                let i = (rng.next() as usize) % m.len();
                m[i] ^= (rng.next() & 0xff) as u8;
                if rng.next() % 5 == 0 {
                    let n = (rng.next() as usize) % m.len();
                    m.truncate(n);
                }
                let _ = parse(&m);
            }
        }
    }
}
