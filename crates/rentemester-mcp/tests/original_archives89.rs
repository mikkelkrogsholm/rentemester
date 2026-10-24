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

fn call(name: &str, args: Value) -> Value {
    json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":name,"arguments":args}})
}
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
                assert!(line.starts_with("GET /api/companies/synthetic/archives"), "{line}");
                assert!(h.to_lowercase().contains("cookie: session=synthetic-private"));
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
fn invoke(name: &str, args: Value, responses: Vec<(u16, Value)>) -> (Value, Vec<String>) {
    let (origin, server) = fixture(responses, false);
    let output = run(&origin, vec![init(), ready(), call(name, args)]);
    let wires = server.join().expect("known bounded fixture");
    (output[1].clone(), wires)
}

fn envelope(v: &Value) -> &Value {
    let result = &v["result"];
    let e = &result["structuredContent"];
    assert_eq!(result["isError"], e["ok"] != true);
    assert_eq!(
        serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
        *e
    );
    e
}
// Exact native history::read fields, including deliberately different quantized
// Money and raw ArchiveDecimal witnesses; proof-only metadata is not source ABI.
fn header() -> Value {
    json!({"fiscalYear":2024,"sourceSystem":"dinero","postingCount":3,"balanceCount":1,"importedAt":"2026-10-20T12:00:00.000Z","importId":1,"sourceHash":"a".repeat(64),"parserContract":"rentemester-native-closed-history/1","declaredCurrency":"DKK","hasBalances":true})
}
fn reply(years: Vec<Value>) -> Value {
    json!({"ok":true,"years":years,"errors":[]})
}
fn detail() -> Value {
    let mut h = header();
    h["postings"] = json!([("1.234567","1.23", "-1.234567"),("0.000001","0.00","0.000001"),("-1.234567","-1.23","0.000000")].iter().enumerate().map(|(i,(raw,money,running))| json!({"lineNo":i,"accountNo":"6200","accountName":"Synthetic","transactionDate":"2024-06-01","voucher":"1","voucherType":"Historical","text":"Synthetic","vatType":"","amount":money,"sourceAmount":raw,"runningBalance":"0.00","sourceRunningBalance":running})).collect::<Vec<_>>());
    h["balances"] = json!([{"lineNo":0,"accountNo":"6200","accountName":"Synthetic","amount":"-1.23","sourceAmount":"-1.234567"}]);
    h
}
#[test]
fn populated_original_list_and_detail_keep_raw_precision_and_source_shape() {
    let (r, w) = invoke(
        "import_archive_list",
        json!({"company":"synthetic"}),
        vec![(200, reply(vec![header()]))],
    );
    let e = envelope(&r);
    assert_eq!(e["ok"], true);
    assert_eq!(e["data"]["years"][0].as_object().unwrap().len(), 5);
    assert!(
        w.iter()
            .any(|w| w.starts_with("GET /api/companies/synthetic/archives "))
    );
    let (r, w) = invoke(
        "import_archive_year",
        json!({"company":"synthetic","fiscalYear":2024,"sourceSystem":"dinero"}),
        vec![(200, reply(vec![header()])), (200, reply(vec![detail()]))],
    );
    let e = envelope(&r);
    assert_eq!(e["ok"], true, "{r}");
    let h = &e["data"]["years"][0];
    assert_eq!(h.as_object().unwrap().len(), 7);
    for (i, expected) in [1.234567, 0.000001, -1.234567].iter().enumerate() {
        assert_eq!(h["postings"][i]["amount"], json!(expected));
    }
    assert_eq!(h["postings"][0]["runningBalance"], json!(-1.234567));
    assert_eq!(h["postings"][1]["runningBalance"], json!(0.000001));
    assert_eq!(h["balances"][0]["amount"], json!(-1.234567));
    assert!(h["postings"][0].get("sourceAmount").is_none());
    assert!(
        w.iter()
            .any(|w| w.starts_with("GET /api/companies/synthetic/archives/2024 "))
    );
}
#[test]
fn source_filters_and_absence_require_authoritative_list() {
    for source in ["other", "Dinero", "dinero"] {
        let (r, w) = invoke(
            "import_archive_list",
            json!({"company":"synthetic","sourceSystem":source}),
            vec![(
                200,
                reply(if source == "dinero" {
                    vec![]
                } else {
                    vec![header()]
                }),
            )],
        );
        assert_eq!(envelope(&r)["data"]["years"], json!([]));
        assert!(!w.is_empty());
    }
    for year in [0, -1, 999, 10000, 9007199254740991i64, -9007199254740991] {
        let (r, w) = invoke(
            "import_archive_year",
            json!({"company":"synthetic","fiscalYear":year}),
            vec![(200, reply(vec![header()]))],
        );
        let e = envelope(&r);
        assert_eq!(
            e["errors"],
            json!([format!("no archived dinero data for fiscal year {year}")])
        );
        assert!(e.get("code").is_none());
        assert!(!w.iter().any(|w| w.contains("archives/")));
    }
    let (r, _) = invoke(
        "import_archive_year",
        json!({"company":"synthetic","fiscalYear":2024,"sourceSystem":"other"}),
        vec![(200, reply(vec![header()]))],
    );
    assert_eq!(envelope(&r)["ok"], false);
}
#[test]
fn malformed_source_inputs_are_rpc_errors_without_archive_http() {
    for args in [
        json!({"company":"synthetic","fiscalYear":null}),
        json!({"company":"synthetic","fiscalYear":1.5}),
        json!({"company":"synthetic","fiscalYear":9007199254740992u64}),
        json!({"company":"synthetic","fiscalYear":2024,"sourceSystem":null}),
        json!({"company":"synthetic","fiscalYear":2024,"extra":true}),
        json!({"company":false,"fiscalYear":2024}),
    ] {
        let (r, w) = invoke("import_archive_year", args, vec![]);
        assert_eq!(r["error"]["code"], -32602);
        assert!(w.iter().all(|s| !s.contains("/archives")));
    }
}
#[test]
fn current_access_loss_and_physical_tamper_after_list_never_return_cached_detail() {
    for (status, code) in [(403, "forbidden"), (409, "archive_original_unverified")] {
        let refusal = json!({"ok":false,"code":code,"errors":["current source access refused"]});
        let (r, _) = invoke(
            "import_archive_year",
            json!({"company":"synthetic","fiscalYear":2024}),
            vec![(200, reply(vec![header()])), (status, refusal)],
        );
        let e = envelope(&r);
        assert_eq!(e["ok"], false);
        assert_eq!(e["code"], code);
        assert!(e.get("data").is_none());
        let (r, _) = invoke(
            "import_archive_list",
            json!({"company":"synthetic","sourceSystem":"unknown"}),
            vec![(
                status,
                json!({"ok":false,"code":code,"errors":["current source access refused"]}),
            )],
        );
        assert_eq!(envelope(&r)["ok"], false);
    }
}
#[test]
fn missing_raw_proofs_and_unrepresentable_numbers_refuse_but_null_and_zero_survive() {
    for mode in ["missing", "rounded", "huge"] {
        let mut d = detail();
        match mode {
            "missing" => {
                d["postings"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("sourceAmount");
            }
            "rounded" => d["postings"][0]["sourceAmount"] = json!("1.23"),
            _ => d["postings"][0]["sourceAmount"] = json!("9007199254740990.123456"),
        }
        let (r, _) = invoke(
            "import_archive_year",
            json!({"company":"synthetic","fiscalYear":2024}),
            vec![(200, reply(vec![header()])), (200, reply(vec![d]))],
        );
        assert_eq!(envelope(&r)["ok"], false, "{r}");
    }
    let mut d = detail();
    d["postings"][0]["sourceAmount"] = json!("0.000000");
    d["postings"][0]["sourceRunningBalance"] = Value::Null;
    let (r, _) = invoke(
        "import_archive_year",
        json!({"company":"synthetic","fiscalYear":2024}),
        vec![(200, reply(vec![header()])), (200, reply(vec![d]))],
    );
    let e = envelope(&r);
    assert_eq!(e["ok"], true, "{r}");
    assert_eq!(e["data"]["years"][0]["postings"][0]["amount"], json!(0.0));
    assert!(e["data"]["years"][0]["postings"][0]["runningBalance"].is_null());
}
