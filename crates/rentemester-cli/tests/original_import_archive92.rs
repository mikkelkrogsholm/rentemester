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
        "rm-archive92-{}-{}",
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
        .args(["import", "archive", "--company", "synthetic"])
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

const LIST: &str = "GET /api/companies/synthetic/archives HTTP/1.1";
const YEAR: &str = "GET /api/companies/synthetic/archives/2024 HTTP/1.1";
fn value(o: &std::process::Output, exit: i32) -> Value {
    assert_eq!(o.status.code(), Some(exit), "{o:?}");
    assert!(o.stderr.is_empty(), "{o:?}");
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v.as_object().unwrap().len(), 3);
    assert!(v.get("years").unwrap().is_array());
    v
}
#[test]
fn list_and_selected_year_preserve_original_shape_and_raw_witnesses() {
    for flags in [
        vec!["--json"],
        vec!["--json", "--system= dinero "],
        vec!["--json", "--system=  ", "--year= "],
    ] {
        let (o, b) = run_fixture(&flags, vec![(LIST, 200, reply(vec![header()]))]);
        let v = value(&o, 0);
        assert_eq!(b.len(), 1);
        assert_eq!(v["years"][0].as_object().unwrap().len(), 5);
        assert_eq!(v["years"][0]["postingCount"], 3);
    }
    for year in ["2024", "2024.0", "0x7e8", "2.024e3"] {
        let (o, b) = run_fixture(
            &["--json", "--year", year],
            vec![
                (LIST, 200, reply(vec![header()])),
                (YEAR, 200, reply(vec![detail()])),
            ],
        );
        let v = value(&o, 0);
        assert_eq!(b.len(), 2);
        let h = &v["years"][0];
        assert_eq!(h.as_object().unwrap().len(), 7);
        for (i, n) in [1.234567, 0.000001, -1.234567].into_iter().enumerate() {
            assert_eq!(h["postings"][i]["amount"], json!(n));
        }
        assert_eq!(h["postings"][0]["runningBalance"], json!(-1.234567));
        assert_eq!(h["postings"][1]["runningBalance"], json!(0.000001));
        assert_eq!(h["balances"][0]["amount"], json!(-1.234567));
        assert!(h["postings"][0].get("sourceAmount").is_none());
    }
}
#[test]
fn arbitrary_sources_and_integer_year_absence_are_after_guarded_list() {
    for source in ["other", "Dinero", "unknown"] {
        let (o, b) = run_fixture(
            &["--json", "--system", source],
            vec![(LIST, 200, reply(vec![header()]))],
        );
        assert_eq!(value(&o, 0)["years"], json!([]));
        assert_eq!(b.len(), 1);
    }
    for year in [
        "2026.0",
        "0x7ea",
        "0",
        "-1",
        "10000",
        "9007199254740992",
        "1e100",
    ] {
        let (o, b) = run_fixture(
            &["--json", "--year", year],
            vec![(LIST, 200, reply(vec![header()]))],
        );
        let v = value(&o, 1);
        assert_eq!(v["years"], json!([]));
        assert!(
            v["errors"][0]
                .as_str()
                .unwrap()
                .starts_with("no archived dinero data for fiscal year ")
        );
        assert_eq!(b.len(), 1);
    }
}
#[test]
fn parse_errors_are_stderr_two_before_http_or_credentials() {
    for flag in [
        "--year=2024.5",
        "--year=NaN",
        "--year=Infinity",
        "--year=1e999",
        "--year=potato",
        "--unknown",
        "--confirm",
        "--file=/never",
        "--year=+0x7e8",
    ] {
        let (o, b) = run_fixture(&[flag, "--json"], vec![]);
        assert_eq!(o.status.code(), Some(2), "{o:?}");
        assert!(o.stdout.is_empty());
        assert!(!o.stderr.is_empty());
        assert!(b.is_empty());
    }
}
#[test]
fn current_refusal_and_tamper_after_list_no_cached_detail_or_fallback() {
    for (status, code) in [(403, "forbidden"), (409, "archive_original_unverified")] {
        let r = json!({"ok":false,"code":code,"errors":["current source access refused"]});
        let (o, b) = run_fixture(
            &["--json", "--year=2024"],
            vec![
                (LIST, 200, reply(vec![header()])),
                (YEAR, status, r.clone()),
            ],
        );
        let v = value(&o, 1);
        assert_eq!(v["years"], json!([]));
        assert_eq!(v["errors"], r["errors"]);
        assert_eq!(b.len(), 2);
        let (o, b) = run_fixture(&["--json", "--system=unknown"], vec![(LIST, status, r)]);
        assert_eq!(value(&o, 1)["ok"], false);
        assert_eq!(b.len(), 1);
    }
    let mut d = detail();
    d["postings"][0]
        .as_object_mut()
        .unwrap()
        .remove("sourceAmount");
    let (o, b) = run_fixture(
        &["--json", "--year=2024"],
        vec![
            (LIST, 200, reply(vec![header()])),
            (YEAR, 200, reply(vec![d])),
        ],
    );
    assert_eq!(value(&o, 1)["years"], json!([]));
    assert_eq!(b.len(), 2);
}
#[test]
fn human_headers_information_and_business_failure_stderr() {
    let (o, _) = run_fixture(&["--format=human"], vec![(LIST, 200, reply(vec![header()]))]);
    assert!(o.status.success());
    let text = String::from_utf8(o.stdout).unwrap();
    for word in [
        "fiscalYear",
        "sourceSystem",
        "postings",
        "balances",
        "importedAt",
        "2024",
        "dinero",
        "2026-10-20T12:00:00.000Z",
    ] {
        assert!(text.contains(word), "{text}");
    }
    let (o, _) = run_fixture(
        &["--year=2024", "--format=human"],
        vec![
            (LIST, 200, reply(vec![header()])),
            (YEAR, 200, reply(vec![detail()])),
        ],
    );
    assert!(o.status.success());
    let text = String::from_utf8(o.stdout).unwrap();
    for word in [
        "Archived dinero fiscal year 2024",
        "read-only, not in the live ledger",
        "konto | dato | bilag | tekst | beloeb",
        "konto | kontonavn | beloeb",
        "1.234567",
        "0.000001",
    ] {
        assert!(text.contains(word), "{text}");
    }
    let (o, _) = run_fixture(&["--year=2026", "--format=human"], vec![(LIST, 200, reply(vec![header()]))]);
    assert_eq!(o.status.code(), Some(1));
    assert!(o.stdout.is_empty());
    assert_eq!(
        String::from_utf8(o.stderr).unwrap(),
        "no archived dinero data for fiscal year 2026\n"
    );
}
#[test]
fn verified_unix_owner_workspace_and_passive_protocol_failure() {
    use std::os::unix::{
        fs::{DirBuilderExt, PermissionsExt},
        net::UnixListener,
    };
    let path = std::fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!("a92-{}", std::process::id()));
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
    let task = thread::spawn(move || {
        let manifest: Value =
            serde_json::from_str(include_str!("../../../protocol/http-v1.json")).unwrap();
        let (mut s, _) = l.accept().unwrap();
        wire(&mut s, "GET /api/protocol HTTP/1.1", 200, &manifest);
        let (mut s, _) = l.accept().unwrap();
        wire(&mut s, LIST, 200, &reply(vec![header()]));
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
            .args(["import", "archive", "--company=synthetic"])
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
        json!({"ok":true,"years":[{"fiscalYear":2024,"sourceSystem":"dinero","postingCount":3,"balanceCount":1,"importedAt":"2026-10-20T12:00:00.000Z"}],"errors":[]})
    );
    let o = call(&[]);
    assert_eq!(o.status.code(), Some(1));
    let text = String::from_utf8_lossy(&o.stdout);
    assert!(text.contains("invalid server response"), "{text}");
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
}
