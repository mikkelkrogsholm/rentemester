#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use common::TcpListener;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-mcp");
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Cookie(PathBuf);
impl Cookie {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "rm-mcp-{}-{}.cookie",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(&path)
            .unwrap()
            .write_all(b"session=never-print-this\n")
            .unwrap();
        Self(path)
    }
}
impl Drop for Cookie {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
fn run(origin: &str, messages: &[Value]) -> Output {
    let lines: String = messages.iter().map(|v| format!("{v}\n")).collect();
    run_raw(origin, lines.as_bytes())
}
fn run_raw(origin: &str, bytes: &[u8]) -> Output {
    let cookie = Cookie::new();
    let mut child = Command::new(BIN)
        .args([
            "--server",
            origin,
            "--allow-loopback-http",
            "--cookie-file",
            cookie.0.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Drain stdout while feeding stdin: discovery and document frames can exceed pipe capacity.
    let bytes = bytes.to_vec();
    let mut stdin = child.stdin.take().unwrap();
    let writer = thread::spawn(move || stdin.write_all(&bytes));
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap().unwrap();
    output
}
fn parsed(output: &Output) -> Vec<Value> {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("never-print-this"));
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn init() -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})
}
fn ready() -> Value {
    json!({"jsonrpc":"2.0","method":"notifications/initialized"})
}
fn call(name: &str, args: Value) -> Value {
    json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":name,"arguments":args}})
}
fn event(kind: &str, payload: Value) -> Value {
    let hash = match kind {
        "ingested" => "a7671d025001bb8b57c90c8ee11dc07a46aad679eeeecf4be4311046eae445b9",
        "candidate_resolved" => "3de177d571a7a291d4caf2858892ea5323ffbf08608982967ec57a7e98b5c883",
        "assigned" => "3de177d571a7a291d4caf2858892ea5323ffbf08608982967ec57a7e98b5c883",
        _ => "ec74a15c481d086c59c555bc55a04690cbeccbcaf19aead8fc7df9b54df02f6b",
    };
    json!({"event_type":kind,"payload_hash":hash,"actor":"user:synthetic","created_at":"2026-01-01T00:00:00.000Z","payload":payload})
}
fn source() -> Value {
    let hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    json!({"sourceId":"source/synthetic","visibilityAnchorSlug":"synthetic","sha256":hash,"filename":"synthetic.txt","mimeType":"text/plain","transport":"upload","transportIdentity":null,"receivedAt":"2026-01-01T00:00:00.000Z","metadata":{"documentType":"cash_register_receipt"},"createdBy":"user:synthetic","createdAt":"2026-01-01T00:00:00.000Z","exception":null,"handoffs":[],"mail":null,"metadataReview":null,"assignments":[],"history":[event("ingested",json!({"sourceHash":hash,"filename":"synthetic.txt","mimeType":"text/plain","transport":"upload","transportIdentity":null})),event("candidate_resolved",json!({"candidates":[{"companySlug":"distinct-target","evidenceKind":"portal","evidence":"Synthetic upload"}]}))]})
}

