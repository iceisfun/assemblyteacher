// Typed client for the Assembly Teacher REST API. Follows the server exactly
// (crates/server/src/routes/*). The one cross-cutting rule: every *machine word*
// — register value, address, immediate — is a `0x`-hex STRING on the wire (see
// src/core/word.ts). Executable *file* addresses in /binfmt/inspect are the
// deliberate exception and stay JSON numbers, because they are always < 2^53.

import type { Word } from "./core/word.ts";

// ---------------------------------------------------------------------------
// Errors — body is { error, kind, line? }
// ---------------------------------------------------------------------------

export type ErrorKind =
  | "assemble"
  | "decode"
  | "hex"
  | "binfmt"
  | "request"
  | "not_found"
  | "too_large"
  | string;

export interface ApiErrorBody {
  error: string;
  kind: ErrorKind;
  line?: number;
}

/** Thrown by request() for any non-2xx response or transport failure. */
export class ApiError extends Error {
  readonly status: number;
  readonly line?: number;
  readonly kind?: ErrorKind;

  constructor(message: string, status: number, kind?: ErrorKind, line?: number) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    if (kind !== undefined) this.kind = kind;
    if (line !== undefined) this.line = line;
  }
}

function isErrorBody(v: unknown): v is ApiErrorBody {
  return (
    typeof v === "object" &&
    v !== null &&
    typeof (v as { error?: unknown }).error === "string"
  );
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let res: Response;
  try {
    res = await fetch(path, init);
  } catch (e) {
    throw new ApiError(
      `network error contacting ${path}: ${(e as Error).message}`,
      0,
    );
  }
  const text = await res.text();
  let body: unknown = undefined;
  if (text.length > 0) {
    try {
      body = JSON.parse(text);
    } catch {
      body = undefined;
    }
  }
  if (!res.ok) {
    if (isErrorBody(body)) {
      throw new ApiError(body.error, res.status, body.kind, body.line);
    }
    throw new ApiError(
      text || `request to ${path} failed with ${res.status}`,
      res.status,
    );
  }
  return body as T;
}

function postJson<T>(path: string, payload: unknown): Promise<T> {
  return request<T>(path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(payload),
  });
}

// ---------------------------------------------------------------------------
// Shared shapes
// ---------------------------------------------------------------------------

/** The seven emulated flags (note `df`, added by the server). */
export interface Flags {
  cf: boolean;
  pf: boolean;
  af: boolean;
  zf: boolean;
  sf: boolean;
  of: boolean;
  df: boolean;
}

/** Register file: name -> word. Values are `0x…` strings. */
export type Registers = Record<string, Word>;

/** One mapped memory region with its contents. `perms` is `"r-x"`-style. */
export interface Region {
  base: Word;
  name: string;
  perms: string;
  hex: string;
  truncated: boolean;
}

// ---------------------------------------------------------------------------
// GET /api/health
// ---------------------------------------------------------------------------

export interface Health {
  status: string;
  version: string;
}

export function health(): Promise<Health> {
  return request<Health>("/api/health");
}

// ---------------------------------------------------------------------------
// POST /api/asm/assemble
// ---------------------------------------------------------------------------

export interface AssembleRequest {
  source: string;
  origin?: Word | number;
}

export interface AssembledLine {
  line: number;
  address: Word;
  hex: string;
  text: string;
}

export interface AssembleResponse {
  hex: string;
  origin: Word;
  labels: Record<string, Word>;
  lines: AssembledLine[];
}

export function assemble(req: AssembleRequest): Promise<AssembleResponse> {
  return postJson<AssembleResponse>("/api/asm/assemble", req);
}

// ---------------------------------------------------------------------------
// POST /api/asm/disassemble
// ---------------------------------------------------------------------------

export interface DisassembleRequest {
  hex: string;
  base?: Word | number;
}

export interface DisassembledInsn {
  ip: Word;
  hex: string;
  text: string;
  description: string;
  length: number;
  mnemonic: string;
  branchTarget: Word | null;
  fallsThrough: boolean;
}

export interface DisassembleResponse {
  instructions: DisassembledInsn[];
  error: string | null;
}

