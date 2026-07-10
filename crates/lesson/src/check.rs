//! Grading submissions.
//!
//! The guiding rule: **grade the effect, not the text.** A student who writes
//! `xor eax, eax` where the reference solution says `mov eax, 0` has not made a
//! mistake, and an exercise that fails them is teaching the wrong thing. So
//! `assemble` exercises compare machine code, `emulate` exercises compare final
//! machine state, and only `quiz` exercises compare an answer literally.

use crate::model::{Exercise, ExerciseKind, Verdict};
use asm_core::{assemble, format, Decoder};
use asm_emu::{Cpu, Stop};

/// The address a graded program is loaded at. Arbitrary, but fixed, so that a
/// submission's RIP-relative operands and absolute label references behave the
/// same for every student.
const LOAD_BASE: u64 = 0x1000;

/// Decode a hex string, tolerating whitespace and `0x` prefixes.
pub fn from_hex(s: &str) -> Option<Vec<u8>> {
    let cleaned: String = s
        .replace("0x", "")
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',' && *c != '_')
        .collect();
    if cleaned.len() % 2 != 0 {
        return None;
    }
    (0..cleaned.len()).step_by(2).map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16).ok()).collect()
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Disassemble for an error message. Best-effort: never fails.
fn describe_bytes(bytes: &[u8]) -> String {
    let mut out = Vec::new();
    for insn in Decoder::new(bytes, 0) {
        match insn {
            Ok(i) => out.push(format::to_string(&i)),
            Err(_) => {
                out.push("<undecodable>".to_string());
                break;
            }
        }
    }
    out.join("; ")
}

