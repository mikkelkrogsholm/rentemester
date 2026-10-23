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
fn response(stream: &mut impl Write, value: Value) {
    let text = value.to_string();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{text}",
        text.len()
    )
    .unwrap();
}
#[test]
fn existing_cookie_and_service_rpc_contracts_use_real_unix_read_and_exact_write() {
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", tcp.local_addr().unwrap());
    let fixture = Fixture::new(&origin);
    let listener = fixture.listener.as_ref().unwrap().try_clone().unwrap();
    let task = thread::spawn(move || {
        let mut requests = vec![];
        for value in [
            json!({"ok":true,"user":"synthetic"}),
            json!({"ok":true,"journalEntryId":7}),
            json!({"ok":true,"user":"service"}),
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            requests.push(request(&mut stream));
            response(&mut stream, value);
        }
        requests
    });
    let envelope = "{ \"confirm\":true,\"input\":{\"idempotencyKey\":\"synthetic-once\",\"text\":\"exact bytes\"} }";
    let output = run(
        &fixture.args("cookie"),
        &messages(vec![
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            call(3, "me", json!({})),
            call(
                4,
                "post_journal",
                json!({"company":"synthetic","envelope":envelope}),
            ),
        ]),
    );
    let parsed = rows(&output);
    let tools = parsed[1]["result"]["tools"].as_array().unwrap();
    let post = tools.iter().find(|t| t["name"] == "post_journal").unwrap();
    assert_eq!(post["annotations"]["readOnlyHint"], false);
    assert_eq!(
        post["inputSchema"]["properties"]["envelope"]["type"],
        "string"
    );
    for row in &parsed[2..] {
        let result = &row["result"];
        assert_eq!(result["isError"], false);
        assert_eq!(
            serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
            result["structuredContent"]
        );
    }
    assert_eq!(
        parsed[3]["result"]["structuredContent"]["data"]["journalEntryId"],
        7
    );
    let output = run(
        &fixture.args("service-key"),
        &messages(vec![call(2, "me", json!({}))]),
    );
    assert_eq!(
        rows(&output)[1]["result"]["structuredContent"]["data"]["user"],
        "service"
    );
    let requests = task.join().unwrap();
    assert!(requests[0].starts_with("GET /api/me "));
    assert!(requests[1].starts_with("POST /api/companies/synthetic/manual-journal "));
    assert_eq!(requests[1].split_once("\r\n\r\n").unwrap().1, envelope);
    assert!(requests[0].contains("cookie: session=synthetic-private"));
    assert!(requests[1].contains("cookie: session=synthetic-private"));
    assert!(requests[2].contains("x-rentemester-service-key: synthetic-service-private"));
    assert!(!requests[2].to_lowercase().contains("\r\ncookie:"));
    for r in &requests {
        assert!(r.to_lowercase().contains(&format!(
            "host: {}",
            origin.strip_prefix("http://").unwrap()
        )));
        assert!(r.to_lowercase().contains(&format!("origin: {origin}")));
    }
    tcp.set_nonblocking(true).unwrap();
    assert!(tcp.accept().is_err());
}
#[test]
fn invalid_selectors_conflicts_and_credential_exclusivity_are_startup_two() {
    let fixture = Fixture::new("http://127.0.0.1:9");
    let mut cases = vec![
        vec!["--local-profile".into()],
        vec![
            "--local-profile".into(),
            "/nonexistent/private".into(),
            "--cookie-file".into(),
            "/unread-cookie".into(),
        ],
        vec!["--cookie-file".into(), "/missing".into()],
        vec![
            "--local-profile".into(),
            fixture.path.to_str().unwrap().into(),
        ],
    ];
    for suffix in [
        vec!["--server", "http://127.0.0.1:9"],
        vec!["--allow-loopback-http"],
        vec!["--local-profile", "/duplicate"],
        vec!["--service-key-file", "/unread-key"],
    ] {
        let mut args = fixture.args("cookie");
        args.extend(suffix.into_iter().map(String::from));
        cases.push(args);
    }
    for args in cases {
        let output = run(&args, &[]);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
    let output = run(
        &[
            "--local-profile".into(),
            "/nonexistent/private".into(),
            "--cookie-file".into(),
            "/unread-cookie".into(),
        ],
        &[],
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("local runtime profile"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("cookie file unavailable"));
    fs::set_permissions(
        fixture.path.join("r.json"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let output = run(&fixture.args("cookie"), &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}
#[test]
fn stopped_unix_tool_errors_never_fall_back_to_live_tcp_and_missing_socket_is_startup_two() {
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", tcp.local_addr().unwrap());
    let mut fixture = Fixture::new(&origin);
    drop(fixture.listener.take());
    let output = run(
        &fixture.args("cookie"),
        &messages(vec![call(2, "me", json!({}))]),
    );
    let parsed = rows(&output);
    let result = &parsed[1]["result"];
    assert_eq!(result["isError"], true);
    assert_eq!(result["structuredContent"]["error"]["code"], "transport");
    assert_eq!(
        serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
        result["structuredContent"]
    );
    tcp.set_nonblocking(true).unwrap();
    assert!(tcp.accept().is_err());
    fs::remove_file(fixture.path.join("r.sock")).unwrap();
    let output = run(&fixture.args("cookie"), &[]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
}
#[test]
fn ordinary_tcp_and_pure_help_catalogue_contracts_are_retained() {
    for arg in ["--help", "--catalogue"] {
        let output = run(&[arg.into()], &[]);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        if arg == "--help" {
            assert!(String::from_utf8_lossy(&output.stdout).contains("--local-profile"));
        } else {
            let value: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(value["format"], "rentemester-native-mcp-catalogue/1");
        }
    }
    for arg in ["--help", "--catalogue"] {
        let output = run(
            &[arg.into(), "--local-profile".into(), "/missing".into()],
            &[],
        );
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", tcp.local_addr().unwrap());
    let fixture = Fixture::new(&origin);
    let task = thread::spawn(move || {
        let (mut stream, _) = tcp.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let wire = request(&mut stream);
        response(&mut stream, json!({"ok":true,"transport":"tcp"}));
        wire
    });
    let args = vec![
        "--server".into(),
        origin,
        "--allow-loopback-http".into(),
        "--cookie-file".into(),
        fixture.path.join("cookie").to_str().unwrap().into(),
    ];
    let output = run(&args, &messages(vec![call(2, "me", json!({}))]));
    assert_eq!(
        rows(&output)[1]["result"]["structuredContent"]["data"]["transport"],
        "tcp"
    );
    assert!(
        task.join()
            .unwrap()
            .contains("cookie: session=synthetic-private")
    );
}

#[test]
fn knowledge_service_root_slug_exact_writes_and_discovery() {
    let fixture = Fixture::new("http://127.0.0.1:9");
    fs::create_dir(fixture.path.join("workspace/synthetic")).unwrap();
    fs::create_dir(fixture.path.join("workspace/b")).unwrap();
    fixture.file("workspace/workspace.json",json!({"version":1,"companies":[{"slug":"synthetic","name":"Synthetic","createdAt":"2026-01-01","archived":false}]}).to_string().as_bytes());
    std::os::unix::fs::symlink(fixture.path.join("workspace/synthetic"),fixture.path.join("workspace/alias")).unwrap();
    let listener=fixture.listener.as_ref().unwrap().try_clone().unwrap();
    let task=thread::spawn(move || {
        let mut requests=vec![];
        for _ in 0..5 {
            let (mut stream,_)=listener.accept().unwrap();
            requests.push(request(&mut stream));
            response(&mut stream,json!({"ok":true,"history":[]}));
        }
        listener.set_nonblocking(true).unwrap();
        assert!(listener.accept().is_err());
        requests
    });
    let root=fixture.path.join("workspace/synthetic").to_str().unwrap().to_owned();
    let proposal = r#"{ "predicate":"markets", "value":["DK"], "source":{"kind":"user","ref":"synthetic"}, "validFrom":"2026-01-01", "confirm":true }"#;
    let review = r#"{ "assertionId":"assertion705", "decision":"approved", "confirm":true }"#;
    let supersede = r#"{ "assertionId":"assertion705", "replacement":{"predicate":"markets","value":"EU","source":{"kind":"user","ref":"synthetic"},"validFrom":"2026-01-01"}, "confirm":true }"#;
    let output=run(&fixture.args("service-key"),&messages(vec![
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        call(3,"company_knowledge",json!({"company":"synthetic"})),
        call(4,"company_knowledge",json!({"company":root})),
        call(5,"propose_company_knowledge",json!({"company":root,"body":proposal,"confirm":true})),
        call(6,"review_company_knowledge",json!({"company":root,"body":review,"confirm":true})),
        call(7,"supersede_company_knowledge",json!({"company":root,"body":supersede,"confirm":true})),
        call(8,"company_knowledge",json!({"company":fixture.path.join("workspace/b")})),
        call(9,"company_knowledge",json!({"company":fixture.path.join("workspace/alias")})),
        call(10,"company_knowledge",json!({"company":fixture.path})),
        call(11,"review_company_knowledge",json!({"company":root,"body":r#"{"assertionId":"assertion705","decision":"approved","actor":"user:forged","confirm":true}"#,"confirm":true})),
    ]));
    let parsed=rows(&output);
    let tools=parsed[1]["result"]["tools"].as_array().unwrap();
    for name in ["company_knowledge","propose_company_knowledge","review_company_knowledge","supersede_company_knowledge"] {
        let tool=tools.iter().find(|t|t["name"]==name).unwrap();
        assert_eq!(tool["inputSchema"]["additionalProperties"],false);
        assert!(tool["inputSchema"]["properties"]["company"]["description"].as_str().unwrap().contains("--local-profile"));
    }
    for row in &parsed[2..7] { assert_eq!(row["result"]["isError"],false,"{row}"); }
    for row in &parsed[7..] { assert!(row["result"]["isError"]==true || row.get("error").is_some(),"{row}"); }
    let requests=task.join().unwrap();
    assert_eq!(requests[0].lines().next(),requests[1].lines().next());
    for (i,suffix,body) in [(2,"propose",proposal),(3,"review",review),(4,"supersede",supersede)] {
        assert!(requests[i].starts_with(&format!("POST /api/companies/synthetic/knowledge/{suffix} HTTP/1.1")));
        assert!(requests[i].ends_with(body));
    }
    for wire in requests {
        assert!(wire.to_lowercase().contains("x-rentemester-service-key: synthetic-service-private"));
        assert!(!wire.to_lowercase().contains("cookie:"));
    }
}
