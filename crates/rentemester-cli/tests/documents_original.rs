use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
fn server(
    status: u16,
    response: String,
    target: &'static str,
    body: Option<Vec<u8>>,
) -> (String, Arc<AtomicUsize>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    let handle = thread::spawn(move || {
        let start = Instant::now();
        let mut last: Option<Instant> = None;
        loop {
            if let Some(t) = last {
                if t.elapsed() >= Duration::from_millis(150) {
                    break;
                }
            } else {
                assert!(
                    start.elapsed() < Duration::from_secs(5),
                    "startup deadline exceeded"
                );
            }
            let (mut s, _) = match listener.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            seen.fetch_add(1, Ordering::SeqCst);
            s.set_nonblocking(false).unwrap();
            s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let mut bytes = Vec::new();
            let end = loop {
                let mut chunk = [0; 4096];
                let n = s.read(&mut chunk).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
                if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    break i + 4;
                }
            };
            let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
            assert!(headers.starts_with(target), "{headers}");
            assert!(headers.to_lowercase().contains("cookie: session=synthetic"));
            let length = headers
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length:")
                        .map(|s| s.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            while bytes.len() < end + length {
                let mut chunk = [0; 4096];
                let n = s.read(&mut chunk).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&chunk[..n]);
            }
            assert_eq!(&bytes[end..], body.as_deref().unwrap_or_default());
            write!(s,"HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
            last = Some(Instant::now());
        }
    });
    (origin, count, handle)
}
fn once(count: Arc<AtomicUsize>, handle: thread::JoinHandle<()>) {
    handle.join().unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

fn root() -> std::path::PathBuf {
    let root = std::env::temp_dir().canonicalize().unwrap().join(format!(
        "documents-original-cli-{}-{:?}",
        std::process::id(),
        thread::current().id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    root
}
fn cookie(root: &std::path::Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = root.join("cookie");
    std::fs::write(&path, "session=synthetic").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    path
}
#[test]
fn documents_original_eight_pure_help_flags_anywhere_and_usage_before_file_http() {
    for op in [
        "ingest",
        "list",
        "enrich",
        "set-company-context",
        "parse",
        "parse-pending",
        "parse-status",
        "parsed-text",
    ] {
        let out = Command::new(BIN)
            .args([
                "--local-profile",
                "/private/not-opened",
                "documents",
                "--company",
                "synthetic",
                op,
                "--help",
            ])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stderr.is_empty());
        assert!(String::from_utf8_lossy(&out.stdout).contains("documents ingest"));
    }
    for args in [
        vec!["documents", "parse", "--document-id", "7"],
        vec![
            "documents",
            "enrich",
            "--document-id",
            "7",
            "--metadata",
            "/never-opened",
            "--confirm",
            "true",
        ],
        vec![
            "documents",
            "parse-pending",
            "--confirm",
            "yes",
            "--limit",
            "101",
        ],
        vec!["documents", "list", "--actor", "invented"],
        vec![
            "documents",
            "parsed-text",
            "--document-id",
            "7",
            "--offset",
            "-1",
        ],
    ] {
        let out = Command::new(BIN).args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(
            out.stdout.is_empty(),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
        assert!(!out.stderr.is_empty());
    }
}
fn fixture() -> Value {
    json!({"id":7,"documentNo":"B-000007","source":"synthetic","originalFilename":"synthetic.pdf","documentType":"purchase_sale","invoiceDate":"2026-10-22","amountIncVat":125.5,"currency":"DKK","status":"ingested","storedPath":null,"senderVatOrCvr":"12345678","supplierCountryCode":"DK","supplierIdentifierKind":"dk_cvr","supplierIdentityStatus":"resolved","sourceBankTransactionId":null,"accountingRationale":null,"preparedBy":null,"preparedByProgram":null,"purchaseVatLines":[{"classification":"dk_purchase_25","netAmount":100.4,"vatAmount":25.1}]})
}
#[test]
fn documents_original_list_pipe_bare_array_and_explicit_human_and_json_wins() {
    let root = root();
    let credential = cookie(&root);
    for (flags, human) in [
        (vec![], false),
        (vec!["--format", "human"], true),
        (vec!["--json", "--format", "human"], false),
    ] {
        let response = json!({"ok":true,"documents":[fixture()],"errors":[]}).to_string();
        let (origin, count, handle) = server(
            200,
            response,
            "GET /api/companies/synthetic/documents/original/list ",
            None,
        );
        let out = Command::new(BIN)
            .args([
                "--server",
                &origin,
                "--allow-loopback-http",
                "documents",
                "--company",
                "synthetic",
                "list",
                "--cookie-file",
                credential.to_str().unwrap(),
            ])
            .args(flags)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stderr.is_empty());
        if human {
            let text = String::from_utf8(out.stdout).unwrap();
            assert!(text.contains("Bilag (1)"));
            assert!(text.contains("#B-000007 — synthetic.pdf"));
            assert!(text.contains("125,50"));
        } else {
            let v: Value = serde_json::from_slice(&out.stdout).unwrap();
            assert!(v.is_array());
            assert_eq!(v[0]["stored_path"], Value::Null);
            assert_eq!(v[0]["document_no"], "B-000007");
            assert_eq!(v[0].as_object().unwrap().len(), 19);
            assert!(v[0].get("payload_json").is_none());
        }
        once(count, handle);
    }
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn documents_original_ingest_local_files_exact_body_and_business_refusal_no_retry() {
    let root = root();
    let credential = cookie(&root);
    let file = root.join("synthetic.pdf");
    let metadata = root.join("metadata.json");
    std::fs::write(&file, b"a").unwrap();
    std::fs::write(&metadata, r#"{"source":"synthetic"}"#).unwrap();
    let body = json!({"confirm":true,"fileName":"synthetic.pdf","fileBase64":"YQ==","metadata":{"source":"synthetic"},"vendorId":9,"force":true});
    let response=json!({"ok":false,"code":"DOCUMENT_INGEST_BLOCKED","errors":["Synthetic duplicate content"]}).to_string();
    let (origin, count, handle) = server(
        422,
        response,
        "POST /api/companies/synthetic/documents/original/ingest ",
        Some(serde_json::to_vec(&body).unwrap()),
    );
    let out = Command::new(BIN)
        .args([
            "documents",
            "ingest",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--company",
            "synthetic",
            "--cookie-file",
            credential.to_str().unwrap(),
            "--file",
            file.to_str().unwrap(),
            "--metadata",
            metadata.to_str().unwrap(),
            "--vendor-id",
            "9",
            "--force",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stderr.is_empty());
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["errors"], json!(["Synthetic duplicate content"]));
    once(count, handle);
    std::fs::write(&metadata, b"not JSON").unwrap();
    let out = Command::new(BIN)
        .args([
            "documents",
            "ingest",
            "--server",
            "http://127.0.0.1:1",
            "--allow-loopback-http",
            "--company",
            "synthetic",
            "--cookie-file",
            credential.to_str().unwrap(),
            "--file",
            file.to_str().unwrap(),
            "--metadata",
            metadata.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("JSON"));
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn documents_original_pdf_and_mutations_use_exact_native_routes_full_flat_dtos() {
    let root = root();
    let credential = cookie(&root);
    let metadata = root.join("metadata.json");
    std::fs::write(&metadata, r#"{"source":"synthetic"}"#).unwrap();
    let status = json!({"documentId":7,"sourceSha256":"a".repeat(64),"parserId":"pdfjs","parserVersion":"1","contractVersion":"1","status":"parsed","errorCode":null,"pageCount":1,"itemCount":1,"textLength":4,"resultHash":"b".repeat(64)});
    for (op, flags, target, body, response) in [
        (
            "enrich",
            vec![
                "--document-id",
                "7",
                "--metadata",
                metadata.to_str().unwrap(),
                "--confirm",
                "yes",
            ],
            "POST /api/companies/synthetic/documents/original/7/enrich ",
            Some(json!({"confirm":true,"metadata":{"source":"synthetic"}})),
            json!({"ok":true,"documentId":7,"enriched":false}),
        ),
        (
            "set-company-context",
            vec![
                "--document-id",
                "7",
                "--source-reference",
                "receipt",
                "--business-use-reason",
                "supplies",
                "--confirm",
                "yes",
            ],
            "POST /api/companies/synthetic/documents/original/7/company-context ",
            Some(
                json!({"confirm":true,"sourceReference":"receipt","businessUseReason":"supplies"}),
            ),
            json!({"ok":true,"documentId":7,"applied":false,"errors":[]}),
        ),
        (
            "parse",
            vec!["--document-id", "7", "--confirm", "yes"],
            "POST /api/companies/synthetic/documents/7/parse ",
            Some(json!({"confirm":true})),
            json!({"ok":true,"parse":{"documentId":7,"status":"parsed","errorCode":null,"cached":true,"pageCount":1,"itemCount":1,"textLength":4,"resultHash":"b".repeat(64)}}),
        ),
        (
            "parse-pending",
            vec!["--confirm", "yes", "--cursor", "0", "--limit", "2"],
            "POST /api/companies/synthetic/documents/parse-pending ",
            Some(json!({"confirm":true,"cursor":0,"limit":2})),
            json!({"ok":true,"batch":{"requested":0,"parsed":0,"failed":0,"cursor":0,"nextCursor":null,"resume":null}}),
        ),
        (
            "parse-status",
            vec!["--document-id", "7"],
            "GET /api/companies/synthetic/documents/7/parse-status ",
            None,
            json!({"ok":true,"parse":status}),
        ),
        (
            "parsed-text",
            vec!["--document-id", "7", "--offset", "0", "--limit", "1"],
            "GET /api/companies/synthetic/documents/7/parsed-text?offset=0&limit=1 ",
            None,
            json!({"ok":true,"parse":status,"pages":[{"pageNumber":1,"width":595,"height":842,"rotation":0,"text":"Test","itemCount":1,"layoutHash":"c".repeat(64)}],"offset":0,"limit":1,"nextOffset":null}),
        ),
    ] {
        let (origin, count, handle) = server(
            200,
            response.to_string(),
            target,
            body.map(|b| serde_json::to_vec(&b).unwrap()),
        );
        let out = Command::new(BIN)
            .args([
                "documents",
                op,
                "--server",
                &origin,
                "--allow-loopback-http",
                "--company",
                "synthetic",
                "--cookie-file",
                credential.to_str().unwrap(),
            ])
            .args(flags)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{op}: {} {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stdout).unwrap(),
            response
        );
        once(count, handle);
    }
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn documents_original_tty_defaults_to_human() {
    let root = root();
    let credential = cookie(&root);
    let (origin, count, handle) = server(
        200,
        json!({"ok":true,"documents":[],"errors":[]}).to_string(),
        "GET /api/companies/synthetic/documents/original/list ",
        None,
    );
    let output = Command::new("/usr/bin/script")
        .args([
            "-q",
            "/dev/null",
            BIN,
            "documents",
            "list",
            "--server",
            &origin,
            "--allow-loopback-http",
            "--company",
            "synthetic",
            "--cookie-file",
            credential.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Bilag (0)"), "{text}");
    assert!(text.contains("Ingen bilag gemt."));
    once(count, handle);
    std::fs::remove_dir_all(root).unwrap();
}
