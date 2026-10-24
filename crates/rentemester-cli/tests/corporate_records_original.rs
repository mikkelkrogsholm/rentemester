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
fn record() -> Value {
    json!({"recordId":"record-synthetic","type":"articles","sha256":rentemester_storage::invoice_email::sha(b"abc"),"filename":"synthetic.bin","source":"synthetic","receivedAt":"2026-01-01T00:00:00Z","recordDate":null,"retentionUntil":null,"uploader":"synthetic","sensitivity":"normal","links":[{"type":"company","id":"synthetic"}],"history":[{"event_type":"ingested","payload_hash":"a".repeat(64),"actor":"user:synthetic","created_at":"2026-01-01T00:00:00Z"}]})
}
fn response(op: &str) -> Value {
    match op {
        "list" => json!({"ok":true,"rows":[record()],"count":1,"nextCursor":null}),
        "download" => {
            json!({"ok":true,"bytesBase64":"YWJj","sha256":rentemester_storage::invoice_email::sha(b"abc")})
        }
        "enrich" | "supersede" => json!({"ok":true,"payloadHash":"a".repeat(64)}),
        _ => json!({"ok":true,"record":record()}),
    }
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
fn args(op: &str) -> Vec<String> {
    let mut a = vec!["corporate-record".into(), op.into()];
    if op != "list" && op != "ingest" {
        a.extend(["--record-id=record-synthetic".into()]);
    }
    match op {
        "link" => a.extend([
            "--link-type=party".into(),
            "--link-id=party-synthetic".into(),
        ]),
        "enrich" => a.push("--assertion=synthetic assertion".into()),
        "supersede" => a.extend([
            "--replacement-record-id=replacement-synthetic".into(),
            "--reason=synthetic reason".into(),
        ]),
        _ => {}
    }
    if matches!(op, "ingest" | "link" | "enrich" | "supersede") {
        a.push("--confirm=yes".into());
    }
    a
}
fn observe(
    op: &str,
    status: u16,
    body: Value,
    human: bool,
    actor: Option<&str>,
    native: bool,
) -> Output {
    observe_custom(op, status, body, human, actor, native, None, None)
}
fn observe_custom(
    op: &str,
    status: u16,
    body: Value,
    human: bool,
    actor: Option<&str>,
    native: bool,
    custom_args: Option<Vec<String>>,
    custom_metadata: Option<Value>,
) -> Output {
    let listener = common::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let dir = std::env::temp_dir().join(format!(
        "rm-corporate716-{}-{}",
        std::process::id(),
        listener.local_addr().unwrap().port()
    ));
    std::fs::create_dir(&dir).unwrap();
    private(&dir.join("cookie"), b"session=synthetic");
    private(&dir.join("bytes"), b"abc");
    let metadata = json!({"recordId":"record-synthetic","type":"articles","filename":"synthetic.bin","source":"synthetic","receivedAt":"2026-01-01T00:00:00Z","recordDate":null,"retentionUntil":null,"uploader":"synthetic","sensitivity":"normal","links":[{"type":"company","id":"synthetic"}]});
    let metadata = custom_metadata.unwrap_or(metadata);
    private(&dir.join("metadata"), metadata.to_string().as_bytes());
    let operation = op.to_owned();
    let asserted = actor.is_some();
    let forged = actor == Some("user:forged");
    let expected_metadata = metadata.clone();
    let expected_values = custom_args
        .as_ref()
        .into_iter()
        .flatten()
        .filter_map(|token| {
            let (key, value) = token.split_once('=')?;
            let key = match key {
                "--assertion" => "assertion",
                "--reason" => "reason",
                "--link-id" => "id",
                "--link-type" => "type",
                _ => return None,
            };
            Some((key.to_string(), json!(value)))
        })
        .collect::<Vec<_>>();

    let task = thread::spawn(move || {
        let start = Instant::now();
        let mut last = None;
        let mut count = 0;
        loop {
            if last.is_some_and(|t: Instant| t.elapsed() > Duration::from_millis(180)) {
                break;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "request deadline"
            );
            let (mut s, _) = match listener.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            count += 1;
            let mut h = Vec::new();
            while !h.ends_with(b"\r\n\r\n") {
                let mut b = [0];
                s.read_exact(&mut b).unwrap();
                h.push(b[0]);
            }
            let h = String::from_utf8(h).unwrap();
            assert!(h.to_lowercase().contains("cookie: session=synthetic"));
            let me = h.starts_with("GET /api/me ");
            let size = h
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            let mut b = vec![0; size];
            s.read_exact(&mut b).unwrap();
            if me {
                assert!(asserted);
                assert_eq!(count, 1);
            } else {
                assert!(!forged);
                let route = match operation.as_str() {
                    "list" => {
                        "GET /api/companies/synthetic/corporate-records?cursor=0&limit=25".into()
                    }
                    "inspect" => {
                        "GET /api/companies/synthetic/corporate-records/record-synthetic".into()
                    }
                    "download" => {
                        "GET /api/companies/synthetic/corporate-records/record-synthetic/download"
                            .into()
                    }
                    "ingest" => "POST /api/companies/synthetic/corporate-records".into(),
                    _ => format!(
                        "POST /api/companies/synthetic/corporate-records/record-synthetic/{operation}"
                    ),
                };
                assert!(h.starts_with(&format!("{route} HTTP/1.1")), "{h}");
                if !b.is_empty() {
                    let v: Value = serde_json::from_slice(&b).unwrap();
                    assert_eq!(v["confirm"], true);
                    assert!(v.get("actor").is_none());
                    for (key, expected) in &expected_values {
                        assert_eq!(&v[key], expected, "exact prose/link wire value");
                    }
                    if operation == "enrich" {
                        assert!(v["assertion"].is_string());
                    }
                    if operation == "supersede" {
                        assert!(v["reason"].is_string());
                    }
                    if operation == "ingest" {
                        let mut expected = expected_metadata.clone();
                        expected["bytesBase64"] = json!("YWJj");
                        expected["confirm"] = json!(true);
                        assert_eq!(v, expected);
                    }
                }
            }
            let reply = if me {
                json!({"ok":true,"user":{"id":"synthetic"}})
            } else {
                body.clone()
            }
            .to_string();
            let code = if me { 200 } else { status };
            // A closed dispatched write models an unknown outcome, never a replay.
            if code != 599 {
                write!(s,"HTTP/1.1 {code} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",reply.len()).unwrap();
            }
            last = Some(Instant::now());
        }
        assert_eq!(count, if asserted && !forged { 2 } else { 1 }, "no retry");
    });
    let mut a = if native {
        vec!["remote".into(), "corporate-records".into()]
    } else {
        custom_args.unwrap_or_else(|| args(op))
    };
    if op == "ingest" {
        a.extend([
            format!("--input={}", dir.join("metadata").display()),
            "--file".into(),
            dir.join("bytes").to_str().unwrap().into(),
        ]);
    }
    a.extend([
        format!("--server={origin}"),
        "--company=synthetic".into(),
        format!("--cookie-file={}", dir.join("cookie").display()),
        "--allow-loopback-http".into(),
        if native {
            "--json".into()
        } else {
            format!("--format={}", if human { "human" } else { "json" })
        },
    ]);
    if native {
        // Native remote grammar intentionally remains split-valued and has no --json flag.
        let mut split = Vec::new();
        for token in a {
            if token == "--json" {
                continue;
            }
            if let Some((k, v)) = token.split_once('=') {
                split.extend([k.to_owned(), v.to_owned()]);
            } else {
                split.push(token);
            }
        }
        a = split;
    }
    if let Some(actor) = actor {
        a.push(format!("--actor={actor}"));
    }
    let out = call(&a.iter().map(String::as_str).collect::<Vec<_>>());
    task.join().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    out
}
#[test]
fn all_seven_commands_project_exact_original_envelopes() {
    for op in [
        "ingest",
        "list",
        "inspect",
        "download",
        "link",
        "enrich",
        "supersede",
    ] {
        let o = observe(op, 200, response(op), false, None, false);
        assert_eq!(
            o.status.code(),
            Some(0),
            "{op}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert!(o.stderr.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&o.stdout).unwrap(),
            response(op)
        );
    }
}
#[test]
fn offline_discovery_and_precredential_usage() {
    for a in [
        vec!["corporate-record", "--help"],
        vec!["corporate-record", "ingest", "--help"],
        vec![
            "--company=synthetic",
            "corporate-record",
            "list",
            "--help",
            "--cookie-file=/missing",
        ],
    ] {
        let o = call(&a);
        assert_eq!(o.status.code(), Some(0));
        assert!(o.stderr.is_empty());
        assert!(String::from_utf8_lossy(&o.stdout).contains("corporate-record"));
    }
    for (op, flags) in [
        ("unknown", vec![]),
        (
            "ingest",
            vec!["--input=/missing", "--file=/missing", "--confirm=no"],
        ),
        ("ingest", vec!["--confirm"]),
        ("list", vec!["--confirm=yes"]),
        ("list", vec!["--actor=user:forged"]),
        ("link", vec!["--link-type=unknown"]),
        ("list", vec!["--limit=101"]),
        ("list", vec!["--cursor=-1"]),
        ("list", vec!["--limit=1", "--limit=2"]),
        ("list", vec!["--principal-id=forged"]),
        ("list", vec!["--service-key-file=/missing"]),
        ("list", vec!["--workspace=/missing"]),
        ("list", vec!["--json=false"]),
        ("list", vec!["--format=csv"]),
        ("list", vec!["--unknown=x"]),
    ] {
        let mut a = vec![
            "corporate-record",
            op,
            "--server=https://example.invalid",
            "--company=synthetic",
            "--cookie-file=/missing",
        ];
        a.extend(flags);
        let o = call(&a);
        assert_eq!(o.status.code(), Some(2), "{a:?}");
        assert!(o.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&o.stderr).contains("/missing"));
    }
}
#[test]
fn human_and_structured_business_refusals() {
    let o = observe("link", 200, response("link"), true, None, false);
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(o.stdout).unwrap(),
        "✔ Knytter en record til eksisterende scope append-only.\n"
    );
    let v = json!({"ok":false,"errors":["synthetic refusal"]});
    let o = observe("enrich", 200, v.clone(), false, None, false);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(serde_json::from_slice::<Value>(&o.stdout).unwrap(), v);
    let o = observe("enrich", 200, v, true, None, false);
    assert_eq!(o.status.code(), Some(1));
    assert!(o.stdout.is_empty());
    assert_eq!(
        String::from_utf8(o.stderr).unwrap(),
        "✘ Tilføjer auditeret metadata uden at ændre original bytes. failed\n  → synthetic refusal\n"
    );
    let o = observe("list", 200, response("list"), true, None, false);
    assert_eq!(
        String::from_utf8(o.stdout).unwrap(),
        "✔ Lister governance-records der er fuldt synlige i virksomh…\n  Antal: 1\n"
    );
}
#[test]
fn actor_assertions_are_not_authority() {
    let o = observe(
        "enrich",
        200,
        response("enrich"),
        false,
        Some("user:synthetic"),
        false,
    );
    assert_eq!(o.status.code(), Some(0));
    let o = observe(
        "enrich",
        200,
        response("enrich"),
        false,
        Some("user:forged"),
        false,
    );
    assert_eq!(o.status.code(), Some(2));
    assert!(o.stdout.is_empty());
}
#[test]
fn refusals_and_malformed_or_unknown_writes_fail_closed_once() {
    for (status, v) in [
        (403, json!({"secret":"RAW_SECRET"})),
        (200, json!({"ok":true})),
        (599, json!({})),
    ] {
        let o = observe("enrich", status, v, false, None, false);
        assert_eq!(o.status.code(), Some(1));
        assert!(o.stderr.is_empty());
        let v: Value = serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(v["ok"], false);
        assert!(!String::from_utf8_lossy(&o.stdout).contains("RAW_SECRET"));
        if status != 403 {
            assert!(v["errors"][0].as_str().unwrap().contains("outcome unknown"));
        }
    }
    let o = observe(
        "inspect",
        404,
        json!({"secret":"RAW_SECRET"}),
        false,
        None,
        false,
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&o.stdout).unwrap(),
        json!({"ok":false,"errors":["corporate record not found"]})
    );
    let mut v = response("download");
    v["sha256"] = json!("a".repeat(64));
    assert_eq!(
        observe("download", 200, v, false, None, false)
            .status
            .code(),
        Some(1)
    );
}
#[test]
fn injected_metadata_is_refused_before_cookie() {
    let dir = std::env::temp_dir().join(format!("rm-corporate-injection-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    for key in [
        "actor",
        "principal",
        "authority",
        "confirm",
        "bytesBase64",
        "unknown",
    ] {
        let p = dir.join(key);
        private(&p, json!({key:"forged"}).to_string().as_bytes());
        let o = call(&[
            "corporate-record",
            "ingest",
            "--server=https://example.invalid",
            "--company=synthetic",
            "--cookie-file=/missing",
            "--confirm=yes",
            "--input",
            p.to_str().unwrap(),
            "--file=/not-read",
        ]);
        assert_eq!(o.status.code(), Some(2));
        assert!(o.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&o.stderr).contains("/not-read"));
    }
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn unsupported_protocol_never_receives_credential_or_business() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut b = Vec::new();
        while !b.ends_with(b"\r\n\r\n") {
            let mut x = [0];
            s.read_exact(&mut x).unwrap();
            b.push(x[0]);
        }
        let h = String::from_utf8(b).unwrap().to_lowercase();
        assert!(h.starts_with("get /api/protocol "));
        assert!(!h.contains("cookie:"));
        let body = "{}";
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{body}"
        )
        .unwrap();
        drop(s);
        listener.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(180));
        assert!(listener.accept().is_err());
    });
    let p = std::env::temp_dir().join(format!("rm-corporate-protocol-{}", std::process::id()));
    private(&p, b"session=synthetic");
    let o = call(&[
        "corporate-record",
        "list",
        "--server",
        &origin,
        "--allow-loopback-http",
        "--company=synthetic",
        "--cookie-file",
        p.to_str().unwrap(),
    ]);
    t.join().unwrap();
    std::fs::remove_file(p).unwrap();
    assert_eq!(o.status.code(), Some(1));
}

