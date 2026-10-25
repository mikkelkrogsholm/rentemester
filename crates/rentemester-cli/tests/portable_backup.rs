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
fn workspace_backup_help_confirmation_and_selector_refusals() {
    let out = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let help: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        help["commands"]
            .as_array()
            .unwrap()
            .contains(&json!("workspace-backup"))
    );
    assert!(
        help["workspaceBackup"]
            .as_str()
            .unwrap()
            .contains("no stdin body")
    );
    for extra in [
        vec![],
        vec!["--confirm", "yes"],
        vec!["--confirm", "--company", "synthetic"],
        vec!["--confirm", "--year", "2026"],
        vec!["--confirm", "--id", "1"],
        vec!["--confirm", "--preview-reviewed"],
        vec!["--confirm", "--body", "{}"],
        vec!["--confirm", "--path", "/tmp/forbidden"],
        vec!["--confirm", "--key", "invented"],
    ] {
        let out = Command::new(BIN)
            .args([
                "remote",
                "--server",
                "http://127.0.0.1:9",
                "--allow-loopback-http",
                "workspace-backup",
            ])
            .args(extra)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2));
    }
}
#[test]
fn workspace_backup_subprocess_exact_post_and_raw_json_bytes() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        let mut request = vec![];
        loop {
            let mut b = [0; 1024];
            let n = socket.read(&mut b).unwrap();
            assert!(n > 0);
            request.extend_from_slice(&b[..n]);
            if request.ends_with(br#"{"confirm":true}"#) {
                break;
            }
        }
        assert!(request.starts_with(b"POST /api/workspace/backup HTTP/1.1\r\n"));
        let split = request.windows(4).position(|v| v == b"\r\n\r\n").unwrap() + 4;
        assert_eq!(&request[split..], br#"{"confirm":true}"#);
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/vnd.rentemester.native-backup+json\r\nContent-Length: 4\r\nConnection: close\r\n\r\n\0\xff{}" ).unwrap();
    });
    let path = std::env::temp_dir().join(format!(
        "rm-portable-cli-{}-{}.cookie",
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
    let out = Command::new(BIN)
        .args([
            "remote",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            path.to_str().unwrap(),
            "workspace-backup",
            "--confirm",
        ])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    task.join().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        value["data"]["backup"],
        json!({"contentType":"application/vnd.rentemester.native-backup+json","bytes":[0,255,123,125]})
    );
    assert!(!String::from_utf8_lossy(&out.stdout).contains("private-synthetic"));
}