export function disassemble(
  req: DisassembleRequest,
): Promise<DisassembleResponse> {
  return postJson<DisassembleResponse>("/api/asm/disassemble", req);
}

// ---------------------------------------------------------------------------
// POST /api/asm/explain
// ---------------------------------------------------------------------------

export interface ExplainRequest {
  hex: string;
}

export interface ExplainField {
  name: string;
  hex: string;
  offset: number;
  explanation: string;
}

export interface ExplainResponse {
  text: string;
  length: number;
  fields: ExplainField[];
}

export function explain(req: ExplainRequest): Promise<ExplainResponse> {
  return postJson<ExplainResponse>("/api/asm/explain", req);
}

// ---------------------------------------------------------------------------
// Trace / state (shared by run and step)
// ---------------------------------------------------------------------------

export interface RegWrite {
  reg: string;
  before: Word;
  after: Word;
}

export interface MemWrite {
  addr: Word;
  before: string; // hex bytes prior to the write
  after: string; // hex bytes written
}

export interface MemRead {
  addr: Word;
  hex: string;
}

export interface Syscall {
  number: Word;
  name: string;
  args: Word[];
  result: Word;
}

export interface TraceEntry {
  ip: Word;
  text: string;
  hex: string; // the instruction's own bytes
  regWrites: RegWrite[];
  memWrites: MemWrite[];
  memReads: MemRead[];
  flagsBefore: Flags;
  flagsAfter: Flags;
  syscall?: Syscall;
}

export type StopKind =
  | "halted"
  | "exited"
  | "stepLimit"
  | "fault"
  | "breakpoint";

export interface Stop {
  kind: StopKind;
  code?: number; // exited
  reason?: string; // fault
  address?: Word; // fault, breakpoint
}

/** Machine state. `memory` is present on /step; omitted from /run's `final`. */
export interface State {
  registers: Registers;
  rip: Word;
  flags: Flags;
  memory?: Region[];
}

// ---------------------------------------------------------------------------
// POST /api/emu/run
// ---------------------------------------------------------------------------

export interface RunRequest {
  source?: string;
  hex?: string;
  base?: Word | number;
  maxSteps?: number;
}

export interface RunResponse {
  stop: Stop;
  steps: number;
  stdout: string;
  stderr: string;
  base: Word;
  final: State;
  trace: TraceEntry[];
  traceTruncated: boolean;
  regions: Region[];
}

export function run(req: RunRequest): Promise<RunResponse> {
  return postJson<RunResponse>("/api/emu/run", req);
}

// ---------------------------------------------------------------------------
// POST /api/emu/step — fully stateless and symmetric
// ---------------------------------------------------------------------------

/** The state a /step request carries; `memory` is required here. */
export interface StepState extends State {
  memory: Region[];
}

export interface StepRequest {
  state: StepState;
}

export interface StepResponse {
  step?: TraceEntry; // absent when the CPU stopped instead of stepping
  stop?: Stop; // present on halt/exit/breakpoint/fault
  state: StepState;
  stdout: string;
}

export function step(req: StepRequest): Promise<StepResponse> {
  return postJson<StepResponse>("/api/emu/step", req);
}

// ---------------------------------------------------------------------------
// POST /api/binfmt/inspect — addresses are JSON NUMBERS here (all < 2^53).
// The image serialises in snake_case (no camelCase rename on these structs).
// ---------------------------------------------------------------------------

export type BinFormat = "elf" | "pe";

/** Arch serialises as the raw enum name, e.g. "X86_64", or {Other: n}. */
export type BinArch = string | { Other: number };

/** rwx permission triad as an object (drives both sections and segments). */
export interface SectionFlags {
  alloc: boolean;
  write: boolean;
  execute: boolean;
}

export interface BinSection {
  name: string;
  address: number;
  size: number;
  file_offset: number;
  file_size: number;
  flags: SectionFlags;
}

export interface BinSegment {
  kind: string;
  vaddr: number;
  filesz: number;
  memsz: number;
  perms: SectionFlags;
  offset: number;
}

