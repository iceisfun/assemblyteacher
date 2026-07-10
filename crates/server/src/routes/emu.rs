//! Run and single-step programs.
//!
//! `/api/emu/step` is deliberately **stateless**: the client sends the machine
//! state and receives the next one. No sessions, no expiry, no server-side
//! garbage to collect, and any point in an execution can be captured in a URL
//! and shared. The state is small — sixteen registers, seven flags, and the
//! mapped regions.

use crate::error::{ApiError, ApiResult};
use crate::hexnum::{from_hex, to_hex, U64};
use asm_emu::{Cpu, Effects, Fault, Flags, Memory, Perms, Stop};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Hard ceilings, tuned for a public deployment. A browser tab — or a bored
/// attacker — should not be able to make the server spin or exhaust its memory.
///
/// The step cap bounds CPU time: each interpreted step is O(1), so half a
/// million of them is tens of milliseconds, and the emulator runs on a blocking
/// thread (see `run`) so even that does not stall the async runtime. A teaching
/// program never approaches this; the recursive-factorial exercise is a few
/// thousand steps.
const MAX_STEPS: u64 = 500_000;
const DEFAULT_STEPS: u64 = 50_000;
/// The trace is the bulk of a response and is bounded independently of the step
/// count: a program may run for `MAX_STEPS` but we never buffer or serialise
/// more than this many effect records. The scrubber in the UI shows exactly
/// this many.
const MAX_TRACE: usize = 10_000;
/// Per-region cap when shipping memory back to the viewer, and when accepting a
/// region on `/step`.
const MAX_REGION_BYTES: usize = 256 * 1024;
/// How many regions a `/step` request may describe. The request body limit
/// already bounds this indirectly; the explicit cap is defence in depth.
const MAX_REGIONS: usize = 64;

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Default, Clone, Copy)]
pub struct FlagsDto {
    pub cf: bool,
    pub pf: bool,
    pub af: bool,
    pub zf: bool,
    pub sf: bool,
    pub of: bool,
    pub df: bool,
}

impl From<Flags> for FlagsDto {
    fn from(f: Flags) -> FlagsDto {
        FlagsDto { cf: f.cf, pf: f.pf, af: f.af, zf: f.zf, sf: f.sf, of: f.of, df: f.df }
    }
}

impl From<FlagsDto> for Flags {
    fn from(f: FlagsDto) -> Flags {
        Flags { cf: f.cf, pf: f.pf, af: f.af, zf: f.zf, sf: f.sf, of: f.of, df: f.df }
    }
}

