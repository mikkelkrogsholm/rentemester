use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "rm159-period-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let fixture = Self(path);
        for (name, bytes) in [
            ("cookie", b"session=synthetic".as_slice()),
            ("key", b"synthetic-key".as_slice()),
        ] {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options
                .open(fixture.0.join(name))
                .unwrap()
                .write_all(bytes)
                .unwrap();
        }
        fixture
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn server(status: u16, value: Value) -> (String, thread::JoinHandle<Value>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = vec![];
        let end = loop {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(i) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let header = String::from_utf8_lossy(&bytes[..end]);
        let request = header.lines().next().unwrap().to_owned();
        assert!(header.contains("session=synthetic"));
        let length: usize = header
            .lines()
            .find_map(|l| {
                l.to_lowercase()
                    .strip_prefix("content-length:")
                    .map(|s| s.trim().parse().unwrap())
            })
            .unwrap_or(0);
        while bytes.len() < end + length {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
        }
        let body = value.to_string();
        write!(
            s,
            "HTTP/1.1 {status} Mock\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        json!({"request":request,"body":if length == 0 { Value::Null } else { serde_json::from_slice::<Value>(&bytes[end..end + length]).unwrap() }})
    });
    (origin, t)
}

fn call(kind: &str, origin: &str, f: &Fixture, service: bool, extra: &[&str]) -> Output {
    Command::new(BIN)
        .args([
            "period",
            kind,
            "--server",
            origin,
            "--allow-loopback-http",
            "--company",
            "synthetic",
            if service {
                "--service-key-file"
            } else {
                "--cookie-file"
            },
            f.0.join(if service { "key" } else { "cookie" })
                .to_str()
                .unwrap(),
        ])
        .args(extra)
        .output()
        .unwrap()
}

