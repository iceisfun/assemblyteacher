//! A small Intel-syntax assembler.
//!
//! # Why two passes are not enough
//!
//! A branch's encoding depends on how far it jumps, and how far it jumps
//! depends on the size of every instruction in between — including other
//! branches. `jmp` over a hundred bytes needs a 5-byte encoding; over ten
//! bytes, 2. Shrinking one branch can bring a second branch's target within
//! reach of *its* short form, which shrinks the code again.
//!
//! Assemblers resolve this by iterating to a fixed point. The direction matters:
//! this one starts by assuming **every branch is short** and only ever grows
//! them. Growth is monotonic, so the loop cannot oscillate and must terminate.
//! Starting from the long form and shrinking has no such guarantee.
//!
//! # Syntax
//!
//! NASM-flavoured Intel syntax, deliberately small:
//!
//! ```text
//!     bits 64                 ; ignored, accepted so nasm sources paste cleanly
//!     org 0x400000            ; set the origin address
//! _start:
//!     mov rax, 1              ; labels, registers, immediates
//!     mov rdi, qword [rsp+8]  ; memory operands, size keywords
//!     lea rsi, [rip+msg]      ; rip-relative references to labels
//!     cmp rax, rdx
//!     jle .done               ; forward and backward branches
//!     add rax, 1
//! .done:
//!     syscall
//! msg:
//!     db "hello", 10, 0       ; data directives
//! ```
//!
//! Comments start with `;`. Labels may be defined anywhere and referenced
//! before they are defined.

use crate::encode::{encode, encode_branch};
use crate::error::{AsmError, AsmErrorKind};
use crate::insn::{Cond, Mnemonic};
use crate::operand::{Mem, Operand};
use crate::reg::{Reg, Seg, Size};
use std::collections::BTreeMap;

/// The result of assembling a source file.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Assembled {
    /// The machine code.
    pub bytes: Vec<u8>,
    /// The address `bytes[0]` was assembled for.
    pub origin: u64,
    /// Every label, and the address it resolved to.
    pub labels: BTreeMap<String, u64>,
    /// One entry per source line that produced bytes, in address order. This is
    /// what the side-by-side source/machine-code view in the web UI consumes.
    pub lines: Vec<AsmLine>,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AsmLine {
    /// 1-based source line number.
    pub line: usize,
    pub address: u64,
    pub bytes: Vec<u8>,
    pub text: String,
}

