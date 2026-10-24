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
    if status == 0 {
        return b[end.unwrap()..].to_vec();
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
        let deadline = std::time::Instant::now();
        while deadline.elapsed() < std::time::Duration::from_millis(80) {
            assert!(
                matches!(l.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
                "unexpected retry"
            );
            thread::sleep(std::time::Duration::from_millis(5));
        }
        bodies
    });
    let out = Command::new(BIN)
        .args(["import", "run", "--company", "synthetic"])
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
fn receipt(system: &str, dry: bool) -> Value {
    let mut v = json!({"ok":true,"sourceSystem":system,"openingBalanceLineCount":2,"historicalEntriesSkipped":0,"auditTrail":["original source audit"],"appliedRules":["DK-OPENING-BALANCE-001"],"errors":[]});
    if dry {
        v["dryRun"] = json!(true);
    }
    v
}
#[test]
fn synthetic_exact_wire_flat_result_and_valued_dry_run() {
    let p = csv();
    for flag in [None, Some("--dry-run=false"), Some("--dry-run=yes")] {
        let dry = flag.is_some();
        let r = receipt("synthetic-csv", dry);
        let mut args = vec!["--file", p.to_str().unwrap(), "--json"];
        if let Some(f) = flag {
            args.push(f);
        }
        let (o, b) = run_fixture(
            &args,
            vec![(
                "POST /api/companies/synthetic/import-original HTTP/1.1",
                200,
                r.clone(),
            )],
        );
        assert!(o.status.success(), "{o:?}");
        assert_eq!(serde_json::from_slice::<Value>(&o.stdout).unwrap(), r);
        assert_eq!(b.len(), 1);
        let v: Value = serde_json::from_slice(&b[0]).unwrap();
        assert_eq!(
            v,
            json!({"sourceSystem":"synthetic-csv","fileName":p.file_name().unwrap().to_str().unwrap(),"contentBase64":"S29udGFrdG5hdm4KU3ludGhldGljCg==","dryRun":dry,"confirm":true})
        );
    }
    std::fs::remove_file(p).unwrap();
}
#[test]
fn dinero_zip_and_directory_snapshot_one_post() {
    let dir = std::env::temp_dir().join(format!("run94-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let zip = dir.join("export.zip");
    // Exact raw container bytes travel unchanged; parsing belongs to the server.
    std::fs::write(&zip, b"PK\x05\x06\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0").unwrap();
    let folder = dir.join("snapshot");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(
        folder.join("Kontoplan.csv"),
        b"Konto;Navn\n1000;Synthetic\n",
    )
    .unwrap();
    for path in [&zip, &folder] {
        let r = receipt("dinero", false);
        let (o, b) = run_fixture(
            &[
                "--file",
                path.to_str().unwrap(),
                "--system= dinero ",
                "--json",
            ],
            vec![(
                "POST /api/companies/synthetic/import-original HTTP/1.1",
                200,
                r.clone(),
            )],
        );
        assert!(o.status.success(), "{o:?}");
        assert_eq!(b.len(), 1);
        let v: Value = serde_json::from_slice(&b[0]).unwrap();
        assert_eq!(v.as_object().unwrap().len(), 5);
        assert_eq!(v["sourceSystem"], "dinero");
        assert_eq!(
            v["fileName"],
            if path == &zip {
                "export.zip"
            } else {
                "dinero-directory-source.json"
            }
        );
        assert_eq!(serde_json::from_slice::<Value>(&o.stdout).unwrap(), r);
    }
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn business_refusal_authority_wrong_receipt_and_503_no_retry() {
    let p = csv();
    let mut refusal = receipt("synthetic-csv", false);
    refusal["ok"] = json!(false);
    refusal["errors"] = json!(["Source chart conflicts with historically used account"]);
    let (o, b) = run_fixture(
        &["--file", p.to_str().unwrap(), "--json"],
        vec![(
            "POST /api/companies/synthetic/import-original HTTP/1.1",
            400,
            refusal.clone(),
        )],
    );
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(b.len(), 1);
    assert_eq!(serde_json::from_slice::<Value>(&o.stdout).unwrap(), refusal);
    for (status, r, unknown) in [
        (0, Value::Null, true),
        (200, Value::Null, true),
        (200, receipt("dinero", false), true),
        (
            503,
            json!({"ok":false,"code":"import_original_unavailable"}),
            true,
        ),
        (403, json!({"ok":false,"code":"access_denied"}), false),
    ] {
        let (o, b) = run_fixture(
            &["--file", p.to_str().unwrap(), "--json"],
            vec![(
                "POST /api/companies/synthetic/import-original HTTP/1.1",
                status,
                r,
            )],
        );
        assert_eq!(o.status.code(), Some(1));
        assert_eq!(b.len(), 1);
        assert_eq!(
            String::from_utf8_lossy(&o.stdout).contains("outcome_unknown"),
            unknown,
            "{o:?}"
        );
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
    std::fs::remove_file(p).unwrap();
}
#[test]
fn usage_and_source_validation_before_credentials() {
    let p = csv();
    for flags in [
        vec!["--dry-run"],
        vec!["--confirm"],
        vec!["--system=unknown"],
        vec!["--system=dinero"],
        vec!["--file=/never/existing"],
    ] {
        let o = Command::new(BIN)
            .args(["import", "run", "--file", p.to_str().unwrap()])
            .args(flags)
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(2), "{o:?}");
    }
    for raw in [vec![], vec![255], vec![b'x'; 48 * 1024 + 1]] {
        std::fs::write(&p, raw).unwrap();
        let o = Command::new(BIN)
            .args([
                "import",
                "run",
                "--file",
                p.to_str().unwrap(),
                "--cookie-file=/never",
            ])
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(2));
        assert!(
            !String::from_utf8_lossy(&o.stderr).contains("Explicit --company"),
            "{o:?}"
        );
    }
    let o = Command::new(BIN)
        .args(["import", "run", "--help"])
        .output()
        .unwrap();
    assert!(o.status.success());
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
        .join(format!("run94unix-{}", std::process::id()));
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
            "POST /api/companies/synthetic/import-original HTTP/1.1",
            200,
            &receipt("synthetic-csv", false),
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
                "run",
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
        receipt("synthetic-csv", false)
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
