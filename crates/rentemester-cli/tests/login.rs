use std::{
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Stdio},
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn login_subprocess_creates_private_current_session_without_exposing_secrets() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        for (path, cookie, body) in [
            (
                "/api/auth/sign-in/email",
                "better-auth.two_factor=pending-secret; Path=/; HttpOnly; SameSite=Lax; Max-Age=600",
                r#"{"twoFactorRedirect":true}"#,
            ),
            (
                "/api/auth/two-factor/verify-backup-code",
                "better-auth.session_token=current-secret; Path=/; HttpOnly; SameSite=Lax",
                r#"{"status":true}"#,
            ),
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            loop {
                let mut chunk = [0; 4096];
                let n = stream.read(&mut chunk).unwrap();
                if n == 0 {
                    break;
                }
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(index) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&bytes[..index]).to_lowercase();
                    let length = headers
                        .lines()
                        .find_map(|v| v.strip_prefix("content-length: "))
                        .map(|v| v.parse::<usize>().unwrap())
                        .unwrap_or(0);
                    if bytes.len() >= index + 4 + length {
                        break;
                    }
                }
            }
            let request = String::from_utf8(bytes).unwrap();
            assert!(request.starts_with(&format!("POST {path} ")));
            if path.ends_with("email") {
                assert!(request.contains("password-secret"));
                assert!(!request.contains("cookie:"));
            } else {
                assert!(request.contains("backup-secret"));
                assert!(request.contains("cookie: better-auth.two_factor=pending-secret"));
            }
            write!(stream,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nSet-Cookie: {cookie}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        }
    });
    let path = std::env::temp_dir().join(format!("rm-cli-login-{}.cookie", std::process::id()));
    let mut child = Command::new(BIN)
        .args([
            "remote",
            "--server",
            &origin,
            "--allow-loopback-http",
            "login",
            "--email",
            "user@example.test",
            "--factor",
            "backup",
            "--session-out",
            path.to_str().unwrap(),
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
        .write_all(b"password-secret\nbackup-secret\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    for bytes in [&output.stdout, &output.stderr] {
        let text = String::from_utf8_lossy(bytes);
        for secret in [
            "password-secret",
            "backup-secret",
            "pending-secret",
            "current-secret",
        ] {
            assert!(!text.contains(secret));
        }
    }
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "better-auth.session_token=current-secret"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    }
    // Existing session output must refuse before reading stdin or sending a password.
    let second = Command::new(BIN)
        .args([
            "remote",
            "--server",
            "http://127.0.0.1:9",
            "--allow-loopback-http",
            "login",
            "--email",
            "user@example.test",
            "--factor",
            "backup",
            "--session-out",
            path.to_str().unwrap(),
        ])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(second.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&second.stdout).contains("already exists"));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "better-auth.session_token=current-secret"
    );
    std::fs::remove_file(path).unwrap();
    task.join().unwrap();
}
