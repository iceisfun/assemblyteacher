//! End-to-end tests over the real router.
//!
//! These drive the application through `tower`'s `oneshot`, so they exercise
//! routing, extractors, serialisation and status codes without binding a port.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use server::{app, AppState};
use std::sync::Arc;
use tower::ServiceExt;

fn state() -> Arc<AppState> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lessons");
    let curriculum = lesson::load(dir).expect("lessons load");
    Arc::new(AppState { curriculum: Box::leak(Box::new(curriculum)), web_dir: None })
}

async fn call(method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder().method(method).uri(uri);
    let request = match body {
        Some(v) => request
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&v).unwrap()))
            .unwrap(),
        None => request.body(Body::empty()).unwrap(),
    };

    let response = app(state()).oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

async fn post(uri: &str, body: Value) -> (StatusCode, Value) {
    call("POST", uri, Some(body)).await
}

async fn get(uri: &str) -> (StatusCode, Value) {
    call("GET", uri, None).await
}

#[tokio::test]
async fn health_reports_ok() {
    let (status, body) = get("/api/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn assemble_returns_bytes_and_a_line_map() {
    let (status, body) = post("/api/asm/assemble", json!({"source": "mov rax, 1\nret"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["hex"], "48c7c001000000c3");
    assert_eq!(body["lines"][0]["line"], 1);
    assert_eq!(body["lines"][0]["address"], "0x0");
    assert_eq!(body["lines"][1]["address"], "0x7");
    assert_eq!(body["lines"][1]["hex"], "c3");
}

#[tokio::test]
async fn assemble_reports_the_offending_line() {
    let (status, body) = post("/api/asm/assemble", json!({"source": "nop\nfrobnicate"})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["line"], 2);
    assert_eq!(body["kind"], "assemble");
    assert!(body["error"].as_str().unwrap().contains("frobnicate"));
}

#[tokio::test]
async fn disassemble_resolves_addresses_against_the_base() {
    let (status, body) =
        post("/api/asm/disassemble", json!({"hex": "488b442408", "base": "0x401000"})).await;
    assert_eq!(status, StatusCode::OK);
    let insn = &body["instructions"][0];
    assert_eq!(insn["ip"], "0x401000");
    assert_eq!(insn["text"], "mov rax, qword [rsp+0x8]");
    assert_eq!(insn["length"], 5);
    assert!(body["error"].is_null());
}

#[tokio::test]
async fn disassemble_returns_what_it_decoded_before_it_derailed() {
    // `90` is nop; `06` is invalid in 64-bit mode.
    let (status, body) = post("/api/asm/disassemble", json!({"hex": "9006"})).await;
    assert_eq!(status, StatusCode::OK, "a partial answer is still an answer");
    assert_eq!(body["instructions"].as_array().unwrap().len(), 1);
    assert!(body["error"].as_str().unwrap().contains("64-bit"));
}

#[tokio::test]
async fn explain_breaks_an_instruction_into_its_fields() {
    let (status, body) = post("/api/asm/explain", json!({"hex": "488b442408"})).await;
    assert_eq!(status, StatusCode::OK);
    let fields = body["fields"].as_array().unwrap();
    let names: Vec<&str> = fields.iter().map(|f| f["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["REX", "opcode", "ModRM", "SIB", "displacement"]);
    // Offsets must walk forward through the instruction.
    assert_eq!(fields[0]["offset"], 0);
    assert_eq!(fields[4]["offset"], 4);
    assert!(fields[0]["explanation"].as_str().unwrap().contains("W=1"));
}

#[tokio::test]
async fn run_executes_a_program_and_traces_every_effect() {
    let (status, body) =
        post("/api/emu/run", json!({"source": "mov eax, 1\nadd eax, 2\nhlt"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["stop"]["kind"], "halted");
    assert_eq!(body["final"]["registers"]["rax"], "0x3");
    let trace = body["trace"].as_array().unwrap();
    assert_eq!(trace.len(), 3);
    assert_eq!(trace[0]["regWrites"][0]["reg"], "rax");
    assert_eq!(trace[0]["regWrites"][0]["before"], "0x0");
    assert_eq!(trace[0]["regWrites"][0]["after"], "0x1");
}

/// The reason machine words are hex strings on the wire. As a JSON number,
/// `0xffffffffffffffff` would arrive as 18446744073709552000.
#[tokio::test]
async fn a_full_width_register_value_survives_the_round_trip() {
    let (status, body) = post("/api/emu/run", json!({"source": "mov rax, -1\nhlt"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["final"]["registers"]["rax"], "0xffffffffffffffff");
}

#[tokio::test]
async fn run_reports_a_fault_with_its_address_rather_than_failing() {
    // Push a value, never pop it, then `ret` into it.
    let source = "call f\nhlt\nf:\nmov rax, 0x2222\npush rax\nret";
    let (status, body) = post("/api/emu/run", json!({"source": source})).await;
    assert_eq!(status, StatusCode::OK, "a guest fault is not a server error");
    assert_eq!(body["stop"]["kind"], "fault");
    assert_eq!(body["stop"]["address"], "0x2222");
}

#[tokio::test]
async fn run_stops_a_program_that_never_terminates() {
    let (status, body) =
        post("/api/emu/run", json!({"source": "here:\njmp here", "maxSteps": 50})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["stop"]["kind"], "stepLimit");
    assert_eq!(body["steps"], 50);
}

#[tokio::test]
async fn run_captures_syscall_output_and_exit_status() {
    let source = r#"
        mov eax, 1
        mov edi, 1
        lea rsi, [rip+msg]
        mov edx, 2
        syscall
        mov eax, 60
        mov edi, 7
        syscall
    msg:
        db "hi"
    "#;
    let (status, body) = post("/api/emu/run", json!({"source": source})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["stdout"], "hi");
    assert_eq!(body["stop"]["kind"], "exited");
    assert_eq!(body["stop"]["code"], 7);
}

#[tokio::test]
async fn run_rejects_a_request_with_neither_source_nor_hex() {
    let (status, body) = post("/api/emu/run", json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"].as_str().unwrap().contains("required"));
}

/// `/emu/step` is stateless: feed it the state it gave you, and it continues.
#[tokio::test]
async fn step_round_trips_its_own_state() {
    let (_, run) = post("/api/emu/run", json!({"source": "mov eax, 1\nadd eax, 2\nhlt"})).await;

    let mut state = json!({
        "registers": run["final"]["registers"],
        "rip": run["base"],
        "flags": {"cf": false, "pf": false, "af": false, "zf": false, "sf": false, "of": false, "df": false},
        "memory": run["regions"],
    });
    // Start from a clean rax so the first step's write is observable.
    state["registers"]["rax"] = json!("0x0");

    let (status, first) = post("/api/emu/step", json!({"state": state})).await;
    assert_eq!(status, StatusCode::OK, "{first}");
    assert_eq!(first["step"]["text"], "mov eax, 0x1");
    assert_eq!(first["state"]["registers"]["rax"], "0x1");

    // Feed the returned state straight back in.
    let (status, second) = post("/api/emu/step", json!({"state": first["state"]})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(second["step"]["text"], "add eax, 0x2");
    assert_eq!(second["state"]["registers"]["rax"], "0x3");

    let (_, third) = post("/api/emu/step", json!({"state": second["state"]})).await;
    assert_eq!(third["stop"]["kind"], "halted", "the `hlt` must be reported as a stop");
}

#[tokio::test]
async fn lessons_index_lists_the_curriculum_in_order() {
    let (status, body) = get("/api/lessons").await;
    assert_eq!(status, StatusCode::OK);
    let parts = body["parts"].as_array().unwrap();
    assert!(!parts.is_empty());
    assert_eq!(parts[0]["number"], 1);
    let first = &parts[0]["lessons"][0];
    assert_eq!(first["id"], "binary-and-hexadecimal");
    assert!(first["exerciseCount"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn a_lesson_is_served_without_its_answers() {
    let (status, body) = get("/api/lessons/registers").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["title"], "Registers");
    assert!(body["body"].as_str().unwrap().contains("zero-extend"));

    let text = serde_json::to_string(&body).unwrap();
    assert!(!text.contains("\"answer\""), "the quiz answer key leaked");
    assert!(!text.contains("\"solution\""), "the reference solution leaked");
    assert!(!text.contains("expectHex") && !text.contains("expect_hex"));
}

#[tokio::test]
async fn an_unknown_lesson_is_a_404() {
    let (status, body) = get("/api/lessons/nonexistent").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["kind"], "not_found");
}

#[tokio::test]
async fn grading_an_assemble_exercise_accepts_any_correct_encoding() {
    let uri = "/api/lessons/registers/exercises/a-zero-eax/check";
    let (status, body) = post(uri, json!({"answer": "xor eax, eax"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["correct"], true);

    // Right effect, wrong bytes — the exercise asked for the two-byte form.
    let (_, body) = post(uri, json!({"answer": "mov eax, 0"})).await;
    assert_eq!(body["correct"], false);
    assert!(!body["hints"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn grading_runs_a_submitted_program_on_the_emulator() {
    let uri = "/api/lessons/the-stack/exercises/e-factorial/check";
    let solution = "mov rdi, 5\ncall fact\nhlt\nfact:\ncmp rdi, 1\njbe base\npush rdi\ndec rdi\ncall fact\npop rdi\nimul rax, rdi\nret\nbase:\nmov rax, 1\nret";
    let (status, body) = post(uri, json!({"answer": solution})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["correct"], true, "{}", body["message"]);

    // A wrong program is graded on its result, and told what it produced.
    let (_, body) = post(uri, json!({"answer": "mov rax, 7\nhlt"})).await;
    assert_eq!(body["correct"], false);
    assert!(body["message"].as_str().unwrap().contains("rax"));
}

#[tokio::test]
async fn a_submitted_program_cannot_hang_the_server() {
    let uri = "/api/lessons/the-stack/exercises/e-factorial/check";
    let (status, body) = post(uri, json!({"answer": "here:\njmp here"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["correct"], false);
    assert!(body["message"].as_str().unwrap().contains("still running"));
}

#[tokio::test]
async fn inspect_parses_a_real_elf() {
    // Our own test binary is an ELF on Linux.
    let path = std::env::current_exe().unwrap();
    let bytes = std::fs::read(&path).unwrap();
    if binfmt::detect(&bytes).is_none() {
        eprintln!("skipping: the test binary is not an ELF or PE");
        return;
    }
    if bytes.len() > 8 * 1024 * 1024 {
        eprintln!("skipping: the test binary is too large to send as hex");
        return;
    }
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();

    let (status, body) = post("/api/binfmt/inspect", json!({"hex": hex})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["format"], "elf");
    let sections = body["sections"].as_array().unwrap();
    assert!(sections.iter().any(|s| s["name"] == ".text"));
}

#[tokio::test]
async fn inspect_rejects_garbage_without_panicking() {
    let (status, body) = post("/api/binfmt/inspect", json!({"hex": "deadbeef"})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["kind"], "binfmt");
}
