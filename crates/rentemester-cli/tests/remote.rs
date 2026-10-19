use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn remote_requires_explicit_server_and_never_falls_back_to_local() {
    let output = Command::new(BIN)
        .args(["remote", "health"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("explicit --server")
    );
}
#[test]
fn remote_health_uses_real_http_server() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = [0; 4096];
        let read = stream.read(&mut bytes).unwrap();
        assert!(bytes[..read].starts_with(b"GET /api/health HTTP/1.1"));
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{\"status\":\"ok\"}").unwrap();
    });
    let output = Command::new(BIN)
        .args([
            "remote",
            "health",
            "--server",
            &origin,
            "--allow-loopback-http",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["status"], "ok");
    task.join().unwrap();
}
#[cfg(unix)]
#[test]
fn cookie_file_permissions_refused_without_secret_disclosure() {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::temp_dir().join(format!("rentemester-cli-cookie-{}", std::process::id()));
    std::fs::write(&path, "session=do-not-display").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let output = Command::new(BIN)
        .args([
            "remote",
            "me",
            "--server",
            "http://127.0.0.1:9",
            "--allow-loopback-http",
            "--cookie-file",
            path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(output.status.code(), Some(2));
    let result = String::from_utf8(output.stdout).unwrap();
    assert!(result.contains("chmod 600"));
    assert!(!result.contains("do-not-display"));
}
