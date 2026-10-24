//! Standalone binary evidence for original registry grammar and canonical projections.
use serde_json::{Value, json};
use std::process::{Command, Stdio};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-mcp");
fn run(origin: &str, messages: Vec<Value>) -> Vec<Value> {
    let cookie = std::env::temp_dir().join(format!(
        "merge688-mcp-{}-{:?}.cookie",
        std::process::id(),
        thread::current().id()
    ));
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(&cookie)
        .unwrap()
        .write_all(b"session=synthetic")
        .unwrap();
    let mut child = Command::new(BIN)
        .args([
            "--server",
            origin,
            "--allow-loopback-http",
            "--cookie-file",
            cookie.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let writer = thread::spawn(move || {
        for m in messages {
            writeln!(stdin, "{m}").unwrap();
        }
    });
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    std::fs::remove_file(cookie).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}

fn init() -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic","version":"1"}}})
}
fn call(id: u64, name: &str, args: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}})
}
fn session(calls: Vec<Value>) -> Vec<Value> {
    let mut m = vec![
        init(),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    ];
    m.extend(calls);
    m
}
fn envelope(v: &Value) -> Value {
    let r = &v["result"];
    assert_eq!(r["content"].as_array().unwrap().len(), 1);
    let e: Value = serde_json::from_str(r["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(e, r["structuredContent"]);
    assert_eq!(r["isError"], e["ok"] != true);
    e
}
const SHA: &str = "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03";
const MERGE: &str = "d157b0f801b28bbb6a6fe392fe76d5dcdfd0e2e17e7c50b8c5494d9ef7e195dd";
fn event(kind: &str, h: &str) -> Value {
    json!({"event_type":kind,"payload_hash":h,"actor":"user:synthetic","created_at":"2026-01-01T00:00:00.000Z"})
}
fn party() -> Value {
    json!({"partyId":"source","kind":"organization","name":"Synthetic","source":"fixture","observedAt":"2026-01-01T00:00:00.000Z","reviewAssertion":"Reviewed","identifiers":[],"aliases":[],"assertions":[],"roles":[{"companySlug":"synthetic","role":"vendor","defaults":{"currency":"DKK"}}],"history":[event("created",SHA),event("proposed_merge",MERGE),event("approved_merge",MERGE),event("superseded",MERGE)]})
}
fn record() -> Value {
    json!({"recordId":"record-synthetic","type":"articles","filename":"synthetic.txt","source":"fixture","receivedAt":"2026-01-01T00:00:00.000Z","recordDate":null,"retentionUntil":null,"uploader":"provenance-not-authority","sensitivity":"normal","sha256":SHA,"links":[{"type":"company","id":"synthetic"},{"type":"legacy_evidence_ref","id":"synthetic-proof"}],"history":[event("ingested",SHA),event("linked",SHA),event("enriched",SHA),event("superseded",SHA)]})
}
// Passive discovery is the committed current production manifest, not a capability bypass.
fn fixture(mode: &str) -> (String, thread::JoinHandle<Vec<(String, Value)>>) {
    let mode = mode.to_owned();
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    l.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let worker = thread::spawn(move || {
        let mut requests = Vec::new();
        let mut idle = Instant::now();
        while idle.elapsed() < Duration::from_secs(5) {
            let (mut s, _) = match l.accept() {
                Ok(s) => s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            idle = Instant::now();
            s.set_nonblocking(false).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut bytes = Vec::new();
            while !bytes.ends_with(b"\r\n\r\n") {
                let mut b = [0];
                s.read_exact(&mut b).unwrap();
                bytes.push(b[0]);
            }
            let h = String::from_utf8(bytes).unwrap();
            let lower = h.to_lowercase();
            let n = lower
                .lines()
                .find_map(|l| {
                    l.strip_prefix("content-length:")
                        .map(|n| n.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            let mut body = vec![0; n];
            s.read_exact(&mut body).unwrap();
            let line = h.lines().next().unwrap().to_owned();
            let (status, text) = if line.starts_with("GET /api/protocol ") {
                assert!(!lower.contains("cookie:") && !lower.contains("authorization:"));
                requests.push((line.clone(), Value::Null));
                (
                    200,
                    include_str!("../../../protocol/http-v1.json").to_owned(),
                )
            } else {
                assert!(lower.contains("cookie: session=synthetic"));
                assert!(lower.contains("x-rentemester-manifest:"));
                let b = if body.is_empty() {
                    Value::Null
                } else {
                    serde_json::from_slice(&body).unwrap()
                };
                requests.push((line.clone(), b));
                if mode == "lost" && line.starts_with("POST ") {
                    drop(s);
                    continue;
                }
                let v = if mode == "bad" {
                    json!({"ok":true,"record":{"recordId":"wrong"},"party":{"partyId":"wrong"},"payloadHash":"invalid","proposalHash":"invalid"})
                } else if line.contains("/download ") {
                    json!({"ok":true,"bytesBase64":"aGVsbG8K","sha256":SHA})
                } else if line.contains("/propose-merge ") {
                    json!({"ok":true,"proposalHash":MERGE})
                } else if line.contains("/enrich ") || line.contains("/supersede ") {
                    json!({"ok":true,"payloadHash":SHA})
                } else if line.contains("corporate-records?") {
                    json!({"ok":true,"rows":[record()],"count":1,"nextCursor":null})
                } else if line.contains("workspace-parties?") {
                    json!({"ok":true,"rows":[party()],"count":1,"nextCursor":null})
                } else if line.contains("corporate-records") {
                    json!({"ok":true,"record":record()})
                } else {
                    json!({"ok":true,"party":party()})
                };
                (if mode == "denied" { 403 } else { 200 }, v.to_string())
            };
            write!(s,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",text.len()).unwrap();
        }
        requests
    });
    (origin, worker)
}
fn cases() -> Vec<(&'static str, Value)> {
    vec![
        (
            "workspace_party_search",
            json!({"company":"synthetic","query":"Synthetic"}),
        ),
        (
            "workspace_party_inspect",
            json!({"company":"synthetic","partyId":"source"}),
        ),
        (
            "workspace_party_create",
            json!({"company":"synthetic","kind":"organization","name":"Synthetic","source":"fixture","observedAt":"2026-01-01","reviewAssertion":"Reviewed","confirm":true}),
        ),
        (
            "workspace_party_link_role",
            json!({"company":"synthetic","partyId":"source","role":"vendor","defaults":{"currency":"DKK"},"confirm":true}),
        ),
        (
            "workspace_party_propose_merge",
            json!({"company":"synthetic","fromPartyId":"source","intoPartyId":"target","reviewAssertion":"Reviewed\nsynthetic","confirm":true}),
        ),
        (
            "workspace_party_approve_merge",
            json!({"company":"synthetic","fromPartyId":"source","proposalHash":MERGE,"confirm":true}),
        ),
        ("corporate_record_list", json!({"company":"synthetic"})),
        (
            "corporate_record_inspect",
            json!({"company":"synthetic","recordId":"record-synthetic"}),
        ),
        (
            "corporate_record_download",
            json!({"company":"synthetic","recordId":"record-synthetic"}),
        ),
        (
            "corporate_record_ingest",
            json!({"company":"synthetic","recordId":"record-synthetic","type":"articles","bytesBase64":"aGVsbG8K","filename":"synthetic.txt","source":"fixture","receivedAt":"2026-01-01","uploader":"provenance-not-authority","links":[{"type":"company","id":"synthetic"}],"confirm":true}),
        ),
        (
            "corporate_record_link",
            json!({"company":"synthetic","recordId":"record-synthetic","type":"legacy_evidence_ref","id":"synthetic-proof","confirm":true}),
        ),
        (
            "corporate_record_enrich",
            json!({"company":"synthetic","recordId":"record-synthetic","assertion":"Reviewed synthetic","confirm":true}),
        ),
        (
            "corporate_record_supersede",
            json!({"company":"synthetic","recordId":"record-synthetic","replacementRecordId":"replacement","reason":"Synthetic correction","confirm":true}),
        ),
    ]
}
#[test]
fn exact_original_stream_and_native_coexistence() {
    let (origin, server) = fixture("ok");
    let cases = cases();
    let mut calls = vec![json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})];
    calls.extend(
        cases
            .iter()
            .enumerate()
            .map(|(i, (n, a))| call(i as u64 + 3, n, a.clone())),
    );
    let rows = run(&origin, session(calls));
    let requests = server.join().unwrap();
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    for (i, (name, args)) in cases.iter().enumerate() {
        let t = tools.iter().find(|t| t["name"] == *name).unwrap();
        let read = args.get("confirm").is_none();
        assert_eq!(t["annotations"]["readOnlyHint"], read);
        assert_eq!(t["annotations"]["idempotentHint"], read);
        assert!(t["inputSchema"]["properties"].get("body").is_none());
        assert!(
            !t["inputSchema"]["required"]
                .as_array()
                .unwrap()
                .contains(&json!("confirm"))
        );
        let e = envelope(&rows[i + 2]);
        if *name == "workspace_party_create" {
            assert_eq!(e["code"], "NATIVE_BOUNDARY");
        } else {
            assert_eq!(e["ok"], true, "{name}: {e}");
            assert_eq!(e["errors"], json!([]));
            assert!(e.get("appliedRules").is_none());
        }
    }
    assert!(
        requests
            .iter()
            .any(|(line, _)| line == "GET /api/protocol HTTP/1.1")
    );
    for n in [
        "propose_workspace_party_merge",
        "approve_workspace_party_merge",
    ] {
        assert!(tools.iter().any(|t| t["name"] == n));
    }
    assert_eq!(
        requests
            .iter()
            .filter(|(l, _)| l.starts_with("POST "))
            .count(),
        7
    );
    let ingest = requests
        .iter()
        .find(|(l, _)| l == "POST /api/companies/synthetic/corporate-records HTTP/1.1")
        .unwrap();
    assert_eq!(ingest.1["uploader"], "provenance-not-authority");
    assert_eq!(ingest.1["bytesBase64"], "aGVsbG8K");
    assert!(ingest.1.get("company").is_none());
    assert!(ingest.1.get("actor").is_none());
    assert_eq!(
        envelope(&rows[10])["data"],
        json!({"bytesBase64":"aGVsbG8K","sha256":SHA})
    );
}
#[test]
fn confirmation_invalid_and_boundary_are_no_effect() {
    let mut calls = vec![];
    for (name, mut a) in cases() {
        if a.get("confirm").is_some() {
            a.as_object_mut().unwrap().remove("confirm");
            calls.push(call(calls.len() as u64 + 2, name, a.clone()));
            a["confirm"] = json!(false);
            calls.push(call(calls.len() as u64 + 2, name, a));
        }
    }
    let rows = run("http://127.0.0.1:9", session(calls));
    for r in &rows[1..] {
        assert_eq!(envelope(r)["code"], "CONFIRM_REQUIRED");
    }
    let mut calls = vec![];
    for (name, mut a) in cases() {
        a["actor"] = json!("user:other");
        calls.push(call(calls.len() as u64 + 2, name, a));
    }
    for a in [
        json!({"company":"../hidden","recordId":"record-synthetic"}),
        json!({"company":"synthetic","recordId":".."}),
        json!({"company":"synthetic","recordId":null}),
    ] {
        calls.push(call(calls.len() as u64 + 2, "corporate_record_inspect", a));
    }
    let rows = run("http://127.0.0.1:9", session(calls));
    for r in &rows[1..] {
        assert!(r.get("error").is_some(), "{r}");
    }
}
#[test]
fn malformed_denied_and_lost_write_never_retry() {
    for mode in ["bad", "denied", "lost"] {
        let (origin, server) = fixture(mode);
        let args = cases()
            .into_iter()
            .find(|(n, _)| *n == "corporate_record_link")
            .unwrap()
            .1;
        let rows = run(
            &origin,
            session(vec![call(2, "corporate_record_link", args)]),
        );
        let e = envelope(&rows[1]);
        assert_eq!(e["ok"], false);
        assert_eq!(
            e["code"],
            if mode == "denied" {
                "server_refused"
            } else {
                "outcome_unknown"
            }
        );
        let requests = server.join().unwrap();
        assert_eq!(
            requests
                .iter()
                .filter(|(l, _)| l.starts_with("POST "))
                .count(),
            1
        );
    }
}
