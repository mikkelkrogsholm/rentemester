#![cfg(unix)]
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Fixture {
    root: PathBuf,
    child: Option<Child>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn sigterm_finishes_an_in_flight_response_and_exits_successfully() {
    let token = rentemester_runtime::auth::new_token().unwrap();
    let root = std::env::temp_dir().join(format!("rm-shutdown-{}", token.as_str()));
    std::fs::create_dir(&root).unwrap();
    let mut fixture = Fixture { root, child: None };
    std::fs::write(fixture.root.join("index.html"), "synthetic").unwrap();
    // Larger than the socket buffers: the response remains in flight until
    // this client resumes reading after SIGTERM.
    let content = vec![b'x'; 16 * 1024 * 1024];
    std::fs::write(fixture.root.join("original.bin"), &content).unwrap();
    let address = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    fixture.child = Some(
        Command::new(env!("CARGO_BIN_EXE_rentemester-server"))
            .args([
                "--listen",
                &address.to_string(),
                "--web-dir",
                fixture.root.to_str().unwrap(),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut stream = loop {
        if let Ok(stream) = TcpStream::connect(address) {
            break stream;
        }
        assert!(
            fixture
                .child
                .as_mut()
                .unwrap()
                .try_wait()
                .unwrap()
                .is_none()
        );
        assert!(Instant::now() < deadline, "server startup timed out");
        std::thread::sleep(Duration::from_millis(25));
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    write!(
        stream,
        "GET /original.bin HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut headers = Vec::new();
    while !headers.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte).unwrap();
        headers.push(byte[0]);
        assert!(headers.len() < 8192);
    }
    assert!(headers.starts_with(b"HTTP/1.1 200 OK\r\n"));
    let pid = fixture.child.as_ref().unwrap().id().to_string();
    assert!(
        Command::new("/bin/kill")
            .args(["-TERM", &pid])
            .status()
            .unwrap()
            .success()
    );
    std::thread::sleep(Duration::from_millis(100));
    assert!(
        fixture
            .child
            .as_mut()
            .unwrap()
            .try_wait()
            .unwrap()
            .is_none(),
        "server exited before its response drained"
    );
    let mut received = Vec::new();
    stream.read_to_end(&mut received).unwrap();
    assert_eq!(received, content);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = fixture.child.as_mut().unwrap().try_wait().unwrap() {
            assert!(status.success(), "shutdown failed: {status}");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "server failed to exit after draining"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}
