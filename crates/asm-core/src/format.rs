//! Intel-syntax formatting.
//!
//! One deliberate departure from NASM: a memory operand always carries its size
//! keyword (`qword [rsp+8]`, not `[rsp+8]`), even when the other operand makes
//! the width unambiguous. NASM omits it because it can infer it. We print it
//! because a student reading `mov rax, [rsp+8]` cannot see that eight bytes
//! move, and the whole point of this crate is to make the invisible visible.
//! The assembler in [`crate::asm`] accepts both spellings.

use crate::insn::{Insn, Mnemonic};
use crate::operand::Operand;

/// Render an instruction as Intel-syntax text, resolving relative branches to
/// absolute addresses.
pub fn to_string(insn: &Insn) -> String {
    let mut s = String::with_capacity(32);

    if insn.lock {
        s.push_str("lock ");
    }

    s.push_str(&insn.mnemonic.name());

    if insn.operands.is_empty() {
        return s;
    }
    s.push(' ');

    for (i, op) in insn.operands.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        match op {
            // A relative displacement is meaningless to a reader. Resolve it.
            Operand::Rel(_) => {
                let target = insn.branch_target().expect("Rel operand implies a branch target");
                s.push_str(&format!("0x{:x}", target));
            }
            other => s.push_str(&other.to_string()),
        }
    }
    s
}

/// A disassembly line with the address, the raw bytes, and the text — the
/// three columns every disassembler shows.
///
/// ```text
/// 0000000000001000  48 89 e5              mov rbp, rsp
/// ```
pub fn to_listing_line(insn: &Insn) -> String {
    let bytes = insn.bytes().iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
    format!("{:016x}  {:<24}  {}", insn.ip, bytes, to_string(insn))
}

/// Render a sequence of instructions as a listing.
pub fn to_listing<'a>(insns: impl IntoIterator<Item = &'a Insn>) -> String {
    insns.into_iter().map(to_listing_line).collect::<Vec<_>>().join("\n")
}

