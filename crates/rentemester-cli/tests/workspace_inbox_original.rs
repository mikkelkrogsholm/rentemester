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
fn event(kind: &str, payload: Value) -> Value {
    let hash = rentemester_storage::invoice_email::sha(
        serde_json::to_string(&payload).unwrap().as_bytes(),
    );
    json!({"event_type":kind,"payload_hash":hash,"actor":"user:synthetic","created_at":"2026-01-01T00:00:00.000Z","payload":payload})
}
fn source() -> Value {
    let hash = rentemester_storage::invoice_email::sha(b"abc");
    json!({"sourceId":"source/synthetic","visibilityAnchorSlug":"synthetic","sha256":hash,"filename":"synthetic.txt","mimeType":"text/plain","transport":"upload","transportIdentity":null,"receivedAt":"2026-01-01T00:00:00.000Z","metadata":{"documentType":"cash_register_receipt"},"createdBy":"user:synthetic","createdAt":"2026-01-01T00:00:00.000Z","exception":null,"assignments":[],"history":[event("ingested",json!({"sourceHash":hash,"filename":"synthetic.txt","mimeType":"text/plain","transport":"upload","transportIdentity":null})),event("candidate_resolved",json!({"candidates":[{"companySlug":"distinct-target","evidenceKind":"portal","evidence":"Synthetic upload"}]}))]})
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
            s["history"].as_array_mut().unwrap().push(event("handoff_completed",json!({"companySlug":"distinct-target","documentId":17,"documentNo":"D17","sourceHash":rentemester_storage::invoice_email::sha(b"abc")})));
        }
    }
    if op == "list" {
        json!({"ok":true,"rows":[s],"count":1,"nextCursor":null})
    } else {
        json!({"ok":true,"source":s})
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
    let mut a = vec!["workspace-inbox".into(), op.into()];
    if !matches!(op, "ingest" | "list") {
        a.push("--source-id=source/synthetic".into());
    }
    if matches!(op, "assign" | "complete") {
        a.push("--target-company=distinct-target".into());
    }
    if matches!(op, "ingest" | "assign" | "complete") {
        a.push("--confirm=yes".into());
    }
    if op == "ingest" {
        a.push("--idempotency-key=caller exact key".into());
    }
    a
}
fn observe(
    op: &str,
    status: u16,
    body: Value,
    human: bool,
    actor: Option<&str>,
    _native: bool,
) -> Output {
    observe_custom(op, status, body, human, actor, false, None, None)
}
fn observe_custom(
    op: &str,
    status: u16,
    body: Value,
    human: bool,
    actor: Option<&str>,
    _native: bool,
    custom_args: Option<Vec<String>>,
    custom_metadata: Option<Value>,
) -> Output {
    let listener = common::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let dir = std::env::temp_dir().join(format!(
        "rm-inbox716-{}-{}",
        std::process::id(),
        listener.local_addr().unwrap().port()
    ));
    std::fs::create_dir(&dir).unwrap();
    private(&dir.join("cookie"), b"session=synthetic");
    private(&dir.join("bytes"), b"abc");
    let metadata = json!({"filename":"synthetic.txt","mimeType":"text/plain","transport":"upload","receivedAt":"2026-01-01T00:00:00.000Z","metadata":{"documentType":"cash_register_receipt"},"sourceId":"source/synthetic","candidates":[{"companySlug":"distinct-target","evidenceKind":"portal","evidence":"Synthetic upload"}]});
    let metadata = custom_metadata.unwrap_or(metadata);
    private(&dir.join("metadata"), metadata.to_string().as_bytes());
    let operation = op.to_owned();
    let asserted = actor.is_some();
    let forged = actor == Some("user:forged");
    let expected_metadata = metadata.clone();
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
            let prior = matches!(operation.as_str(), "assign" | "complete")
                && h.starts_with(
                    "GET /api/companies/synthetic/workspace-inbox/source%2Fsynthetic ",
                );
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
                let route = if prior {
                    "GET /api/companies/synthetic/workspace-inbox/source%2Fsynthetic".into()
                } else {
                    match operation.as_str() {
                        "list" => {
                            "GET /api/companies/synthetic/workspace-inbox?cursor=0&limit=25".into()
                        }
                        "inspect" | "status" => {
                            "GET /api/companies/synthetic/workspace-inbox/source%2Fsynthetic".into()
                        }
                        "ingest" => "POST /api/companies/synthetic/workspace-inbox".into(),
                        _ => format!(
                            "POST /api/companies/synthetic/workspace-inbox/source%2Fsynthetic/{operation}"
                        ),
                    }
                };
                assert!(h.starts_with(&format!("{route} HTTP/1.1")), "{h}");
                if !b.is_empty() {
                    let v: Value = serde_json::from_slice(&b).unwrap();
                    assert_eq!(v["confirm"], true);
                    assert!(v.get("actor").is_none());
                    if matches!(operation.as_str(), "assign" | "complete") {
                        assert_eq!(v["companySlug"], "distinct-target");
                    }
                    if operation == "ingest" {
                        let mut expected = expected_metadata.clone();
                        expected["bytesBase64"] = json!("YWJj");
                        expected["confirm"] = json!(true);
                        expected["idempotencyKey"] = json!("caller exact key");
                        assert_eq!(v, expected);
                    }
                }
            }
            let reply = if me {
                json!({"ok":true,"user":{"id":"synthetic"}})
            } else if prior {
                response("inspect")
            } else {
                body.clone()
            }
            .to_string();
            let code = if me || prior { 200 } else { status };
            // A closed dispatched write models an unknown outcome, never a replay.
            if code != 599 {
                write!(s,"HTTP/1.1 {code} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",reply.len()).unwrap();
            }
            last = Some(Instant::now());
        }
        assert_eq!(
            count,
            if forged {
                1
            } else {
                1 + usize::from(asserted)
                    + usize::from(matches!(operation.as_str(), "assign" | "complete"))
            },
            "no retry"
        );
    });
    let mut a = custom_args.unwrap_or_else(|| args(op));
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
        format!("--format={}", if human { "human" } else { "json" }),
    ]);
    if let Some(actor) = actor {
        a.push(format!("--actor={actor}"));
    }
    let out = call(&a.iter().map(String::as_str).collect::<Vec<_>>());
    task.join().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    out
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
    let path = home.join(format!(".rm-inbox716-{}", std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
    let workspace = path.join("workspace");
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&workspace)
        .unwrap();
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
        "workspace-inbox",
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
        format!("GET /api/companies/{company}/workspace-inbox?cursor=0&limit=25 HTTP/1.1");
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
        "workspace-inbox",
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
fn six_commands_flat_exact_transport_and_alias() {
    for op in ["ingest", "list", "inspect", "status", "assign", "complete"] {
        let o = observe(op, 200, response(op), false, None, false);
        assert_eq!(
            o.status.code(),
            Some(0),
            "{op}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        let v: Value = serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(v, response(op));
        assert!(v.get("data").is_none());
        assert!(!String::from_utf8_lossy(&o.stdout).contains("journal"));
    }
    assert_eq!(response("inspect"), response("status"));
}
#[test]
fn passive_help_and_precredential_grammar() {
    for op in ["list", "inspect", "status", "ingest", "assign", "complete"] {
        let o = call(&[
            "--company=synthetic",
            "workspace-inbox",
            op,
            "--help",
            "--cookie-file=/missing",
        ]);
        assert_eq!(o.status.code(), Some(0));
        let help = String::from_utf8_lossy(&o.stdout);
        assert!(help.contains("upload only") && help.contains("service-key"));
    }
    for (op, flags) in [
        ("unknown", vec![]),
        ("ingest", vec!["--confirm"]),
        ("ingest", vec!["--confirm=no"]),
        ("assign", vec!["--source-id=s", "--confirm=yes"]),
        ("complete", vec!["--source-id=s", "--target-company=t"]),
        ("list", vec!["--limit=101"]),
        ("list", vec!["--cursor=00"]),
        ("list", vec!["--cursor=-1"]),
        ("list", vec!["--limit=1", "--limit=2"]),
        ("list", vec!["--json=false"]),
        ("list", vec!["--actor=user:forged"]),
        ("list", vec!["--service-key-file=/missing"]),
        ("list", vec!["--workspace=/missing"]),
        ("list", vec!["--principal-id=forged"]),
        ("list", vec!["--format=csv"]),
        ("list", vec!["--unknown=x"]),
    ] {
        let mut a = vec![
            "workspace-inbox",
            op,
            "--server=https://example.invalid",
            "--company=synthetic",
            "--cookie-file=/missing",
        ];
        a.extend(flags);
        let o = call(&a);
        assert_eq!(
            o.status.code(),
            Some(2),
            "{a:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert!(o.stdout.is_empty());
    }
}
#[test]
fn current_actor_and_human_business_exit_codes() {
    let o = observe(
        "assign",
        200,
        response("assign"),
        true,
        Some("user:synthetic"),
        false,
    );
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(o.stdout).unwrap(),
        "✔ workspace-inbox assign\n"
    );
    let o = observe(
        "assign",
        200,
        response("assign"),
        false,
        Some("user:forged"),
        false,
    );
    assert_eq!(o.status.code(), Some(2));
    let refusal = json!({"ok":false,"errors":["synthetic refusal"]});
    let o = observe("complete", 200, refusal.clone(), false, None, false);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(serde_json::from_slice::<Value>(&o.stdout).unwrap(), refusal);
    let o = observe("complete", 200, refusal, true, None, false);
    assert_eq!(o.status.code(), Some(1));
    assert!(o.stdout.is_empty());
    assert_eq!(
        String::from_utf8(o.stderr).unwrap(),
        "✘ workspace-inbox complete failed\n  → synthetic refusal\n"
    );
    let o = observe("inspect", 404, json!({}), false, None, false);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&o.stdout).unwrap(),
        json!({"ok":false,"errors":["workspace inbox source not found"]})
    );
}
#[test]
fn malformed_mismatched_or_lost_write_is_never_success_or_retried() {
    let mut bad = Vec::new();
    bad.push(json!({"ok":true}));
    bad.push(json!({"ok":false}));
    for (key, value) in [
        ("sourceId", json!("wrong")),
        ("sha256", json!("not-hash")),
        ("visibilityAnchorSlug", json!("other")),
        ("history", json!([])),
        ("assignments", json!([])),
    ] {
        let mut v = response("complete");
        v["source"][key] = value;
        bad.push(v);
    }
    let mut v = response("complete");
    v["source"]["assignments"][0]["state"] = json!("approved");
    bad.push(v);
    let mut v = response("complete");
    v["source"]["assignments"][0]["companySlug"] = json!("synthetic");
    bad.push(v);
    for v in bad {
        let o = observe("complete", 200, v, false, None, false);
        assert_eq!(
            o.status.code(),
            Some(1),
            "{}",
            String::from_utf8_lossy(&o.stdout)
        );
        assert!(String::from_utf8_lossy(&o.stdout).contains("outcome unknown"));
    }
    let mut v = response("ingest");
    v["source"]["sha256"] = json!("b".repeat(64));
    let o = observe("ingest", 200, v, false, None, false);
    assert_eq!(o.status.code(), Some(1));
    let o = observe("assign", 599, Value::Null, false, None, false);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stdout).contains("unknown"));
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
    let p = std::env::temp_dir().join(format!("rm-inbox-protocol-{}", std::process::id()));
    private(&p, b"session=synthetic");
    let o = call(&[
        "workspace-inbox",
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
fn exact_json_precedence_and_split_globals() {
    let mut a = args("list");
    a.extend(["--format=human".into(), "--json".into()]);
    let o = observe_custom(
        "list",
        200,
        response("list"),
        true,
        None,
        false,
        Some(a),
        None,
    );
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(
        serde_json::from_slice::<Value>(&o.stdout).unwrap(),
        response("list")
    );
}
#[test]
fn native_boundaries_and_authority_injection_refuse_before_credentials() {
    let dir = std::env::temp_dir().join(format!("rm-inbox-injection-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let base = json!({"filename":"synthetic.txt","mimeType":"text/plain","transport":"upload","receivedAt":"2026-01-01T00:00:00.000Z","metadata":{}});
    let mut cases = Vec::new();
    for key in [
        "actor",
        "principal",
        "authority",
        "visibilityAnchorSlug",
        "visibleCompanySlugs",
        "confirm",
        "bytesBase64",
        "idempotencyKey",
        "at",
        "unknownFact",
    ] {
        let mut v = base.clone();
        v[key] = json!("forged");
        cases.push(v);
    }
    for transport in ["mail", "connector"] {
        let mut v = base.clone();
        v["transport"] = json!(transport);
        cases.push(v);
    }
    let mut v = base.clone();
    v["filename"] = json!("x".repeat(256));
    cases.push(v);
    let mut v = base.clone();
    v["metadata"] = json!({"huge":"x".repeat(65536)});
    cases.push(v);
    let mut nested = Value::Null;
    for _ in 0..34 {
        nested = json!({"nested":nested});
    }
    let mut v = base;
    v["metadata"] = nested;
    cases.push(v);
    for (n, v) in cases.iter().enumerate() {
        let p = dir.join(n.to_string());
        private(&p, v.to_string().as_bytes());
        let o = call(&[
            "workspace-inbox",
            "ingest",
            "--server=https://example.invalid",
            "--company=synthetic",
            "--cookie-file=/missing",
            "--confirm=yes",
            "--idempotency-key=caller",
            "--input",
            p.to_str().unwrap(),
            "--file=/not-read",
        ]);
        assert_eq!(
            o.status.code(),
            Some(2),
            "case {n}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert!(o.stdout.is_empty());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("public-cookie");
        private(&p, b"session=synthetic");
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        let o = call(&[
            "workspace-inbox",
            "list",
            "--server=https://example.invalid",
            "--company=synthetic",
            "--cookie-file",
            p.to_str().unwrap(),
        ]);
        assert_eq!(o.status.code(), Some(1));
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn canonical_ingest_receipt_and_original_intent_are_checked_without_rewriting_wire() {
    let metadata = json!({"sourceId":" source/synthetic ","filename":" synthetic.txt","mimeType":"text/plain","transport":"upload","receivedAt":"2026-01-01T01:00:00+01:00","metadata":{"documentType":"cash_register_receipt"},"candidates":[{"companySlug":"distinct-target","evidenceKind":"portal","evidence":"Synthetic upload"}]});
    let good = observe_custom(
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
        good.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&good.stdout)
    );
    let metadata = json!({"sourceId":"source/synthetic","filename":"synthetic.txt","mimeType":"text/plain","transport":"upload","receivedAt":"2026-01-01T00:00:00Z","metadata":{"documentType":"cash_register_receipt"},"candidates":[{"companySlug":"distinct-target","evidenceKind":"portal","evidence":"Synthetic upload"}]});
    let mut wrong = response("ingest");
    wrong["source"]["receivedAt"] = json!("2026-02-01T00:00:00.000Z");
    let bad = observe_custom(
        "ingest",
        200,
        wrong,
        false,
        None,
        false,
        None,
        Some(metadata.clone()),
    );
    assert_eq!(bad.status.code(), Some(1));
    let mut metadata = metadata;
    metadata["transportIdentity"] = json!("explicit-upload-identity");
    let bad = observe_custom(
        "ingest",
        200,
        response("ingest"),
        false,
        None,
        false,
        None,
        Some(metadata),
    );
    assert_eq!(bad.status.code(), Some(1));
    // Advisory staging facts remain exact and do not assert completion eligibility.
    let facts = json!({"futureFact":{"kept":true}});
    let metadata = json!({"filename":"synthetic.txt","mimeType":"text/plain","transport":"upload","receivedAt":"2026-01-01T00:00:00.000Z","metadata":facts,"sourceId":"source/synthetic","candidates":[{"companySlug":"distinct-target","evidenceKind":"portal","evidence":"Synthetic upload"}]});
    let mut reply = response("ingest");
    reply["source"]["metadata"] = facts;
    let staged = observe_custom(
        "ingest",
        200,
        reply,
        false,
        None,
        false,
        None,
        Some(metadata),
    );
    assert_eq!(staged.status.code(), Some(0));
}
