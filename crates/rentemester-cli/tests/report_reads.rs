#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use common::TcpListener;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    process::Command,
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn report_source_and_export_dispatch_reach_exact_http_without_silent_selector_loss() {
    for command in ["journal", "overview", "report-export"] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let task = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = vec![];
            while !bytes.ends_with(b"\r\n\r\n") {
                let mut b = [0; 4096];
                let n = s.read(&mut b).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&b[..n]);
            }
            let (reply, mime) = if command == "report-export" {
                ("exact-csv\r\n", "text/csv; charset=utf-8")
            } else {
                (
                    r#"{"ok":true,"amount":"9007199254740993.01","prior":null,"source":"archive"}"#,
                    "application/json",
                )
            };
            write!(s,"HTTP/1.1 200 OK\r\nContent-Length:{}\r\nContent-Type:{mime}\r\nConnection:close\r\n\r\n{reply}",reply.len()).unwrap();
            String::from_utf8(bytes).unwrap()
        });
        let path = std::env::temp_dir().join(format!(
            "rm-report-cli-{}-{command}.cookie",
            std::process::id()
        ));
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        opts.open(&path)
            .unwrap()
            .write_all(b"session=synthetic")
            .unwrap();
        let mut cmd = Command::new(BIN);
        cmd.args([
            "remote",
            command,
            "--server",
            &origin,
            "--cookie-file",
            path.to_str().unwrap(),
            "--allow-loopback-http",
            "--company",
            "synthetic",
            "--year",
            "2024",
            "--source",
            "archive",
        ]);
        if command == "report-export" {
            cmd.args(["--report", "journal", "--format", "csv"]);
        }
        let out = cmd.output().unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        let wire = task.join().unwrap();
        if command == "report-export" {
            assert_eq!(v["data"]["bytes"], json!(b"exact-csv\r\n"));
            assert!(wire.starts_with("GET /api/companies/synthetic/journal/export?year=2024&source=archive&format=csv HTTP/1.1"));
        } else {
            assert_eq!(v["data"]["amount"], "9007199254740993.01");
            assert!(wire.starts_with(&format!(
                "GET /api/companies/synthetic/{command}?year=2024&source=archive HTTP/1.1"
            )));
        }
    }
}
#[test]
fn irrelevant_report_selectors_and_write_confirmation_are_refused_before_io() {
    for args in [
        vec!["health", "--source", "archive"],
        vec!["overview", "--account", "1000"],
        vec!["journal", "--confirm"],
        vec!["journal", "--format", "pdf"],
        vec!["login", "--source", "live"],
    ] {
        let out = Command::new(BIN).arg("remote").args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
    }
}
