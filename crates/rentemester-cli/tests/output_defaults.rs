//! Source stdout defaults exercised against actual pipes and a real PTY.
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    process::{Command, Output},
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");

fn server(value: Value) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        listener.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(std::time::Instant::now() < deadline, "CLI did not connect");
                    thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(e) => panic!("{e}"),
            }
        };
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let mut bytes = [0; 4096];
            let n = socket.read(&mut bytes).unwrap();
            assert!(n > 0);
            request.extend_from_slice(&bytes[..n]);
        }
        let body = value.to_string();
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        String::from_utf8(request).unwrap()
    });
    (origin, task)
}

// Only stdout is attached to the slave: stderr remains independently observable.
// Python's standard-library PTY avoids a product/test dependency or shell wrapper.
#[cfg(unix)]
fn invoke(args: &[String], tty: bool) -> Output {
    if !tty {
        return Command::new(BIN).args(args).output().unwrap();
    }
    Command::new("python3").args(["-c", r#"
import errno, os, pty, subprocess, sys
master, slave = pty.openpty()
child = subprocess.Popen(sys.argv[1:], stdin=subprocess.DEVNULL, stdout=slave, stderr=subprocess.PIPE)
os.close(slave)
while True:
    try:
        chunk = os.read(master, 65536)
        if not chunk: break
        sys.stdout.buffer.write(chunk)
    except OSError as error:
        if error.errno != errno.EIO: raise
        break
os.close(master)
sys.stderr.buffer.write(child.communicate(timeout=10)[1])
sys.exit(child.returncode)
"#, BIN]).args(args).output().unwrap()
}

#[cfg(unix)]
#[test]
fn migrated_journal_and_bank_use_stdout_default_and_source_precedence() {
    use std::os::unix::fs::OpenOptionsExt;
    let cookie = std::env::temp_dir().join(format!("rm181-output-cookie-{}", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&cookie)
        .unwrap();
    file.write_all(b"session=synthetic").unwrap();
    for (family, response, expected, heading, path) in [
        (
            "journal",
            json!({"ok":true,"journalHistory":{"entries":[],"total":0,"count":0,"limit":0,"offset":0,"hasMore":false}}),
            json!([]),
            "Finansposteringer (0)",
            "/journal",
        ),
        (
            "bank",
            json!({"ok":true,"data":{"rows":[],"count":0},"errors":[]}),
            json!({"rows":[],"count":0,"ok":true,"errors":[]}),
            "Banktransaktioner",
            "/bank",
        ),
    ] {
        for tty in [false, true] {
            for (flags, human) in [
                (vec![], tty),
                (vec!["--format=human"], true),
                (vec!["--format=json"], false),
                (vec!["--format=json", "--format", "human"], true),
                (vec!["--format=human", "--format=json"], false),
                (vec!["--json", "--format=human"], false),
                (vec!["--format=human", "--json"], false),
            ] {
                let (origin, task) = server(response.clone());
                // Metadata before/between command words must not affect resolution.
                let mut args = vec![
                    "--company=synthetic".into(),
                    family.into(),
                    "--server".into(),
                    origin,
                    "list".into(),
                    "--allow-loopback-http".into(),
                    "--cookie-file".into(),
                    cookie.to_str().unwrap().into(),
                ];
                args.extend(flags.iter().map(|s| (*s).to_owned()));
                let out = invoke(&args, tty);
                assert!(
                    out.status.success(),
                    "{family} tty={tty} flags={flags:?}: {out:?}"
                );
                assert!(out.stderr.is_empty(), "{out:?}");
                let request = task.join().unwrap();
                assert!(request.lines().next().unwrap().contains(path), "{request}");
                if human {
                    let text = String::from_utf8(out.stdout).unwrap();
                    assert!(text.contains(heading), "{text}");
                    assert!(serde_json::from_str::<Value>(&text).is_err());
                } else {
                    assert_eq!(
                        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
                        expected
                    );
                }
            }
        }
    }
    fs::remove_file(cookie).unwrap();
}