/// Grade a submission against an exercise.
///
/// `answer` is always a string: the choice index for a quiz, source text
/// otherwise. That keeps the HTTP surface trivial.
pub fn check(exercise: &Exercise, answer: &str) -> Verdict {
    let hints = &exercise.hints;
    match &exercise.kind {
        ExerciseKind::Quiz { answer: correct, explanation, choices } => {
            let Ok(choice) = answer.trim().parse::<usize>() else {
                return Verdict::wrong("That is not one of the choices.", hints);
            };
            if choice >= choices.len() {
                return Verdict::wrong("That is not one of the choices.", hints);
            }
            if choice == *correct {
                Verdict::correct(if explanation.is_empty() { "Correct." } else { explanation })
            } else {
                Verdict::wrong(
                    if explanation.is_empty() {
                        "Not quite. Try again.".to_string()
                    } else {
                        format!("Not quite. {}", explanation)
                    },
                    hints,
                )
            }
        }

        ExerciseKind::Assemble { expect_hex, .. } => {
            let Some(expected) = from_hex(expect_hex) else {
                return Verdict::wrong("This exercise is misconfigured: bad expect_hex.", &[]);
            };
            match assemble(answer) {
                Err(e) => Verdict::wrong(format!("That does not assemble: {}", e), hints),
                Ok(out) if out.bytes == expected => Verdict::correct(format!(
                    "Correct — {} assembles to {}.",
                    describe_bytes(&out.bytes),
                    to_hex(&out.bytes)
                )),
                Ok(out) => Verdict::wrong(
                    format!(
                        "That assembles to {} ({}), but the exercise wants {} ({}).",
                        to_hex(&out.bytes),
                        describe_bytes(&out.bytes),
                        to_hex(&expected),
                        describe_bytes(&expected),
                    ),
                    hints,
                ),
            }
        }

        ExerciseKind::Disassemble { hex, expect_text, .. } => {
            let Some(expected) = from_hex(hex) else {
                return Verdict::wrong("This exercise is misconfigured: bad hex.", &[]);
            };
            // Prefer to grade by meaning: assemble what the student wrote and
            // compare bytes. That accepts `je 0x2` and `jz 0x2` alike. Only if
            // their text will not assemble do we fall back to comparing text,
            // which is the case for answers like `nop` written as prose.
            if let Ok(out) = assemble(answer) {
                if out.bytes == expected {
                    return Verdict::correct("Correct.");
                }
                return Verdict::wrong(
                    format!(
                        "`{}` is a real instruction, but it encodes to {} — these bytes are {}.",
                        answer.trim(),
                        to_hex(&out.bytes),
                        to_hex(&expected)
                    ),
                    hints,
                );
            }
            if normalize(answer) == normalize(expect_text) {
                Verdict::correct("Correct.")
            } else {
                Verdict::wrong("Not quite — read the bytes field by field.", hints)
            }
        }

        ExerciseKind::Emulate { expect_registers, expect_stdout, max_steps, .. } => {
            // Assemble *at* the address the program will be loaded at, so that
            // absolute label references (`mov rsi, msg`) resolve to where the
            // bytes actually land. An `org` directive in the submission wins,
            // and `program.origin` follows it.
            let program = match asm_core::asm::assemble_at(answer, LOAD_BASE) {
                Ok(p) => p,
                Err(e) => return Verdict::wrong(format!("That does not assemble: {}", e), hints),
            };

            let mut cpu = Cpu::with_code(&program.bytes, program.origin);
            // Grading only inspects the final state, so record no trace: on a
            // public server this is an attacker-supplied program under an
            // author-chosen step limit, and a full trace would allocate memory
            // proportional to that limit for data we never read.
            let run = cpu.run_bounded(*max_steps, 0);

            match run.stop {
                Stop::Halted | Stop::Exited(_) => {}
                Stop::StepLimit => {
                    return Verdict::wrong(
                        format!(
                            "Your program was still running after {} instructions. \
                             Is a loop missing its exit condition?",
                            max_steps
                        ),
                        hints,
                    )
                }
                Stop::Fault(f) => {
                    return Verdict::wrong(format!("Your program faulted: {}", f), hints)
                }
                Stop::Breakpoint(a) => {
                    return Verdict::wrong(format!("Stopped at a breakpoint at {:#x}.", a), hints)
                }
            }

            for (name, want) in expect_registers {
                let got = cpu
                    .regs
                    .iter_named()
                    .find(|(n, _)| n.eq_ignore_ascii_case(name))
                    .map(|(_, v)| v);
                match got {
                    None => {
                        return Verdict::wrong(
                            format!("This exercise names an unknown register `{}`.", name),
                            &[],
                        )
                    }
                    Some(v) if v != *want => {
                        return Verdict::wrong(
                            format!(
                            "After {} instructions, {} is {:#x} ({}), but should be {:#x} ({}).",
                            run.steps, name, v, v, want, want
                        ),
                            hints,
                        )
                    }
                    Some(_) => {}
                }
            }

            if let Some(want) = expect_stdout {
                let got = String::from_utf8_lossy(cpu.stdout()).into_owned();
                if &got != want {
                    return Verdict::wrong(
                        format!("Your program printed {:?}, but should print {:?}.", got, want),
                        hints,
                    );
                }
            }

            Verdict::correct(format!("Correct — {} instructions executed.", run.steps))
        }
    }
}