/// One mapped region, with its contents. `perms` is `"r-x"`-style.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RegionDto {
    pub base: U64,
    pub name: String,
    pub perms: String,
    pub hex: String,
    /// True when `hex` holds only a prefix of the region because it was large.
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StateDto {
    /// Register name (`"rax"`) to value.
    pub registers: BTreeMap<String, U64>,
    pub rip: U64,
    pub flags: FlagsDto,
    /// Present on `/step` requests and responses; omitted from `/run`'s
    /// `final`, where the regions are reported once at the top level.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub memory: Vec<RegionDto>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegWriteDto {
    pub reg: &'static str,
    pub before: U64,
    pub after: U64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemWriteDto {
    pub addr: U64,
    pub before: String,
    pub after: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemReadDto {
    pub addr: U64,
    pub hex: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyscallDto {
    pub number: U64,
    pub name: String,
    pub args: Vec<U64>,
    pub result: U64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceEntry {
    pub ip: U64,
    pub text: String,
    pub hex: String,
    pub reg_writes: Vec<RegWriteDto>,
    pub mem_writes: Vec<MemWriteDto>,
    pub mem_reads: Vec<MemReadDto>,
    pub flags_before: FlagsDto,
    pub flags_after: FlagsDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syscall: Option<SyscallDto>,
}

impl From<Effects> for TraceEntry {
    fn from(e: Effects) -> TraceEntry {
        TraceEntry {
            ip: U64(e.rip_before),
            text: asm_core::format::to_string(&e.insn),
            hex: to_hex(&e.insn.bytes()),
            reg_writes: e
                .reg_writes
                .into_iter()
                .map(|w| RegWriteDto { reg: w.reg, before: U64(w.before), after: U64(w.after) })
                .collect(),
            mem_writes: e
                .mem_writes
                .into_iter()
                .map(|w| MemWriteDto {
                    addr: U64(w.addr),
                    before: to_hex(&w.before),
                    after: to_hex(&w.after),
                })
                .collect(),
            mem_reads: e
                .mem_reads
                .into_iter()
                .map(|r| MemReadDto { addr: U64(r.addr), hex: to_hex(&r.bytes) })
                .collect(),
            flags_before: e.flags_before.into(),
            flags_after: e.flags_after.into(),
            syscall: e.syscall.map(|s| SyscallDto {
                number: U64(s.number),
                name: s.name,
                args: s.args.iter().copied().map(U64).collect(),
                result: U64(s.result),
            }),
        }
    }
}

/// `{"kind": "fault", "reason": "...", "address": "0x2222"}`
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StopDto {
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub address: Option<U64>,
}

fn fault_address(f: &Fault) -> Option<u64> {
    match f {
        Fault::NotMapped { addr, .. }
        | Fault::Permission { addr, .. }
        | Fault::Misaligned { addr, .. } => Some(*addr),
        _ => None,
    }
}

impl From<Stop> for StopDto {
    fn from(s: Stop) -> StopDto {
        let mut dto = StopDto { kind: "halted", code: None, reason: None, address: None };
        match s {
            Stop::Halted => {}
            Stop::Exited(code) => {
                dto.kind = "exited";
                dto.code = Some(code);
            }
            Stop::StepLimit => dto.kind = "stepLimit",
            Stop::Breakpoint(addr) => {
                dto.kind = "breakpoint";
                dto.address = Some(U64(addr));
            }
            Stop::Fault(f) => {
                dto.kind = "fault";
                dto.address = fault_address(&f).map(U64);
                dto.reason = Some(f.to_string());
            }
        }
        dto
    }
}

// ---------------------------------------------------------------------------
// run
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRequest {
    /// Intel-syntax source. Assembled at `base` before running.
    #[serde(default)]
    pub source: Option<String>,
    /// Machine code, if you already have it. Mutually exclusive with `source`.
    #[serde(default)]
    pub hex: Option<String>,
    #[serde(default)]
    pub base: Option<U64>,
    #[serde(default)]
    pub max_steps: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunResponse {
    pub stop: StopDto,
    pub steps: u64,
    pub stdout: String,
    pub stderr: String,
    pub base: U64,
    #[serde(rename = "final")]
    pub final_state: StateDto,
    pub trace: Vec<TraceEntry>,
    /// True when the trace was cut short; the run itself still completed.
    pub trace_truncated: bool,
    pub regions: Vec<RegionDto>,
}

const DEFAULT_BASE: u64 = 0x1000;

/// Assemble `source`, or decode `hex`, into machine code at `base`.
fn program(req: &RunRequest) -> ApiResult<(Vec<u8>, u64)> {
    match (&req.source, &req.hex) {
        (Some(_), Some(_)) => Err(ApiError::bad("request", "give `source` or `hex`, not both")),
        (Some(src), None) => {
            let base = req.base.unwrap_or(U64(DEFAULT_BASE)).0;
            // Assemble *at* the load address so absolute label references
            // resolve to where the bytes actually land. An `org` in the source
            // overrides it, and `origin` follows.
            let out = asm_core::asm::assemble_at(src, base)?;
            Ok((out.bytes, out.origin))
        }
        (None, Some(hex)) => {
            let bytes = from_hex(hex)
                .ok_or_else(|| ApiError::bad("hex", "`hex` is not a hex byte string"))?;
            Ok((bytes, req.base.unwrap_or(U64(DEFAULT_BASE)).0))
        }
        (None, None) => Err(ApiError::bad("request", "one of `source` or `hex` is required")),
    }
}

fn regions_of(cpu: &Cpu) -> Vec<RegionDto> {
    cpu.mem
        .regions()
        .iter()
        .map(|r| {
            let truncated = r.data.len() > MAX_REGION_BYTES;
            let slice = &r.data[..r.data.len().min(MAX_REGION_BYTES)];
            RegionDto {
                base: U64(r.base),
                name: r.name.clone(),
                perms: r.perms.to_string(),
                hex: to_hex(slice),
                truncated,
            }
        })
        .collect()
}

fn state_of(cpu: &Cpu) -> StateDto {
    StateDto {
        registers: cpu.regs.iter_named().map(|(n, v)| (n.to_string(), U64(v))).collect(),
        rip: U64(cpu.rip),
        flags: cpu.flags.into(),
        memory: Vec::new(),
    }
}

pub async fn run(Json(req): Json<RunRequest>) -> ApiResult<Json<RunResponse>> {
    let (code, base) = program(&req)?;
    if code.is_empty() {
        return Err(ApiError::bad("request", "the program is empty"));
    }

    let max_steps = req.max_steps.unwrap_or(DEFAULT_STEPS).min(MAX_STEPS);

    // Interpretation is synchronous CPU work. Run it on a blocking thread so a
    // long program never stalls an async worker that other requests need, and
    // bound the trace so memory stays proportional to what we return, not to
    // how many steps the program ran.
    let (outcome, cpu) = tokio::task::spawn_blocking(move || {
        let mut cpu = Cpu::with_code(&code, base);
        let outcome = cpu.run_bounded(max_steps, MAX_TRACE);
        (outcome, cpu)
    })
    .await
    .map_err(|_| ApiError::internal("the emulation task panicked"))?;

    // The program ran further than we recorded when its step count outran the
    // trace cap.
    let trace_truncated = outcome.steps > outcome.trace.len() as u64;
    let trace: Vec<TraceEntry> = outcome.trace.into_iter().map(TraceEntry::from).collect();

    Ok(Json(RunResponse {
        stop: outcome.stop.into(),
        steps: outcome.steps,
        stdout: String::from_utf8_lossy(cpu.stdout()).into_owned(),
        stderr: String::from_utf8_lossy(cpu.stderr()).into_owned(),
        base: U64(base),
        final_state: state_of(&cpu),
        trace,
        trace_truncated,
        regions: regions_of(&cpu),
    }))
}

// ---------------------------------------------------------------------------
// step
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepRequest {
    pub state: StateDto,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepResponse {
    /// Absent when the CPU stopped instead of executing an instruction.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<TraceEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<StopDto>,
    pub state: StateDto,
    pub stdout: String,
}

fn parse_perms(s: &str) -> Perms {
    let mut p = Perms::NONE;
    if s.contains('r') {
        p = Perms(p.bits() | Perms::R.bits());
    }
    if s.contains('w') {
        p = Perms(p.bits() | Perms::W.bits());
    }
    if s.contains('x') {
        p = Perms(p.bits() | Perms::X.bits());
    }
    p
}

fn cpu_from_state(state: &StateDto) -> ApiResult<Cpu> {
    if state.memory.is_empty() {
        return Err(ApiError::bad("request", "`state.memory` must describe at least one region"));
    }
    if state.memory.len() > MAX_REGIONS {
        return Err(ApiError::too_large(format!(
            "a state may describe at most {MAX_REGIONS} memory regions"
        )));
    }

    let mut mem = Memory::new();
    for region in &state.memory {
        let bytes = from_hex(&region.hex).ok_or_else(|| {
            ApiError::bad("hex", format!("region `{}` has a bad hex payload", region.name))
        })?;
        if bytes.len() > MAX_REGION_BYTES {
            return Err(ApiError::too_large(format!("region `{}` is too large", region.name)));
        }
        mem.map_with(region.base.0, bytes, parse_perms(&region.perms), &region.name);
    }

    let mut cpu = Cpu::new(mem);
    cpu.rip = state.rip.0;
    cpu.flags = state.flags.into();

    for (name, value) in &state.registers {
        let reg = asm_core::Reg::parse(name)
            .ok_or_else(|| ApiError::bad("request", format!("`{name}` is not a register")))?;
        if reg.size != asm_core::Size::Qword {
            return Err(ApiError::bad(
                "request",
                format!("name registers at full width, not `{name}`"),
            ));
        }
        cpu.regs.write(reg, value.0);
    }
    Ok(cpu)
}

pub async fn step(Json(req): Json<StepRequest>) -> ApiResult<Json<StepResponse>> {
    let mut cpu = cpu_from_state(&req.state)?;

    let (step, stop) = match cpu.step() {
        Ok(effects) => {
            // A terminating instruction still produces an effect. Ask the CPU
            // whether it stopped rather than guessing from the mnemonic —
            // `exit` is a syscall, and looks like any other `syscall`.
            let stop = cpu.stopped().cloned().map(StopDto::from);
            (Some(TraceEntry::from(effects)), stop)
        }
        Err(f) => (None, Some(StopDto::from(Stop::Fault(f)))),
    };

    let mut state = state_of(&cpu);
    state.memory = regions_of(&cpu);

    Ok(Json(StepResponse {
        step,
        stop,
        state,
        stdout: String::from_utf8_lossy(cpu.stdout()).into_owned(),
    }))
}