fn response(op: &str) -> Value {
    let mut s = source();
    if matches!(op, "assign" | "complete") {
        s["assignments"] = json!([{"companySlug":"distinct-target","state":if op=="assign"{"approved"}else{"completed"},"documentId":if op=="complete"{json!(17)}else{Value::Null},"documentNo":if op=="complete"{json!("D17")}else{Value::Null},"assignedBy":"user:synthetic","assignedAt":"2026-01-01T00:00:00.000Z","completedAt":if op=="complete"{json!("2026-01-01T00:00:00.000Z")}else{Value::Null}}]);
    }
    if matches!(op, "assign" | "complete") {
        s["history"]
            .as_array_mut()
            .unwrap()
            .push(event("assigned", json!({"companySlug":"distinct-target"})));
        if op == "complete" {
            s["history"].as_array_mut().unwrap().push(event("handoff_completed",json!({"companySlug":"distinct-target","documentId":17,"documentNo":"D17","sourceHash":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"})));
        }
    }
    if op == "complete" {
        s["handoffs"] = json!([{"companySlug":"distinct-target","state":"completed","leaseExpiresAt":"2026-01-01T00:02:00.000Z","documentId":17,"documentNo":"D17"}]);
    }
    if op == "list" {
        json!({"ok":true,"rows":[s],"count":1,"nextCursor":null})
    } else {
        json!({"ok":true,"source":s})
    }
}
use std::time::{Duration, Instant};
fn args(op: &str) -> Value {
    match op {
        "list" => json!({"company":"synthetic"}),
        "ingest" => {
            json!({"company":"synthetic","sourceId":"source/synthetic","idempotencyKey":"caller exact key","bytesBase64":"YWJj","filename":"synthetic.txt","mimeType":"text/plain","transport":"upload","transportIdentity":null,"receivedAt":"2026-01-01T00:00:00.000Z","metadata":{"documentType":"cash_register_receipt"},"candidates":[{"companySlug":"distinct-target","evidenceKind":"portal","evidence":"Synthetic upload"}],"confirm":true})
        }
        "assign" | "complete" => {
            json!({"company":"synthetic","sourceId":"source/synthetic","companySlug":"distinct-target","confirm":true})
        }
        _ => json!({"company":"synthetic","sourceId":"source/synthetic"}),
    }
}
fn invoke(
    op: &str,
    mut a: Value,
    core: Value,
    status: u16,
    lost: bool,
    service: bool,
) -> (Value, Vec<String>) {
    if op == "ingest" && a["transportIdentity"].is_null() {
        a.as_object_mut().unwrap().remove("transportIdentity");
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let op = op.to_owned();
    let copy = op.clone();
    let task = thread::spawn(move || {
        let start = Instant::now();
        let mut last = None;
        let mut wires = Vec::new();
        loop {
            if last.is_some_and(|t: Instant| t.elapsed() > Duration::from_millis(180)) {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(8),
                "no business request"
            );
            let mut socket = match listener.accept() {
                Ok((s, _)) => s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            let mut bytes = Vec::new();
            while !bytes.ends_with(b"\r\n\r\n") {
                let mut b = [0];
                socket.read_exact(&mut b).unwrap();
                bytes.push(b[0]);
            }
            let header = String::from_utf8(bytes.clone()).unwrap();
            let n = header
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|s| s.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            let mut body = vec![0; n];
            socket.read_exact(&mut body).unwrap();
            bytes.extend(body);
            let wire = String::from_utf8(bytes).unwrap();
            assert!(wire.contains("x-rentemester-protocol:"));
            if service {
                assert!(wire.contains("x-rentemester-service-key:"));
                assert!(!wire.to_lowercase().contains("cookie:"));
            } else {
                assert!(wire.contains("cookie: session=never-print-this"));
            }
            let prior = matches!(op.as_str(), "assign" | "complete") && wire.starts_with("GET ");
            let value = if prior {
                response("inspect")
            } else {
                core.clone()
            };
            wires.push(wire);
            if !(lost && !prior) {
                let text = value.to_string();
                let code = if prior { 200 } else { status };
                socket.write_all(format!("HTTP/1.1 {code} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",text.len()).as_bytes()).unwrap();
            }
            drop(socket);
            last = Some(Instant::now());
        }
        wires
    });
    let messages = [init(), ready(), call(&format!("workspace_inbox_{copy}"), a)];
    let output = if !service {
        run(&origin, &messages)
    } else {
        let key = Cookie::new();
        std::fs::write(&key.0, b"synthetic-service").unwrap();
        let mut c = Command::new(BIN)
            .args([
                "--server",
                &origin,
                "--allow-loopback-http",
                "--service-key-file",
                key.0.to_str().unwrap(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        c.stdin
            .take()
            .unwrap()
            .write_all(
                messages
                    .iter()
                    .map(|v| format!("{v}\n"))
                    .collect::<String>()
                    .as_bytes(),
            )
            .unwrap();
        c.wait_with_output().unwrap()
    };
    let rows = parsed(&output);
    let result = &rows[1]["result"];
    assert!(result.is_object(), "unexpected reply: {rows:?}");
    let envelope = result["structuredContent"].clone();
    assert_eq!(result["content"].as_array().unwrap().len(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
        envelope
    );
    assert_eq!(result["isError"], envelope["ok"] != true);
    (envelope, task.join().unwrap())
}
#[test]
fn original_catalogue_and_native_coexistence() {
    let rows = parsed(&run(
        "http://127.0.0.1:9",
        &[
            init(),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    ));
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    let names: std::collections::BTreeSet<_> =
        tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names.len(), tools.len());
    for op in ["list", "inspect", "status", "ingest", "assign", "complete"] {
        let t = tools
            .iter()
            .find(|t| t["name"] == format!("workspace_inbox_{op}"))
            .unwrap();
        let read = matches!(op, "list" | "inspect" | "status");
        assert_eq!(
            t["annotations"],
            json!({"readOnlyHint":read,"destructiveHint":false,"idempotentHint":read,"openWorldHint":false})
        );
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
        assert!(
            !t["inputSchema"]["required"]
                .as_array()
                .unwrap()
                .contains(&json!("confirm"))
        );
        assert!(t["inputSchema"]["properties"].get("body").is_none());
    }
    for name in [
        "workspace_inbox",
        "workspace_inbox_source",
        "ingest_workspace_inbox",
        "assign_workspace_inbox",
        "complete_workspace_inbox",
    ] {
        assert!(names.contains(name));
    }
    let native = tools
        .iter()
        .find(|t| t["name"] == "ingest_workspace_inbox")
        .unwrap();
    assert_eq!(
        native["inputSchema"]["properties"]["body"]["type"],
        "string"
    );
}
#[test]
fn all_six_positive_exact_envelopes_wire_and_independent_destination() {
    for op in ["list", "inspect", "status", "ingest", "assign", "complete"] {
        let core = response(op);
        let (e, w) = invoke(op, args(op), core.clone(), 200, false, false);
        let data = if op == "list" {
            json!({"rows":core["rows"],"count":1,"nextCursor":null})
        } else {
            json!({"source":core["source"]})
        };
        assert_eq!(e, json!({"ok":true,"errors":[],"data":data}), "{op}");
        assert_eq!(
            w.len(),
            if matches!(op, "assign" | "complete") {
                2
            } else {
                1
            }
        );
        let last = w.last().unwrap();
        match op {
            "list" => assert!(last.starts_with(
                "GET /api/companies/synthetic/workspace-inbox?cursor=0&limit=25 HTTP/1.1"
            )),
            "inspect" | "status" => assert!(last.starts_with(
                "GET /api/companies/synthetic/workspace-inbox/source%2Fsynthetic HTTP/1.1"
            )),
            _ => {
                let body: Value =
                    serde_json::from_str(last.split("\r\n\r\n").nth(1).unwrap()).unwrap();
                let mut expected = args(op);
                expected.as_object_mut().unwrap().remove("company");
                if op != "ingest" {
                    expected.as_object_mut().unwrap().remove("sourceId");
                    assert_eq!(
                        body,
                        json!({"companySlug":"distinct-target","confirm":true})
                    );
                } else {
                    expected
                        .as_object_mut()
                        .unwrap()
                        .remove("transportIdentity");
                }
                assert_eq!(body, expected);
            }
        }
    }
}
#[test]
fn confirmation_and_invalid_inputs_have_no_transport_effect() {
    for op in ["ingest", "assign", "complete"] {
        for confirm in [None, Some(false)] {
            let mut a = args(op);
            a.as_object_mut().unwrap().remove("transportIdentity");
            a.as_object_mut().unwrap().remove("confirm");
            if let Some(v) = confirm {
                a["confirm"] = json!(v);
            }
            let rows = parsed(&run(
                "http://127.0.0.1:9",
                &[init(), ready(), call(&format!("workspace_inbox_{op}"), a)],
            ));
            assert_eq!(
                rows[1]["result"]["structuredContent"],
                json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":[format!("confirm: true required for write tool workspace_inbox_{op}")]})
            );
        }
    }
    for (op, key, value) in [
        ("list", "limit", json!(0)),
        ("list", "cursor", json!(-1)),
        ("inspect", "sourceId", json!("..")),
        ("assign", "companySlug", Value::Null),
        ("assign", "actor", json!("user:forged")),
        ("complete", "companySlug", json!("../target")),
        ("ingest", "transport", json!("mail")),
        ("ingest", "filename", json!("../x.txt")),
        ("ingest", "receivedAt", json!("yesterday")),
        ("ingest", "bytesBase64", json!("YWJ=")),
        ("ingest", "metadata", json!([])),
        ("ingest", "confirm", Value::Null),
    ] {
        let mut a = args(op);
        a.as_object_mut().unwrap().remove("transportIdentity");
        a[key] = value;
        let rows = parsed(&run(
            "http://127.0.0.1:9",
            &[init(), ready(), call(&format!("workspace_inbox_{op}"), a)],
        ));
        assert_eq!(rows[1]["error"]["code"], -32602, "{op} {key}");
    }
}
#[test]
fn canonical_js_text_and_engine_iso_regressions() {
    for field in ["filename", "sourceId", "transportIdentity"] {
        let mut a = args("ingest");
        let raw = if field == "filename" {
            "synthetic.txt"
        } else if field == "sourceId" {
            "source/synthetic"
        } else {
            "identity"
        };
        a[field] = json!(format!("\u{feff} {raw}"));
        let mut core = response("ingest");
        if field == "transportIdentity" {
            core["source"][field] = json!(raw);
            core["source"]["history"][0]["payload"][field] = json!(raw);
        }
        let (e, w) = invoke("ingest", a.clone(), core, 201, false, false);
        assert_eq!(e["ok"], true, "{field}: {e}");
        assert!(w[0].contains(&a[field].to_string()));
    }
    let mut a = args("inspect");
    a["sourceId"] = json!(" source/synthetic ");
    let (e, w) = invoke("inspect", a, response("inspect"), 200, false, false);
    assert_eq!(e["ok"], true);
    assert!(w[0].contains("/%20source%2Fsynthetic%20 "));
    for field in [
        "receivedAt",
        "transportIdentity",
        "sha256",
        "visibilityAnchorSlug",
    ] {
        let mut core = response("ingest");
        core["source"][field] = json!(if field == "receivedAt" {
            "2026-01-02T00:00:00.000Z"
        } else {
            "mismatch"
        });
        let (e, w) = invoke("ingest", args("ingest"), core, 201, false, false);
        assert_eq!(e["code"], "outcome_unknown");
        assert_eq!(w.len(), 1);
    }
}
#[test]
fn refusal_malformed_receipt_and_lost_accepted_write_are_never_retried() {
    for (status, core, lost, service, code) in [
        (
            403,
            json!({"private":"never-print-this"}),
            false,
            true,
            "server_refused",
        ),
        (401, json!({}), false, false, "server_refused"),
        (
            200,
            json!({"ok":true,"source":{}}),
            false,
            false,
            "outcome_unknown",
        ),
        (200, response("ingest"), true, false, "outcome_unknown"),
        (503, json!({}), false, false, "outcome_unknown"),
    ] {
        let (e, w) = invoke("ingest", args("ingest"), core, status, lost, service);
        assert_eq!(e["code"], code);
        assert!(e.get("data").is_none());
        assert_eq!(w.len(), 1);
        assert!(w[0].starts_with("POST "));
    }
    let (e, _) = invoke("status", args("status"), json!({}), 404, false, false);
    assert_eq!(
        e,
        json!({"ok":false,"errors":["workspace inbox source not found"],"code":"WORKSPACE_INBOX_NOT_FOUND"})
    );
    let mut core = response("complete");
    core["source"]["assignments"][0]["companySlug"] = json!("synthetic");
    let (e, w) = invoke("complete", args("complete"), core, 200, false, false);
    assert_eq!(e["code"], "outcome_unknown");
    assert_eq!(w.len(), 2);
}
#[test]
fn stage_only_future_metadata_and_empty_metadata_are_preserved() {
    for metadata in [
        json!({}),
        json!({"futureFact":{"exact":[null,"  source assertion  ",42]},"notAnInvoice":true}),
    ] {
        let mut a = args("ingest");
        a["metadata"] = metadata.clone();
        let mut core = response("ingest");
        core["source"]["metadata"] = metadata.clone();
        let (e, w) = invoke("ingest", a, core, 201, false, false);
        assert_eq!(e["ok"], true);
        assert_eq!(e["data"]["source"]["metadata"], metadata);
        assert!(
            e["data"]["source"]["assignments"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let body: Value = serde_json::from_str(w[0].split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(body["metadata"], metadata);
    }
}
