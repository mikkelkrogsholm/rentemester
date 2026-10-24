use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-mcp");
fn run(origin: &str, msgs: Vec<Value>) -> Vec<Value> {
    use std::os::unix::fs::OpenOptionsExt;
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "ic717-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .unwrap()
        .write_all(b"session=synthetic-private")
        .unwrap();
    let mut c = Command::new(BIN)
        .args([
            "--server",
            origin,
            "--allow-loopback-http",
            "--cookie-file",
            path.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = c.stdin.take().unwrap();
    let writer = thread::spawn(move || {
        for m in msgs {
            writeln!(input, "{m}").unwrap();
        }
    });
    let out = c.wait_with_output().unwrap();
    writer.join().unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stderr.is_empty());
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
}
fn init() -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})
}
fn ready() -> Value {
    json!({"jsonrpc":"2.0","method":"notifications/initialized"})
}
fn call(op: &str, a: Value) -> Value {
    json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":format!("intercompany_disposition_{op}"),"arguments":a}})
}
fn disposition() -> Value {
    json!({"dispositionId":"synthetic-id","type":"loan","economicDate":"2026-10-20","amount":75,"currency":"DKK","partyIds":["synthetic-party"],"evidenceRecordIds":["synthetic-record"],"left":{"companySlug":"left","role":"lender","expectedSide":"receivable"},"right":{"companySlug":"right","role":"borrower","expectedSide":"payable"}})
}
fn disposition_hash() -> String {
    rentemester_engine::sha256_bytes(
        rentemester_engine::imported_receivables::canonical(&disposition())
            .unwrap()
            .as_bytes(),
    )
}
fn args(op: &str) -> Value {
    if matches!(op, "plan" | "propose") {
        let mut a = json!({"company":"left","disposition":disposition()});
        if op == "propose" {
            a["confirm"] = json!(true);
        }
        return a;
    }
    let mut a = json!({"company":"left","dispositionId":"synthetic-id"});
    if op == "status" {
        return a;
    }
    a["payloadHash"] = json!(disposition_hash());
    a["confirm"] = json!(true);
    match op {
        "link" => {
            a["side"] = json!("left");
            a["journalEntryId"] = json!(3);
            a["ledgerHeadHash"] = json!("b".repeat(64));
        }
        "settle" => a["settlementEvidenceRecordIds"] = json!(["synthetic-settlement"]),
        "reopen" => a["reason"] = json!("Reviewed reopen"),
        "supersede" => {
            a["reason"] = json!("Reviewed replacement");
            a["replacementDispositionId"] = json!("synthetic-replacement");
        }
        _ => {}
    }
    a
}
fn event(kind: &str, p: Value) -> Value {
    json!({"event_type":kind,"actor":"user:synthetic","principal_kind":"user","principal_id":if kind=="proposed"{"author"}else{"reviewer"},"payload_hash":rentemester_engine::sha256_bytes(rentemester_engine::imported_receivables::canonical(&p).unwrap().as_bytes()),"canonical_payload":p.to_string(),"created_at":"2026-10-20T12:00:00.000Z"})
}
fn receipt(op: &str) -> Value {
    if op == "plan" {
        return json!({"ok":true,"mode":"dry-run","disposition":disposition(),"payloadHash":disposition_hash(),"ledgerEffects":[],"proposedSides":["left","right"],"warnings":["Each legal company must post separately through its normal policy, document, VAT and period controls."],"journalCandidates":{"left":[],"right":[]}});
    }
    let mut ev = vec![event("proposed", disposition())];
    if op != "propose" {
        ev.push(event("approved", json!({"payloadHash":disposition_hash()})));
    }
    let mut life = vec![];
    let mut links = vec![];
    if matches!(op, "link" | "settle" | "reopen") {
        for side in if op == "link" {
            vec!["left"]
        } else {
            vec!["left", "right"]
        } {
            links.push(json!({"side":side,"company_slug":side,"journal_entry_id":3,"journal_entry_no":"J3","journal_entry_hash":"c".repeat(64),"ledger_head_hash":"b".repeat(64),"linked_at":"2026-10-20T12:00:00.000Z","actor":"user:synthetic","principal_kind":"user","principal_id":"reviewer"}));
        }
    }
    let a = args(op);
    let mut p = json!({"payloadHash":disposition_hash()});
    for k in [
        "settlementEvidenceRecordIds",
        "reason",
        "replacementDispositionId",
    ] {
        if let Some(v) = a.get(k) {
            p[k] = v.clone();
        }
    }
    match op {
        "settle" => life.push(event("settled", p)),
        "reopen" => {
            life.push(event("settled",json!({"payloadHash":disposition_hash(),"settlementEvidenceRecordIds":["synthetic-settlement"]})));
            life.push(event("reopened", p));
        }
        "supersede" => life.push(event("superseded", p)),
        _ => {}
    }
    let status = match op {
        "propose" => "proposed",
        "link" => "partly_posted",
        "settle" => "settled",
        "supersede" => "superseded",
        "reopen" => "posted",
        _ => "approved",
    };
    let mut v = json!({"ok":true,"disposition":disposition(),"payloadHash":disposition_hash(),"status":status,"events":ev,"lifecycleEvents":life,"links":links});
    if op == "status" {
        v["exceptions"] = json!([]);
        v["balanced"] = json!(false);
        v["scope"] = json!("intercompany-disposition-status");
        v["journalCandidates"] = json!({"left":[],"right":[]});
    }
    v
}
fn invoke(
    op: &str,
    a: Value,
    response: Value,
    code: u16,
    lost: bool,
    admitted: bool,
) -> (Value, Vec<String>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    l.set_nonblocking(true).unwrap();
    let t = thread::spawn(move || {
        let start = Instant::now();
        let mut last = None;
        let mut wires = vec![];
        loop {
            if last.is_some_and(|t: Instant| t.elapsed() > Duration::from_millis(160)) {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(8),
                "no business request"
            );
            let mut s = match l.accept() {
                Ok((s, _)) => s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            s.set_nonblocking(false).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut b = vec![];
            while !b.ends_with(b"\r\n\r\n") {
                let mut c = [0];
                s.read_exact(&mut c).unwrap();
                b.push(c[0]);
            }
            let h = String::from_utf8(b.clone()).unwrap();
            let n = h
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length:")
                        .map(|s| s.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            let mut body = vec![0; n];
            s.read_exact(&mut body).unwrap();
            b.extend(body);
            let wire = String::from_utf8(b).unwrap();
            let discovery = wire.starts_with("GET /api/protocol ");
            let anchor = wire.starts_with("GET /api/companies ");
            let (text, status) = if discovery {
                assert!(!wire.to_lowercase().contains("cookie:"));
                (
                    include_str!("../../../protocol/http-v1.json").to_owned(),
                    200,
                )
            } else if anchor {
                (json!({"ok":true,"companies":[{"slug":if admitted{"left"}else{"unrelated"},"archived":false}],"count":1}).to_string(),200)
            } else {
                (response.to_string(), code)
            };
            if !discovery {
                assert!(wire.contains("session=synthetic-private"));
                wires.push(wire);
                last = Some(Instant::now());
            }
            if !(lost && !discovery && !anchor) {
                write!(s,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",text.len()).unwrap();
            }
        }
        wires
    });
    let rows = run(&origin, vec![init(), ready(), call(op, a)]);
    (rows[1].clone(), t.join().unwrap())
}
fn envelope(r: &Value) -> &Value {
    let r = &r["result"];
    let e = &r["structuredContent"];
    assert_eq!(r["isError"], e["ok"] != true);
    assert_eq!(r["content"].as_array().unwrap().len(), 1);
    assert_eq!(
        serde_json::from_str::<Value>(r["content"][0]["text"].as_str().unwrap()).unwrap(),
        *e
    );
    e
}
const OPS: &[&str] = &[
    "plan",
    "propose",
    "approve",
    "link",
    "status",
    "settle",
    "reopen",
    "supersede",
];
#[test]
fn discovery_and_local_grammar_confirmation() {
    let rows = run(
        "http://127.0.0.1:9",
        vec![
            init(),
            ready(),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    );
    let ts = rows[1]["result"]["tools"].as_array().unwrap();
    for op in OPS {
        let n = format!("intercompany_disposition_{op}");
        let t = ts.iter().find(|t| t["name"] == n).unwrap();
        let read = matches!(*op, "plan" | "status");
        assert_eq!(t["annotations"]["readOnlyHint"], read);
        assert_eq!(t["annotations"]["idempotentHint"], read);
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
        assert!(
            !t["inputSchema"]["required"]
                .as_array()
                .unwrap()
                .contains(&json!("confirm"))
        );
        let mut a = args(op);
        a["actor"] = json!("forged");
        let rows = run("http://127.0.0.1:9", vec![init(), ready(), call(op, a)]);
        assert_eq!(rows[1]["error"]["code"], -32602);
        for a in [json!([]), json!({}), json!({"company":3})] {
            let rows = run("http://127.0.0.1:9", vec![init(), ready(), call(op, a)]);
            assert_eq!(rows[1]["error"]["code"], -32602);
        }
        if !read {
            for value in [None, Some(false)] {
                let mut a = args(op);
                a.as_object_mut().unwrap().remove("confirm");
                if let Some(v) = value {
                    a["confirm"] = json!(v);
                }
                let rows = run("http://127.0.0.1:9", vec![init(), ready(), call(op, a)]);
                assert_eq!(envelope(&rows[1])["code"], "CONFIRM_REQUIRED");
            }
        }
    }
    assert!(ts.iter().any(|t| t["name"] == "group_disposition_status"));
    assert!(ts.iter().any(|t| t["name"] == "plan_group_disposition"));
}
#[test]
fn all_eight_project_original_receipts_once() {
    for op in OPS {
        let (r, w) = invoke(op, args(op), receipt(op), 200, false, true);
        assert_eq!(envelope(&r)["ok"], true, "{op}: {r}");
        assert_eq!(
            w.iter().filter(|w| w.starts_with("POST ")).count(),
            usize::from(*op != "status")
        );
        let d = &envelope(&r)["data"];
        assert!(d.get("journalCandidates").is_none());
        if *op != "plan" {
            assert!(d.get("ok").is_none());
        } else {
            assert_eq!(d["ok"], true);
        }
    }
}
#[test]
fn malformed_and_wrong_receipts_never_succeed() {
    for op in OPS {
        let mut v = receipt(op);
        v["disposition"]["left"]["companySlug"] = json!("unrelated");
        let (r, _) = invoke(op, args(op), v, 200, false, true);
        assert_eq!(envelope(&r)["ok"], false);
        assert_eq!(
            envelope(&r)["code"],
            if matches!(*op, "plan" | "status") {
                "protocol"
            } else {
                "outcome_unknown"
            }
        );
    }
}
#[test]
fn lost_write_once_and_current_anchor_denial() {
    let (r, w) = invoke("link", args("link"), Value::Null, 200, true, true);
    assert_eq!(envelope(&r)["code"], "outcome_unknown");
    assert_eq!(w.len(), 1);
    assert!(w[0].starts_with("POST /api/companies/left/group-dispositions/link "));
    let (r, w) = invoke(
        "status",
        args("status"),
        receipt("status"),
        200,
        false,
        false,
    );
    assert_eq!(envelope(&r)["code"], "access_denied");
    assert_eq!(w.len(), 1);
    assert!(w[0].starts_with("GET /api/companies "));
}
#[test]
fn credential_refusal_and_plan_failure_are_not_unknown() {
    for op in ["plan", "status", "approve"] {
        let (r, _) = invoke(
            op,
            args(op),
            json!({"ok":false,"code":"invalid_credentials"}),
            401,
            false,
            true,
        );
        assert_eq!(envelope(&r)["code"], "invalid_credentials");
    }
    let (r, _) = invoke("plan", args("plan"), Value::Null, 503, false, true);
    assert_ne!(envelope(&r)["code"], "outcome_unknown");
}
