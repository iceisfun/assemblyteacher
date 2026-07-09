//! Integration tests against binaries built by the real toolchain at test time.
//!
//! Everything here is *comparative*: we parse a binary with `binfmt` and then
//! check our answers against the canonical tools (`readelf`) reading the same
//! file. If `gcc`/`readelf` are missing the tests skip gracefully with an
//! `eprintln!` rather than failing — CI without a C toolchain still goes green.

use std::path::{Path, PathBuf};
use std::process::Command;

/// True if a program exists and runs `--version` (or similar) successfully.
fn have(tool: &str) -> bool {
    Command::new(tool).arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// A throwaway working directory unique to this process.
fn workdir() -> PathBuf {
    let mut d = std::env::temp_dir();
    d.push(format!("binfmt_it_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&d);
    d
}

fn write_source(dir: &Path) -> PathBuf {
    let src = dir.join("hello.c");
    std::fs::write(
        &src,
        r#"#include <stdio.h>
int main(void) {
    printf("hello %d\n", 42);
    puts("world");
    return 0;
}
"#,
    )
    .expect("write source");
    src
}

fn gcc_build(src: &Path, out: &Path, extra: &[&str]) -> bool {
    let status = Command::new("gcc").arg(src).arg("-o").arg(out).args(extra).status();
    matches!(status, Ok(s) if s.success()) && out.exists()
}

fn readelf(args: &[&str], file: &Path) -> String {
    let out = Command::new("readelf").args(args).arg(file).output().expect("run readelf");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Pull the entry point out of `readelf -h`.
fn readelf_entry(file: &Path) -> Option<u64> {
    let text = readelf(&["-h"], file);
    for line in text.lines() {
        if line.contains("Entry point address:") {
            let hex = line.split("0x").nth(1)?.trim();
            return u64::from_str_radix(hex, 16).ok();
        }
    }
    None
}

/// Pull (address, size) of a named section from `readelf -SW`.
fn readelf_section(file: &Path, name: &str) -> Option<(u64, u64)> {
    let text = readelf(&["-SW"], file);
    for line in text.lines() {
        // Strip the "[ N]" index prefix, then tokenise.
        let rest = match line.split_once(']') {
            Some((_, r)) => r.trim(),
            None => continue,
        };
        let toks: Vec<&str> = rest.split_whitespace().collect();
        // toks: name, type, address, off, size, ...
        if toks.len() >= 5 && toks[0] == name {
            let addr = u64::from_str_radix(toks[2], 16).ok()?;
            let size = u64::from_str_radix(toks[4], 16).ok()?;
            return Some((addr, size));
        }
    }
    None
}

#[test]
fn elf_static_no_pie() {
    if !have("gcc") || !have("readelf") {
        eprintln!("SKIP elf_static_no_pie: gcc/readelf not available");
        return;
    }
    let dir = workdir();
    let src = write_source(&dir);
    let bin = dir.join("hello_static");
    if !gcc_build(&src, &bin, &["-no-pie", "-static", "-O0"]) {
        eprintln!("SKIP elf_static_no_pie: static build failed (no static libc?)");
        return;
    }

    let bytes = std::fs::read(&bin).unwrap();
    assert_eq!(binfmt::detect(&bytes), Some(binfmt::Format::Elf));
    let img = binfmt::parse(&bytes).expect("parse static ELF");

    // Entry point agrees with readelf -h.
    let entry = readelf_entry(&bin).expect("readelf entry");
    assert_eq!(img.entry, entry, "entry point mismatch");

    // -no-pie ⇒ ET_EXEC ⇒ not PIE.
    assert!(!img.is_pie, "static -no-pie should not be PIE");

    // .text address/size agree with readelf -S.
    let (addr, size) = readelf_section(&bin, ".text").expect("readelf .text");
    let my_text = img.sections.iter().find(|s| s.name == ".text").expect("our .text");
    assert_eq!(my_text.address, addr, ".text address mismatch");
    assert_eq!(my_text.size, size, ".text size mismatch");

    // _start and main are present in the static symbol table.
    assert!(img.symbols.iter().any(|s| s.name == "_start"), "_start missing");
    assert!(img.symbols.iter().any(|s| s.name == "main"), "main missing");

    // text() hands back the executable bytes at the right address.
    let (taddr, code) = img.text(&bytes).expect("text()");
    assert_eq!(taddr, addr);
    assert!(!code.is_empty());
}

#[test]
fn elf_dynamic_pie() {
    if !have("gcc") || !have("readelf") {
        eprintln!("SKIP elf_dynamic_pie: gcc/readelf not available");
        return;
    }
    let dir = workdir();
    let src = write_source(&dir);
    let bin = dir.join("hello_pie");
    if !gcc_build(&src, &bin, &["-fPIE", "-pie", "-O0"]) {
        eprintln!("SKIP elf_dynamic_pie: PIE build failed");
        return;
    }

    let bytes = std::fs::read(&bin).unwrap();
    let img = binfmt::parse(&bytes).expect("parse PIE ELF");

    // Entry + .text agree with readelf.
    assert_eq!(img.entry, readelf_entry(&bin).unwrap());
    let (addr, size) = readelf_section(&bin, ".text").unwrap();
    let t = img.sections.iter().find(|s| s.name == ".text").unwrap();
    assert_eq!((t.address, t.size), (addr, size));

    // Dynamic executable ⇒ PIE.
    assert!(img.is_pie, "gcc default should be PIE");

    // main is defined; the libc calls appear as imports.
    assert!(img.symbols.iter().any(|s| s.name == "main"));
    let import_names: Vec<&str> = img.imports.iter().map(|i| i.name.as_str()).collect();
    assert!(
        import_names.contains(&"puts") || import_names.contains(&"printf"),
        "expected puts/printf in imports, got {import_names:?}"
    );

    // Mitigations vs `readelf -lWd`.
    let prog = readelf(&["-lW"], &bin);
    let dynh = readelf(&["-dW"], &bin);

    // NX: GNU_STACK must not carry the 'E' (execute) flag → our nx == true.
    if let Some(stack_line) = prog.lines().find(|l| l.contains("GNU_STACK")) {
        let exec_stack = stack_line.contains("RWE");
        assert_eq!(
            img.mitigations.nx, !exec_stack,
            "nx disagrees with GNU_STACK flags: {stack_line}"
        );
    }

    // RELRO: presence of GNU_RELRO ⇒ at least Partial.
    if prog.contains("GNU_RELRO") {
        assert!(
            matches!(
                img.mitigations.relro,
                Some(binfmt::Relro::Partial) | Some(binfmt::Relro::Full)
            ),
            "expected RELRO but got {:?}",
            img.mitigations.relro
        );
    }

    // BIND_NOW: if readelf reports it in the dynamic flags, we must too.
    let readelf_bind_now = dynh.contains("BIND_NOW") || dynh.contains("NOW");
    if readelf_bind_now {
        assert!(img.mitigations.bind_now, "missed BIND_NOW");
        assert!(matches!(img.mitigations.relro, Some(binfmt::Relro::Full)));
    }
}

#[test]
fn elf_shared_object_exports() {
    if !have("gcc") || !have("readelf") {
        eprintln!("SKIP elf_shared_object_exports: gcc/readelf not available");
        return;
    }
    let dir = workdir();
    let src = dir.join("lib.c");
    std::fs::write(&src, "int add(int a, int b){return a+b;} int mul(int a,int b){return a*b;}")
        .unwrap();
    let so = dir.join("libmath.so");
    if !gcc_build(&src, &so, &["-shared", "-fPIC", "-O0"]) {
        eprintln!("SKIP elf_shared_object_exports: shared build failed");
        return;
    }
    let bytes = std::fs::read(&so).unwrap();
    let img = binfmt::parse(&bytes).unwrap();
    let exports: Vec<&str> = img.exports.iter().map(|e| e.name.as_str()).collect();
    assert!(exports.contains(&"add"), "add export missing: {exports:?}");
    assert!(exports.contains(&"mul"), "mul export missing: {exports:?}");
}

#[test]
fn smoke_system_binary() {
    // Parse a large, real system binary to prove we don't panic and do find a
    // .text. Prefer /bin/ls; fall back to this test binary.
    let candidates = ["/bin/ls", "/usr/bin/ls", "/proc/self/exe"];
    let Some(path) = candidates.iter().find(|p| Path::new(p).exists()) else {
        eprintln!("SKIP smoke_system_binary: no candidate binary");
        return;
    };
    let Ok(bytes) = std::fs::read(path) else {
        eprintln!("SKIP smoke_system_binary: unreadable {path}");
        return;
    };
    if binfmt::detect(&bytes).is_none() {
        eprintln!("SKIP smoke_system_binary: {path} not ELF/PE");
        return;
    }
    let img = binfmt::parse(&bytes).expect("parse system binary");
    assert!(img.text(&bytes).is_some(), "system binary {path} should have .text");
}

#[test]
fn pe_from_windows_target() {
    // If the x86_64-pc-windows-gnu target is installed, build a tiny PE and
    // check we parse its imports/exports. Skip gracefully otherwise.
    if !have("rustc") {
        eprintln!("SKIP pe_from_windows_target: no rustc");
        return;
    }
    let targets = Command::new("rustc").args(["--print", "target-list"]).output();
    let has_target = matches!(targets, Ok(o) if String::from_utf8_lossy(&o.stdout)
        .lines()
        .any(|l| l == "x86_64-pc-windows-gnu"));
    if !has_target {
        eprintln!("SKIP pe_from_windows_target: target not listed");
        return;
    }

    let dir = workdir();
    let src = dir.join("win.rs");
    std::fs::write(&src, "fn main(){ let _ = std::hint::black_box(0u8); }").unwrap();
    let exe = dir.join("win.exe");
    let built = Command::new("rustc")
        .args(["--target", "x86_64-pc-windows-gnu", "-O"])
        .arg(&src)
        .arg("-o")
        .arg(&exe)
        .status();
    if !matches!(built, Ok(s) if s.success()) || !exe.exists() {
        eprintln!("SKIP pe_from_windows_target: windows-gnu std/linker not installed");
        return;
    }

    let bytes = std::fs::read(&exe).unwrap();
    assert_eq!(binfmt::detect(&bytes), Some(binfmt::Format::Pe));
    let img = binfmt::parse(&bytes).expect("parse PE");
    assert_eq!(img.arch, binfmt::Arch::X86_64);
    assert!(img.entry != 0);
    // A real Windows console app pulls in KERNEL32 &c.
    assert!(!img.imports.is_empty(), "expected at least one PE import");
    assert!(
        img.imports.iter().any(|i| i
            .library
            .as_deref()
            .map(|l| l.to_ascii_uppercase().contains("KERNEL32"))
            .unwrap_or(false)),
        "expected a KERNEL32 import"
    );
    // Sections should include .text and it must be executable.
    assert!(img.sections.iter().any(|s| s.name == ".text" && s.flags.execute));
}
