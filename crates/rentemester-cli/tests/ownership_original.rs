#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
fn call(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .stdin(Stdio::null())
        .env_remove("RENTEMESTER_COMPANY")
        .env_remove("RENTEMESTER_WORKSPACE")
        .output()
        .unwrap()
}
fn private(path: &std::path::Path, bytes: &[u8]) {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(path).unwrap().write_all(bytes).unwrap();
}
fn snap(state: &str) -> Value {
    json!({"snapshotId":"source-token","source":"retained legal source","observedAt":"2026-06-01T00:00:00Z","snapshotHash":"a".repeat(64),"diffHash":"b".repeat(64),"diff":{"add":[],"endings":[],"conflicts":[]},"state":state,"actor":"user:synthetic","principal":{"kind":"user","id":"synthetic"},"history":[{"event_type":state,"actor":"user:synthetic","principal_kind":"user","principal_id":"synthetic","created_at":"2026-06-01T00:00:00Z"}]})
}
fn proposal() -> Value {
    json!({"snapshotId":"source-token","source":"retained legal source","observedAt":"2026-06-01T00:00:00Z","facts":[{"owner":{"kind":"company","companySlug":"synthetic"},"ownedCompanySlug":"synthetic","validFrom":"2026-06-01","economicBasisPoints":10000,"controlType":"equity","jurisdiction":"DK","evidenceRefs":["retained-legal-evidence"]}]})
}
fn observe(
    command: &str,
    flags: &[&str],
    status: u16,
    reply: &str,
    input: Option<Value>,
    me: Option<Value>,
    attached: bool,
    native: bool,
) -> (Output, Vec<(String, Value)>) {
    let listener = common::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let path = std::env::temp_dir().join(format!(
        "rm-ownership716-{}-{}",
        std::process::id(),
        origin.rsplit(':').next().unwrap()
    ));
    private(&path, b"session=synthetic");
    let input_path = path.with_extension("json");
    if let Some(v) = input {
        private(&input_path, v.to_string().as_bytes());
    }
    let reply = reply.to_owned();
    let task = thread::spawn(move || {
        let mut requests = vec![];
        let start = Instant::now();
        let mut last = None;
        loop {
            if last.is_some_and(|t: Instant| t.elapsed() > Duration::from_millis(150)) {
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(8), "request deadline");
            let (mut s, _) = match listener.accept() {
                Ok(s) => s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            let mut head = vec![];
            while !head.ends_with(b"\r\n\r\n") {
                let mut b = [0];
                s.read_exact(&mut b).unwrap();
                head.push(b[0]);
            }
            let head = String::from_utf8(head).unwrap();
            assert!(head.to_lowercase().contains("cookie: session=synthetic"));
            let len = head
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|s| s.parse::<usize>().ok())
                })
                .unwrap_or(0);
            let mut bytes = vec![0; len];
            s.read_exact(&mut bytes).unwrap();
            let body = if len == 0 {
                Value::Null
            } else {
                serde_json::from_slice(&bytes).unwrap()
            };
            let is_me = head.starts_with("GET /api/me ");
            requests.push((head, body));
            let (status, body) = if is_me {
                (200, me.as_ref().expect("unexpected me").to_string())
            } else {
                (status, reply.clone())
            };
            write!(
                s,
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            last = Some(Instant::now());
        }
        requests
    });
    let mut args = if native {
        vec!["remote".into(), "ownership".into()]
    } else {
        vec!["ownership".into(), command.into()]
    };
    if attached {
        args.splice(
            0..0,
            [
                format!("--server={origin}"),
                "--format=human".into(),
                "--json".into(),
            ],
        );
        args.extend([
            "--company=synthetic".into(),
            format!("--cookie-file={}", path.display()),
        ]);
    } else {
        args.extend([
            "--server".into(),
            origin,
            "--company".into(),
            "synthetic".into(),
            "--cookie-file".into(),
            path.to_str().unwrap().into(),
        ]);
    }
    args.push("--allow-loopback-http".into());
    if input_path.exists() {
        args.extend(["--input".into(), input_path.to_str().unwrap().into()]);
    }
    args.extend(flags.iter().map(|s| s.to_string()));
    let out = call(&args.iter().map(String::as_str).collect::<Vec<_>>());
    let requests = task.join().unwrap();
    std::fs::remove_file(path).unwrap();
    if input_path.exists() {
        std::fs::remove_file(input_path).unwrap();
    }
    (out, requests)
}
fn value(o: &Output) -> Value {
    serde_json::from_slice(&o.stdout).unwrap()
}
#[test]
fn six_commands_preserve_exact_original_payloads_and_native_observation() {
    let graph = json!({"asOf":"2026-06-01","facts":[],"partial":true,"consolidation":{"eligible":false,"reason":"hidden endpoints"}});
    for attached in [false, true] {
        let (o, r) = observe(
            "query",
            &["--as-of=2026-06-01"],
            200,
            &graph.to_string(),
            None,
            None,
            attached,
            false,
        );
        assert_eq!(
            o.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert_eq!(value(&o)["partial"], true);
        assert!(value(&o).get("data").is_none());
        assert_eq!(r.len(), 1);
        assert!(
            r[0].0
                .starts_with("GET /api/companies/synthetic/ownership?asOf=2026-06-01 ")
        );
    }
    for (command, flags, state) in [
        ("propose", vec!["--confirm=yes"], "proposed"),
        (
            "review",
            vec![
                "--confirm",
                "yes",
                "--snapshot-id=source-token",
                "--decision=rejected",
            ],
            "rejected",
        ),
        (
            "apply",
            vec!["--confirm=yes", "--snapshot-id=source-token"],
            "applied",
        ),
    ] {
        let mut flags = flags.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        if command == "apply" {
            flags.extend([
                format!("--snapshot-hash={}", "a".repeat(64)),
                format!("--diff-hash={}", "b".repeat(64)),
            ]);
        }
        let reply = if command == "apply" {
            json!({"status":"applied","snapshot":snap(state)})
        } else {
            json!({"snapshot":snap(state)})
        };
        let (o, r) = observe(
            command,
            &flags.iter().map(String::as_str).collect::<Vec<_>>(),
            200,
            &reply.to_string(),
            (command == "propose").then(proposal),
            None,
            false,
            false,
        );
        assert_eq!(
            o.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&o.stderr)
        );
        let mut expected = reply;
        expected["ok"] = json!(true);
        assert_eq!(value(&o), expected);
        assert_eq!(r.len(), 1);
        assert!(r[0].0.starts_with(&format!(
            "POST /api/companies/synthetic/ownership/{command} "
        )));
        assert_eq!(r[0].1["confirm"], true);
        assert!(r[0].1.get("actor").is_none());
    }
    let history = json!({"history":[snap("approved"),snap("rejected")]});
    let (o, r) = observe(
        "history",
        &["--snapshot-id", "source-token"],
        200,
        &history.to_string(),
        None,
        None,
        false,
        false,
    );
    assert_eq!(value(&o), json!({"ok":true,"history":history["history"]}));
    assert_eq!(r.len(), 1);
    let projection = json!({"eligible":true,"edges":[]});
    let (o, r) = observe(
        "projection",
        &["--as-of=2026-06-01"],
        200,
        &json!({"history":[],"projection":projection}).to_string(),
        None,
        None,
        false,
        false,
    );
    assert_eq!(value(&o), json!({"ok":true,"projection":projection}));
    assert!(
        r[0].0
            .starts_with("GET /api/companies/synthetic/ownership/history?asOf=2026-06-01 ")
    );
    let (o, _) = observe(
        "query",
        &["--as-of", "2026-06-01"],
        200,
        &graph.to_string(),
        None,
        None,
        false,
        true,
    );
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(value(&o)["data"], graph);
}
#[test]
fn offline_help_and_usage_before_private_files() {
    for cmd in [
        "query",
        "propose",
        "review",
        "apply",
        "history",
        "projection",
    ] {
        let o = call(&["ownership", cmd, "--help", "--cookie-file=/never-open"]);
        assert_eq!(o.status.code(), Some(0));
        assert!(o.stderr.is_empty());
    }
    for args in [
        vec!["ownership", "unknown", "--help"],
        vec!["ownership", "query", "--confirm=yes"],
        vec!["ownership", "query", "--as-of=2026-02-30"],
        vec![
            "ownership",
            "propose",
            "--input=/never-open",
            "--confirm=no",
        ],
        vec!["ownership", "propose", "--input=/never-open", "--confirm"],
        vec!["ownership", "review", "--decision=accept"],
        vec!["ownership", "history", "--workspace=/never-open"],
        vec!["ownership", "history", "--service-key-file=/never-open"],
        vec![
            "ownership",
            "query",
            "--as-of=2026-06-01",
            "--as-of=2026-06-01",
        ],
        vec!["ownership", "apply", "--snapshot-hash=not-hash"],
        vec!["ownership", "query", "--server=http://example.invalid"],
    ] {
        let o = call(&args);
        assert_eq!(o.status.code(), Some(2), "{args:?}");
        assert!(o.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&o.stderr).contains("/never-open"));
    }
}
#[test]
fn refuse_partial_projection_malformed_business_and_unknown_write_without_retry() {
    for (command, flags, status, reply, exit) in [
        (
            "query",
            vec!["--as-of=2026-06-01"],
            403,
            "{\"secret\":\"RAW_SECRET\"}",
            1,
        ),
        ("query", vec!["--as-of=2026-06-01"], 200, "{}", 1),
        ("query", vec!["--as-of=2026-06-01"], 200, "not-json", 1),
        (
            "history",
            vec![],
            200,
            "{\"ok\":false,\"errors\":[\"synthetic refusal\"]}",
            1,
        ),
        (
            "projection",
            vec!["--as-of=2026-06-01"],
            200,
            "{\"history\":[]}",
            1,
        ),
        (
            "review",
            vec![
                "--snapshot-id=source-token",
                "--decision=approved",
                "--confirm=yes",
            ],
            500,
            "{}",
            1,
        ),
        (
            "review",
            vec![
                "--snapshot-id=source-token",
                "--decision=approved",
                "--confirm=yes",
            ],
            200,
            "{}",
            1,
        ),
        (
            "review",
            vec![
                "--snapshot-id=source-token",
                "--decision=approved",
                "--confirm=yes",
            ],
            200,
            "not-json",
            1,
        ),
    ] {
        let (o, r) = observe(command, &flags, status, reply, None, None, false, false);
        assert_eq!(o.status.code(), Some(exit));
        assert_eq!(value(&o)["ok"], false);
        assert_eq!(r.len(), 1);
        assert!(!String::from_utf8_lossy(&o.stdout).contains("RAW_SECRET"));
        if command == "review" {
            assert!(String::from_utf8_lossy(&o.stdout).contains("outcome unknown"));
        }
    }
    let (o, _) = observe(
        "history",
        &["--format=human"],
        200,
        "{\"history\":[]}",
        None,
        None,
        false,
        false,
    );
    assert_eq!(
        String::from_utf8(o.stdout).unwrap(),
        "✔ Viser append-only snapshot-, review- og apply-historik.\n"
    );
    let (o, _) = observe(
        "history",
        &["--format=human"],
        200,
        "{\"ok\":false,\"errors\":[\"synthetic refusal\"]}",
        None,
        None,
        false,
        false,
    );
    assert!(o.stdout.is_empty());
    assert_eq!(
        String::from_utf8(o.stderr).unwrap(),
        "✘ Viser append-only snapshot-, review- og apply-historik. failed\n  → synthetic refusal\n"
    );
}
#[test]
fn actor_and_principal_are_assertions_not_authority() {
    for assertion in ["--actor=user:forged", "--principal-id=forged"] {
        let (o, r) = observe(
            "review",
            &[
                "--snapshot-id=source-token",
                "--decision=approved",
                "--confirm=yes",
                assertion,
            ],
            200,
            "{}",
            None,
            Some(json!({"user":{"id":"synthetic"}})),
            false,
            false,
        );
        assert_eq!(o.status.code(), Some(2));
        assert_eq!(r.len(), 1);
        assert!(r[0].0.starts_with("GET /api/me "));
    }
    let mut input = proposal();
    input["actor"] = json!("user:synthetic");
    input["principal"] = json!({"kind":"user","id":"synthetic"});
    let (o, r) = observe(
        "propose",
        &[
            "--confirm=yes",
            "--actor=user:synthetic",
            "--principal-id=synthetic",
        ],
        200,
        &json!({"snapshot":snap("proposed")}).to_string(),
        Some(input),
        Some(json!({"user":{"id":"synthetic"}})),
        false,
        false,
    );
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(r.len(), 2);
    assert!(r[1].1.get("principal").is_none());
    // No server is started for schema/authority injection failures.
    for (key, v) in [
        ("confirm", json!(true)),
        ("authorized", json!(true)),
        ("bytesBase64", json!("forged")),
        ("authority", json!({})),
        (
            "principal",
            json!({"kind":"local_operator","id":"synthetic"}),
        ),
        ("metadata", json!({"mustNotDisappear":true})),
    ] {
        let p =
            std::env::temp_dir().join(format!("rm-ownership-input-{}-{key}", std::process::id()));
        let mut input = proposal();
        input[key] = v;
        private(&p, input.to_string().as_bytes());
        let o = call(&[
            "ownership",
            "propose",
            "--company=synthetic",
            "--server=http://127.0.0.1:9",
            "--allow-loopback-http",
            "--cookie-file=/never-open",
            "--confirm=yes",
            "--input",
            p.to_str().unwrap(),
        ]);
        std::fs::remove_file(p).unwrap();
        assert_eq!(o.status.code(), Some(2), "{key}");
        assert!(o.stdout.is_empty());
    }
}
#[test]
fn unsupported_protocol_sends_no_credentials_or_business() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let path = std::env::temp_dir().join(format!("rm-ownership-protocol-{}", std::process::id()));
    private(&path, b"session=synthetic");
    let task = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut h = vec![];
        while !h.ends_with(b"\r\n\r\n") {
            let mut b = [0];
            s.read_exact(&mut b).unwrap();
            h.push(b[0]);
        }
        let h = String::from_utf8(h).unwrap().to_lowercase();
        assert!(h.starts_with("get /api/protocol "));
        assert!(!h.contains("cookie:"));
        assert!(!h.contains("authorization:"));
        let b = "{}";
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{b}"
        )
        .unwrap();
        drop(s);
        listener.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(150));
        assert!(listener.accept().is_err());
    });
    let o = call(&[
        "ownership",
        "query",
        "--company=synthetic",
        "--as-of=2026-06-01",
        "--server",
        &origin,
        "--allow-loopback-http",
        "--cookie-file",
        path.to_str().unwrap(),
    ]);
    task.join().unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(value(&o)["ok"], false);
}
#[cfg(unix)]
#[test]
fn local_unix_binding_and_workspace_equality_no_tcp_fallback() {
    use std::{
        fs,
        os::unix::{
            fs::{DirBuilderExt, PermissionsExt},
            net::UnixListener,
        },
    };
    let home = fs::canonicalize(std::env::var_os("HOME").unwrap()).unwrap();
    let path = home.join(format!(".rm-ownership716-{}", std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
    let workspace = path.join("workspace");
    fs::create_dir(&workspace).unwrap();
    let company = format!("synthetic-{}", "x".repeat(70));
    fs::create_dir(workspace.join(&company)).unwrap();
    let tcp = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    tcp.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", tcp.local_addr().unwrap());
    let listener = UnixListener::bind(path.join("r.sock")).unwrap();
    fs::set_permissions(path.join("r.sock"), fs::Permissions::from_mode(0o600)).unwrap();
    private(&path.join("r.json"),json!({"format":"rentemester-local-runtime/1","origin":origin,"socket":"r.sock","workspaceRoot":workspace,"workspaceId":"synthetic"}).to_string().as_bytes());
    private(&path.join("cookie"), b"session=synthetic");
    let company_path = workspace.join(&company);
    assert!(company_path.as_os_str().len() > 128);
    let base = [
        "ownership",
        "history",
        "--company",
        company_path.to_str().unwrap(),
        "--local-profile",
        path.to_str().unwrap(),
        "--cookie-file",
        path.join("cookie").to_str().unwrap(),
    ]
    .map(str::to_string);
    let mut wrong = base.clone().to_vec();
    wrong.extend(["--workspace".into(), path.to_str().unwrap().into()]);
    let o = call(&wrong.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(o.status.code(), Some(2));
    assert!(o.stdout.is_empty());
    let task = thread::spawn(move || {
        let (mut s, _) = common::unix_accept(&listener).unwrap();
        let mut h = vec![];
        while !h.ends_with(b"\r\n\r\n") {
            let mut b = [0];
            s.read_exact(&mut b).unwrap();
            h.push(b[0]);
        }
        assert!(
            String::from_utf8(h)
                .unwrap()
                .starts_with(&format!("GET /api/companies/{company}/ownership/history "))
        );
        let b = "{\"history\":[]}";
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
            b.len()
        )
        .unwrap();
    });
    let mut correct = base.to_vec();
    correct.extend(["--workspace".into(), workspace.to_str().unwrap().into()]);
    let o = call(&correct.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert_eq!(value(&o), json!({"ok":true,"history":[]}));
    task.join().unwrap();
    assert!(tcp.accept().is_err());
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn malformed_receipts_and_mismatched_tokens_stay_unknown() {
    for field in [
        "snapshotHash",
        "diffHash",
        "principal",
        "history",
        "diff",
        "snapshotId",
        "applyHash",
    ] {
        let mut snapshot = snap(if field == "applyHash" {
            "applied"
        } else {
            "approved"
        });
        match field {
            "snapshotHash" | "diffHash" => snapshot[field] = json!("x"),
            "principal" => snapshot[field] = json!({}),
            "history" => snapshot[field] = json!([]),
            "diff" => snapshot[field] = json!({}),
            "snapshotId" => snapshot[field] = json!("another-source-token"),
            _ => snapshot["snapshotHash"] = json!("c".repeat(64)),
        }
        let mut flags = vec![
            "--confirm=yes".to_string(),
            "--snapshot-id=source-token".to_string(),
        ];
        let apply = field == "applyHash";
        let reply = if apply {
            flags.extend([
                format!("--snapshot-hash={}", "a".repeat(64)),
                format!("--diff-hash={}", "b".repeat(64)),
            ]);
            json!({"status":"applied","snapshot":snapshot})
        } else {
            flags.push("--decision=approved".into());
            json!({"snapshot":snapshot})
        };
        let (o, r) = observe(
            if apply { "apply" } else { "review" },
            &flags.iter().map(String::as_str).collect::<Vec<_>>(),
            200,
            &reply.to_string(),
            None,
            None,
            false,
            false,
        );
        assert_eq!(
            o.status.code(),
            Some(1),
            "malformed receipt field {field}: {}",
            String::from_utf8_lossy(&o.stdout)
        );
        assert_eq!(r.len(), 1);
        let v = value(&o);
        assert_eq!(v["ok"], false);
        assert!(v["errors"].to_string().contains("outcome unknown"));
        assert!(v.get("snapshot").is_none());
    }
}
