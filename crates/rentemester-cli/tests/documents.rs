use serde_json::Value;
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[cfg(unix)]
#[test]
fn document_original_cli_returns_exact_authorized_binary() {
    use std::os::unix::fs::OpenOptionsExt;
    let path = std::env::temp_dir().join(format!("rm-document-cli-{}.cookie", std::process::id()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .unwrap()
        .write_all(b"session=private-document-secret")
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let pdf = b"\x89PNG\r\n\x1a\n\xff\0binary";
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = [0; 4096];
        let n = stream.read(&mut bytes).unwrap();
        assert!(bytes[..n].starts_with(b"GET /api/companies/synthetic/documents/7/file HTTP/1.1"));
        assert!(
            String::from_utf8_lossy(&bytes[..n])
                .to_lowercase()
                .contains("cookie: session=private-document-secret")
        );
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection:close\r\n\r\n",pdf.len()).unwrap();
        stream.write_all(pdf).unwrap();
    });
    let result = Command::new(BIN)
        .args([
            "remote",
            "document-original",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            path.to_str().unwrap(),
            "--company",
            "synthetic",
            "--id",
            "7",
        ])
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    let text = String::from_utf8(result.stdout).unwrap();
    assert!(!text.contains("private-document-secret"));
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["data"]["original"]["contentType"], "image/png");
    assert_eq!(
        serde_json::from_value::<Vec<u8>>(value["data"]["original"]["bytes"].clone()).unwrap(),
        pdf
    );
    task.join().unwrap();
}
#[test]
fn document_cli_discovery_and_positive_id_are_strict() {
    let help = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let value: Value = serde_json::from_slice(&help.stdout).unwrap();
    assert!(
        value["commands"]
            .as_array()
            .unwrap()
            .contains(&Value::String("document-original".into()))
    );
    assert!(
        value["documentEvidence"]["usage"]
            .as_str()
            .unwrap()
            .contains("document-original")
    );
    for extra in [
        vec![],
        vec!["--id", "0"],
        vec!["--id", "-1"],
        vec!["--id", "no"],
        vec!["--id", "7", "--confirm"],
        vec!["--id", "7", "--year", "2026"],
    ] {
        let output = Command::new(BIN)
            .args([
                "remote",
                "document-original",
                "--server",
                "http://127.0.0.1:9",
                "--allow-loopback-http",
                "--company",
                "synthetic",
            ])
            .args(extra)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
    }
}

#[cfg(unix)]
#[test]
fn document_ingestion_cli_forwards_exact_confirmed_stdin_once() {
    use std::{os::unix::fs::OpenOptionsExt, process::Stdio};
    let path = std::env::temp_dir().join(format!(
        "rm-document-ingest-cli-{}.cookie",
        std::process::id()
    ));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .unwrap()
        .write_all(b"session=synthetic")
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let body=b"{ \"confirm\":true,\"fileName\":\"synthetic.txt\",\"fileBase64\":\"YQ==\",\"metadata\":{\"source\":\"synthetic\",\"documentType\":\"cash_register_receipt\"} }\n";
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        let mut request = vec![];
        loop {
            let mut chunk = [0; 4096];
            let n = stream.read(&mut chunk).unwrap();
            assert!(n > 0);
            request.extend_from_slice(&chunk[..n]);
            if request.ends_with(body) {
                break;
            }
        }
        assert!(request.starts_with(b"POST /api/companies/synthetic/documents/ingest HTTP/1.1"));
        let end = request.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        assert_eq!(&request[end..], body);
        let response = br#"{"ok":true,"document":{"id":7,"documentNo":"SYN-1"}}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection:close\r\n\r\n",
            response.len()
        )
        .unwrap();
        stream.write_all(response).unwrap();
    });
    let mut child = Command::new(BIN)
        .args([
            "remote",
            "ingest-document",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            path.to_str().unwrap(),
            "--company",
            "synthetic",
            "--confirm",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(body).unwrap();
    let output = child.wait_with_output().unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["document"]["id"], 7);
    task.join().unwrap();
}