/** kind/binding are lowercase strings, or {other: n} for unclassified values. */
export type SymbolKind = string | { other: number };
export type SymbolBinding = string | { other: number };

export interface BinSymbol {
  name: string;
  address: number;
  size: number;
  kind: SymbolKind;
  binding: SymbolBinding;
  section: string | null;
}

export interface BinImport {
  name: string;
  library: string | null;
  kind: string; // "function" | "data" | "unknown"
  ordinal: number | null;
  iat_address: number | null;
}

export interface BinExport {
  name: string;
  address: number;
  ordinal: number | null;
  /** "OTHERDLL.Symbol" when this export forwards elsewhere. */
  forwarder: string | null;
}

export interface BinRelocation {
  offset: number;
  kind: string;
  symbol: string | null;
  addend: number;
}

export type Relro = "none" | "partial" | "full";

export interface BinMitigations {
  nx: boolean;
  pie: boolean;
  relro: Relro | null; // null on PE (does not apply)
  bind_now: boolean;
  stack_canary: boolean;
  aslr: boolean;
  cfg: boolean;
  cet: boolean;
}

export interface InspectResponse {
  format: BinFormat;
  arch: BinArch;
  entry: number;
  image_base: number;
  is_pie: boolean;
  sections: BinSection[];
  segments: BinSegment[];
  symbols: BinSymbol[];
  imports: BinImport[];
  exports: BinExport[];
  relocations: BinRelocation[];
  mitigations: BinMitigations;
}

/** Inspect via multipart upload of a File (the drag-and-drop path). */
export function inspectFile(file: File): Promise<InspectResponse> {
  const form = new FormData();
  form.append("file", file);
  return request<InspectResponse>("/api/binfmt/inspect", {
    method: "POST",
    body: form,
  });
}

/** Inspect a small input supplied as hex. */
export function inspectHex(hex: string): Promise<InspectResponse> {
  return postJson<InspectResponse>("/api/binfmt/inspect", { hex });
}

// ---------------------------------------------------------------------------
// Lessons. NOTE: the index (LessonSummary) is camelCase; the lesson DETAIL
// response is snake_case (estimated_minutes), matching the server structs.
// ---------------------------------------------------------------------------

export interface LessonSummary {
  id: string;
  title: string;
  order: number;
  objectives: string[];
  prerequisites: string[];
  estimatedMinutes?: number;
  exerciseCount: number;
}

export interface LessonPart {
  number: number;
  title: string;
  lessons: LessonSummary[];
}

export interface LessonIndex {
  parts: LessonPart[];
}

export function lessons(): Promise<LessonIndex> {
  return request<LessonIndex>("/api/lessons");
}

export type ExampleLanguage = "asm" | "c" | "rust" | "other";

export interface LessonExample {
  name: string;
  language: ExampleLanguage;
  source: string;
}

export type ExerciseKind = "quiz" | "assemble" | "disassemble" | "emulate";

/** Public exercise: the `kind` tag is flattened, plus one kind-specific field. */
export interface Exercise {
  id: string;
  prompt: string;
  hints: string[];
  kind: ExerciseKind;
  choices?: string[]; // quiz
  starter?: string; // assemble, emulate
  hex?: string; // disassemble (the bytes to read)
}

export interface Lesson {
  id: string;
  title: string;
  order: number;
  part: number;
  estimated_minutes?: number;
  objectives: string[];
  prerequisites: string[];
  body: string; // markdown
  examples: LessonExample[];
  exercises: Exercise[];
}

export function lesson(id: string): Promise<Lesson> {
  return request<Lesson>(`/api/lessons/${encodeURIComponent(id)}`);
}

export interface CheckRequest {
  /** Always a string: the choice index ("0","1",…) for quizzes, source else. */
  answer: string;
}

export interface CheckResponse {
  correct: boolean;
  message: string;
  hints: string[];
}

export function checkExercise(
  lessonId: string,
  exerciseId: string,
  req: CheckRequest,
): Promise<CheckResponse> {
  return postJson<CheckResponse>(
    `/api/lessons/${encodeURIComponent(lessonId)}/exercises/${encodeURIComponent(
      exerciseId,
    )}/check`,
    req,
  );
}