#[test]
fn native_observation_stays_wrapped() {
    let o = observe("list", 200, response("list"), false, None, true);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"], response("list"));
}
#[cfg(unix)]
#[test]
fn local_unix_binding_and_workspace_assertion_never_fall_back_to_tcp() {
    use std::{
        fs,
        os::unix::{
            fs::{DirBuilderExt, PermissionsExt},
            net::UnixListener,
        },
    };
    let home = fs::canonicalize(std::env::var_os("HOME").unwrap()).unwrap();
    let path = home.join(format!(".rm-corporate716-{}", std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
    let workspace = path.join("workspace");
    fs::create_dir(&workspace).unwrap();
    let company = format!("synthetic-{}", "x".repeat(70));
    fs::create_dir(workspace.join(&company)).unwrap();
    assert!(workspace.join(&company).as_os_str().len() > 128);
    let tcp = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", tcp.local_addr().unwrap());
    tcp.set_nonblocking(true).unwrap();
    let listener = UnixListener::bind(path.join("r.sock")).unwrap();
    fs::set_permissions(path.join("r.sock"), fs::Permissions::from_mode(0o600)).unwrap();
    private(&path.join("r.json"),json!({"format":"rentemester-local-runtime/1","origin":origin,"socket":"r.sock","workspaceRoot":workspace,"workspaceId":"synthetic"}).to_string().as_bytes());
    private(&path.join("cookie"), b"session=synthetic");
    let mismatch = call(&[
        "corporate-record",
        "list",
        "--local-profile",
        path.to_str().unwrap(),
        "--workspace",
        path.to_str().unwrap(),
        "--company",
        workspace.join(&company).to_str().unwrap(),
        "--cookie-file",
        path.join("cookie").to_str().unwrap(),
    ]);
    assert_eq!(mismatch.status.code(), Some(2));
    assert!(mismatch.stdout.is_empty());
    let expected =
        format!("GET /api/companies/{company}/corporate-records?cursor=0&limit=25 HTTP/1.1");
    let t = thread::spawn(move || {
        let (mut s, _) = common::unix_accept(&listener).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut b = Vec::new();
        while !b.ends_with(b"\r\n\r\n") {
            let mut x = [0];
            s.read_exact(&mut x).unwrap();
            b.push(x[0]);
        }
        let h = String::from_utf8(b).unwrap();
        assert!(h.starts_with(&expected));
        assert!(h.to_lowercase().contains("cookie: session=synthetic"));
        let body = json!({"ok":true,"rows":[],"count":0,"nextCursor":null}).to_string();
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let o = call(&[
        "--local-profile",
        path.to_str().unwrap(),
        "corporate-record",
        "--workspace",
        workspace.to_str().unwrap(),
        "list",
        "--company",
        workspace.join(&company).to_str().unwrap(),
        "--cookie-file",
        path.join("cookie").to_str().unwrap(),
    ]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    t.join().unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&o.stdout).unwrap(),
        json!({"ok":true,"rows":[],"count":0,"nextCursor":null})
    );
    assert!(tcp.accept().is_err());
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn source_bounded_multiline_prose_and_large_ingest_metadata_are_supported() {
    let prose = "first line\nsecond line";
    for (op, flag) in [
        ("enrich", "--assertion"),
        ("supersede", "--reason"),
        ("link", "--link-id"),
    ] {
        let mut a = args(op);
        a.retain(|token| !token.starts_with(&format!("{flag}=")));
        if op == "link" {
            a.retain(|token| !token.starts_with("--link-type="));
            a.push("--link-type=legacy_evidence_ref".into());
        }
        a.push(format!("{flag}={prose}"));
        let out = observe_custom(op, 200, response(op), false, None, false, Some(a), None);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{op}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).unwrap(),
            response(op)
        );
    }
    let mut metadata = json!({"recordId":"record-synthetic","type":"articles","filename":"synthetic.bin","source":"synthetic","receivedAt":"2026-01-01T00:00:00Z","recordDate":null,"retentionUntil":null,"uploader":"synthetic","sensitivity":"normal","links":[{"type":"company","id":"synthetic"}]});
    for i in 0..140 {
        metadata["links"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type":"legacy_evidence_ref","id":format!("{i:03}-{}","x".repeat(240))}));
    }
    assert!(metadata.to_string().len() > 32 * 1024);
    let out = observe_custom(
        "ingest",
        200,
        response("ingest"),
        false,
        None,
        false,
        None,
        Some(metadata),
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