impl Assembled {
    /// A `objdump`-style listing: address, bytes, source.
    pub fn listing(&self) -> String {
        self.lines
            .iter()
            .map(|l| {
                let hex =
                    l.bytes.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
                format!("{:016x}  {:<24}  {}", l.address, hex, l.text.trim())
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Assemble Intel-syntax source into machine code.
pub fn assemble(source: &str) -> Result<Assembled, AsmError> {
    assemble_at(source, 0)
}

/// Assemble with an explicit default origin. An `org` directive overrides it.
pub fn assemble_at(source: &str, origin: u64) -> Result<Assembled, AsmError> {
    let mut stmts = Vec::new();
    let mut origin = origin;

    for (idx, raw) in source.lines().enumerate() {
        let line = idx + 1;
        let text = strip_comment(raw).trim();
        if text.is_empty() {
            continue;
        }
        parse_line(text, line, &mut stmts, &mut origin)?;
    }

    layout(stmts, origin)
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum Stmt {
    Label(String),
    Insn { mnemonic: Mnemonic, ops: Vec<Pop>, line: usize, text: String, lock: bool },
    Data { bytes: Vec<u8>, line: usize, text: String },
}

/// A parsed operand, possibly still referring to an unresolved label.
#[derive(Clone, Debug)]
enum Pop {
    Reg(Reg),
    Imm(i64),
    /// A label used as an absolute value, e.g. `mov rsi, msg`.
    ImmLabel(String),
    /// A branch destination named by a label.
    RelLabel(String),
    /// A branch destination written as an absolute address, e.g. `jmp 0x1234`.
    /// Disassembly prints branch targets this way, so this is what makes
    /// `assemble(disassemble(bytes))` round-trip.
    RelAbs(i64),
    Mem(PMem),
}

#[derive(Clone, Debug, Default)]
struct PMem {
    seg: Option<Seg>,
    base: Option<Reg>,
    index: Option<Reg>,
    scale: u8,
    disp: i64,
    /// `[rip + label]` or `[label]`.
    disp_label: Option<String>,
    rip_relative: bool,
    size: Option<Size>,
}

fn strip_comment(s: &str) -> &str {
    let mut in_str = false;
    for (i, c) in s.char_indices() {
        match c {
            '"' | '\'' => in_str = !in_str,
            ';' if !in_str => return &s[..i],
            '#' if !in_str => return &s[..i],
            _ => {}
        }
    }
    s
}

fn err(line: usize, kind: AsmErrorKind) -> AsmError {
    AsmError { line, kind }
}

fn parse_line(
    text: &str,
    line: usize,
    stmts: &mut Vec<Stmt>,
    origin: &mut u64,
) -> Result<(), AsmError> {
    let mut text = text;

    // A leading `label:` may share a line with an instruction.
    if let Some(colon) = find_label_colon(text) {
        let name = text[..colon].trim();
        if !is_ident(name) {
            return Err(err(
                line,
                AsmErrorKind::Expected { expected: "label", found: name.into() },
            ));
        }
        stmts.push(Stmt::Label(name.to_string()));
        text = text[colon + 1..].trim();
        if text.is_empty() {
            return Ok(());
        }
    }

    // `lock` is a prefix, not a mnemonic: it decorates the instruction that
    // follows it. Only read-modify-write instructions may carry it.
    let mut lock = false;
    let (mut head, mut rest) = split_head(text);
    if head.eq_ignore_ascii_case("lock") {
        lock = true;
        let (h, r) = split_head(rest.trim());
        head = h;
        rest = r;
    }
    let head_lc = head.to_ascii_lowercase();

    match head_lc.as_str() {
        // Accepted and ignored so that NASM sources paste in without edits.
        "bits" | "global" | "section" | "extern" | "default" => return Ok(()),
        "org" => {
            *origin = parse_number(rest.trim())
                .ok_or_else(|| err(line, AsmErrorKind::BadNumber(rest.trim().into())))?
                as u64;
            return Ok(());
        }
        "db" | "dw" | "dd" | "dq" => {
            let width = match head_lc.as_str() {
                "db" => 1,
                "dw" => 2,
                "dd" => 4,
                _ => 8,
            };
            let bytes = parse_data(rest, width, line)?;
            stmts.push(Stmt::Data { bytes, line, text: text.to_string() });
            return Ok(());
        }
        _ => {}
    }

    let mnemonic = parse_mnemonic(&head_lc)
        .ok_or_else(|| err(line, AsmErrorKind::UnknownMnemonic(head.to_string())))?;

    let branch = matches!(mnemonic, Mnemonic::Jmp | Mnemonic::Jcc(_) | Mnemonic::Call);
    let mut ops = Vec::new();
    for part in split_operands(rest) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        ops.push(parse_operand(part, branch, line)?);
    }

    stmts.push(Stmt::Insn { mnemonic, ops, line, text: text.to_string(), lock });
    Ok(())
}

/// Find the colon that terminates a leading label, ignoring the colon in a
/// segment override like `fs:[0x28]` and the one inside brackets.
fn find_label_colon(s: &str) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => depth -= 1,
            ':' if depth == 0 => {
                // `fs:` is only a segment override when it follows a mnemonic,
                // which means a space has already appeared.
                if s[..i].contains(char::is_whitespace) {
                    return None;
                }
                return Some(i);
            }
            _ => {}
        }
    }
    None
}

fn split_head(s: &str) -> (&str, &str) {
    match s.find(char::is_whitespace) {
        Some(i) => (&s[..i], &s[i..]),
        None => (s, ""),
    }
}

fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '.')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '$')
}