/// A one-line plain-English gloss of what the instruction does.
///
/// Deliberately informal. It exists so a lesson can render a tooltip without
/// each lesson re-deriving the same sentence.
pub fn describe(insn: &Insn) -> String {
    use Mnemonic as M;
    let ops = &insn.operands;
    let o = |i: usize| ops.get(i).map(|o| o.to_string()).unwrap_or_default();

    match insn.mnemonic {
        M::Mov => format!("copy {} into {}", o(1), o(0)),
        M::Movzx => format!("copy {} into {}, filling the upper bits with zeroes", o(1), o(0)),
        M::Movsx | M::Movsxd => {
            format!("copy {} into {}, replicating its sign bit into the upper bits", o(1), o(0))
        }
        M::Lea => {
            format!("compute the address {} and put it in {} without reading memory", o(1), o(0))
        }
        M::Add => format!("add {} to {}, storing the result in {}", o(1), o(0), o(0)),
        M::Sub => format!("subtract {} from {}, storing the result in {}", o(1), o(0), o(0)),
        M::Xor if ops.len() == 2 && ops[0] == ops[1] => {
            format!("set {} to zero (xor with itself), and clear CF and OF", o(0))
        }
        M::Xor => format!("bitwise exclusive-or {} into {}", o(1), o(0)),
        M::And => format!("bitwise and {} into {}", o(1), o(0)),
        M::Or => format!("bitwise or {} into {}", o(1), o(0)),
        M::Cmp => format!("compute {} - {} and set the flags, discarding the result", o(0), o(1)),
        M::Test if ops.len() == 2 && ops[0] == ops[1] => {
            format!("set the flags from {}; ZF becomes 1 if it is zero", o(0))
        }
        M::Test => format!("compute {} & {} and set the flags, discarding the result", o(0), o(1)),
        M::Push => format!("subtract 8 from rsp, then store {} at [rsp]", o(0)),
        M::Pop => format!("load {} from [rsp], then add 8 to rsp", o(0)),
        M::Call => format!("push the address of the next instruction, then jump to {}", o(0)),
        M::Ret => "pop a return address off the stack and jump to it".to_string(),
        M::Leave => "restore rsp from rbp, then pop rbp — undo a standard prologue".to_string(),
        M::Jmp => format!("jump to {}", o(0)),
        M::Jcc(c) => {
            let kind = if c.is_signed() {
                "signed comparison"
            } else if c.is_unsigned() {
                "unsigned comparison"
            } else {
                "flag test"
            };
            format!("jump to {} if the {} condition holds ({})", o(0), c.suffix(), kind)
        }
        M::Setcc(c) => format!("set {} to 1 if the {} condition holds, else 0", o(0), c.suffix()),
        M::Cmovcc(c) => {
            format!("copy {} into {} only if the {} condition holds", o(1), o(0), c.suffix())
        }
        M::Inc => format!("add 1 to {} without touching CF", o(0)),
        M::Dec => format!("subtract 1 from {} without touching CF", o(0)),
        M::Neg => format!("replace {} with its two's-complement negation", o(0)),
        M::Not => format!("flip every bit of {}", o(0)),
        M::Shl => format!("shift {} left by {}, filling with zeroes", o(0), o(1)),
        M::Shr => format!("shift {} right by {}, filling with zeroes", o(0), o(1)),
        M::Sar => format!("shift {} right by {}, replicating the sign bit", o(0), o(1)),
        M::Imul if ops.len() == 1 => {
            format!("signed multiply of rax by {}, result in rdx:rax", o(0))
        }
        M::Imul => format!("signed multiply {} by {}", o(0), o(1)),
        M::Mul => format!("unsigned multiply of rax by {}, result in rdx:rax", o(0)),
        M::Div => format!("unsigned divide rdx:rax by {}; quotient to rax, remainder to rdx", o(0)),
        M::Idiv => format!("signed divide rdx:rax by {}; quotient to rax, remainder to rdx", o(0)),
        M::Cdq | M::Cqo | M::Cwd => {
            "sign-extend the accumulator into rdx, preparing for a signed divide".to_string()
        }
        M::Cdqe | M::Cwde | M::Cbw => "sign-extend the accumulator in place".to_string(),
        M::Syscall => "trap into the kernel; the call number is in rax".to_string(),
        M::Int3 => "raise a breakpoint trap — one byte, so a debugger can patch any instruction"
            .to_string(),
        M::Ud2 => "raise an invalid-opcode fault; execution never continues past here".to_string(),
        M::Endbr64 => "a valid indirect-branch landing pad, required by CET".to_string(),
        M::Nop => "do nothing".to_string(),
        M::Hlt => "halt the processor until an interrupt arrives".to_string(),
        M::Xchg => format!("swap the contents of {} and {}", o(0), o(1)),
        M::Bswap => format!("reverse the byte order of {} — convert endianness", o(0)),
        _ => format!("{}", insn.mnemonic),
    }
}

#[cfg(test)]
mod tests {
    use crate::decode::decode;

    #[test]
    fn listing_lines_show_address_bytes_and_text() {
        let insn = decode(&[0x48, 0x89, 0xe5], 0x1000).unwrap();
        let line = super::to_listing_line(&insn);
        assert!(line.starts_with("0000000000001000  48 89 e5"));
        assert!(line.ends_with("mov rbp, rsp"));
    }

    #[test]
    fn xor_of_a_register_with_itself_is_described_as_zeroing() {
        let insn = decode(&[0x31, 0xc0], 0).unwrap();
        assert!(super::describe(&insn).contains("set eax to zero"));
    }

    #[test]
    fn lock_prefix_is_printed() {
        let insn = decode(&[0xf0, 0x48, 0x01, 0x08], 0).unwrap();
        assert!(super::to_string(&insn).starts_with("lock add"));
    }
}
