// Typed client for the Assembly Teacher REST API. Mirrors docs/api.md exactly.
// Every response interface here corresponds to a documented JSON shape; do not
// invent fields. The single request() helper turns the {error, line, kind}
// error body into a typed ApiError.

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/** The documented error body: `{ error, line?, kind? }`. */
export interface ApiErrorBody {
  error: string;
  line?: number;
  kind?: string;
}

/** Thrown by request() for any non-2xx response or transport failure. */
export class ApiError extends Error {
  readonly status: number;
  readonly line?: number;
  readonly kind?: string;

  constructor(message: string, status: number, line?: number, kind?: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    if (line !== undefined) this.line = line;
    if (kind !== undefined) this.kind = kind;
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
      throw new ApiError(body.error, res.status, body.line, body.kind);
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

export interface Flags {
  zf: boolean;
  cf: boolean;
  sf: boolean;
  of: boolean;
  pf: boolean;
  af: boolean;
}

/** Register file as a name->u64 map. u64 values may exceed Number precision. */
export type Registers = Record<string, number>;

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
  origin?: number;
}

export interface AssembledLine {
  line: number;
  address: number;
  hex: string;
  text: string;
}

export interface AssembleResponse {
  hex: string;
  origin: number;
  labels: Record<string, number>;
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
  base?: number;
}

export interface DisassembledInsn {
  ip: number;
  hex: string;
  text: string;
  description: string;
  length: number;
  mnemonic: string;
  branchTarget: number | null;
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
// POST /api/emu/run
// ---------------------------------------------------------------------------

export interface RunRequest {
  source?: string;
  hex?: string;
  maxSteps?: number;
  stdin?: string;
  base?: number;
}

export interface RegWrite {
  reg: string;
  before: number;
  after: number;
}

export interface MemWrite {
  addr: number;
  bytes: string;
}

export interface TraceEntry {
  ip: number;
  text: string;
  regWrites: RegWrite[];
  memWrites: MemWrite[];
  flagsAfter: Flags;
}

export type StopKind =
  | "halted"
  | "exited"
  | "stepLimit"
  | "fault"
  | "breakpoint";

export interface Stop {
  kind: StopKind;
  code?: number;
  reason?: string;
  address?: number;
}

export interface MachineState {
  registers: Registers;
  rip: number;
  flags: Flags;
}

export interface RunResponse {
  stop: Stop;
  steps: number;
  stdout: string;
  final: MachineState;
  trace: TraceEntry[];
}

export function run(req: RunRequest): Promise<RunResponse> {
  return postJson<RunResponse>("/api/emu/run", req);
}

// ---------------------------------------------------------------------------
// POST /api/emu/step
// ---------------------------------------------------------------------------

/** A memory image chunk. `[address, hexBytes]` per docs' `memory: [...]`. */
export interface MemoryChunk {
  address: number;
  bytes: string;
}

export interface StepState extends MachineState {
  memory: MemoryChunk[];
}

export interface StepRequest {
  hex: string;
  base?: number;
  state: StepState;
}

export interface StepResponse {
  trace: TraceEntry;
  state: StepState;
}

export function step(req: StepRequest): Promise<StepResponse> {
  return postJson<StepResponse>("/api/emu/step", req);
}

// ---------------------------------------------------------------------------
// POST /api/binfmt/inspect
// ---------------------------------------------------------------------------

export interface BinSection {
  name: string;
  address: number;
  size: number;
  offset: number;
  flags: string[];
}

export interface BinSegment {
  type: string;
  vaddr: number;
  memsz: number;
  perms: string;
}

export interface BinSymbol {
  name: string;
  address: number;
  size: number;
  kind: string;
  binding: string;
}

export interface BinImport {
  name: string;
  library: string;
  kind: string;
}

export interface BinExport {
  name: string;
  address: number;
  kind?: string;
}

export interface BinRelocation {
  offset: number;
  kind: string;
  symbol: string;
  addend: number;
}

/**
 * Mitigations panel. Not shown in the api.md example but described in the
 * Inspector requirements (NX/PIE/RELRO/canary/CFG/CET). Treated as optional so
 * a server that omits it still parses.
 */
export interface BinMitigations {
  nx?: boolean;
  pie?: boolean;
  relro?: "full" | "partial" | "none" | string;
  canary?: boolean;
  cfg?: boolean;
  cet?: boolean;
}

export interface InspectResponse {
  format: string;
  arch: string;
  entry: number;
  sections: BinSection[];
  segments: BinSegment[];
  symbols: BinSymbol[];
  imports: BinImport[];
  exports: BinExport[];
  relocations: BinRelocation[];
  mitigations?: BinMitigations;
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
// Lessons
// ---------------------------------------------------------------------------

export interface LessonSummary {
  id: string;
  title: string;
  order: number;
  objectives: string[];
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

export type ExerciseKind = "quiz" | "assemble" | "emulate";

export interface QuizChoice {
  index: number;
  text: string;
}

export interface Exercise {
  id: string;
  kind: ExerciseKind;
  prompt: string;
  /** Present for quiz exercises. */
  choices?: QuizChoice[];
  /** Optional starter source for assemble/emulate exercises. */
  starter?: string;
}

export interface LessonExample {
  title?: string;
  source?: string;
  hex?: string;
  description?: string;
}

export interface Lesson {
  id: string;
  title: string;
  order: number;
  objectives: string[];
  /** Rendered markdown body of the lesson. */
  body: string;
  examples: LessonExample[];
  exercises: Exercise[];
}

export function lesson(id: string): Promise<Lesson> {
  return request<Lesson>(`/api/lessons/${encodeURIComponent(id)}`);
}

export interface CheckRequest {
  /** Source, hex, or the chosen index (as a string) depending on kind. */
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
