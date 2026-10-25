use common::TcpListener;
#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    process::Command,
    thread,
    time::Duration,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn mileage_posting_discovery_local_validation_and_service_refusal() {
    let out = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    for name in [
        "mileage-rate-versions",
        "configure-mileage-rates",
        "preview-mileage-post",
        "post-mileage",
        "mileage-postings",
    ] {
        assert!(v["commands"].as_array().unwrap().contains(&json!(name)));
    }
    for args in [
        vec!["post-mileage"],
        vec!["configure-mileage-rates"],
        vec!["preview-mileage-post", "--confirm"],
        vec!["mileage-postings", "--confirm"],
        vec!["mileage-rate-versions", "--year", "2026"],
        vec![
            "post-mileage",
            "--confirm",
            "--service-key-file",
            "/does-not-exist",
        ],
        vec![
            "mileage-postings",
            "--cookie-file",
            "/missing",
            "--service-key-file",
            "/missing",
        ],
    ] {
        let out = Command::new(BIN).args(args).output().unwrap();
        assert_eq!(
            out.status.code(),
            Some(2),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    for args in [
        vec!["mileage", "--help"],
        vec!["mileage", "log", "--help"],
        vec!["mileage", "list", "--help"],
        vec!["mileage", "report", "--help"],
        vec!["mileage", "export", "--help"],
    ] {
        assert!(
            Command::new(BIN)
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}
#[test]
fn mileage_posting_cli_exact_body_file_path_and_no_retry() {
    let root = std::env::temp_dir().join(format!("mileage-posting-cli-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let cookie = root.join("cookie");
    let mut opts = std::fs::OpenOptions::new();
    opts.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(&cookie)
        .unwrap()
        .write_all(b"session=synthetic")
        .unwrap();
    for command in [
        "mileage-rate-versions",
        "configure-mileage-rates",
        "preview-mileage-post",
        "post-mileage",
        "mileage-postings",
    ] {
        let body=match command {
            "configure-mileage-rates"=>r#"{ "version":"synthetic", "validFrom":"2026-01-01","validTo":"2026-12-31","vehicle":"car","ratePerKm":"2.00","basis":"synthetic","source":"explicit evidence","expenseAccountNo":"E","liabilityAccountNo":"L","confirm":true }"#.to_owned(),
            "post-mileage"=>format!(r#"{{ "mileageEntryId":1,"rateVersion":"synthetic","documentId":2,"expectedPreviewHash":"{}","operationKey":"original-key","confirm":true }}"#,"a".repeat(64)),
            _=>r#"{ "mileageEntryId":1, "rateVersion":"synthetic", "documentId":2 }"#.to_owned(),
        };
        let file = root.join("body.json");
        std::fs::write(&file, &body).unwrap();
        let read = matches!(command, "mileage-rate-versions" | "mileage-postings");
        let write = matches!(command, "configure-mileage-rates" | "post-mileage");
        for unknown in [false, true] {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            let origin = format!("http://{}", l.local_addr().unwrap());
            let task = thread::spawn(move || {
                let (mut s, _) = l.accept().unwrap();
                s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut b = vec![];
                loop {
                    let mut c = [0; 4096];
                    let n = s.read(&mut c).unwrap();
                    assert!(n > 0);
                    b.extend_from_slice(&c[..n]);
                    if let Some(end) = b.windows(4).position(|w| w == b"\r\n\r\n") {
                        let h = String::from_utf8_lossy(&b[..end]).to_lowercase();
                        let len = h
                            .lines()
                            .find_map(|s| s.strip_prefix("content-length: "))
                            .map(|s| s.parse::<usize>().unwrap())
                            .unwrap_or(0);
                        if b.len() >= end + 4 + len {
                            break;
                        }
                    }
                }
                let reply = if unknown {
                    "{}"
                } else {
                    r#"{"ok":true,"mileagePosting":{"exact":"server"}}"#
                };
                let status = if unknown {
                    "503 Service Unavailable"
                } else {
                    "200 OK"
                };
                write!(s,"HTTP/1.1 {status}\r\nContent-Type:application/json\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",reply.len()).unwrap();
                drop(s);
                let l = l.into_inner();
                l.set_nonblocking(true).unwrap();
                thread::sleep(Duration::from_millis(60));
                assert!(l.accept().is_err(), "must never retry");
                b
            });
            let mut cmd = Command::new(BIN);
            cmd.args([
                command,
                "--server",
                &origin,
                "--allow-loopback-http",
                "--cookie-file",
                cookie.to_str().unwrap(),
                "--company",
                "synthetic",
            ]);
            if !read {
                cmd.args(["--body-file", file.to_str().unwrap()]);
            }
            if write {
                cmd.arg("--confirm");
            }
            let out = cmd.output().unwrap();
            assert_eq!(
                out.status.code(),
                Some(if unknown { 1 } else { 0 }),
                "{} {}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let request = task.join().unwrap();
            let suffix = match command {
                "mileage-rate-versions" | "configure-mileage-rates" => "rate-versions",
                "preview-mileage-post" => "post-preview",
                "post-mileage" => "post",
                _ => "postings",
            };
            assert!(
                request.starts_with(
                    format!(
                        "{} /api/companies/synthetic/mileage/{suffix} HTTP/1.1",
                        if read { "GET" } else { "POST" }
                    )
                    .as_bytes()
                )
            );
            if !read {
                assert!(request.ends_with(body.as_bytes()));
            }
            if !unknown {
                let v: Value = serde_json::from_slice(&out.stdout).unwrap();
                assert_eq!(v["data"]["mileagePosting"]["exact"], "server");
            }
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