#[test]
fn five_exact_flat_contracts() {
    let f = Fixture::new();
    let hash = "a".repeat(64);
    let packet = json!({"version":3,"periodStart":"2024-01-01","periodEnd":"2024-03-31","cutoff":"2024-03-31","controlsRun":[],"items":[],"blockers":0,"warnings":0,"hash":hash});
    let review = json!({"id":7,"packet":packet,"reviewerActor":"user:server","reviewerPrincipal":null,"createdAt":"persisted"});
    let receipt = json!({"ok":true,"periodId":19,"periodStart":"2024-01-01","periodEnd":"2024-03-31","kind":"vat_period","appliedRules":["DK-BOOKKEEPING-PERIOD-LOCK-001"],"errors":[],"readinessPacket":packet});
    for (op, args, path, body, result) in [
        (
            "readiness",
            vec!["--from=2024-01-01", "--to", "2024-03-31"],
            "GET /api/companies/synthetic/periods/original/readiness?from=2024-01-01&to=2024-03-31 HTTP/1.1",
            Value::Null,
            json!({"ok":true,"packet":packet}),
        ),
        (
            "review",
            vec![
                "--from",
                "2024-01-01",
                "--to",
                "2024-03-31",
                "--packet-hash",
                &hash,
                "--confirm=yes",
            ],
            "POST /api/companies/synthetic/periods/original/review HTTP/1.1",
            json!({"periodStart":"2024-01-01","periodEnd":"2024-03-31","packetHash":hash,"confirm":true}),
            review.clone(),
        ),
        (
            "status",
            vec!["--review-id=7"],
            "GET /api/companies/synthetic/periods/original/status?reviewId=7 HTTP/1.1",
            Value::Null,
            json!({"ok":true,"review":review}),
        ),
        (
            "close",
            vec![
                "--from",
                "2024-01-01",
                "--to",
                "2024-03-31",
                "--packet-hash",
                &hash,
                "--review-id",
                "7",
                "--status=reported",
                "--reference=receipt=a",
                "--confirm=yes",
            ],
            "POST /api/companies/synthetic/periods/original/close HTTP/1.1",
            json!({"periodStart":"2024-01-01","periodEnd":"2024-03-31","packetHash":hash,"reviewId":7,"status":"reported","reference":"receipt=a","confirm":true}),
            { let mut v=receipt.clone(); v["status"]=json!("reported"); v["reference"]=json!("receipt=a"); v },
        ),
        (
            "reopen",
            vec![
                "--from",
                "2024-01-01",
                "--to",
                "2024-03-31",
                "--reason= correction ",
                "--kind=custom",
            ],
            "POST /api/companies/synthetic/periods/original/reopen HTTP/1.1",
            json!({"periodStart":"2024-01-01","periodEnd":"2024-03-31","reason":" correction ","kind":"custom","confirm":true}),
            { let mut v=receipt.clone(); v["kind"]=json!("custom"); v["effectiveStatus"]=json!("open"); v["reopenedBy"]=json!("user:server"); v["reason"]=json!("correction"); v },
        ),
    ] {
        let (origin, t) = server(200, result.clone());
        let mut args = args;
        args.extend(["--format=json"]);
        let out = call(op, &origin, &f, false, &args);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stderr.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).unwrap(),
            result
        );
        assert_eq!(t.join().unwrap(), json!({"request":path,"body":body}));
    }
}
#[test]
fn strict_refusals_before_profile_or_credentials() {
    for args in [
        vec![
            "review",
            "--from=2024-01-01",
            "--to=2024-03-31",
            "--confirm=yes",
        ],
        vec![
            "close",
            "--from=2024-01-01",
            "--to=2024-03-31",
            "--confirm=no",
        ],
        vec![
            "close",
            "--from=2024-01-01",
            "--to=2024-03-31",
            "--confirm=yes",
            "--force",
        ],
        vec![
            "close",
            "--from=2024-01-01",
            "--to=2024-03-31",
            "--confirm=yes",
            "--force=false",
        ],
        vec![
            "reopen",
            "--from=2024-01-01",
            "--to=2024-03-31",
            "--reason= ",
        ],
        vec![
            "reopen",
            "--from=2024-01-01",
            "--to=2024-03-31",
            "--reason=x",
            "--confirm=yes",
        ],
        vec!["status", "--review-id=9007199254740992"],
        vec!["status", "--review-id=1", "--review-id=2"],
        vec!["status", "--review-id=1", "--json", "--json"],
        vec!["readiness", "--from=2024-02-30", "--to=2024-03-31"],
        vec![
            "readiness",
            "--from=2024-01-01",
            "--to=2024-03-31",
            "--actor=user:x",
        ],
    ] {
        let out = Command::new(BIN)
            .arg("period")
            .args(&args)
            .args([
                "--local-profile=/missing/private-profile",
                "--cookie-file=/missing/cookie",
            ])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(
            out.stdout.is_empty(),
            "must reject argv before loading profile: {args:?}"
        );
    }
    for op in ["readiness", "review", "status", "close", "reopen"] {
        let out = Command::new(BIN)
            .args([
                "period",
                op,
                "--help",
                "--local-profile=/missing/profile",
                "--cookie-file=/missing/cookie",
            ])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(0));
        assert!(out.stderr.is_empty());
    }
}
#[test]
fn business_refusals_and_no_missing_pair_fabrication() {
    let f = Fixture::new();
    for status in [200, 422] {
        let result = json!({"ok":false,"code":"PERIOD_CLOSE_PACKET_STALE_OR_MISSING","errors":["PERIOD_CLOSE_PACKET_STALE_OR_MISSING"]});
        let (origin, t) = server(status, result.clone());
        let out = call(
            "close",
            &origin,
            &f,
            false,
            &[
                "--from=2024-01-01",
                "--to=2024-03-31",
                "--confirm=yes",
                "--json",
            ],
        );
        assert_eq!(out.status.code(), Some(1));
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).unwrap(),
            result
        );
        assert_eq!(
            t.join().unwrap()["body"],
            json!({"periodStart":"2024-01-01","periodEnd":"2024-03-31","confirm":true})
        );
    }
    let (origin, t) = server(
        200,
        json!({"ok":true,"status":"unavailable","code":"PERIOD_CLOSE_REVIEW_SCHEMA_UNAVAILABLE","review":null}),
    );
    let out = Command::new(BIN)
        .env("RENTEMESTER_COMPANY", "synthetic")
        .args([
            "--server",
            &origin,
            "--allow-loopback-http",
            "period",
            "--cookie-file",
            f.0.join("cookie").to_str().unwrap(),
            "status",
            "--review-id=7",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["review"],
        Value::Null
    );
    t.join().unwrap();
}
