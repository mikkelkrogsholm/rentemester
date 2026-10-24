//! Actual binary and current native HTTP producers, not a replacement engine.
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
fn wire<S: Read + Write>(s: &mut S, expected: &str, status: u16, result: &Value) -> Vec<u8> {
    let mut b = vec![];
    let mut end = None;
    let mut length = 0;
    loop {
        let mut buf = [0; 4096];
        let n = s.read(&mut buf).unwrap();
        assert!(n > 0);
        b.extend_from_slice(&buf[..n]);
        if end.is_none() {
            if let Some(i) = b.windows(4).position(|w| w == b"\r\n\r\n") {
                end = Some(i + 4);
                for l in String::from_utf8_lossy(&b[..i]).lines() {
                    if let Some(v) = l.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap();
                    }
                }
            }
        }
        if end.is_some_and(|i| b.len() >= i + length) {
            break;
        }
    }
    let text = String::from_utf8_lossy(&b);
    assert!(text.starts_with(expected), "{text}");
    if expected.starts_with("GET /api/protocol ") {
        assert!(!text.to_ascii_lowercase().contains("cookie:"));
    } else {
        assert!(
            text.to_ascii_lowercase()
                .contains("cookie: session=synthetic")
        );
        assert!(
            text.to_ascii_lowercase()
                .contains("x-rentemester-manifest:")
        );
    }
    let body = result.to_string();
    write!(s,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection:close\r\n\r\n{body}",body.len()).unwrap();
    b[end.unwrap()..].to_vec()
}
fn bounded_accept(l: &TcpListener) -> Option<std::net::TcpStream> {
    let at = std::time::Instant::now();
    loop {
        match l.accept() {
            Ok((s, _)) => {
                s.set_nonblocking(false).unwrap();
                return Some(s);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if at.elapsed() > std::time::Duration::from_secs(5) {
                    return None;
                }
                thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(e) => panic!("{e}"),
        }
    }
}
fn run_fixture(
    args: &[&str],
    steps: Vec<(&'static str, u16, Value)>,
) -> (std::process::Output, Vec<Vec<u8>>) {
    use std::os::unix::fs::OpenOptionsExt;
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let cookie = std::env::temp_dir().join(format!(
        "rm-company716-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&cookie)
        .unwrap()
        .write_all(b"session=synthetic")
        .unwrap();
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    l.set_nonblocking(true).unwrap();
    let task = thread::spawn(move || {
        let manifest: Value =
            serde_json::from_str(include_str!("../../../protocol/http-v1.json")).unwrap();
        let mut bodies = vec![];
        for (path, status, v) in steps {
            let Some(mut s) = bounded_accept(&l) else {
                return bodies;
            };
            s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            wire(&mut s, "GET /api/protocol HTTP/1.1", 200, &manifest);
            let Some(mut s) = bounded_accept(&l) else {
                return bodies;
            };
            s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            bodies.push(wire(&mut s, path, status, &v));
        }
        bodies
    });
    let out = Command::new(BIN)
        .args(["import", "contacts", "--company", "synthetic"])
        .args(args)
        .args([
            "--server",
            &origin,
            "--cookie-file",
            cookie.to_str().unwrap(),
            "--allow-loopback-http",
        ])
        .output()
        .unwrap();
    std::fs::remove_file(cookie).unwrap();
    (out, task.join().unwrap())
}
fn csv() -> std::path::PathBuf {
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let p = std::env::temp_dir().join(format!(
        "import716-{}-{}.csv",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&p, "Kontaktnavn\nSynthetic\n").unwrap();
    p
}
fn receipt() -> Value {
    json!({"ok":true,"import":{"detected":{"id":"dinero-contacts","label":"Dinero — Kontakter (kunder og leverandører)","system":"Dinero","dataType":"contacts"},"summary":{"parsed":1,"customersCreated":0,"vendorsCreated":1,"skipped":0,"enriched":0,"enrichmentFailures":1},"errors":["Synthetic warning"]}})
}
#[test]
fn offline_systems_help_human_and_no_company() {
    let o = Command::new(BIN)
        .args([
            "import",
            "systems",
            "--local-profile=/never-opened",
            "--workspace=/never-opened",
            "--cookie-file=/never-opened",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(o.status.success(), "{o:?}");
    assert!(String::from_utf8_lossy(&o.stdout).contains("synthetic-csv"));
    for args in [
        vec!["import", "systems", "--json"],
        vec!["import", "systems", "--format=human"],
        vec!["import", "--help"],
    ] {
        let o = Command::new(BIN)
            .args(&args)
            .env_remove("RENTEMESTER_COMPANY")
            .output()
            .unwrap();
        assert!(o.status.success(), "{o:?}");
        let text = String::from_utf8_lossy(&o.stdout);
        if args[1] == "systems" {
            assert!(text.contains("Synthetic CSV (import-framework example)"));
            assert!(text.contains(
                "Dinero (data export — chart of accounts, master data & opening balance)"
            ));
        } else {
            assert!(text.contains("--default-role"));
        }
    }
    for args in [
        vec!["import", "run"],
        vec!["import", "archive"],
        vec!["import", "systems", "--file=/never"],
        vec!["import", "systems", "--format=bad"],
    ] {
        let o = Command::new(BIN).args(args).output().unwrap();
        assert_eq!(o.status.code(), Some(2));
    }
}
#[test]
fn exact_body_defaults_trim_enrich_warnings_and_business_failure() {
    let p = csv();
    for (flags, role, enrich) in [
        (vec![], "vendor", false),
        (
            vec!["--default-role= customer ", "--enrich-cvr"],
            "customer",
            true,
        ),
        (vec!["--default-role=   "], "vendor", false),
    ] {
        let mut args = vec!["--file", p.to_str().unwrap(), "--format=human", "--json"];
        args.extend(flags);
        let r = receipt();
        let (o, b) = run_fixture(
            &args,
            vec![(
                "POST /api/companies/synthetic/import HTTP/1.1",
                200,
                r.clone(),
            )],
        );
        assert!(o.status.success(), "{o:?}");
        assert_eq!(b.len(), 1);
        assert_eq!(
            serde_json::from_slice::<Value>(&b[0]).unwrap(),
            json!({"fileName":p.file_name().unwrap().to_str().unwrap(),"content":"Kontaktnavn\nSynthetic\n","enrichCvr":enrich,"confirm":true,"defaultRole":role})
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&o.stdout).unwrap(),
            json!({"ok":true,"summary":r["import"]["summary"],"errors":["Synthetic warning"]})
        );
    }
    let (o, b) = run_fixture(
        &["--file", p.to_str().unwrap(), "--json"],
        vec![(
            "POST /api/companies/synthetic/import HTTP/1.1",
            400,
            json!({"ok":false,"code":"contact_import_invalid","errors":["Invalid CSV"]}),
        )],
    );
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(b.len(), 1);
    assert!(String::from_utf8_lossy(&o.stdout).contains("Invalid CSV"));
    std::fs::remove_file(p).unwrap();
}
#[test]
fn malformed_unknown_and_authority_one_post_no_retry() {
    let p = csv();
    for r in [
        Value::Null,
        json!({"ok":true}),
        {
            let mut r = receipt();
            r["extra"] = json!(true);
            r
        },
        {
            let mut r = receipt();
            r["import"]["summary"]["parsed"] = json!("1");
            r
        },
    ] {
        let (o, b) = run_fixture(
            &["--file", p.to_str().unwrap(), "--json"],
            vec![("POST /api/companies/synthetic/import HTTP/1.1", 200, r)],
        );
        assert_eq!(o.status.code(), Some(1));
        assert_eq!(b.len(), 1);
        assert!(String::from_utf8_lossy(&o.stdout).contains("outcome_unknown"));
    }
    let (o, b) = run_fixture(
        &[
            "--file",
            p.to_str().unwrap(),
            "--actor=user:forged",
            "--json",
        ],
        vec![(
            "GET /api/me HTTP/1.1",
            200,
            json!({"ok":true,"user":{"id":"current"}}),
        )],
    );
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(b.len(), 1);
    assert!(String::from_utf8_lossy(&o.stdout).contains("actor differs"));
    let (o, b) = run_fixture(
        &["--file", p.to_str().unwrap(), "--json"],
        vec![(
            "POST /api/companies/synthetic/import HTTP/1.1",
            403,
            json!({"ok":false,"code":"access_denied"}),
        )],
    );
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(b.len(), 1);
    std::fs::remove_file(p).unwrap();
}
#[test]
fn filesystem_and_invalid_role_precede_credentials() {
    let p = csv();
    std::fs::write(&p, [0xff]).unwrap();
    for file in [p.to_str().unwrap(), "/never/existing.csv", "/tmp"] {
        let o = Command::new(BIN)
            .args([
                "import",
                "contacts",
                "--company=synthetic",
                "--file",
                file,
                "--server=https://example.invalid",
                "--cookie-file=/never",
            ])
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(2), "{o:?}");
        assert!(o.stdout.is_empty());
    }
    let o = Command::new(BIN)
        .args([
            "import",
            "contacts",
            "--default-role=invalid",
            "--file",
            p.to_str().unwrap(),
            "--cookie-file=/never",
        ])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&o.stderr).contains("--default-role"));
    std::fs::remove_file(p).unwrap();
}
#[test]
fn verified_unix_owner_workspace_and_passive_protocol_failure() {
    use std::os::unix::{
        fs::{DirBuilderExt, PermissionsExt},
        net::UnixListener,
    };
    let path = std::fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!("i716-{}", std::process::id()));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&path)
        .unwrap();
    let workspace = path.join("workspace");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&workspace)
        .unwrap();
    let l = UnixListener::bind(path.join("r.sock")).unwrap();
    std::fs::set_permissions(path.join("r.sock"), std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::write(path.join("r.json"),json!({"format":"rentemester-local-runtime/1","origin":"http://127.0.0.1:33449","socket":"r.sock","workspaceRoot":workspace,"workspaceId":"synthetic"}).to_string()).unwrap();
    std::fs::set_permissions(path.join("r.json"), std::fs::Permissions::from_mode(0o600)).unwrap();
    let cookie = path.join("cookie");
    std::fs::write(&cookie, "session=synthetic").unwrap();
    std::fs::set_permissions(&cookie, std::fs::Permissions::from_mode(0o600)).unwrap();
    let csv_file = csv();
    let task = thread::spawn(move || {
        let manifest: Value =
            serde_json::from_str(include_str!("../../../protocol/http-v1.json")).unwrap();
        let (mut s, _) = l.accept().unwrap();
        wire(&mut s, "GET /api/protocol HTTP/1.1", 200, &manifest);
        let (mut s, _) = l.accept().unwrap();
        wire(
            &mut s,
            "POST /api/companies/synthetic/import HTTP/1.1",
            200,
            &receipt(),
        );
        let (mut s, _) = l.accept().unwrap();
        wire(
            &mut s,
            "GET /api/protocol HTTP/1.1",
            200,
            &json!({"format":"bad"}),
        );
    });
    let call = |args: &[&str]| {
        Command::new(BIN)
            .args([
                "import",
                "contacts",
                "--company=synthetic",
                "--file",
                csv_file.to_str().unwrap(),
            ])
            .args(args)
            .args([
                "--local-profile",
                path.to_str().unwrap(),
                "--cookie-file",
                cookie.to_str().unwrap(),
                "--json",
            ])
            .output()
            .unwrap()
    };
    let o = call(&["--workspace", workspace.to_str().unwrap()]);
    assert!(o.status.success(), "{:?}", o);
    assert_eq!(
        serde_json::from_slice::<Value>(&o.stdout).unwrap(),
        json!({"ok":true,"summary":receipt()["import"]["summary"],"errors":["Synthetic warning"]})
    );
    let o = call(&[]);
    assert_eq!(o.status.code(), Some(1));
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(text.contains("protocol"));
    assert!(!text.contains("outcome_unknown"));
    task.join().unwrap();
    let o = call(&["--workspace", "/wrong"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stdout).contains("binding refused"));
    std::fs::set_permissions(path.join("r.json"), std::fs::Permissions::from_mode(0o644)).unwrap();
    let o = call(&[]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stdout).contains("binding refused"));
    std::fs::remove_dir_all(path).unwrap();
    std::fs::remove_file(csv_file).unwrap();
}
