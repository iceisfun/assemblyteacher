//! Differential tests against the system assembler and disassembler.
//!
//! A decoder validated only against its own encoder proves nothing: a shared
//! misconception is invisible from the inside. These tests check our work
//! against `nasm` and `objdump`, which were written by other people from the
//! same manual.
//!
//! They skip themselves when the tools are absent, so `cargo test` still works
//! on a bare machine. The Docker build environment in `contrib/` installs both,
//! so CI always runs them.

use asm_core::{assemble, decode, format, Decoder};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn have(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn tmpdir() -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("asm-core-diff-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Tests run on several threads; each needs its own scratch files.
static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique(ext: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    tmpdir().join(format!("t{}.{}", n, ext))
}

/// Assemble one instruction with nasm and return its bytes.
///
/// `-O0` disables nasm's optimiser. That matters: by default nasm rewrites
/// `mov rax, 1` into the shorter `mov eax, 1`, exploiting the fact that a
/// 32-bit write zero-extends. That is a *different instruction* with the same
/// effect, and comparing against it would tell us nothing about whether our
/// encoder is correct. With `-O0` nasm emits literally what it was asked for,
/// and any disagreement is a real bug in one of us.
fn nasm_bytes(text: &str) -> Option<Vec<u8>> {
    let src = unique("asm");
    let out = unique("bin");
    std::fs::write(&src, format!("bits 64\n{}\n", text)).ok()?;
    let status = Command::new("nasm")
        .args(["-O0", "-f", "bin", "-o"])
        .arg(&out)
        .arg(&src)
        .stderr(std::process::Stdio::null())
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }
    let bytes = std::fs::read(&out).ok();
    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_file(&out);
    bytes
}

/// Instructions spanning every encoding path the decoder has: REX in all four
/// bits, SIB, RIP-relative, disp8/disp32, the opcode groups, the two-byte map,
/// immediates at every width, and the awkward corners (rsp base, rbp base,
/// r12 index, r13 base, high-byte registers).
const CORPUS: &[&str] = &[
    "mov rax, rbx",
    "mov eax, ebx",
    "mov ax, bx",
    "mov al, bl",
    "mov al, ah",
    "mov r8, r15",
    "mov r8b, r15b",
    "mov rax, qword [rsp+8]",
    "mov rax, qword [rbp]",
    "mov rax, qword [rbp+0x1000]",
    "mov rax, qword [r13]",
    "mov rax, qword [r12]",
    "mov rax, qword [rax+r12*8]",
    "mov rax, qword [rbx+rcx*4-16]",
    "mov qword [rax], rbx",
    "mov qword [rax], 1",
    "mov dword [rax], 1",
    "mov byte [rax], 1",
    "mov rax, 1",
    "mov rax, -1",
    "mov rax, 0xffffffff",
    "mov rax, 0x123456789abc",
    "mov eax, 0x12345678",
    "mov eax, 0xffffffff",
    "movzx eax, byte [rdi]",
    "movzx eax, bl",
    "movsx rax, word [rdi]",
    "movsxd rax, dword [rdi]",
    "lea rax, [rbx+rcx*4]",
    "add rax, rdx",
    "add rax, 8",
    "add rax, 0x12345678",
    "add eax, 0x12345678",
    "add byte [rax], 1",
    "sub rsp, 0x20",
    "and eax, 0xf",
    "or rax, rbx",
    "xor eax, eax",
    "xor rax, rax",
    "cmp rax, rdx",
    "cmp byte [rdi], 0",
    "test rax, rax",
    "test al, 1",
    "not rbx",
    "neg rbx",
    "inc dword [rax]",
    "dec rax",
    "mul rcx",
    "imul rcx",
    "imul rax, rcx",
    "imul rax, rcx, 3",
    "imul rax, rcx, 0x1000",
    "div rcx",
    "idiv rcx",
    "shl rax, 4",
    "shl rax, 1",
    "shr eax, cl",
    "sar rax, 63",
    "rol eax, 8",
    "push rbp",
    "push r12",
    "push 1",
    "push 0x1000",
    "pop rbp",
    "pop r15",
    "xchg rax, rcx",
    "xchg rbx, rcx",
    "ret",
    "ret 8",
    "leave",
    "nop",
    "hlt",
    "int3",
    "syscall",
    "ud2",
    "cdq",
    "cqo",
    "cdqe",
    "bswap eax",
    "sete al",
    "setne bl",
    "setg r8b",
    "cmove rax, rbx",
    "cmovb eax, ebx",
    "call rax",
    "call qword [rax]",
    "jmp rax",
    "jmp 0x0",
    "je 0x0",
    "call 0x0",
    "endbr64",
    "mov rax, qword fs:[0x28]",
    "lock add qword [rax], rbx",
];

/// Do two decoded instructions do the same thing?
///
/// Normally this is plain equality. `xchg` is the one exception: it is
/// symmetric, so `xchg rbx, rcx` and `xchg rcx, rbx` are the same instruction
/// with the ModRM `reg` and `rm` fields swapped. nasm picks one order, we pick
/// the other, and neither is wrong.
fn same_meaning(a: &asm_core::Insn, b: &asm_core::Insn) -> bool {
    if a.mnemonic != b.mnemonic {
        return false;
    }
    // A relative branch means "go here". Two encodings of different lengths
    // carry different displacements and still land on the same address, so the
    // target is what must match — not the displacement.
    if let (Some(x), Some(y)) = (a.branch_target(), b.branch_target()) {
        return x == y;
    }
    if a.mnemonic == asm_core::Mnemonic::Xchg && a.operands.len() == 2 {
        let mut x = a.operands.clone();
        let mut y = b.operands.clone();
        x.sort_by_key(|o| format!("{o}"));
        y.sort_by_key(|o| format!("{o}"));
        return x == y;
    }
    a.operands == b.operands
}

/// For every instruction in the corpus: nasm's bytes and our bytes must decode
/// to the same instruction.
///
/// We compare *decoded instructions*, not bytes, because more than one byte
/// sequence can mean the same thing and nasm is entitled to a different choice.
/// What must never differ is the meaning.
#[test]
fn our_encoding_agrees_with_nasm_on_meaning() {
    if !have("nasm") {
        eprintln!("skipping: nasm not installed");
        return;
    }

    let mut checked = 0;
    for text in CORPUS {
        let Some(theirs) = nasm_bytes(text) else {
            panic!("nasm rejected `{}`, which our corpus claims is valid", text);
        };
        let ours = assemble(text)
            .unwrap_or_else(|e| panic!("we failed to assemble `{}`: {e}", text))
            .bytes;

        let their_insn = decode(&theirs, 0)
            .unwrap_or_else(|e| panic!("we cannot decode nasm's bytes for `{}`: {e}", text));
        let our_insn = decode(&ours, 0)
            .unwrap_or_else(|e| panic!("we cannot decode our own bytes for `{}`: {e}", text));

        assert_eq!(
            their_insn.len(),
            theirs.len(),
            "`{}`: we think nasm's {:02x?} is {} bytes, nasm emitted {}",
            text,
            theirs,
            their_insn.len(),
            theirs.len()
        );

        assert!(
            same_meaning(&their_insn, &our_insn),
            "`{}`: nasm emitted {:02x?} (we read `{}`), we emitted {:02x?} (`{}`)",
            text,
            theirs,
            format::to_string(&their_insn),
            ours,
            format::to_string(&our_insn),
        );
        checked += 1;
    }
    assert_eq!(checked, CORPUS.len());
}

/// Our encoder should never be *longer* than nasm's unoptimised output. It is
/// very often shorter, because `-O0` nasm does not pick short forms at all.
/// A *longer* encoding would mean we missed one.
#[test]
fn our_encoding_is_never_longer_than_nasm() {
    if !have("nasm") {
        eprintln!("skipping: nasm not installed");
        return;
    }
    for text in CORPUS {
        let Some(theirs) = nasm_bytes(text) else { continue };
        let ours = assemble(text).unwrap().bytes;
        assert!(
            ours.len() <= theirs.len(),
            "`{}`: we emit {} bytes {:02x?}, nasm emits {} bytes {:02x?}",
            text,
            ours.len(),
            ours,
            theirs.len(),
            theirs
        );
    }
}

/// Decode a real compiled function and check our instruction boundaries against
/// objdump's.
///
/// Instruction *length* is the property that matters: if the decoder gets a
/// length wrong it desynchronises and every subsequent instruction is garbage.
/// Comparing boundaries catches that immediately, without depending on the two
/// disassemblers agreeing about syntax.
#[test]
fn instruction_boundaries_agree_with_objdump() {
    if !have("nasm") || !have("objdump") {
        eprintln!("skipping: nasm or objdump not installed");
        return;
    }

    let program = CORPUS.join("\n");
    let src = unique("corpus.asm");
    let bin = unique("corpus.bin");
    std::fs::write(&src, format!("bits 64\n{}\n", program)).unwrap();

    let ok = Command::new("nasm")
        .args(["-O0", "-f", "bin", "-o"])
        .arg(&bin)
        .arg(&src)
        .status()
        .unwrap()
        .success();
    assert!(ok, "nasm failed on the corpus");
    let code = std::fs::read(&bin).unwrap();

    // objdump on a flat binary: -b binary -m i386:x86-64.
    let out = Command::new("objdump")
        // --insn-width stops objdump wrapping a long instruction's bytes onto a
        // continuation line, which carries an address and would look to the
        // parser below like an extra instruction boundary.
        .args(["-D", "-b", "binary", "-m", "i386:x86-64", "-M", "intel", "--insn-width=16"])
        .arg(&bin)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);

    // Their boundaries: every line that starts with "  <hex>:".
    let mut theirs: Vec<usize> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some((addr, rest)) = line.split_once(':') {
            if !rest.trim_start().starts_with(|c: char| c.is_ascii_hexdigit()) {
                continue;
            }
            if let Ok(a) = usize::from_str_radix(addr.trim(), 16) {
                theirs.push(a);
            }
        }
    }
    assert!(theirs.len() > 50, "objdump produced too few instructions to be a real check");

    // Ours.
    let mut ours: Vec<usize> = Vec::new();
    let mut dec = Decoder::new(&code, 0);
    loop {
        let at = dec.offset();
        match dec.next() {
            Some(Ok(_)) => ours.push(at),
            Some(Err(e)) => panic!(
                "failed to decode at offset {:#x} (objdump reads it fine): {e}\nbytes: {:02x?}",
                at,
                &code[at..(at + 8).min(code.len())]
            ),
            None => break,
        }
    }

    assert_eq!(
        ours,
        theirs,
        "instruction boundaries diverged; first difference at index {:?}",
        ours.iter().zip(&theirs).position(|(a, b)| a != b)
    );
}

/// Every instruction we decode, we can re-assemble from its own printed text,
/// and the result means the same thing. This closes the loop:
/// bytes -> Insn -> text -> bytes -> Insn.
#[test]
fn decode_print_assemble_decode_is_a_fixed_point() {
    if !have("nasm") {
        eprintln!("skipping: nasm not installed");
        return;
    }
    for text in CORPUS {
        let Some(bytes) = nasm_bytes(text) else { continue };
        let first = decode(&bytes, 0).unwrap();
        let printed = format::to_string(&first);

        let reassembled = assemble(&printed)
            .unwrap_or_else(|e| {
                panic!("`{}` printed as `{}`, which we cannot assemble: {e}", text, printed)
            })
            .bytes;
        let second = decode(&reassembled, 0).unwrap();

        assert!(
            same_meaning(&first, &second),
            "`{}` -> `{}` -> {:02x?} changed meaning",
            text,
            printed,
            reassembled
        );
    }
}
