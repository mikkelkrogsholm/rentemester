#![cfg(unix)]
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    os::unix::{
        fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
        net::UnixListener,
    },
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-mcp");
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    path: PathBuf,
    listener: Option<UnixListener>,
}
impl Fixture {
    fn new(origin: &str) -> Self {
        let home = fs::canonicalize(std::env::var_os("HOME").unwrap()).unwrap();
        let path = home.join(format!(
            ".rm139-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
        let mut fixture = Self {
            path,
            listener: None,
        };
        fs::DirBuilder::new()
            .mode(0o700)
            .create(fixture.path.join("workspace"))
            .unwrap();
        fixture.listener = Some(UnixListener::bind(fixture.path.join("r.sock")).unwrap());
        fs::set_permissions(
            fixture.path.join("r.sock"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        fixture.file("r.json",json!({"format":"rentemester-local-runtime/1","origin":origin,"socket":"r.sock","workspaceRoot":fixture.path.join("workspace"),"workspaceId":"synthetic-workspace"}).to_string().as_bytes());
        fixture.file("cookie", b"session=synthetic-private");
        fixture.file("key", b"synthetic-service-private");
        fixture
    }
    fn file(&self, name: &str, bytes: &[u8]) {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(self.path.join(name))
            .unwrap()
            .write_all(bytes)
            .unwrap();
    }
    fn args(&self, credential: &str) -> Vec<String> {
        vec![
            "--local-profile".into(),
            self.path.to_str().unwrap().into(),
            format!("--{credential}-file"),
            self.path
                .join(if credential == "cookie" {
                    "cookie"
                } else {
                    "key"
                })
                .to_str()
                .unwrap()
                .into(),
        ]
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
fn init() -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic","version":"1"}}})
}
fn call(id: i32, name: &str, args: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}})
}
fn messages(calls: Vec<Value>) -> Vec<Value> {
    let mut result = vec![
        init(),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    ];
    result.extend(calls);
    result
}
fn run(args: &[String], messages: &[Value]) -> Output {
    let bytes: Vec<u8> = messages
        .iter()
        .flat_map(|v| format!("{v}\n").into_bytes())
        .collect();
    let mut child = Command::new(BIN)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let writer = thread::spawn(move || input.write_all(&bytes));
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap().unwrap();
    output
}
fn rows(output: &Output) -> Vec<Value> {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(!text.contains("synthetic-private"));
    assert!(!text.contains("synthetic-service-private"));
    text.lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
fn bounded_unix_accept(listener: &std::os::unix::net::UnixListener) -> std::os::unix::net::UnixStream {
    listener.set_nonblocking(true).unwrap();
    let start = std::time::Instant::now();
    loop {
        match listener.accept() {
            Ok((stream, _)) => { stream.set_nonblocking(false).unwrap(); return stream; }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(start.elapsed() < std::time::Duration::from_secs(10), "expected bounded native CFO request");
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(error) => panic!("{error}"),
        }
    }
}
fn request(stream: &mut impl Read) -> String {
    let mut bytes = vec![];
    loop {
        let mut b = [0; 4096];
        let n = stream.read(&mut b).unwrap();
        assert!(n > 0);
        bytes.extend(&b[..n]);
        if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&bytes[..i]).to_lowercase();
            let len = head
                .lines()
                .find_map(|s| s.strip_prefix("content-length:"))
                .map(|s| s.trim().parse::<usize>().unwrap())
                .unwrap_or(0);
            if bytes.len() >= i + 4 + len {
                break;
            }
        }
    }
    String::from_utf8(bytes).unwrap()
}
fn selectors(workspace: Value, scope: &str) -> Value {
    json!({"workspace":workspace,"scope":scope,"from":"2024-01-01","to":"2024-12-31","companySlug":"synthetic"})
}
fn agree(row: &Value) -> &Value {
    let r = &row["result"];
    assert_eq!(
        serde_json::from_str::<Value>(r["content"][0]["text"].as_str().unwrap()).unwrap(),
        r["structuredContent"]
    );
    assert_eq!(r["isError"], r["structuredContent"]["ok"] != true);
    &r["structuredContent"]
}
#[test]
fn original_local_cfo_selectors_source_bodies_and_refusals() {
    let fixture = Fixture::new("http://127.0.0.1:9");
    let workspace = json!(fixture.path.join("workspace"));
    let ledger = json!({"ok":true,"schemaVersion":"rentemester-cfo-analytics-v1","scope":"company","status":"ready","asOf":"2024-12-31","from":"2024-01-01","to":"2024-12-31","companies":["synthetic"],"partial":false,"mode":"legal-company","aggregate":"none","limitations":[],"rows":[{"companySlug":"synthetic","sourceType":"archive","sourceHash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","sourceId":"archive:1:posting:2","journalEntryId":null,"journalEntryNo":"A-2","documentId":null,"documentHash":null,"partyId":null,"partyName":null,"documentPartyName":null,"partyLinkProvenance":null,"accountNo":"1000","accountName":null,"transactionDate":"2024-02-01","currency":"DKK","amount":12.34,"text":null,"dimensions":[]}],"page":{"limit":7,"nextCursor":"server-cursor"},"freshness":[{"source":"archive","companySlug":"synthetic","latestTransactionDate":"2024-02-01"}],"evidenceCompleteness":[{"companySlug":"synthetic","status":"ready","postedWithoutDocument":0,"openExceptions":1}],"reconciliation":{"rowCount":1,"amountByCurrency":{"DKK":12.34},"sourceHashes":["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],"method":"source linked"}});
    let mut portfolio = ledger.clone();
    portfolio["scope"] = json!("portfolio");
    portfolio["status"] = json!("incomplete");
    portfolio["partial"] = json!(true);
    portfolio["mode"] = json!("juxtaposed-non-consolidated");
    portfolio["reconciliation"] = json!({"rowCount":1,"sourceHashes":[],"method":"source linked","omitted":"portfolio aggregate not requested or access is partial"});
    let group = json!({"ok":true,"schemaVersion":"rentemester-cfo-analytics-v1","scope":"group","status":"unsupported","asOf":"2024-12-31","limitations":["The approved consolidated report is blocked; no analytics totals are manufactured."],"group":{"status":"blocked","blockers":["missing approved source mapping"]}});
    let expected = vec![ledger.clone(), portfolio.clone(), group.clone()];
    let listener = fixture.listener.as_ref().unwrap().try_clone().unwrap();
    let replies = vec![
        (200, ledger),
        (200, portfolio),
        (200, group),
        (
            403,
            json!({"ok":false,"code":"access_denied","errors":["company is not accessible"]}),
        ),
        (
            200,
            json!({"ok":false,"code":"cfo_analytics_refused","errors":["invalid from date"]}),
        ),
        (200, json!({"ok":true})),
    ];
    let task = thread::spawn(move || {
        let mut wires = vec![];
        for (status, body) in replies {
            let mut probe = bounded_unix_accept(&listener);
            probe.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
            let probe_wire = request(&mut probe);
            assert!(probe_wire.starts_with("GET /api/protocol HTTP/1.1"));
            assert!(!probe_wire.to_lowercase().contains("cookie:"));
            let manifest = include_str!("../../../protocol/http-v1.json");
            write!(probe,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{manifest}",manifest.len()).unwrap();
            drop(probe);
            let mut stream = bounded_unix_accept(&listener);
            stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
            let actual = request(&mut stream);
            assert!(actual.starts_with("GET /api/cfo-analytics?"));
            wires.push(actual);
            let body = body.to_string();
            write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",body.len()).unwrap();
        }
        listener.set_nonblocking(true).unwrap();
        assert!(listener.accept().is_err());
        wires
    });
    let mut company = selectors(workspace.clone(), "company");
    company["companySlugs"] = json!(["ignored-hidden"]);
    company["limit"] = json!(7);
    company["cursor"] = json!("exact-cursor");
    company["account"] = json!("1000");
    company["party"] = json!("canonical-party");
    company["dimension"] = json!("project:p");
    company["currency"] = json!("DKK");
    let mut portfolio = selectors(json!("synthetic-workspace"), "portfolio");
    portfolio["companySlugs"] = json!(["synthetic", "hidden", "synthetic"]);
    portfolio["aggregate"] = json!("sum");
    let mut group = selectors(workspace.clone(), "group");
    group["groupProfileId"] = json!("approved-profile");
    group["companySlugs"] = json!(["synthetic"]);
    let mut hidden = selectors(workspace.clone(), "company");
    hidden["companySlug"] = json!("hidden");
    let mut bad_date = selectors(workspace.clone(), "company");
    bad_date["from"] = json!("not-a-date");
    let mut calls = vec![
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        call(3, "cfo_analytics_query", company),
        call(4, "cfo_analytics_query", portfolio),
        call(5, "cfo_analytics_query", group),
        call(6, "cfo_analytics_query", hidden),
        call(7, "cfo_analytics_query", bad_date),
        call(
            8,
            "cfo_analytics_query",
            selectors(workspace.clone(), "company"),
        ),
        call(
            9,
            "cfo_analytics_query",
            selectors(json!("wrong-workspace"), "company"),
        ),
    ];
    for (i, change) in [
        json!({"account":null}),
        json!({"unknown":true}),
        json!({"scope":"invalid"}),
        json!({"limit":0}),
        json!({"companySlugs":vec!["x";101]}),
        json!({"companySlug":2}),
        json!({"workspace":""}),
    ]
    .into_iter()
    .enumerate()
    {
        let mut a = selectors(workspace.clone(), "company");
        a.as_object_mut()
            .unwrap()
            .extend(change.as_object().unwrap().clone());
        calls.push(call(10 + i as i32, "cfo_analytics_query", a));
    }
    let output = run(&fixture.args("cookie"), &messages(calls));
    let rows = rows(&output);
    let t = rows[1]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "cfo_analytics_query")
        .unwrap();
    assert_eq!(t["title"], "Query source-linked CFO analytics");
    assert_eq!(
        t["annotations"],
        json!({"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false})
    );
    assert_eq!(
        t["inputSchema"]["properties"]["companySlugs"]["maxItems"],
        100
    );
    for (row, mut body) in rows[2..5].iter().zip(expected) {
        body.as_object_mut().unwrap().remove("ok");
        assert_eq!(agree(row), &json!({"ok":true,"data":body,"errors":[]}));
    }
    for row in &rows[5..9] {
        assert_eq!(agree(row)["ok"], false);
        assert!(agree(row).get("data").is_none());
    }
    assert_eq!(agree(&rows[5])["code"], "access_denied");
    assert_eq!(agree(&rows[7])["code"], "protocol");
    for row in &rows[9..] {
        assert_eq!(row["error"]["code"], -32602, "{row}");
    }
    let wires = task.join().unwrap();
    assert_eq!(wires.len(), 6);
    assert!(wires[0].starts_with("GET /api/cfo-analytics?"));
    assert!(wires[0].contains("companySlug=synthetic"));
    assert!(!wires[0].contains("ignored-hidden"));
    for field in [
        "from=2024-01-01",
        "to=2024-12-31",
        "limit=7",
        "cursor=exact-cursor",
        "account=1000",
        "party=canonical-party",
        "dimension=project%3Ap",
        "currency=DKK",
    ] {
        assert!(wires[0].contains(field), "{}", wires[0]);
    }
    assert!(wires[1].contains("aggregate=sum"));
    assert!(wires[1].contains("companySlug=hidden"));
    assert!(!wires[1].contains("workspace"));
    assert!(wires[2].contains("groupProfileId=approved-profile"));
    for wire in wires {
        assert!(wire.contains("cookie: session=synthetic-private"));
    }
}
#[test]
fn original_remote_workspace_fails_closed_without_tcp() {
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", tcp.local_addr().unwrap());
    let fixture = Fixture::new(&origin);
    let args = vec![
        "--server".into(),
        origin,
        "--allow-loopback-http".into(),
        "--cookie-file".into(),
        fixture.path.join("cookie").to_str().unwrap().into(),
    ];
    let output = run(
        &args,
        &messages(vec![call(
            2,
            "cfo_analytics_query",
            selectors(json!("synthetic-workspace"), "company"),
        )]),
    );
    let rows = rows(&output);
    let e = agree(&rows[1]);
    assert_eq!(e["ok"], false);
    assert!(
        e["errors"][0]
            .as_str()
            .unwrap()
            .contains("remote workspace identity is unavailable")
    );
    tcp.set_nonblocking(true).unwrap();
    assert!(tcp.accept().is_err());
}
