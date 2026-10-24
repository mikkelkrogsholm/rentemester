use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};
const HASH: &str = "d157b0f801b28bbb6a6fe392fe76d5dcdfd0e2e17e7c50b8c5494d9ef7e195dd";
// Passive standalone manifest with the two frozen additions and canonical manifestId.
const MANIFEST: &str = r#"{"format":"rentemester-http-protocol/1","protocolMajor":1,"wireProfiles":["legacy-react-v1","rust-http-v1"],"operations":[{"method":"POST","path":"/api/companies/{slug}/workspace-parties/{id}/approve-merge","capability":"POST /api/companies/{slug}/workspace-parties/{id}/approve-merge"},{"method":"POST","path":"/api/companies/{slug}/workspace-parties/{id}/propose-merge","capability":"POST /api/companies/{slug}/workspace-parties/{id}/propose-merge"},{"method":"GET","path":"/api/me","capability":"GET /api/me"}],"manifestId":"f0a51ed7fa95da80416d27bf8ddbd6720d2a77e65417f786e514c12673a0b319"}"#;
fn party() -> Value {
    json!({"partyId":"source","kind":"organization","name":"Synthetic","source":"fixture","observedAt":"2026-01-01T00:00:00Z","reviewAssertion":"Reviewed","identifiers":[],"aliases":[],"assertions":[],"roles":[],"history":[
 {"event_type":"created","payload_hash":"a".repeat(64),"actor":"user:synthetic","created_at":"2026-01-01T00:00:00Z"},
 {"event_type":"proposed_merge","payload_hash":HASH,"actor":"user:synthetic","created_at":"2026-01-01T00:00:00Z"},
 {"event_type":"approved_merge","payload_hash":HASH,"actor":"user:synthetic","created_at":"2026-01-01T00:00:00Z"},
 {"event_type":"superseded","payload_hash":HASH,"actor":"user:synthetic","created_at":"2026-01-01T00:00:00Z"}]})
}
fn reply(op: &str) -> Value {
    if op == "propose" {
        json!({"ok":true,"proposalHash":HASH})
    } else {
        json!({"ok":true,"party":party()})
    }
}
fn body(op: &str) -> Value {
    if op == "propose" {
        json!({"intoPartyId":"target","reviewAssertion":"Reviewed\nsynthetic","confirm":true})
    } else {
        json!({"proposalHash":HASH,"confirm":true})
    }
}
fn fixture(
    response: Value,
    status: u16,
    protocol: bool,
) -> (String, thread::JoinHandle<Vec<(String, Value)>>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    l.set_nonblocking(true).unwrap();
    let worker = thread::spawn(move || {
        let mut writes = Vec::new();
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
            let header = String::from_utf8(bytes.clone()).unwrap();
            let lower = header.to_lowercase();
            let n = lower
                .lines()
                .find_map(|l| {
                    l.strip_prefix("content-length:")
                        .map(|n| n.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            let mut data = vec![0; n];
            s.read_exact(&mut data).unwrap();
            let (code, text) = if header.starts_with("GET /api/protocol ") {
                assert!(
                    !lower.contains("cookie:")
                        && !lower.contains("authorization:")
                        && !lower.contains("x-rentemester-protocol:")
                );
                (
                    200,
                    if protocol {
                        MANIFEST.to_owned()
                    } else {
                        "{}".into()
                    },
                )
            } else {
                assert!(lower.contains("cookie: session=synthetic"));
                assert!(lower.contains("x-rentemester-manifest:"));
                assert!(
                    !lower.contains("authorization:")
                        && !lower.contains("x-rentemester-service-key:")
                );
                if header.starts_with("GET /api/me ") {
                    (
                        200,
                        json!({"ok":true,"user":{"id":"synthetic"}}).to_string(),
                    )
                } else {
                    writes.push((
                        header.lines().next().unwrap().into(),
                        serde_json::from_slice(&data).unwrap(),
                    ));
                    (status, response.to_string())
                }
            };
            write!(s,"HTTP/1.1 {code} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",text.len()).unwrap();
        }
        writes
    });
    (origin, worker)
}
use std::process::{Command, Output, Stdio};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
fn run(args: &[String], stdin: Option<&str>) -> Output {
    let mut child = Command::new(BIN)
        .args(args)
        .env("RENTEMESTER_COMPANY", "must-not-inherit")
        .env("USER", "must-not-inherit")
        .env_remove("RENTEMESTER_WORKSPACE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(text) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
    } else {
        drop(child.stdin.take());
    }
    child.wait_with_output().unwrap()
}
fn args(op: &str) -> Vec<String> {
    let mut a = vec![
        "party".into(),
        format!("{op}-merge"),
        "--from-party-id=source".into(),
        "--confirm=yes".into(),
    ];
    if op == "propose" {
        a.extend([
            "--into-party-id=target".into(),
            "--review-assertion=Reviewed\nsynthetic".into(),
        ]);
    } else {
        a.push(format!("--proposal-hash={HASH}"));
    }
    a
}
fn cookie() -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "merge688-{}-{:?}.cookie",
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
    o.open(&p).unwrap().write_all(b"session=synthetic").unwrap();
    p
}
fn observe(
    op: &str,
    response: Value,
    status: u16,
    extra: &[&str],
    native: bool,
    protocol: bool,
) -> (Output, Vec<(String, Value)>) {
    let (origin, server) = fixture(response, status, protocol);
    let cookie = cookie();
    let mut a = if native {
        vec![
            "remote".into(),
            format!("{op}-workspace-party-merge"),
            "--party-id".into(),
            "source".into(),
            "--confirm".into(),
        ]
    } else {
        args(op)
    };
    a.extend([
        format!("--server={origin}"),
        "--allow-loopback-http".into(),
        format!("--cookie-file={}", cookie.display()),
        "--company=synthetic".into(),
    ]);
    a.extend(
        extra
            .iter()
            .filter(|s| !native || **s != "--json")
            .map(|s| s.to_string()),
    );
    if native {
        a = a
            .into_iter()
            .flat_map(|s| {
                if let Some((k, v)) = s.split_once('=') {
                    vec![k.to_string(), v.to_string()]
                } else {
                    vec![s]
                }
            })
            .collect();
    }
    let wire = body(op).to_string();
    let out = run(&a, if native { Some(&wire) } else { None });
    let requests = server.join().unwrap();
    std::fs::remove_file(cookie).unwrap();
    (out, requests)
}
#[test]
fn original_and_native_success_exact_flat_json_human_current_actor() {
    for op in ["propose", "approve"] {
        for native in [false, true] {
            let (out, requests) = observe(op, reply(op), 200, &["--json"], native, true);
            assert!(
                out.status.success(),
                "stdout={} stderr={}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let v: Value = serde_json::from_slice(&out.stdout).unwrap();
            if native {
                assert_eq!(v["data"], reply(op));
            } else {
                assert_eq!(v, reply(op));
            }
            assert_eq!(requests.len(), 1);
            assert_eq!(requests[0].1, body(op));
        }
        let (out, requests) = observe(
            op,
            reply(op),
            200,
            &["--format=human", "--actor=user:synthetic"],
            false,
            true,
        );
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).starts_with("✔ "));
        assert_eq!(requests.len(), 1);
        let (out, requests) = observe(op, reply(op), 200, &["--actor=user:other"], false, true);
        assert_eq!(out.status.code(), Some(2));
        assert!(requests.is_empty());
    }
}
#[test]
fn passive_discovery_and_syntax_refusals_before_credentials_or_files() {
    for a in [
        vec!["party", "--help"],
        vec![
            "party",
            "propose-merge",
            "--help",
            "--cookie-file=/not/read",
            "--local-profile=/not/read",
        ],
        vec!["remote", "--help"],
    ] {
        let out = run(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>(), None);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            String::from_utf8_lossy(&out.stdout).contains(if a[0] == "remote" {
                "propose-workspace-party-merge"
            } else {
                "propose-merge"
            })
        );
    }
    for flag in [
        "--confirm=false",
        "--confirm=YES",
        "--service-key-file=/not/read",
        "--actor=agent:synthetic",
        "--proposal-hash=bad",
        "--workspace=/not/read",
        "--read-confirm=false",
    ] {
        let mut a = args("propose");
        a.push(flag.into());
        a.extend([
            "--company=synthetic".into(),
            "--server=https://example.invalid".into(),
            "--cookie-file=/not/read".into(),
        ]);
        let out = run(&a, None);
        assert_eq!(out.status.code(), Some(2), "{flag}");
    }
    let mut a = args("propose");
    a.extend([
        "--server=https://example.invalid".into(),
        "--cookie-file=/not/read".into(),
    ]);
    assert_eq!(run(&a, None).status.code(), Some(2));
    // Split/attached valued flags and global placement are normalized without credentials on passive help.
    let a = [
        "--company=synthetic",
        "party",
        "--confirm",
        "yes",
        "propose-merge",
        "--from-party-id",
        "source",
        "--into-party-id=target",
        "--review-assertion",
        "Reviewed\nsynthetic",
        "--help",
    ];
    assert!(
        run(&a.iter().map(|s| s.to_string()).collect::<Vec<_>>(), None)
            .status
            .success()
    );
}
#[test]
fn unknown_bad_receipts_business_refusal_and_protocol_stay_false_without_retry() {
    for (status, response, protocol) in [
        (200, json!({"ok":true}), true),
        (500, reply("propose"), true),
        (200, json!({"ok":false,"errors":["refused"]}), true),
        (200, reply("propose"), false),
    ] {
        let (out, requests) = observe("propose", response, status, &["--json"], false, protocol);
        assert_eq!(out.status.code(), Some(1));
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["ok"], false);
        assert_eq!(requests.len(), usize::from(protocol));
    }
}
#[cfg(unix)]
#[test]
fn local_unix_workspace_assertion_long_company_path_and_no_tcp_fallback() {
    use std::{
        fs,
        os::unix::{
            fs::{DirBuilderExt, PermissionsExt},
            net::UnixListener,
        },
    };
    let base = fs::canonicalize(std::env::var_os("HOME").unwrap())
        .unwrap()
        .join(format!(".rm-party688-{}", std::process::id()));
    fs::DirBuilder::new().mode(0o700).create(&base).unwrap();
    let workspace = base.join("workspace");
    fs::create_dir(&workspace).unwrap();
    let company = format!("synthetic-{}", "x".repeat(85));
    let company_path = workspace.join(&company);
    fs::create_dir(&company_path).unwrap();
    assert!(company_path.as_os_str().len() > 128);
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    tcp.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", tcp.local_addr().unwrap());
    let listener = UnixListener::bind(base.join("r.sock")).unwrap();
    fs::set_permissions(base.join("r.sock"), fs::Permissions::from_mode(0o600)).unwrap();
    let private = |p: &std::path::Path, b: &[u8]| {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(p)
            .unwrap()
            .write_all(b)
            .unwrap();
    };
    private(&base.join("r.json"),json!({"format":"rentemester-local-runtime/1","origin":origin,"socket":"r.sock","workspaceRoot":workspace,"workspaceId":"synthetic"}).to_string().as_bytes());
    private(&base.join("cookie"), b"session=synthetic");
    let local = |op: &str, w: &std::path::Path| {
        let mut a = args(op);
        a.extend([
            format!("--local-profile={}", base.display()),
            format!("--workspace={}", w.display()),
            format!("--company={}", company_path.display()),
            format!("--cookie-file={}", base.join("cookie").display()),
        ]);
        a
    };
    assert_eq!(run(&local("propose", &base), None).status.code(), Some(2));
    let server = thread::spawn(move || {
        for op in ["propose", "approve"] {
            for discovery in [true, false] {
                let (mut s, _) = listener.accept().unwrap();
                s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut b = Vec::new();
                while !b.ends_with(b"\r\n\r\n") {
                    let mut x = [0];
                    s.read_exact(&mut x).unwrap();
                    b.push(x[0]);
                }
                let header = String::from_utf8(b).unwrap();
                let lower = header.to_lowercase();
                let text = if discovery {
                    assert!(header.starts_with("GET /api/protocol "));
                    assert!(!lower.contains("cookie:"));
                    MANIFEST.to_owned()
                } else {
                    assert!(header.starts_with(&format!(
                        "POST /api/companies/{company}/workspace-parties/source/{op}-merge "
                    )));
                    assert!(lower.contains("cookie: session=synthetic"));
                    let n = lower
                        .lines()
                        .find_map(|l| {
                            l.strip_prefix("content-length:")
                                .map(|n| n.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    let mut b = vec![0; n];
                    s.read_exact(&mut b).unwrap();
                    assert_eq!(serde_json::from_slice::<Value>(&b).unwrap(), body(op));
                    reply(op).to_string()
                };
                write!(s,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",text.len()).unwrap();
            }
        }
    });
    for op in ["propose", "approve"] {
        let out = run(&local(op, &workspace), None);
        assert!(
            out.status.success(),
            "{} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).unwrap(),
            reply(op)
        );
    }
    server.join().unwrap();
    assert!(tcp.accept().is_err());
    fs::remove_file(base.join("r.sock")).unwrap();
    assert_eq!(
        run(&local("propose", &workspace), None).status.code(),
        Some(2)
    );
    assert!(tcp.accept().is_err());
    fs::remove_dir_all(base).unwrap();
}
