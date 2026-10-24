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
        "profile89-mcp-{}-{:?}.cookie",
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
// Mirrors storage::company_setup::profile, including native-only payment/createdAt.
fn profile() -> Value {
    json!({"id":1,"name":"Synthetic Shifted Company","country":"DK","currency":"EUR","cvr":"12345678","fiscalYearStartMonth":7,"fiscalYearLabelStrategy":"span","address":"Synthetic Street 9","postalCode":"1234","city":"Fixture","companyForm":"ApS","industryCode":"620100","industryText":"Synthetic services","cvrStatus":"active","auditWaived":false,"cvrSyncedAt":"2026-01-02T03:04:05Z","paymentTermsDays":30,"vatPeriodType":"half-year","createdAt":"2025-01-01T00:00:00Z","payment":{"bankName":"Synthetic","registrationNo":null,"accountNo":"123456","iban":null}})
}
fn source(p: &Value) -> Value {
    let mut p = p.clone();
    let o = p.as_object_mut().unwrap();
    o.remove("payment");
    o.remove("createdAt");
    p["vatRegistered"] = json!(!p["vatPeriodType"].is_null());
    p
}
// Each fixture has exactly one protocol admission, then the specified GETs.
fn fixture(
    responses: Vec<(u16, Value)>,
    bad_protocol: bool,
) -> (String, thread::JoinHandle<Vec<String>>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    l.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let mut lines = vec![];
        let mut responses = responses.into_iter();
        let mut idle = Instant::now();
        while idle.elapsed() < Duration::from_secs(1) {
            let (mut s, _) = match l.accept() {
                Ok(s) => s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            idle = Instant::now();
            s.set_nonblocking(false).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut b = vec![];
            while !b.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                s.read_exact(&mut byte).unwrap();
                b.push(byte[0]);
            }
            let h = String::from_utf8(b).unwrap();
            let line = h.lines().next().unwrap().to_owned();
            let (status, text) = if line == "GET /api/protocol HTTP/1.1" {
                assert!(!h.to_lowercase().contains("cookie:"));
                (
                    200,
                    if bad_protocol {
                        "{}".into()
                    } else {
                        include_str!("../../../protocol/http-v1.json").into()
                    },
                )
            } else {
                assert_eq!(line, "GET /api/companies/synthetic/company HTTP/1.1");
                assert!(h.to_lowercase().contains("cookie: session=synthetic"));
                assert!(h.to_lowercase().contains("x-rentemester-manifest:"));
                let (status, v) = responses.next().expect("unexpected retry/request");
                (status, v.to_string())
            };
            lines.push(line);
            write!(s,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",text.len()).unwrap();
        }
        assert!(responses.next().is_none(), "missing native request");
        lines
    });
    (origin, t)
}
fn profile_call(id: u64, args: Value) -> Value {
    call(id, "company_profile_get", args)
}
#[test]
fn full_original_profile_shifted_nonvat_and_nullable_are_exact() {
    let p = profile();
    let mut n = p.clone();
    for k in [
        "cvr",
        "address",
        "postalCode",
        "city",
        "companyForm",
        "industryCode",
        "industryText",
        "cvrStatus",
        "cvrSyncedAt",
        "auditWaived",
        "vatPeriodType",
    ] {
        n[k] = Value::Null;
    }
    n["fiscalYearStartMonth"] = json!(10);
    n["fiscalYearLabelStrategy"] = json!("end-year");
    n["payment"] = Value::Null;
    let mut month = p.clone();
    month["vatPeriodType"] = json!("month");
    let mut quarter = p.clone();
    quarter["vatPeriodType"] = json!("quarter");
    let profiles = vec![p, n, month, quarter];
    let (origin, t) = fixture(
        profiles
            .iter()
            .map(|p| (200, json!({"ok":true,"company":p})))
            .collect(),
        false,
    );
    let mut calls = vec![json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})];
    calls.extend(
        (0..profiles.len()).map(|i| profile_call(i as u64 + 3, json!({"company":"synthetic"}))),
    );
    let rows = run(&origin, session(calls));
    let lines = t.join().unwrap();
    assert_eq!(lines.iter().filter(|s| s.as_str() == "GET /api/protocol HTTP/1.1").count(), profiles.len());
    assert_eq!(lines.iter().filter(|s| s.as_str() == "GET /api/companies/synthetic/company HTTP/1.1").count(), profiles.len());
    let tools = rows[1]["result"]["tools"].as_array().unwrap();
    let tool = tools
        .iter()
        .find(|t| t["name"] == "company_profile_get")
        .unwrap();
    assert_eq!(tool["title"], "Get the stored company profile");
    assert_eq!(
        tool["annotations"],
        json!({"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false})
    );
    assert_eq!(tool["inputSchema"]["required"], json!(["company"]));
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    for (r, p) in rows[2..].iter().zip(profiles) {
        let e = envelope(r);
        assert_eq!(
            e,
            json!({"ok":true,"data":{"profile":source(&p)},"errors":[]})
        );
        assert_eq!(e["data"]["profile"].as_object().unwrap().len(), 19);
    }
}
#[test]
fn malformed_source_arguments_are_parse_errors_without_http() {
    let (origin, t) = fixture(vec![], false);
    let args = vec![
        json!({}),
        json!({"company":null}),
        json!({"company":1}),
        json!({"company":true}),
        json!({"company":[]}),
        json!({"company":{}}),
        json!({"company":""}),
        json!({"company":"synthetic","actor":"user:other"}),
        json!({"company":"synthetic","principal":"other"}),
        json!({"company":"synthetic","companySlug":"other"}),
        json!({"company":"synthetic","confirm":null}),
        Value::Null,
        json!([]),
    ];
    let rows = run(
        &origin,
        session(
            args.into_iter()
                .enumerate()
                .map(|(i, a)| profile_call(i as u64 + 2, a))
                .collect(),
        ),
    );
    for r in &rows[1..] {
        assert_eq!(r["error"]["code"], -32602, "{r}");
    }
    assert!(t.join().unwrap().is_empty());
}
#[test]
fn valid_unbound_path_is_business_refusal_not_parse_or_local_fallback() {
    let (origin, t) = fixture(vec![], false);
    let rows = run(
        &origin,
        session(vec![profile_call(
            2,
            json!({"company":"/unavailable/company"}),
        )]),
    );
    let e = envelope(&rows[1]);
    assert_eq!(e["ok"], false);
    assert_eq!(e["code"], "invalid_input");
    assert!(t.join().unwrap().is_empty());
}
#[test]
fn source_envelopes_native_extras_and_every_missing_or_wrong_field_fail_closed() {
    let p = profile();
    let mut bad = vec![
        json!({"ok":true,"data":{"profile":source(&p)},"errors":[]}),
        json!({"ok":true,"profile":source(&p)}),
        json!({"ok":true,"company":p,"errors":[]}),
        json!({"ok":true,"company":p,"data":{}}),
        json!({"ok":false,"errors":[]}),
        json!({"ok":false,"errors":[1]}),
        json!({"ok":true,"company":{}}),
    ];
    for k in p.as_object().unwrap().keys() {
        let mut missing = p.clone();
        missing.as_object_mut().unwrap().remove(k);
        bad.push(json!({"ok":true,"company":missing}));
        let mut wrong = p.clone();
        wrong[k] = json!({"wrong":true});
        bad.push(json!({"ok":true,"company":wrong}));
    }
    for (k, v) in [
        ("vatRegistered", json!(true)),
        ("vatPeriodType", json!("weekly")),
        ("fiscalYearStartMonth", json!(0)),
        ("fiscalYearLabelStrategy", json!("unknown")),
    ] {
        let mut wrong = p.clone();
        wrong[k] = v;
        bad.push(json!({"ok":true,"company":wrong}));
    }
    let count = bad.len();
    let (origin, t) = fixture(bad.into_iter().map(|v| (200, v)).collect(), false);
    let rows = run(
        &origin,
        session(
            (0..count)
                .map(|i| profile_call(i as u64 + 2, json!({"company":"synthetic"})))
                .collect(),
        ),
    );
    for r in &rows[1..] {
        let e = envelope(r);
        assert_eq!(e["ok"], false, "{r}");
        assert_eq!(e["code"], "protocol");
        assert!(e.get("data").is_none());
    }
    assert_eq!(t.join().unwrap().len(), 2 * count);
}
#[test]
fn business_denial_protocol_admission_and_transport_are_tool_failures() {
    let business =
        json!({"ok":false,"errors":["Canonical profile unavailable"],"code":"BACKUP_LOCKED"});
    let (origin, t) = fixture(
        vec![
            (200, business.clone()),
            (403, json!({"ok":false,"error":"forbidden"})),
        ],
        false,
    );
    let rows = run(
        &origin,
        session(vec![
            profile_call(2, json!({"company":"synthetic"})),
            profile_call(3, json!({"company":"synthetic"})),
        ]),
    );
    assert_eq!(envelope(&rows[1]), business);
    assert_eq!(envelope(&rows[2])["code"], "server_refused");
    assert_eq!(t.join().unwrap().len(), 4);
    let (origin, t) = fixture(vec![], true);
    let rows = run(
        &origin,
        session(vec![profile_call(2, json!({"company":"synthetic"}))]),
    );
    assert_eq!(envelope(&rows[1])["code"], "protocol");
    assert_eq!(t.join().unwrap().len(), 1);
    let rows = run(
        "http://127.0.0.1:9",
        session(vec![profile_call(2, json!({"company":"synthetic"}))]),
    );
    assert_eq!(envelope(&rows[1])["ok"], false);
}
