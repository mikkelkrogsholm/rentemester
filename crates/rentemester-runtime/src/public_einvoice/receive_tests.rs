use super::*;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
};
struct Fixture {
    dir: tempfile::TempDir,
    child: Child,
    port: u16,
    ca: String,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn fixture(san: &str) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let cert = dir.path().join("ca.pem");
    let key = dir.path().join("key.pem");
    let ca = dir.path().join("root.pem");
    let ca_key = dir.path().join("root.key");
    let csr = dir.path().join("leaf.csr");
    let ext = dir.path().join("leaf.ext");
    std::fs::write(&ext,format!("basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName={san}\n")).unwrap();
    let cmds: Vec<Vec<String>> = vec![
        vec![
            "req".into(),
            "-x509".into(),
            "-newkey".into(),
            "rsa:2048".into(),
            "-nodes".into(),
            "-days".into(),
            "1".into(),
            "-subj".into(),
            "/CN=Synthetic DigiSense test CA".into(),
            "-addext".into(),
            "basicConstraints=critical,CA:TRUE".into(),
            "-keyout".into(),
            ca_key.to_str().unwrap().into(),
            "-out".into(),
            ca.to_str().unwrap().into(),
        ],
        vec![
            "req".into(),
            "-new".into(),
            "-newkey".into(),
            "rsa:2048".into(),
            "-nodes".into(),
            "-subj".into(),
            "/CN=localhost".into(),
            "-keyout".into(),
            key.to_str().unwrap().into(),
            "-out".into(),
            csr.to_str().unwrap().into(),
        ],
        vec![
            "x509".into(),
            "-req".into(),
            "-in".into(),
            csr.to_str().unwrap().into(),
            "-CA".into(),
            ca.to_str().unwrap().into(),
            "-CAkey".into(),
            ca_key.to_str().unwrap().into(),
            "-CAcreateserial".into(),
            "-days".into(),
            "1".into(),
            "-extfile".into(),
            ext.to_str().unwrap().into(),
            "-out".into(),
            cert.to_str().unwrap().into(),
        ],
    ];
    for args in cmds {
        let out = Command::new("openssl").args(args).output().unwrap();
        assert!(out.status.success());
    }
    let mut child = Command::new("python3")
        .args(["-u", "-c", include_str!("receive_test_support.py")])
        .arg(cert)
        .arg(key)
        .arg(dir.path().join("trace.jsonl"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    Fixture {
        port: line.trim().parse().unwrap(),
        ca: std::fs::read_to_string(ca).unwrap(),
        dir,
        child,
    }
}
fn config(value: Value) -> Config {
    serde_json::from_value(value).unwrap()
}
fn client(f: &Fixture, mode: &str) -> Client {
    Client::new(config(json!({"apiLicenseKey":"synthetic-private-api-key-74","environment":"test","baseUrl":format!("https://127.0.0.1:{}/{mode}/ap/api/rest",f.port),"trustedCaPem":f.ca,"timeoutMs":250}))).unwrap()
}
fn trace(f: &Fixture) -> Vec<Value> {
    std::fs::read_to_string(f.dir.path().join("trace.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|v| serde_json::from_str(v).unwrap())
        .collect()
}
#[test]
fn exact_scope_query_safe_metadata_and_signed_bytes_without_authorization() {
    let f = fixture("IP:127.0.0.1");
    let c = client(&f, "second");
    let page = c
        .list_received_documents("company_key", 1, 0, Some("2026-01-01T12:30:00Z"))
        .unwrap();
    assert!(page.has_next);
    assert_eq!(page.documents.len(), 1);
    let row = &page.documents[0];
    assert_eq!(row.internal_id(), "stable-1");
    let metadata = row.metadata();
    assert_eq!(metadata["senderName"], "Synthetic sender");
    assert_eq!(metadata.as_object().unwrap().len(), FIELDS.len());
    let encoded = metadata.to_string();
    for private in [
        "downloadUrl\"",
        "licenseKey",
        "participants",
        "searchParams",
        "synthetic-url-secret",
        "synthetic-private-api-key-74",
    ] {
        assert!(!encoded.contains(private));
    }
    assert_eq!(c.download_received_document(row).unwrap(), b"\xef\xbb\xbf<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<Invoice>synthetic \xc3\xa6</Invoice>\r\n");
    assert!(
        c.list_received_documents("company_key", 1, 1, None)
            .unwrap()
            .documents
            .is_empty()
    );
    let calls = trace(&f);
    assert_eq!(calls.len(), 3);
    assert_eq!(
        calls[0]["path"],
        "/second/ap/api/rest/list-received-documents"
    );
    assert_eq!(
        calls[0]["query"],
        json!({"limit":["1"],"offset":["0"],"maxTimestamp":["2026-01-01T12:30:00Z"],"companyKey":["company_key"]})
    );
    assert_eq!(calls[0]["authOk"], true);
    assert_eq!(calls[1]["authorizationPresent"], false);
    assert_eq!(calls[1]["accept"], "application/xml");
    assert_eq!(
        calls[2]["query"],
        json!({"limit":["1"],"offset":["1"],"companyKey":["company_key"]})
    );
}
#[test]
fn incomplete_booking_rows_remain_visible_and_page_signals_are_only_signals() {
    let f = fixture("IP:127.0.0.1");
    let c = client(&f, "missing");
    let page = c
        .list_received_documents("company_key", 100, 0, None)
        .unwrap();
    assert_eq!(
        page.documents[0].metadata(),
        json!({"internalId":"stable-1"})
    );
    assert_eq!(
        c.download_received_document(&page.documents[0])
            .unwrap_err(),
        Error::Protocol
    );
    assert_eq!(trace(&f).len(), 1);
    assert!(
        !client(&f, "last")
            .list_received_documents("company_key", 100, 0, None)
            .unwrap()
            .has_next
    );
}
#[test]
fn list_refuses_malformed_oversized_secret_and_unsafe_rows_without_retries() {
    let f = fixture("IP:127.0.0.1");
    for mode in [
        "malformed",
        "large",
        "stream-large",
        "encoded",
        "too-many",
        "bad-type",
        "secret-field",
        "secret-encoded",
        "bad-url",
        "no-id",
        "redirect",
        "refused",
        "hang",
    ] {
        let before = trace(&f).len();
        let error = client(&f, mode)
            .list_received_documents("company_key", 100, 0, None)
            .err()
            .unwrap();
        assert!(!format!("{error:?} {error}").contains("synthetic"));
        assert_eq!(trace(&f).len(), before + 1, "{mode}");
    }
}
#[test]
fn signed_download_bounds_redirect_timeout_encoding_and_refusal_are_safe() {
    let f = fixture("IP:127.0.0.1");
    for (mode, expected) in [
        ("xml-large", Error::Protocol),
        ("xml-stream-large", Error::Protocol),
        ("xml-encoded", Error::Protocol),
        ("xml-empty", Error::Protocol),
        ("xml-redirect", Error::Refused { status: 307 }),
        ("xml-refused", Error::Refused { status: 403 }),
        ("xml-hang", Error::Unavailable),
    ] {
        let c = client(&f, mode);
        let page = c
            .list_received_documents("company_key", 100, 0, None)
            .unwrap();
        let before = trace(&f).len();
        let error = c
            .download_received_document(&page.documents[0])
            .unwrap_err();
        assert_eq!(error, expected, "{mode}");
        assert!(!format!("{error:?} {error}").contains("synthetic"));
        let calls = trace(&f);
        assert_eq!(calls.len(), before + 1);
        assert_eq!(calls.last().unwrap()["authorizationPresent"], false);
    }
    let c = client(&f, "xml-exact-bound");
    let p = c
        .list_received_documents("company_key", 100, 0, None)
        .unwrap();
    assert_eq!(
        c.download_received_document(&p.documents[0]).unwrap().len(),
        MAX_XML
    );
}
#[test]
fn input_url_and_wire_structure_boundaries() {
    let f = fixture("IP:127.0.0.1");
    let c = client(&f, "normal");
    for (key, limit, timestamp) in [
        ("bad key", 100, None),
        ("company_key", 0, None),
        ("company_key", 101, None),
        ("company_key", 1, Some("bad\nvalue")),
    ] {
        assert!(matches!(
            c.list_received_documents(key, limit, 0, timestamp),
            Err(Error::InvalidInput)
        ));
    }
    assert!(trace(&f).is_empty());
    for url in [
        "http://example.invalid/a",
        "https://user@example.invalid/a",
        "https://@example.invalid/a",
        "https://example.invalid/a#fragment",
        "https://example.invalid\\a",
        "https://127.0.0.1/a",
        "https://localhost/a",
        " https://example.invalid/a",
        "https:////example.invalid/a",
    ] {
        assert!(signed_url(url, false).is_err(), "{url}");
    }
    for value in [
        json!([]),
        json!({"documents":{}}),
        json!({"documents":[null]}),
        json!({"documents":[],"nextPageUrl":5}),
        json!({"documents":[{"internalId":"x","senderName":"x".repeat(4097)}]}),
    ] {
        assert!(c.received_page(value).is_err());
    }
}
#[test]
fn credential_generation_is_stable_and_changes_with_private_admission() {
    let c = |key: &str, environment: &str| {
        Client::new(config(
            json!({"apiLicenseKey":key,"environment":environment}),
        ))
        .unwrap()
    };
    let original = c("synthetic-key", "test");
    assert_eq!(
        original.credential_generation(),
        original.credential_generation()
    );
    let rotated = c("rotated-key", "test");
    let restored = c("synthetic-key", "test");
    assert_ne!(
        original.credential_generation(),
        rotated.credential_generation()
    );
    assert_ne!(
        original.credential_generation(),
        restored.credential_generation()
    );
    assert_ne!(
        c("synthetic-key", "test").credential_generation(),
        c("rotated-key", "test").credential_generation()
    );
    assert_ne!(
        c("synthetic-key", "test").credential_generation(),
        c("synthetic-key", "production").credential_generation()
    );
}
#[test]
fn receive_tls_refuses_wrong_host_and_untrusted_root() {
    let f = fixture("DNS:wrong.invalid");
    assert!(matches!(
        client(&f, "normal").list_received_documents("company_key", 100, 0, None),
        Err(Error::Unavailable)
    ));
    assert!(trace(&f).is_empty());
    let other = fixture("IP:127.0.0.1");
    let c = Client::new(config(json!({"apiLicenseKey":"synthetic-private-api-key-74","environment":"test","baseUrl":format!("https://127.0.0.1:{}/normal/ap/api/rest",other.port),"trustedCaPem":f.ca,"timeoutMs":250}))).unwrap();
    assert!(matches!(
        c.list_received_documents("company_key", 100, 0, None),
        Err(Error::Unavailable)
    ));
    assert!(trace(&other).is_empty());
}
