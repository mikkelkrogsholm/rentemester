//! Work-Issue: #717. Compiled stdio original public-invoice proof.
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
        "invoice717-mcp-{}-{:?}.cookie",
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
// Based on registry_original717::fixture: actual committed manifest, bare discovery,
// then authenticated mutation, with a bounded post-response observation window.
fn fixture(
    status: u16,
    text: String,
    truncated: bool,
) -> (String, thread::JoinHandle<Vec<(String, Value)>>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    l.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let mut requests = vec![];
        let mut idle = Instant::now();
        let mut posted = false;
        while idle.elapsed()
            < if posted {
                Duration::from_millis(400)
            } else {
                Duration::from_secs(5)
            }
        {
            let (mut s, _) = match l.accept() {
                Ok(s) => s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            idle = Instant::now();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut bytes = vec![];
            while !bytes.ends_with(b"\r\n\r\n") {
                let mut b = [0];
                s.read_exact(&mut b).unwrap();
                bytes.push(b[0]);
            }
            let h = String::from_utf8(bytes).unwrap();
            let lower = h.to_lowercase();
            let len = lower
                .lines()
                .find_map(|l| {
                    l.strip_prefix("content-length:")
                        .map(|n| n.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            let mut body = vec![0; len];
            s.read_exact(&mut body).unwrap();
            let line = h.lines().next().unwrap().to_owned();
            let (code, response, cut) = if line == "GET /api/protocol HTTP/1.1" {
                assert!(requests.is_empty());
                assert!(!lower.contains("cookie:") && !lower.contains("authorization:"));
                requests.push((line, Value::Null));
                (
                    200,
                    include_str!("../../../protocol/http-v1.json").to_owned(),
                    false,
                )
            } else {
                assert!(line.starts_with("POST "), "no readback: {line}");
                assert!(lower.contains("cookie: session=synthetic"));
                assert!(lower.contains("x-rentemester-manifest:"));
                requests.push((line, serde_json::from_slice(&body).unwrap()));
                posted = true;
                (status, text.clone(), truncated)
            };
            write!(s,"HTTP/1.1 {code} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()+if cut {30} else {0}).unwrap();
        }
        requests
    });
    (origin, t)
}
fn args(peppol: bool) -> Value {
    let mut a =
        json!({"company":"synthetic","documentId":7,"invoiceNumber":" fallback æ ","confirm":true});
    if peppol {
        a["accessPoint"] = json!({"accessPointId":" raw identity ","endpointUrl":"https://metadata.invalid/never-fetch","senderEndpointId":" raw sender "});
    }
    a
}
fn name(peppol: bool) -> &'static str {
    if peppol {
        "peppol_submit_public_invoice"
    } else {
        "efaktura_send"
    }
}
fn receipt(status: &str, duplicate: bool) -> Value {
    // sha256("hello\n"), shared existing fixture artifact.
    json!({"ok":true,"errors":[],"appliedRules":["source-rule"],"invoiceNumber":"INV æ","submissionReference":"native/reference","idempotencyKey":"server-key","oioublSha256":"oioubl-evidence","envelope":"hello\n","envelopeSha256":"5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03","status":status,"duplicate":duplicate,"transmissionId":"actual/id","redeliveryBlocked":true,"legacy":false,"providerStatus":"queued","history":[{"stateHash":"canonical-evidence"}]})
}
#[test]
fn catalogue_original_grammar_annotations_and_native_coexistence() {
    let out = Command::new(BIN).arg("--catalogue").output().unwrap();
    assert!(out.status.success());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let tools = v["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 431);
    let rows = run(
        "http://127.0.0.1:9",
        session(vec![json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})]),
    );
    assert_eq!(&rows[1]["result"]["tools"], &v["tools"]);

    for p in [true, false] {
        let t = tools.iter().find(|t| t["name"] == name(p)).unwrap();
        assert_eq!(
            t["annotations"],
            json!({"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":!p})
        );
        assert_eq!(
            t["inputSchema"]["required"],
            if p {
                json!(["company", "accessPoint"])
            } else {
                json!(["company"])
            }
        );
        assert!(
            t["inputSchema"]["properties"]["company"]
                .get("pattern")
                .is_none()
        );
        assert!(
            t["inputSchema"]["properties"]["invoiceNumber"]
                .get("maxLength")
                .is_none()
        );
        assert!(t["inputSchema"]["properties"].get("body").is_none());
    }
    for n in [
        "send_public_invoice",
        "refresh_public_invoice_status",
        "register_public_invoice",
    ] {
        let t = tools.iter().find(|t| t["name"] == n).unwrap();
        assert!(
            t["inputSchema"]["required"]
                .as_array()
                .unwrap()
                .contains(&json!("body"))
        );
        assert_eq!(t["inputSchema"]["properties"]["confirm"]["const"], true);
    }
}
#[test]
fn both_routes_exact_payload_and_canonical_evidence_once() {
    for (p, status, duplicate) in [
        (true, "prepared", false),
        (true, "acknowledged", true),
        (false, "prepared", false),
        (false, "uncertain", true),
    ] {
        let expected = receipt(status, duplicate);
        let (origin, t) = fixture(200, expected.to_string(), false);
        let mut a = args(p);
        // Mathematical integers in decimal/exponent representation normalize safely.
        a["documentId"] = serde_json::from_str(if p { "7.0" } else { "7e0" }).unwrap();
        if p {
            a["acknowledgement"] =
                json!({"transmissionId":" caller assertion ","acknowledgedAt":" not parsed "});
        } else {
            a["digisenseCompanyKey"] = json!("assert-current");
        }
        let rows = run(&origin, session(vec![call(2, name(p), a.clone())]));
        let e = envelope(&rows[1]);
        assert_eq!(e["ok"], true, "{e}");
        assert_eq!(e["appliedRules"], expected["appliedRules"]);
        let mut data = expected;
        for k in ["ok", "errors", "appliedRules"] {
            data.as_object_mut().unwrap().remove(k);
        }
        assert_eq!(e["data"], data);
        assert!(e["data"].get("ok").is_none());
        let req = t.join().unwrap();
        assert_eq!(req.len(), 2);
        assert_eq!(
            req[1].0,
            format!(
                "POST /api/companies/synthetic/invoices/original/{} HTTP/1.1",
                if p {
                    "submit-public-peppol"
                } else {
                    "transmit-digisense"
                }
            )
        );
        a.as_object_mut().unwrap().remove("company");
        a["documentId"] = json!(7);
        if let Some(key) = a.as_object_mut().unwrap().remove("digisenseCompanyKey") {
            a["companyKey"] = key;
        }
        assert_eq!(req[1].1, a);
    }
}
#[test]
fn omitted_optionals_remain_absent() {
    for p in [true, false] {
        let (origin, t) = fixture(200, receipt("prepared", false).to_string(), false);
        let mut a = args(p);
        a.as_object_mut().unwrap().remove("invoiceNumber");
        let rows = run(&origin, session(vec![call(2, name(p), a)]));
        assert_eq!(envelope(&rows[1])["ok"], true);
        let req = t.join().unwrap();
        assert_eq!(req.len(), 2);
        for k in [
            "invoiceNumber",
            "acknowledgement",
            "companyKey",
            "actor",
            "company",
        ] {
            assert!(req[1].1.get(k).is_none());
        }
    }
}
#[test]
fn grammar_then_confirmation_then_business_boundaries_without_post() {
    let mut calls = vec![];
    let mut expected = vec![];
    for p in [true, false] {
        for confirm in [None, Some(false)] {
            let mut a = args(p);
            a["company"] = json!("/absolute/company");
            if !p {
                a["accessPoint"] = Value::Null;
            }
            a.as_object_mut().unwrap().remove("confirm");
            if let Some(c) = confirm {
                a["confirm"] = json!(c);
            }
            calls.push(call(calls.len() as u64 + 2, name(p), a));
            expected.push("CONFIRM_REQUIRED");
        }
        for (k, v) in [
            ("documentId", json!("1")),
            ("documentId", json!(true)),
            ("documentId", json!(0)),
            ("documentId", json!(1.5)),
            ("documentId", json!(9007199254740992u64)),
            ("documentId", Value::Null),
            ("invoiceNumber", Value::Null),
            ("confirm", Value::Null),
            ("actor", json!("user:other")),
            ("company", json!("")),
        ] {
            let mut a = args(p);
            a[k] = v;
            calls.push(call(calls.len() as u64 + 2, name(p), a));
            expected.push("parse");
        }
        for company in ["/absolute/company", "../company", " "] {
            let mut a = args(p);
            a["company"] = json!(company);
            calls.push(call(calls.len() as u64 + 2, name(p), a));
            expected.push("invalid_input");
        }
        for number in ["", " ", &"x".repeat(257)] {
            let mut a = args(p);
            a["invoiceNumber"] = json!(number);
            calls.push(call(calls.len() as u64 + 2, name(p), a));
            expected.push(if !p && number.is_empty() {
                "parse"
            } else {
                "invalid_input"
            });
        }
        let mut a = args(p);
        a.as_object_mut().unwrap().remove("documentId");
        a.as_object_mut().unwrap().remove("invoiceNumber");
        calls.push(call(calls.len() as u64 + 2, name(p), a));
        expected.push("invalid_input");
    }
    for ap in [
        Value::Null,
        json!({}),
        json!(true),
        json!([]),
        json!("metadata"),
        json!(1),
    ] {
        let mut a = args(false);
        a["accessPoint"] = ap;
        calls.push(call(calls.len() as u64 + 2, name(false), a));
        expected.push("forbidden");
    }
    for (key, value) in [
        ("accessPoint", Value::Null),
        (
            "accessPoint",
            json!({"accessPointId":"x","endpointUrl":"x","senderEndpointId":"x","extra":1}),
        ),
        ("acknowledgement", Value::Null),
    ] {
        let mut a = args(true);
        a[key] = value;
        calls.push(call(calls.len() as u64 + 2, name(true), a));
        expected.push("parse");
    }
    for (p, k, v) in [
        (false, "digisenseCompanyKey", json!("x".repeat(513))),
        (
            true,
            "accessPoint",
            json!({"accessPointId":"\u{1}","endpointUrl":"x","senderEndpointId":"x"}),
        ),
    ] {
        let mut a = args(p);
        a[k] = v;
        calls.push(call(calls.len() as u64 + 2, name(p), a));
        expected.push("invalid_input");
    }
    // Valid individual metadata fields, but over the native aggregate body bound.
    let mut a = args(true);
    a["accessPoint"] = json!({"accessPointId":"x".repeat(4000),"endpointUrl":"x".repeat(4000),"senderEndpointId":"x".repeat(4000)});
    a["acknowledgement"] =
        json!({"transmissionId":"x".repeat(4000),"acknowledgedAt":"x".repeat(4000)});
    calls.push(call(calls.len() as u64 + 2, name(true), a));
    expected.push("invalid_input");
    let rows = run("http://127.0.0.1:9", session(calls));
    for (row, expect) in rows[1..].iter().zip(expected) {
        if expect == "parse" {
            assert_eq!(row["error"]["code"], -32602, "{row}");
        } else {
            let e = envelope(row);
            assert_eq!(e["ok"], false);
            if expect == "forbidden" {
                assert_eq!(
                    e["errors"],
                    json!([
                        "accessPoint is not allowed; Digisense identity is derived from companyKey"
                    ])
                );
            } else {
                assert_eq!(e["code"], expect, "{e}");
            }
        }
    }
}
#[test]
fn authoritative_refusals_and_unknown_receipts_never_retry_or_readback() {
    for (status,text,cut,code) in [
        (422,json!({"ok":false,"code":"EXACT_SERVER_CODE","errors":["does not exist is not a code heuristic"],"appliedRules":["refusal-rule"]}).to_string(),false,"EXACT_SERVER_CODE"),
        (422,json!({"ok":false,"errors":["missing authoritative code"]}).to_string(),false,"server_refused"),
        (403,json!({"ok":false,"code":"service_refused"}).to_string(),false,"server_refused"),
        (200,"not json".into(),false,"outcome_unknown"),
        (200,receipt("prepared",false).to_string(),true,"outcome_unknown"),
        (503,json!({"ok":false,"code":"unavailable"}).to_string(),false,"outcome_unknown"),
    ] {
        for p in [true,false] {
            let (origin,t)=fixture(status,text.clone(),cut); let rows=run(&origin,session(vec![call(2,name(p),args(p))]));
            let e=envelope(&rows[1]);assert_eq!(e["code"],code,"{e}");
            if code=="EXACT_SERVER_CODE" { assert_eq!(e["errors"],json!(["does not exist is not a code heuristic"]));assert_eq!(e["appliedRules"],json!(["refusal-rule"])); }
            let req=t.join().unwrap();assert_eq!(req.len(),2,"one discovery + one POST only");
        }
    }
}