/// Split on commas at bracket depth zero, respecting string literals.
fn split_operands(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in s.chars() {
        match c {
            '"' | '\'' if quote.is_none() => {
                quote = Some(c);
                cur.push(c);
            }
            c2 if Some(c2) == quote => {
                quote = None;
                cur.push(c2);
            }
            _ if quote.is_some() => cur.push(c),
            '[' => {
                depth += 1;
                cur.push(c);
            }
            ']' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

// ---------------------------------------------------------------------------
// Mnemonics
// ---------------------------------------------------------------------------

fn parse_mnemonic(s: &str) -> Option<Mnemonic> {
    use Mnemonic as M;
    let m = match s {
        "add" => M::Add,
        "or" => M::Or,
        "adc" => M::Adc,
        "sbb" => M::Sbb,
        "and" => M::And,
        "sub" => M::Sub,
        "xor" => M::Xor,
        "cmp" => M::Cmp,
        "test" => M::Test,
        "not" => M::Not,
        "neg" => M::Neg,
        "inc" => M::Inc,
        "dec" => M::Dec,
        "mul" => M::Mul,
        "imul" => M::Imul,
        "div" => M::Div,
        "idiv" => M::Idiv,
        // `movabs` is the GAS name for the 64-bit-immediate form. The encoder
        // picks that form on its own when the value needs it.
        "mov" | "movabs" => M::Mov,
        "movzx" => M::Movzx,
        "movsx" => M::Movsx,
        "movsxd" => M::Movsxd,
        "lea" => M::Lea,
        "push" => M::Push,
        "pop" => M::Pop,
        "xchg" => M::Xchg,
        // `sal` and `shl` are the same instruction: shifting left is the same
        // operation whether you call the operand signed or not.
        "shl" | "sal" => M::Shl,
        "shr" => M::Shr,
        "sar" => M::Sar,
        "rol" => M::Rol,
        "ror" => M::Ror,
        "rcl" => M::Rcl,
        "rcr" => M::Rcr,
        "jmp" => M::Jmp,
        "call" => M::Call,
        "ret" | "retn" => M::Ret,
        "leave" => M::Leave,
        "nop" => M::Nop,
        "hlt" => M::Hlt,
        "int3" => M::Int3,
        "int" => M::Int,
        "syscall" => M::Syscall,
        "cdq" => M::Cdq,
        "cqo" => M::Cqo,
        "cwd" => M::Cwd,
        "cdqe" => M::Cdqe,
        "cbw" => M::Cbw,
        "cwde" => M::Cwde,
        "bswap" => M::Bswap,
        "endbr64" => M::Endbr64,
        "ud2" => M::Ud2,
        _ => {
            // jcc / setcc / cmovcc, e.g. "jne", "setg", "cmovb".
            if let Some(rest) = s.strip_prefix("cmov") {
                return Cond::parse(rest).map(M::Cmovcc);
            }
            if let Some(rest) = s.strip_prefix("set") {
                return Cond::parse(rest).map(M::Setcc);
            }
            if let Some(rest) = s.strip_prefix('j') {
                return Cond::parse(rest).map(M::Jcc);
            }
            return None;
        }
    };
    Some(m)
}

// ---------------------------------------------------------------------------
// Operands
// ---------------------------------------------------------------------------

fn parse_operand(s: &str, branch: bool, line: usize) -> Result<Pop, AsmError> {
    let mut s = s.trim();

    // Optional size keyword, with or without the MASM-style `ptr`.
    let mut size = None;
    for kw in ["byte", "word", "dword", "qword"] {
        if let Some(rest) = strip_word_prefix(s, kw) {
            size = Size::from_bytes(match kw {
                "byte" => 1,
                "word" => 2,
                "dword" => 4,
                _ => 8,
            });
            s = strip_word_prefix(rest, "ptr").unwrap_or(rest).trim();
            break;
        }
    }

    // Segment override: `fs:[...]`.
    let mut seg = None;
    if let Some(colon) = s.find(':') {
        if s[colon + 1..].trim_start().starts_with('[') {
            let name = s[..colon].trim().to_ascii_lowercase();
            seg = match name.as_str() {
                "es" => Some(Seg::Es),
                "cs" => Some(Seg::Cs),
                "ss" => Some(Seg::Ss),
                "ds" => Some(Seg::Ds),
                "fs" => Some(Seg::Fs),
                "gs" => Some(Seg::Gs),
                _ => {
                    return Err(err(
                        line,
                        AsmErrorKind::Expected { expected: "segment register", found: name },
                    ))
                }
            };
            s = s[colon + 1..].trim();
        }
    }

    if s.starts_with('[') {
        if !s.ends_with(']') {
            return Err(err(line, AsmErrorKind::BadMemory(s.into())));
        }
        let mut mem = parse_mem(&s[1..s.len() - 1], line)?;
        mem.seg = seg;
        mem.size = size;
        return Ok(Pop::Mem(mem));
    }

    if let Some(r) = Reg::parse(s) {
        return Ok(Pop::Reg(r));
    }

    if let Some(v) = parse_number(s) {
        // For a branch, a bare number is the address to land on, not a value to
        // load. `jmp 0x1234` still encodes as a *relative* displacement; the
        // assembler does the subtraction. Every disassembler prints branch
        // targets absolutely, so this is what lets our own output be reassembled.
        return Ok(if branch { Pop::RelAbs(v) } else { Pop::Imm(v) });
    }

    if is_ident(s) {
        return Ok(if branch {
            Pop::RelLabel(s.to_string())
        } else {
            Pop::ImmLabel(s.to_string())
        });
    }

    Err(err(line, AsmErrorKind::Expected { expected: "an operand", found: s.into() }))
}

/// Strip `word` from `"word ptr [rax]"`, but not from `"wordlike"`.
fn strip_word_prefix<'a>(s: &'a str, word: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(word)?;
    if rest.is_empty() || rest.starts_with(|c: char| c.is_whitespace() || c == '[') {
        Some(rest.trim_start())
    } else {
        None
    }
}

/// Parse the inside of `[...]` — a sum of registers, scaled registers, a
/// displacement, and at most one label.
fn parse_mem(inner: &str, line: usize) -> Result<PMem, AsmError> {
    let mut mem = PMem { scale: 1, ..Default::default() };

    for (negative, term) in split_terms(inner) {
        let term = term.trim();
        if term.is_empty() {
            continue;
        }

        if term.eq_ignore_ascii_case("rip") {
            mem.rip_relative = true;
            continue;
        }

        // `reg*scale` or `scale*reg`
        if let Some((a, b)) = term.split_once('*') {
            let (reg_txt, scale_txt) = match Reg::parse(a.trim()) {
                Some(_) => (a.trim(), b.trim()),
                None => (b.trim(), a.trim()),
            };
            let reg = Reg::parse(reg_txt)
                .ok_or_else(|| err(line, AsmErrorKind::UnknownRegister(reg_txt.into())))?;
            let scale = parse_number(scale_txt)
                .ok_or_else(|| err(line, AsmErrorKind::BadNumber(scale_txt.into())))?;
            if !matches!(scale, 1 | 2 | 4 | 8) {
                return Err(err(line, AsmErrorKind::BadScale(scale as u64)));
            }
            if mem.index.is_some() {
                return Err(err(line, AsmErrorKind::BadMemory("two index registers".into())));
            }
            mem.index = Some(reg);
            mem.scale = scale as u8;
            continue;
        }

        if let Some(reg) = Reg::parse(term) {
            // The first bare register is the base; a second becomes the index
            // with scale 1. `[rax+rbx]` therefore means base rax, index rbx.
            if mem.base.is_none() {
                mem.base = Some(reg);
            } else if mem.index.is_none() {
                mem.index = Some(reg);
                mem.scale = 1;
            } else {
                return Err(err(line, AsmErrorKind::BadMemory("too many registers".into())));
            }
            continue;
        }

        if let Some(v) = parse_number(term) {
            mem.disp = mem.disp.wrapping_add(if negative { -v } else { v });
            continue;
        }

        if is_ident(term) {
            if mem.disp_label.is_some() {
                return Err(err(line, AsmErrorKind::BadMemory("two labels".into())));
            }
            if negative {
                return Err(err(line, AsmErrorKind::BadMemory("cannot negate a label".into())));
            }
            mem.disp_label = Some(term.to_string());
            continue;
        }

        return Err(err(line, AsmErrorKind::BadMemory(term.into())));
    }

    Ok(mem)
}

/// Split `rbp - 8 + rax*4` into `[(false,"rbp"),(true,"8"),(false,"rax*4")]`.
fn split_terms(s: &str) -> Vec<(bool, String)> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut neg = false;
    for c in s.chars() {
        match c {
            '+' | '-' => {
                if !cur.trim().is_empty() {
                    out.push((neg, std::mem::take(&mut cur)));
                } else {
                    cur.clear();
                }
                neg = c == '-';
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push((neg, cur));
    }
    out
}

fn parse_number(s: &str) -> Option<i64> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('-') {
        return parse_number(rest).map(|v| -v);
    }
    if let Some(rest) = s.strip_prefix('+') {
        return parse_number(rest);
    }
    if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return i64::from_str_radix(&rest.replace('_', ""), 16).ok();
    }
    if let Some(rest) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        return i64::from_str_radix(&rest.replace('_', ""), 2).ok();
    }
    if let Some(rest) = s.strip_prefix("0o") {
        return i64::from_str_radix(&rest.replace('_', ""), 8).ok();
    }
    // NASM's trailing-h form.
    if let Some(rest) = s.strip_suffix('h').or_else(|| s.strip_suffix('H')) {
        if rest.chars().all(|c| c.is_ascii_hexdigit()) && !rest.is_empty() {
            return i64::from_str_radix(rest, 16).ok();
        }
    }
    // Character literal: 'A'
    let b = s.as_bytes();
    if b.len() == 3 && b[0] == b'\'' && b[2] == b'\'' {
        return Some(b[1] as i64);
    }
    s.replace('_', "").parse::<i64>().ok()
}

