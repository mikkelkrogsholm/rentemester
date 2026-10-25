#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use common::TcpListener;
use serde_json::Value;
use std::{
    io::{Read, Write},
    process::Command,
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn dashboard_publishes_exact_bytes_replaces_atomically_and_cleans_failed_promotion() {
    for fail in [false, true] {
        let dir = std::env::temp_dir().join(format!("rm-dashboard-{}-{fail}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cookie = dir.join("cookie");
        let mut o = std::fs::OpenOptions::new();
        o.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            o.mode(0o600);
        }
        o.open(&cookie)
            .unwrap()
            .write_all(b"session=synthetic")
            .unwrap();
        let out = dir.join("nested").join("dashboard.html");
        std::fs::create_dir_all(out.parent().unwrap()).unwrap();
        if fail {
            std::fs::create_dir(&out).unwrap();
        } else {
            std::fs::write(&out, "old").unwrap();
        }
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
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length:9\r\nContent-Type:text/html; charset=utf-8\r\nContent-Disposition:attachment; filename=\"dashboard-synthetic-2024-02-29.html\"\r\nx-rentemester-as-of:2024-02-29\r\nx-rentemester-generated-at:2024-02-29T12:00:00Z\r\nx-rentemester-commit-sha:unknown\r\nx-rentemester-rule-bundle-version:synthetic1\r\nx-rentemester-render-ms:0.25\r\nConnection:close\r\n\r\n<p>ok</p>").unwrap();
            String::from_utf8(bytes).unwrap()
        });
        let mut command = Command::new(BIN);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let opener = dir.join(if cfg!(target_os = "macos") {
                "open"
            } else {
                "xdg-open"
            });
            std::fs::write(&opener,"#!/bin/sh\nIFS= read -r content < \"$1\"\n[ \"$content\" = '<p>ok</p>' ] || exit 2\nprintf attempted > \"$1.opened\"\nexit 1\n").unwrap();
            std::fs::set_permissions(&opener, std::fs::Permissions::from_mode(0o700)).unwrap();
            command.env("PATH", &dir);
        }
        command.args([
            "dashboard",
            "--server",
            &origin,
            "--cookie-file",
            cookie.to_str().unwrap(),
            "--allow-loopback-http",
            "--company",
            "synthetic",
            "--as-of",
            "2024-02-29",
            "--out",
            out.to_str().unwrap(),
        ]);
        #[cfg(unix)]
        command.arg("--open");
        let result = command.output().unwrap();
        assert_eq!(
            result.status.success(),
            !fail,
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
        assert!(task.join().unwrap().starts_with(
            "GET /api/companies/synthetic/dashboard/export?format=html&asOf=2024-02-29 HTTP/1.1"
        ));
        if !fail {
            assert_eq!(std::fs::read(&out).unwrap(), b"<p>ok</p>");
            let v: Value = serde_json::from_slice(&result.stdout).unwrap();
            assert_eq!(v["opened"], cfg!(unix));
            assert_eq!(v["renderMs"], 0.25);
            assert_eq!(v["commitSha"], "unknown");
        }
        #[cfg(unix)]
        assert_eq!(out.with_extension("html.opened").exists(), !fail);
        assert_eq!(
            std::fs::read_dir(out.parent().unwrap()).unwrap().count(),
            if cfg!(unix) && !fail { 2 } else { 1 }
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
#[test]
fn missing_output_and_irrelevant_or_invalid_inputs_fail_before_network() {
    for extra in [
        vec![],
        vec!["--as-of", "2024-02-30", "--out", "x"],
        vec!["--out", "x", "--confirm"],
        vec!["--out", "x", "--source", "live"],
        vec!["--out", "x", "--service-key-file", "x"],
    ] {
        let out = Command::new(BIN)
            .args([
                "remote",
                "dashboard",
                "--server",
                "http://127.0.0.1:9",
                "--cookie-file",
                "missing",
                "--company",
                "synthetic",
            ])
            .args(extra)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
    }
    let out = Command::new(BIN)
        .args(["remote", "health", "--open"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}
