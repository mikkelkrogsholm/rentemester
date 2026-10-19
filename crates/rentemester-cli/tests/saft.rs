use std::{
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Stdio},
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn discoverable_and_requires_explicit_confirmation() {
    let out = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("saft-export"));
    let out = Command::new(BIN)
        .args([
            "remote",
            "--server",
            "http://127.0.0.1:9",
            "--allow-loopback-http",
            "saft-export",
            "--company",
            "synthetic",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}
#[test]
fn real_subprocess_downloads_exact_binary_envelope_without_writing_files() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let body = r#"{ "periodStart":"2026-01-01","periodEnd":"2026-12-31","confirm":true }"#;
    let task = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut b = vec![];
        loop {
            let mut part = [0; 4096];
            let n = s.read(&mut part).unwrap();
            assert!(n > 0);
            b.extend_from_slice(&part[..n]);
            if b.ends_with(body.as_bytes()) {
                break;
            }
        }
        assert!(b.starts_with(b"POST /api/companies/synthetic/saft-export HTTP/1.1"));
        s.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-tar\r\nContent-Length:1024\r\nConnection:close\r\n\r\n").unwrap();
        s.write_all(&vec![42; 1024]).unwrap();
    });
    let path = std::env::temp_dir().join(format!(
        "rm-saft-cli-{}-{}.cookie",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut opts = std::fs::OpenOptions::new();
    opts.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(&path)
        .unwrap()
        .write_all(b"session=private-synthetic")
        .unwrap();
    let mut child = Command::new(BIN)
        .args([
            "remote",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            path.to_str().unwrap(),
            "saft-export",
            "--company",
            "synthetic",
            "--confirm",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(body.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["data"]["export"]["contentType"], "application/x-tar");
    assert_eq!(
        value["data"]["export"]["bytes"],
        serde_json::json!(vec![42; 1024])
    );
    assert!(!String::from_utf8_lossy(&out.stdout).contains("private-synthetic"));
    task.join().unwrap();
}