fn parse_data(rest: &str, width: usize, line: usize) -> Result<Vec<u8>, AsmError> {
    let mut out = Vec::new();
    for item in split_operands(rest) {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        if (item.starts_with('"') && item.ends_with('"') && item.len() >= 2)
            || (width == 1 && item.starts_with('\'') && item.ends_with('\'') && item.len() > 3)
        {
            out.extend_from_slice(unescape(&item[1..item.len() - 1]).as_bytes());
            continue;
        }
        let v = parse_number(item)
            .ok_or_else(|| err(line, AsmErrorKind::BadNumber(item.to_string())))?;
        out.extend_from_slice(&v.to_le_bytes()[..width]);
    }
    Ok(out)
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Layout and relaxation
// ---------------------------------------------------------------------------

/// Assign addresses, resolve labels, and iterate until instruction sizes stop
/// changing.
fn layout(stmts: Vec<Stmt>, origin: u64) -> Result<Assembled, AsmError> {
    // Emitters are the statements that occupy space.
    let emitters: Vec<&Stmt> = stmts.iter().filter(|s| !matches!(s, Stmt::Label(_))).collect();

    // Each branch starts out assumed short, and is allowed only to grow. This
    // is what makes the fixed point reachable.
    let mut lens: Vec<usize> = vec![0; emitters.len()];
    let mut short: Vec<bool> = vec![true; emitters.len()];

    for round in 0.. {
        let addrs = addresses(&stmts, &lens, origin);
        let labels = label_map(&stmts, &lens, origin)?;

        let mut changed = false;
        let mut new_lens = Vec::with_capacity(emitters.len());

        for (i, stmt) in emitters.iter().enumerate() {
            let bytes = emit(stmt, addrs[i], &labels, short[i], lens[i])?;
            // Once a branch has grown to its long form it stays there.
            if bytes.len() > lens[i] && lens[i] != 0 {
                if let Stmt::Insn { mnemonic, .. } = stmt {
                    if matches!(mnemonic, Mnemonic::Jmp | Mnemonic::Jcc(_)) {
                        short[i] = false;
                    }
                }
            }
            if bytes.len() != lens[i] {
                changed = true;
            }
            new_lens.push(bytes.len());
        }
        lens = new_lens;

        if !changed {
            break;
        }
        if round > 64 {
            // Monotonic growth guarantees this is unreachable; the guard is
            // here so a future bug fails loudly instead of hanging.
            unreachable!("branch relaxation did not converge");
        }
    }

    // Final emission with the settled layout.
    let addrs = addresses(&stmts, &lens, origin);
    let labels = label_map(&stmts, &lens, origin)?;
    let mut bytes = Vec::new();
    let mut lines = Vec::new();

    for (i, stmt) in emitters.iter().enumerate() {
        let b = emit(stmt, addrs[i], &labels, short[i], lens[i])?;
        let (line, text) = match stmt {
            Stmt::Insn { line, text, .. } | Stmt::Data { line, text, .. } => (*line, text.clone()),
            Stmt::Label(_) => unreachable!("labels were filtered out"),
        };
        lines.push(AsmLine { line, address: addrs[i], bytes: b.clone(), text });
        bytes.extend_from_slice(&b);
    }

    Ok(Assembled { bytes, origin, labels, lines })
}

/// Address of each emitting statement, given the current size estimates.
fn addresses(stmts: &[Stmt], lens: &[usize], origin: u64) -> Vec<u64> {
    let mut out = Vec::with_capacity(lens.len());
    let mut pc = origin;
    let mut i = 0;
    for s in stmts {
        if matches!(s, Stmt::Label(_)) {
            continue;
        }
        out.push(pc);
        pc += lens[i] as u64;
        i += 1;
    }
    out
}

fn label_map(
    stmts: &[Stmt],
    lens: &[usize],
    origin: u64,
) -> Result<BTreeMap<String, u64>, AsmError> {
    let mut map = BTreeMap::new();
    let mut pc = origin;
    let mut i = 0;
    for s in stmts {
        match s {
            Stmt::Label(name) => {
                if map.insert(name.clone(), pc).is_some() {
                    return Err(err(0, AsmErrorKind::DuplicateLabel(name.clone())));
                }
            }
            _ => {
                pc += lens[i] as u64;
                i += 1;
            }
        }
    }
    Ok(map)
}

/// Emit one statement. `cur_len` is the size this statement had in the previous
/// round, which is what RIP-relative and branch displacements are measured
/// against; it converges along with the layout.
fn emit(
    stmt: &Stmt,
    addr: u64,
    labels: &BTreeMap<String, u64>,
    short: bool,
    cur_len: usize,
) -> Result<Vec<u8>, AsmError> {
    let (mnemonic, ops, line, lock) = match stmt {
        Stmt::Data { bytes, .. } => return Ok(bytes.clone()),
        Stmt::Label(_) => return Ok(Vec::new()),
        Stmt::Insn { mnemonic, ops, line, lock, .. } => (*mnemonic, ops, *line, *lock),
    };

    // A relative branch: the displacement is measured from the end of this
    // instruction, whose length is what we are trying to determine.
    let rel_target: Option<(i64, String)> = match ops.as_slice() {
        [Pop::RelLabel(name)] => {
            let t = *labels
                .get(name)
                .ok_or_else(|| err(line, AsmErrorKind::UndefinedLabel(name.clone())))?;
            Some((t as i64, name.clone()))
        }
        [Pop::RelAbs(v)] => Some((*v, format!("{:#x}", v))),
        _ => None,
    };

    if let Some((target, name)) = rel_target {
        // On the very first round cur_len is 0; assume the short form so that
        // relaxation only ever has to grow.
        let assumed = if cur_len == 0 {
            if mnemonic == Mnemonic::Call {
                5
            } else {
                2
            }
        } else {
            cur_len
        };
        let disp = target - (addr as i64 + assumed as i64);

        let want_short = short && mnemonic != Mnemonic::Call && i8::try_from(disp).is_ok();
        return encode_branch(mnemonic, disp, want_short).map_err(|e| match e {
            crate::error::EncodeError::ImmediateOutOfRange { value, bytes } => {
                err(line, AsmErrorKind::BranchOutOfRange { label: name, distance: value, bytes })
            }
            other => err(line, AsmErrorKind::Encode(other)),
        });
    }

    let next_ip = addr.wrapping_add(cur_len as u64);
    let resolved: Vec<Operand> =
        ops.iter().map(|p| resolve(p, labels, next_ip, line)).collect::<Result<_, _>>()?;

    let mut bytes = encode(mnemonic, &resolved).map_err(|e| err(line, AsmErrorKind::Encode(e)))?;
    if lock {
        // Legacy prefixes may appear in any order, and `lock` must precede the
        // REX byte, so prepending is always correct.
        bytes.insert(0, 0xf0);
    }
    Ok(bytes)
}

fn resolve(
    p: &Pop,
    labels: &BTreeMap<String, u64>,
    next_ip: u64,
    line: usize,
) -> Result<Operand, AsmError> {
    Ok(match p {
        Pop::Reg(r) => Operand::Reg(*r),
        Pop::Imm(v) => Operand::Imm(*v),
        Pop::ImmLabel(name) => {
            let v = *labels
                .get(name)
                .ok_or_else(|| err(line, AsmErrorKind::UndefinedLabel(name.clone())))?;
            Operand::Imm(v as i64)
        }
        Pop::RelLabel(name) => {
            let v = *labels
                .get(name)
                .ok_or_else(|| err(line, AsmErrorKind::UndefinedLabel(name.clone())))?;
            Operand::Rel(v as i64 - next_ip as i64)
        }
        Pop::RelAbs(v) => Operand::Rel(*v - next_ip as i64),
        Pop::Mem(m) => {
            let mut disp = m.disp;
            if let Some(name) = &m.disp_label {
                let v = *labels
                    .get(name)
                    .ok_or_else(|| err(line, AsmErrorKind::UndefinedLabel(name.clone())))?
                    as i64;
                // `[rip + label]` means "the label", not "the label past rip".
                // The assembler subtracts the address of the next instruction,
                // because that is what the hardware will add back.
                disp += if m.rip_relative { v - next_ip as i64 } else { v };
            }
            Operand::Mem(Mem {
                seg: m.seg,
                base: m.base,
                index: m.index,
                scale: if m.index.is_some() { m.scale } else { 1 },
                disp,
                size: m.size,
                rip_relative: m.rip_relative,
            })
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::Decoder;
    use crate::format;

    fn asm(src: &str) -> Vec<u8> {
        assemble(src).unwrap_or_else(|e| panic!("{e}")).bytes
    }

    fn round_trip(src: &str) -> Vec<String> {
        let out = assemble(src).unwrap_or_else(|e| panic!("{e}"));
        Decoder::new(&out.bytes, out.origin)
            .map(|i| format::to_string(&i.expect("decodes")))
            .collect()
    }

    #[test]
    fn assembles_a_function_prologue_and_epilogue() {
        assert_eq!(asm("push rbp"), [0x55]);
        assert_eq!(asm("mov rbp, rsp"), [0x48, 0x89, 0xe5]);
        assert_eq!(asm("pop rbp"), [0x5d]);
        assert_eq!(asm("ret"), [0xc3]);
        assert_eq!(asm("leave"), [0xc9]);
    }

    #[test]
    fn everything_it_assembles_it_can_disassemble() {
        let src = "
            mov rax, 1
            mov rdi, qword [rsp+8]
            lea rsi, [rbx + rcx*4 - 16]
            add rax, rdx
            sub rsp, 0x20
            xor eax, eax
            cmp rax, rdx
            imul rax, rcx, 3
            movzx eax, byte [rdi]
            shl rax, 4
            not rbx
            neg rbx
            push r12
            pop r12
            syscall
            ret
        ";
        assert_eq!(
            round_trip(src),
            [
                "mov rax, 0x1",
                "mov rdi, qword [rsp+0x8]",
                "lea rsi, [rbx+rcx*4-0x10]",
                "add rax, rdx",
                "sub rsp, 0x20",
                "xor eax, eax",
                "cmp rax, rdx",
                "imul rax, rcx, 0x3",
                "movzx eax, byte [rdi]",
                "shl rax, 0x4",
                "not rbx",
                "neg rbx",
                "push r12",
                "pop r12",
                "syscall",
                "ret",
            ]
        );
    }

    #[test]
    fn backward_branches_use_the_short_form() {
        // The classic two-byte infinite loop.
        assert_eq!(asm("here:\njmp here"), [0xeb, 0xfe]);
    }

    #[test]
    fn forward_branches_relax_to_the_short_form() {
        let out = asm("jmp done\nnop\ndone:\nret");
        assert_eq!(out, [0xeb, 0x01, 0x90, 0xc3]);
    }

    #[test]
    fn branches_grow_to_rel32_when_the_target_is_far() {
        let mut src = String::from("jmp done\n");
        for _ in 0..200 {
            src.push_str("nop\n");
        }
        src.push_str("done:\nret\n");
        let out = asm(&src);
        assert_eq!(out[0], 0xe9, "should have relaxed to a 5-byte jmp");
        assert_eq!(i32::from_le_bytes([out[1], out[2], out[3], out[4]]), 200);
        assert_eq!(out.len(), 5 + 200 + 1);
    }

    #[test]
    fn relaxation_reaches_a_fixed_point_when_shrinking_enables_shrinking() {
        // Two branches whose targets straddle the 127-byte boundary. Naive
        // single-pass sizing gets this wrong in one direction or the other.
        let mut src = String::from("je a\njmp b\n");
        for _ in 0..120 {
            src.push_str("nop\n");
        }
        src.push_str("a:\n");
        src.push_str("b:\nret\n");
        let out = assemble(&src).unwrap();
        // Everything must still decode cleanly and land on the right target.
        let insns: Vec<_> =
            Decoder::new(&out.bytes, 0).collect::<Result<Vec<_>, _>>().expect("decodes");
        assert_eq!(insns[0].branch_target(), Some(out.labels["a"]));
        assert_eq!(insns[1].branch_target(), Some(out.labels["b"]));
    }

    #[test]
    fn labels_resolve_forwards_and_backwards() {
        let out =
            assemble("start:\n  mov rax, 1\nloop_top:\n  dec rax\n  jnz loop_top\n  ret").unwrap();
        assert_eq!(out.labels["start"], 0);
        assert_eq!(out.labels["loop_top"], 7);
        let insns: Vec<_> = Decoder::new(&out.bytes, 0).collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(insns[2].branch_target(), Some(7));
    }

    #[test]
    fn rip_relative_label_references_resolve_against_the_next_instruction() {
        let out = assemble("lea rsi, [rip+msg]\nret\nmsg:\ndb \"hi\", 0").unwrap();
        let insn = crate::decode(&out.bytes, 0).unwrap();
        let mem = insn.operands[1].as_mem().unwrap();
        assert!(mem.rip_relative);
        assert_eq!(mem.effective_address(insn.next_ip(), |_| 0), out.labels["msg"]);
    }

    #[test]
    fn a_numeric_branch_target_is_an_address_not_an_immediate() {
        // This is what our own disassembler prints, so it must reassemble.
        assert_eq!(asm("jmp 0x0"), [0xeb, 0xfe]);
        assert_eq!(asm("je 0x2"), [0x74, 0x00]);
        assert_eq!(asm("call 0x5"), [0xe8, 0x00, 0x00, 0x00, 0x00]);
        // ...while a register or memory operand still means an indirect branch.
        assert_eq!(asm("jmp rax"), [0xff, 0xe0]);
        assert_eq!(asm("call qword [rax]"), [0xff, 0x10]);
    }

    #[test]
    fn disassembly_of_a_branch_reassembles_to_the_same_bytes() {
        for bytes in [
            vec![0xeb, 0xfeu8],
            vec![0x74, 0x00],
            vec![0xe8, 0x00, 0x00, 0x00, 0x00],
            vec![0x0f, 0x8f, 0x10, 0x00, 0x00, 0x00],
        ] {
            let insn = crate::decode(&bytes, 0).unwrap();
            let text = format::to_string(&insn);
            let back = assemble(&text).unwrap_or_else(|e| panic!("`{text}`: {e}")).bytes;
            let reinsn = crate::decode(&back, 0).unwrap();
            assert_eq!(
                insn.branch_target(),
                reinsn.branch_target(),
                "`{text}` from {bytes:02x?} reassembled to {back:02x?}"
            );
        }
    }

    #[test]
    fn org_sets_the_origin_and_labels_follow() {
        let out = assemble("org 0x400000\n_start:\n nop\nend:\n ret").unwrap();
        assert_eq!(out.origin, 0x400000);
        assert_eq!(out.labels["_start"], 0x400000);
        assert_eq!(out.labels["end"], 0x400001);
    }

    #[test]
    fn data_directives_emit_little_endian_bytes() {
        assert_eq!(asm("db 1, 2, 3"), [1, 2, 3]);
        assert_eq!(asm("dw 0x1234"), [0x34, 0x12]);
        assert_eq!(asm("dd 0x12345678"), [0x78, 0x56, 0x34, 0x12]);
        assert_eq!(asm("db \"hi\", 10, 0"), [b'h', b'i', 10, 0]);
        assert_eq!(asm("db 'A'"), [0x41]);
    }

    #[test]
    fn nasm_boilerplate_is_accepted_and_ignored() {
        let out = assemble("bits 64\nglobal _start\nsection .text\n_start:\n ret").unwrap();
        assert_eq!(out.bytes, [0xc3]);
    }

    #[test]
    fn comments_and_labels_can_share_a_line() {
        assert_eq!(asm("start: ret ; done"), [0xc3]);
    }

    #[test]
    fn segment_overrides_parse() {
        // The canonical stack-cookie load on Linux.
        let out = assemble("mov rax, qword fs:[0x28]").unwrap();
        let insn = crate::decode(&out.bytes, 0).unwrap();
        assert_eq!(insn.operands[1].as_mem().unwrap().seg, Some(Seg::Fs));
    }

    #[test]
    fn masm_style_ptr_keyword_is_accepted() {
        assert_eq!(asm("mov qword ptr [rax], 1"), asm("mov qword [rax], 1"));
    }

    #[test]
    fn undefined_labels_are_reported_with_a_line_number() {
        let e = assemble("nop\njmp nowhere").unwrap_err();
        assert_eq!(e.line, 2);
        assert!(matches!(e.kind, AsmErrorKind::UndefinedLabel(ref n) if n == "nowhere"));
    }

    #[test]
    fn unknown_mnemonics_are_reported_with_a_line_number() {
        let e = assemble("nop\nfrobnicate rax").unwrap_err();
        assert_eq!(e.line, 2);
        assert!(matches!(e.kind, AsmErrorKind::UnknownMnemonic(_)));
    }

    #[test]
    fn ambiguous_memory_writes_are_rejected() {
        let e = assemble("mov [rax], 1").unwrap_err();
        assert!(matches!(e.kind, AsmErrorKind::Encode(_)));
    }

    #[test]
    fn the_listing_pairs_source_with_bytes() {
        let out = assemble("nop\nret").unwrap();
        assert_eq!(out.lines.len(), 2);
        assert_eq!(out.lines[0].bytes, [0x90]);
        assert_eq!(out.lines[1].address, 1);
        assert!(out.listing().contains("ret"));
    }

    #[test]
    fn a_full_hello_world_assembles() {
        let src = r#"
            org 0x400000
        _start:
            mov eax, 1              ; __NR_write
            mov edi, 1              ; stdout
            lea rsi, [rip+msg]
            mov edx, 14
            syscall
            mov eax, 60             ; __NR_exit
            xor edi, edi
            syscall
        msg:
            db "hello, world", 10, 0
        "#;
        let out = assemble(src).unwrap();
        assert_eq!(out.origin, 0x400000);
        // The message really is at the label, reachable from the lea.
        let start = (out.labels["msg"] - 0x400000) as usize;
        assert_eq!(&out.bytes[start..start + 5], b"hello");
        // And every instruction decodes.
        let text = round_trip(src);
        assert!(text.iter().filter(|s| *s == "syscall").count() == 2);
    }
}
