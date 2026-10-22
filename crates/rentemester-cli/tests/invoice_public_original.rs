use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "rm153-public-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let fixture = Self(path);
        for (name, bytes) in [
            ("cookie", b"session=synthetic".as_slice()),
            ("key", b"synthetic-key".as_slice()),
        ] {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options
                .open(fixture.0.join(name))
                .unwrap()
                .write_all(bytes)
                .unwrap();
        }
        fixture
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn server(status: u16, value: Value) -> (String, thread::JoinHandle<(String, Value)>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = vec![];
        let end = loop {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(i) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let header = String::from_utf8_lossy(&bytes[..end]).to_string();
        let length: usize = header
            .lines()
            .find_map(|l| {
                l.to_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse().unwrap())
            })
            .unwrap_or(0);
        while bytes.len() < end + length {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
        }
        let body = if length == 0 {
            Value::Null
        } else {
            serde_json::from_slice(&bytes[end..end + length]).unwrap()
        };
        let response = value.to_string();
        write!(
            s,
            "HTTP/1.1 {status} Mock\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{response}",
            response.len()
        )
        .unwrap();
        (header, body)
    });
    (origin, t)
}

const ARTIFACT: &str = "<source>æ &amp; exact</source>\n";
const HASH: &str = "82936549ab45c9664092f4552c5c03ac5a779a6e75087904a80ce094d7e01235";
fn value(export: bool) -> Value {
    if export {
        json!({"ok":true,"invoiceNumber":"INV","xml":ARTIFACT,"sha256":HASH,"appliedRules":["export"],"errors":[]})
    } else {
        json!({"ok":true,"invoiceNumber":"INV","envelope":ARTIFACT,"envelopeSha256":HASH,"oioublSha256":HASH,"submissionReference":"source-ref","idempotencyKey":"source-key","status":"acknowledged","transmissionId":"actual-id","duplicate":false,"appliedRules":["submit"],"errors":[]})
    }
}
fn call(kind: &str, origin: &str, f: &Fixture, extra: &[&str]) -> Output {
    Command::new(BIN)
        .args([
            "invoice",
            kind,
            "--server",
            origin,
            "--allow-loopback-http",
            "--company",
            "synthetic",
            "--cookie-file",
            f.0.join("cookie").to_str().unwrap(),
            "--json",
        ])
        .args(extra)
        .output()
        .unwrap()
}
#[test]
fn four_commands_preserve_selector_config_and_flat_output_artifacts() {
    let f = Fixture::new();
    let out = f.0.join("out.xml");
    let config = f.0.join("access.json");
    fs::write(&config,r#"{"accessPointId":" raw AP ","endpointUrl":"https://metadata.invalid/never-fetch","senderEndpointId":" raw sender ","acknowledgement":{"transmissionId":"manual-id","acknowledgedAt":"raw-date"}}"#).unwrap();
    for kind in [
        "export-public",
        "export-public-oioubl",
        "submit-public-peppol",
        "transmit-digisense",
    ] {
        let export = kind.starts_with("export-");
        let expected = value(export);
        let (origin, t) = server(200, expected.clone());
        let mut extra = vec!["--document-id", "7", "--invoice-number", " raw fallback "];
        if export || kind == "submit-public-peppol" {
            extra.extend(["--out", out.to_str().unwrap()]);
        }
        if kind == "submit-public-peppol" {
            extra.extend(["--access-point", config.to_str().unwrap()]);
        }
        if kind == "transmit-digisense" {
            extra.extend([
                "--confirm",
                "yes",
                "--digisense-company-key",
                "assert-current",
            ]);
        }
        let output = call(kind, &origin, &f, &extra);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut actual: Value = serde_json::from_slice(&output.stdout).unwrap();
        if kind != "transmit-digisense" {
            assert_eq!(fs::read(&out).unwrap(), ARTIFACT.as_bytes());
            assert_eq!(
                actual.as_object_mut().unwrap().remove("outPath"),
                Some(json!(out.to_str().unwrap()))
            );
            fs::remove_file(&out).unwrap();
        }
        assert_eq!(actual, expected);
        let (header, body) = t.join().unwrap();
        assert!(header.starts_with(&format!(
            "POST /api/companies/synthetic/invoices/original/{kind} "
        )));
        assert!(header.contains("session=synthetic"));
        let mut expected_body = json!({"documentId":7,"invoiceNumber":" raw fallback "});
        if !export {
            expected_body["confirm"] = json!(true);
        }
        if kind == "submit-public-peppol" {
            expected_body["accessPoint"] = json!({"accessPointId":" raw AP ","endpointUrl":"https://metadata.invalid/never-fetch","senderEndpointId":" raw sender "});
            expected_body["acknowledgement"] =
                json!({"transmissionId":"manual-id","acknowledgedAt":"raw-date"});
        }
        if kind == "transmit-digisense" {
            expected_body["companyKey"] = json!("assert-current");
        }
        assert_eq!(body, expected_body);
    }
}
#[test]
fn invalid_flags_config_private_credentials_and_selector_usage_never_send() {
    let f = Fixture::new();
    let out = f.0.join("out");
    let config = f.0.join("config");
    fs::write(&config,r#"{"accessPointId":"a","endpointUrl":"metadata","senderEndpointId":"s","secret":"reject"}"#).unwrap();
    for (kind, extra) in [
        ("export-public", vec!["--out", out.to_str().unwrap()]),
        ("export-public", vec!["--document-id", "7"]),
        (
            "export-public",
            vec!["--document-id", "0", "--out", out.to_str().unwrap()],
        ),
        (
            "export-public",
            vec!["--invoice-number", " ", "--out", out.to_str().unwrap()],
        ),
        (
            "export-public",
            vec![
                "--document-id",
                "7",
                "--out",
                out.to_str().unwrap(),
                "--out",
                "duplicate",
            ],
        ),
        (
            "export-public",
            vec![
                "--document-id",
                "7",
                "--out",
                out.to_str().unwrap(),
                "--confirm",
                "yes",
            ],
        ),
        (
            "submit-public-peppol",
            vec![
                "--document-id",
                "7",
                "--access-point",
                config.to_str().unwrap(),
            ],
        ),
        ("transmit-digisense", vec!["--invoice-number", "INV"]),
        (
            "transmit-digisense",
            vec!["--document-id", "7", "--confirm"],
        ),
        (
            "transmit-digisense",
            vec!["--document-id", "7", "--confirm", "true"],
        ),
        (
            "transmit-digisense",
            vec![
                "--document-id",
                "7",
                "--confirm",
                "yes",
                "--out",
                out.to_str().unwrap(),
            ],
        ),
    ] {
        let o = call(kind, "http://127.0.0.1:9", &f, &extra);
        assert_eq!(
            o.status.code(),
            Some(2),
            "{kind}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert!(!out.exists());
    }
    let cookie_path = f.0.join("cookie");
    for missing in ["--server", "--company", "--cookie-file"] {
        let mut args = vec![
            "invoice",
            "export-public",
            "--server",
            "http://127.0.0.1:9",
            "--company",
            "synthetic",
            "--cookie-file",
            cookie_path.to_str().unwrap(),
            "--document-id",
            "7",
            "--out",
            out.to_str().unwrap(),
        ];
        let pos = args.iter().position(|a| *a == missing).unwrap();
        args.drain(pos..pos + 2);
        assert_eq!(
            Command::new(BIN).args(args).output().unwrap().status.code(),
            Some(2)
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(f.0.join("cookie"), fs::Permissions::from_mode(0o644)).unwrap();
        let o = call(
            "export-public",
            "http://127.0.0.1:9",
            &f,
            &["--document-id", "7", "--out", out.to_str().unwrap()],
        );
        assert!(!o.status.success());
        assert!(!out.exists());
    }
}
#[test]
fn business_refusal_bad_hash_and_unknown_preserve_existing_artifact_and_exit_status() {
    let f = Fixture::new();
    let out = f.0.join("out");
    fs::write(&out, "existing").unwrap();
    for (kind, status, response) in [
        (
            "export-public",
            200,
            json!({"ok":false,"code":"tampered","errors":["Source refused"]}),
        ),
        ("export-public-oioubl", 200, {
            let mut v = value(true);
            v["sha256"] = json!("bad");
            v
        }),
        ("export-public-oioubl", 503, json!({})),
    ] {
        let (origin, t) = server(status, response);
        let o = call(
            kind,
            &origin,
            &f,
            &["--invoice-number", "INV", "--out", out.to_str().unwrap()],
        );
        assert_eq!(o.status.code(), Some(1));
        let v: Value = serde_json::from_slice(&o.stdout).unwrap();
        assert_eq!(v["ok"], false);
        if kind == "export-public-oioubl" {
            assert_eq!(v["code"], "outcome_unknown");
        }
        assert_eq!(fs::read_to_string(&out).unwrap(), "existing");
        t.join().unwrap();
    }
    let (origin, t) = server(503, json!({}));
    let o = call(
        "transmit-digisense",
        &origin,
        &f,
        &["--document-id", "7", "--confirm", "yes"],
    );
    assert_eq!(o.status.code(), Some(1));
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["code"], "outcome_unknown");
    t.join().unwrap();
}

#[test]
fn prepare_without_ack_or_out_preserves_metadata_and_number_only_selector() {
    let f = Fixture::new();
    let config = f.0.join("config.json");
    fs::write(
        &config,
        r#"{"accessPointId":"a","endpointUrl":"not-a-network-url","senderEndpointId":"s"}"#,
    )
    .unwrap();
    let mut expected = value(false);
    expected["status"] = json!("prepared");
    expected.as_object_mut().unwrap().remove("transmissionId");
    let (origin, t) = server(200, expected.clone());
    let o = call(
        "submit-public-peppol",
        &origin,
        &f,
        &[
            "--invoice-number",
            "ONLY-NUMBER",
            "--access-point",
            config.to_str().unwrap(),
        ],
    );
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&o.stdout).unwrap(),
        expected
    );
    assert_eq!(
        t.join().unwrap().1,
        json!({"invoiceNumber":"ONLY-NUMBER","confirm":true,"accessPoint":{"accessPointId":"a","endpointUrl":"not-a-network-url","senderEndpointId":"s"}})
    );
}
