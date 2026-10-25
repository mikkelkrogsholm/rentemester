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
#[cfg(unix)]
#[test]
fn issued_pdf_cli_returns_exact_original_bytes_without_confirmation_or_files() {
    use std::os::unix::fs::OpenOptionsExt;
    let path =
        std::env::temp_dir().join(format!("rm-invoice-pdf-cli-{}.cookie", std::process::id()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .unwrap()
        .write_all(b"session=private-pdf-secret")
        .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let pdf = b"%PDF-1.7\n\xff\0binary\n%%EOF\n";
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = [0; 4096];
        let n = stream.read(&mut bytes).unwrap();
        assert!(bytes[..n].starts_with(b"GET /api/companies/synthetic/invoices/7/pdf HTTP/1.1"));
        assert!(
            String::from_utf8_lossy(&bytes[..n])
                .to_lowercase()
                .contains("cookie: session=private-pdf-secret")
        );
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/pdf\r\nContent-Length: {}\r\nConnection:close\r\n\r\n",pdf.len()).unwrap();
        stream.write_all(pdf).unwrap();
    });
    let result = Command::new(BIN)
        .args([
            "remote",
            "invoice-pdf",
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
    assert!(!text.contains("private-pdf-secret"));
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["data"]["pdf"]["contentType"], "application/pdf");
    assert_eq!(
        serde_json::from_value::<Vec<u8>>(value["data"]["pdf"]["bytes"].clone()).unwrap(),
        pdf
    );
    task.join().unwrap();
}
#[test]
fn issued_pdf_cli_discovery_and_positive_id_are_strict() {
    let help = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let value: Value = serde_json::from_slice(&help.stdout).unwrap();
    assert!(
        value["commands"]
            .as_array()
            .unwrap()
            .contains(&Value::String("invoice-pdf".into()))
    );
    assert!(
        value["invoicePdf"]
            .as_str()
            .unwrap()
            .contains("No stdin body or --confirm")
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
                "invoice-pdf",
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
