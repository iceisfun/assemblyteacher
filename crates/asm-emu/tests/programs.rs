//! End-to-end tests: assemble real programs with `asm_core::assemble` and run
//! them on the interpreter, asserting on their output, registers and exit code.
//!
//! These exercise the whole pipeline (assemble -> load -> decode -> execute)
//! the way a lesson does, so a regression in any layer shows up here.

use asm_core::asm::assemble_at;
use asm_emu::{Cpu, Stop};

const BASE: u64 = 0x40_0000;

/// Assemble at `BASE`, load, and run to completion (or the step budget).
fn run(src: &str, max: u64) -> Cpu {
    let asm = assemble_at(src, BASE).expect("program assembles");
    let mut cpu = Cpu::with_code(&asm.bytes, BASE);
    let outcome = cpu.run(max);
    // Stash the stop reason on nothing — callers that care re-run; most just
    // want the final state. Assert we did not blow the budget by accident.
    assert!(!matches!(outcome.stop, Stop::StepLimit), "program hit the step limit unexpectedly");
    cpu
}

#[test]
fn hello_world_via_write_and_exit() {
    // The canonical first program: write "Hello, world!\n" to stdout, exit 0.
    let src = r#"
        _start:
            mov rax, 1          ; sys_write
            mov rdi, 1          ; fd = stdout
            mov rsi, msg        ; buf
            mov rdx, 14         ; len
            syscall

            mov rax, 60         ; sys_exit
            mov rdi, 0          ; status = 0
            syscall
        msg:
            db "Hello, world!", 10
    "#;
    let asm = assemble_at(src, BASE).unwrap();
    let mut cpu = Cpu::with_code(&asm.bytes, BASE);
    let outcome = cpu.run(100);
    assert_eq!(outcome.stop, Stop::Exited(0));
    assert_eq!(cpu.stdout(), b"Hello, world!\n");
}

#[test]
fn write_result_is_the_byte_count() {
    // sys_write returns the number of bytes written, in rax.
    let src = r#"
            mov rax, 1
            mov rdi, 1
            mov rsi, msg
            mov rdx, 3
            syscall
            hlt
        msg:
            db "abcdef"
    "#;
    let cpu = run(src, 100);
    assert_eq!(cpu.regs.read_full(0), 3, "rax holds the count written");
    assert_eq!(cpu.stdout(), b"abc");
}

#[test]
fn iterative_sum_1_to_10_is_55() {
    // for (i = 1; i <= 10; i++) sum += i;
    let src = r#"
            xor eax, eax        ; sum = 0
            mov ecx, 1          ; i = 1
        top:
            cmp ecx, 10
            jg  done
            add eax, ecx
            inc ecx
            jmp top
        done:
            hlt
    "#;
    let cpu = run(src, 1000);
    assert_eq!(cpu.regs.read_full(0), 55);
}

#[test]
fn fibonacci_tenth_term() {
    // Compute fib(10) = 55 with a two-register rolling sum.
    let src = r#"
            mov eax, 0          ; a
            mov ebx, 1          ; b
            mov ecx, 10         ; count
        loop:
            test ecx, ecx
            jz done
            mov edx, eax
            add edx, ebx        ; next = a + b
            mov eax, ebx        ; a = b
            mov ebx, edx        ; b = next
            dec ecx
            jmp loop
        done:
            hlt
    "#;
    let cpu = run(src, 1000);
    assert_eq!(cpu.regs.read_full(0), 55, "fib(10) = 55");
}

#[test]
fn string_length_then_write_it_back() {
    // strlen(msg) into rdx, then write(1, msg, rdx). Proves computed lengths
    // flow into a syscall correctly.
    let src = r#"
            mov rsi, msg
            xor rdx, rdx
        scan:
            mov al, [rsi + rdx]
            test al, al
            jz  found
            inc rdx
            jmp scan
        found:
            mov rax, 1
            mov rdi, 1
            ; rsi already points at msg; rdx already holds the length
            syscall
            mov rax, 60
            xor rdi, rdi
            syscall
        msg:
            db "counted", 0
    "#;
    let asm = assemble_at(src, BASE).unwrap();
    let mut cpu = Cpu::with_code(&asm.bytes, BASE);
    let outcome = cpu.run(1000);
    assert_eq!(outcome.stop, Stop::Exited(0));
    assert_eq!(cpu.stdout(), b"counted");
}

#[test]
fn stack_frame_prologue_and_leave() {
    // A textbook prologue/epilogue: it must return rsp and rbp to where they
    // started. We store a value through the frame pointer and read it back.
    let src = r#"
            push rbp
            mov rbp, rsp
            sub rsp, 16
            mov rcx, 0x1122334455667788   ; movabs: the only 64-bit immediate
            mov [rbp - 8], rcx
            mov rax, [rbp - 8]
            leave
            hlt
    "#;
    let asm = assemble_at(src, BASE).unwrap();
    let mut cpu = Cpu::with_code(&asm.bytes, BASE);
    let sp0 = cpu.regs.read(asm_core::Reg::RSP);
    let outcome = cpu.run(100);
    assert_eq!(outcome.stop, Stop::Halted);
    assert_eq!(cpu.regs.read_full(0), 0x1122334455667788);
    assert_eq!(cpu.regs.read(asm_core::Reg::RSP), sp0, "leave unwound the frame");
}

#[test]
fn effects_trace_is_serialisable() {
    // The server serialises the trace to JSON; make sure the derives hold up on
    // a real run with register writes, a memory write, and a syscall.
    let src = r#"
            mov rax, 1
            mov rdi, 1
            mov rsi, msg
            mov rdx, 2
            push rax
            syscall
            mov rax, 60
            xor rdi, rdi
            syscall
        msg:
            db "hi"
    "#;
    let asm = assemble_at(src, BASE).unwrap();
    let mut cpu = Cpu::with_code(&asm.bytes, BASE);
    let outcome = cpu.run(100);
    let json = serde_json::to_string(&outcome).expect("Run serialises");
    assert!(json.contains("\"stop\""));
    assert!(json.contains("Exited"));
    // Spot-check that a syscall event made it into the trace.
    assert!(json.contains("write"));
}