/// Collapse whitespace and case so that `MOV RAX,1` and `mov rax, 1` compare
/// equal. Used only on the text fallback path.
fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = true;
    for c in s.trim().chars() {
        if c.is_whitespace() || c == ',' {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.extend(c.to_lowercase());
            last_space = false;
        }
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Exercise;

    fn ex(kind: ExerciseKind) -> Exercise {
        Exercise { id: "e".into(), prompt: "p".into(), hints: vec!["hint".into()], kind }
    }

    #[test]
    fn hex_round_trips_and_tolerates_formatting() {
        assert_eq!(from_hex("48 89 e5").unwrap(), [0x48, 0x89, 0xe5]);
        assert_eq!(from_hex("4889e5").unwrap(), [0x48, 0x89, 0xe5]);
        assert_eq!(from_hex("48,89,e5").unwrap(), [0x48, 0x89, 0xe5]);
        assert_eq!(to_hex(&[0x48, 0x89, 0xe5]), "4889e5");
        assert!(from_hex("abc").is_none(), "odd length must be rejected");
        assert!(from_hex("zz").is_none());
    }

    #[test]
    fn quiz_grades_the_index() {
        let e = ex(ExerciseKind::Quiz {
            choices: vec!["a".into(), "b".into()],
            answer: 1,
            explanation: "because".into(),
        });
        assert!(check(&e, "1").correct);
        assert!(!check(&e, "0").correct);
        assert!(!check(&e, "7").correct, "out-of-range choice");
        assert!(!check(&e, "banana").correct);
    }

    #[test]
    fn assemble_grades_bytes_so_any_correct_encoding_passes() {
        // `xor eax, eax` is 31 c0.
        let e = ex(ExerciseKind::Assemble {
            expect_hex: "31c0".into(),
            starter: String::new(),
            solution: "xor eax, eax".into(),
        });
        assert!(check(&e, "xor eax, eax").correct);
        // Same bytes, different spelling of the source.
        assert!(check(&e, "XOR EAX,EAX").correct);
        // Same effect, different bytes: correctly rejected, since the exercise
        // asked for these bytes.
        let v = check(&e, "mov eax, 0");
        assert!(!v.correct);
        assert!(
            v.message.contains("b800000000"),
            "message shows what they produced: {}",
            v.message
        );
    }

    #[test]
    fn assemble_reports_the_assembler_error() {
        let e = ex(ExerciseKind::Assemble {
            expect_hex: "c3".into(),
            starter: String::new(),
            solution: "ret".into(),
        });
        let v = check(&e, "frobnicate");
        assert!(!v.correct);
        assert!(v.message.contains("does not assemble"));
        assert_eq!(v.hints, vec!["hint".to_string()]);
    }

    #[test]
    fn disassemble_accepts_any_source_that_encodes_to_the_bytes() {
        let e = ex(ExerciseKind::Disassemble {
            hex: "7400".into(),
            expect_text: "je 0x2".into(),
            solution: String::new(),
        });
        assert!(check(&e, "je 0x2").correct);
        // `jz` is an alias for `je`, and must be accepted.
        assert!(check(&e, "jz 0x2").correct);
        assert!(!check(&e, "jne 0x2").correct);
    }

    #[test]
    fn emulate_grades_the_final_machine_state() {
        let e = ex(ExerciseKind::Emulate {
            starter: String::new(),
            expect_registers: [("rax".to_string(), 120u64)].into_iter().collect(),
            expect_stdout: None,
            max_steps: 1000,
            solution: "mov rax, 120\nhlt".into(),
        });
        assert!(check(&e, "mov rax, 120\nhlt").correct);
        // A different route to the same state also passes.
        assert!(check(&e, "mov rax, 100\nadd rax, 20\nhlt").correct);
        let v = check(&e, "mov rax, 7\nhlt");
        assert!(!v.correct);
        assert!(v.message.contains("rax"), "{}", v.message);
    }

    #[test]
    fn emulate_catches_a_program_that_never_terminates() {
        let e = ex(ExerciseKind::Emulate {
            starter: String::new(),
            expect_registers: Default::default(),
            expect_stdout: None,
            max_steps: 50,
            solution: "hlt".into(),
        });
        let v = check(&e, "here:\njmp here");
        assert!(!v.correct);
        assert!(v.message.contains("still running"), "{}", v.message);
    }

    #[test]
    fn emulate_reports_a_fault_rather_than_hanging_or_panicking() {
        let e = ex(ExerciseKind::Emulate {
            starter: String::new(),
            expect_registers: Default::default(),
            expect_stdout: None,
            max_steps: 100,
            solution: "hlt".into(),
        });
        let v = check(&e, "mov rax, 0\ndiv rax\nhlt");
        assert!(!v.correct);
        assert!(v.message.contains("faulted"), "{}", v.message);
    }

    #[test]
    fn normalize_ignores_case_commas_and_spacing() {
        assert_eq!(normalize("MOV  RAX ,1"), normalize("mov rax, 1"));
    }
}
