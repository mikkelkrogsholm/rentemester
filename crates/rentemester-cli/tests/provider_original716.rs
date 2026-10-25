//! Synthetic, protocol-aware blackbox coverage. Shared wiring is owned by integration.
#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    process::Command,
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
const KEYS: [(&str, &str); 13] = [
    ("efaktura", "konfigurer"),
    ("efaktura", "leveringsstatus"),
    ("efaktura", "modtag"),
    ("efaktura", "modtag-workspace"),
    ("efaktura", "onboard"),
    ("efaktura", "onboarding-status"),
    ("efaktura", "registrer"),
    ("efaktura", "registrer-test-afsender"),
    ("efaktura", "registrer-test-gln"),
    ("efaktura", "status"),
    ("mail-intake", "ingest"),
    ("imap-intake", "poll"),
    ("invoice", "send"),
];
fn run(args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .args(args)
        .env_remove("RENTEMESTER_COMPANY")
        .output()
        .unwrap()
}
fn private(content: &[u8]) -> std::path::PathBuf {
    use std::os::unix::fs::OpenOptionsExt;
    static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let p = std::env::temp_dir().join(format!(
        "provider716-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&p)
        .unwrap()
        .write_all(content)
        .unwrap();
    p
}
fn fixture(args: &[&str], expected: &str, response: Value) -> (std::process::Output, Value) {
    let listener = common::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let expected = expected.to_owned();
    let worker = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = Vec::new();
        let mut end = None;
        let mut length = 0;
        loop {
            let mut b = [0; 4096];
            let n = stream.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if end.is_none() {
                if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    end = Some(i + 4);
                    for l in String::from_utf8_lossy(&bytes[..i]).lines() {
                        if let Some(v) = l.to_lowercase().strip_prefix("content-length:") {
                            length = v.trim().parse().unwrap();
                        }
                    }
                }
            }
            if end.is_some_and(|i| bytes.len() >= i + length) {
                break;
            }
        }
        let header = String::from_utf8_lossy(&bytes[..end.unwrap()]);
        assert!(header.starts_with(&expected), "{header}");
        assert!(header.to_lowercase().contains("cookie: session=synthetic"));
        assert!(header.to_lowercase().contains("x-rentemester-manifest:"));
        let body = if length == 0 {
            json!({})
        } else {
            serde_json::from_slice(&bytes[end.unwrap()..]).unwrap()
        };
        let response = response.to_string();
        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",response.len()).unwrap();
        body
    });
    let cookie = private(b"session=synthetic");
    let out = Command::new(BIN)
        .args(args)
        .args([
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            cookie.to_str().unwrap(),
            "--company",
            "synthetic",
        ])
        .output()
        .unwrap();
    std::fs::remove_file(cookie).unwrap();
    (out, worker.join().unwrap())
}
fn value(out: &std::process::Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
        panic!(
            "stdout={} stderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}
#[test]
fn all_owned_literal_keys_help_and_unknown_flags() {
    for (family, cmd) in KEYS {
        let o = run(&["--format=json", family, cmd, "--help"]);
        assert!(o.status.success(), "{family} {cmd}: {:?}", o);
        assert_eq!(value(&o)["key"], format!("{family} {cmd}"));
        let o = run(&[family, cmd, "--unknown-provider-flag=x"]);
        assert_eq!(o.status.code(), Some(2));
        assert!(o.stdout.is_empty());
    }
}
#[test]
fn exact_confirm_refusals_and_parse_order() {
    for (_, cmd) in KEYS
        .into_iter()
        .filter(|(f, c)| *f == "efaktura" && *c != "onboarding-status")
    {
        let mut a = vec!["efaktura", cmd, "--json"];
        if cmd == "registrer" {
            a.extend(["--cvr", "DK12345678", "--company-name", "Synthetic"]);
        }
        let o = run(&a);
        assert_eq!(o.status.code(), Some(1), "{cmd}: {:?}", o);
        assert_eq!(value(&o)["ok"], false);
        assert!(
            value(&o)["errors"][0]
                .as_str()
                .unwrap()
                .contains("--confirm yes required")
        );
    }
    for cmd in ["registrer-test-gln", "registrer-test-afsender"] {
        let o = run(&["efaktura", cmd, "--confirm", "YES", "--json"]);
        assert_eq!(o.status.code(), Some(1));
        assert!(
            value(&o)["errors"][0]
                .as_str()
                .unwrap()
                .starts_with("--confirm yes")
        );
    }
    let o = run(&["efaktura", "modtag", "--confirm"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(o.stdout.is_empty());
    let o = run(&[
        "efaktura",
        "modtag",
        "--confirm=yes",
        "--limit=not-number",
        "--json",
    ]);
    assert_eq!(o.status.code(), Some(1));
    let o = run(&["invoice", "send", "--kind=unknown"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(o.stdout.is_empty());
    let o = run(&["mail-intake", "ingest", "--json"]);
    assert_eq!(o.status.code(), Some(2));
    assert!(o.stdout.is_empty());
}
#[test]
fn metadata_read_error_and_human_refusal_stream() {
    let o = run(&[
        "efaktura",
        "modtag",
        "--confirm=yes",
        "--metadata=/nonexistent/provider716.json",
        "--json",
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        value(&o)["errors"][0]
            .as_str()
            .unwrap()
            .starts_with("could not read --metadata")
    );
    let o = run(&["efaktura", "onboard", "--format=human"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(o.stdout.is_empty());
    assert!(String::from_utf8_lossy(&o.stderr).contains("  → --confirm yes required"));
}
#[test]
fn config_private_file_literal_equals_and_public_receipt() {
    let key = private(b"--synthetic=secret\n");
    let (o, b) = fixture(
        &[
            "efaktura",
            "konfigurer",
            "--api-license-key-file",
            key.to_str().unwrap(),
            "--confirm= YES ",
            "--environment= TEST ",
            "--json",
        ],
        "POST /api/companies/synthetic/original-efaktura-konfigurer ",
        json!({"ok":true,"result":{"ok":true,"configPath":"config/digisense.json","environment":"test"}}),
    );
    std::fs::remove_file(key).unwrap();
    assert!(o.status.success(), "{:?}", o);
    assert_eq!(b["apiLicenseKey"], "--synthetic=secret");
    assert_eq!(b["environment"], "test");
    assert_eq!(b["confirm"], true);
    assert!(!String::from_utf8_lossy(&o.stdout).contains("secret"));
    assert!(o.stderr.is_empty());
}
#[test]
fn source_registration_receive_onboard_and_status_read_routes() {
    let failure = json!({"ok":true,"result":{"ok":false,"errors":["synthetic refusal"]}});
    for (cmd, extra, route) in [
        (
            "registrer",
            vec![
                "--cvr=DK12345678",
                "--company-name=--literal=a",
                "--network=other",
            ],
            "POST /api/companies/synthetic/original-efaktura-registrer ",
        ),
        (
            "onboard",
            vec![],
            "POST /api/companies/synthetic/original-efaktura-onboard ",
        ),
        (
            "modtag",
            vec!["--limit=0x10", "--digisense-company-key=--key=a", "--force"],
            "POST /api/companies/synthetic/original-efaktura-modtag ",
        ),
        (
            "onboarding-status",
            vec![],
            "GET /api/companies/synthetic/original-efaktura-onboarding-status ",
        ),
    ] {
        let mut args = vec!["efaktura", cmd, "--json"];
        if cmd != "onboarding-status" {
            args.push("--confirm=yes");
        }
        args.extend(extra);
        let (o, b) = fixture(&args, route, failure.clone());
        assert_eq!(o.status.code(), Some(1));
        assert_eq!(value(&o)["errors"][0], "synthetic refusal");
        if cmd == "modtag" {
            assert_eq!(b["limit"], 16);
            assert_eq!(b["digisenseCompanyKey"], "--key=a");
            assert_eq!(b["force"], true);
        }
        if cmd == "registrer" {
            assert_eq!(b["companyName"], "--literal=a");
            assert!(b.get("network").is_none());
        }
    }
}
#[test]
fn explicit_workspace_requires_verified_local_profile() {
    let o = run(&[
        "efaktura",
        "modtag-workspace",
        "--confirm=yes",
        "--workspace=/arbitrary/remote",
        "--server=http://127.0.0.1:9",
        "--allow-loopback-http",
        "--cookie-file=/missing",
    ]);
    assert!(!o.status.success());
    assert!(o.stdout.is_empty());
}
#[test]
fn email_source_dry_run_adapter_single_request_no_live_transport() {
    let r = json!({"ok":true,"result":{"ok":true,"invoiceNumber":"SYN-1","kind":"invoice","recipient":"synthetic@example.invalid","subject":"Synthetic","messageId":"synthetic-id","duplicate":false}});
    let (o, b) = fixture(
        &[
            "invoice",
            "send",
            "--document-id=0x10",
            "--to=synthetic@example.invalid",
            "--json",
        ],
        "POST /api/companies/synthetic/original-invoice-send-email ",
        r,
    );
    assert!(o.status.success(), "{:?}", o);
    assert_eq!(b["documentId"], 16);
    assert_eq!(b["confirm"], true);
    assert_eq!(b["kind"], "invoice");
    let (o, _) = fixture(
        &["invoice", "send", "--invoice-number=--SYN=1", "--json"],
        "POST /api/companies/synthetic/original-invoice-send-email ",
        json!({"ok":true,"result":{"ok":false,"errors":["the built-in SMTP transport runs in dryRun only"]}}),
    );
    assert_eq!(o.status.code(), Some(1));
    assert!(
        value(&o)["errors"][0]
            .as_str()
            .unwrap()
            .contains("dryRun only")
    );
}
#[test]
fn mail_packages_local_bytes_and_message_metadata() {
    let eml = private(b"Message-ID: <synthetic>\r\n\r\nsynthetic");
    let meta = private(br#"{"<synthetic>":{"invoiceNo":"SYN-1"}}"#);
    let result = json!({"ok":true,"result":{"ok":true,"documents":[],"messagesProcessed":1,"attachmentsIngested":0,"attachmentsSkipped":0,"exceptionsCreated":1}});
    let (o, b) = fixture(
        &[
            "mail-intake",
            "ingest",
            "--source",
            eml.to_str().unwrap(),
            "--metadata",
            meta.to_str().unwrap(),
            "--force",
            "--json",
        ],
        "POST /api/companies/synthetic/original-mail-intake ",
        result,
    );
    std::fs::remove_file(eml).unwrap();
    std::fs::remove_file(meta).unwrap();
    assert!(o.status.success(), "{:?}", o);
    assert_eq!(b["metadataPerMessage"]["<synthetic>"]["invoiceNo"], "SYN-1");
    assert_eq!(b["messages"].as_array().unwrap().len(), 1);
    assert!(b.get("source").is_none());
}
#[test]
fn imap_uses_common_route_and_never_places_password_in_body() {
    let (o, b) = fixture(
        &[
            "imap-intake",
            "poll",
            "--imap-host=imap.example.invalid",
            "--imap-username=synthetic",
            "--imap-port=993",
            "--since-uid=0",
            "--json",
        ],
        "POST /api/companies/synthetic/original-imap-intake ",
        json!({"ok":true,"result":{"ok":false,"errors":["synthetic IMAP refusal"]}}),
    );
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(b["imapPort"], 993);
    assert_eq!(b["sinceUid"], 0);
    assert!(b.get("password").is_none());
}
#[test]
fn both_delivery_literals_use_source_status_once_and_keep_failure() {
    for cmd in ["status", "leveringsstatus"] {
        let (o, b) = fixture(
            &[
                "efaktura",
                cmd,
                "--confirm=yes",
                "--document-id=1",
                "--digisense-company-key=--key=a",
                "--json",
            ],
            "POST /api/companies/synthetic/invoices/original/status-digisense ",
            json!({"ok":false,"errors":["synthetic status refusal"]}),
        );
        assert_eq!(o.status.code(), Some(1));
        assert_eq!(b["documentId"], 1);
        assert_eq!(b["companyKey"], "--key=a");
    }
}

#[test]
fn cli_only_test_registration_executes_and_returns_only_public_receipt() {
    for cmd in ["registrer-test-gln", "registrer-test-afsender"] {
        let route = format!("POST /api/companies/synthetic/original-efaktura-{cmd} ");
        let (o, b) = fixture(
            &["efaktura", cmd, "--confirm=yes", "--json"],
            &route,
            json!({"ok":true,"result":{"ok":true,"registered":true}}),
        );
        assert!(o.status.success(), "{o:?}");
        assert_eq!(value(&o), json!({"ok":true,"registered":true}));
        assert_eq!(b["confirm"], true);
        for secret in [
            "testGlnNumber",
            "participantId",
            "companyKey",
            "apiLicenseKey",
        ] {
            assert!(b.get(secret).is_none());
            assert!(value(&o).get(secret).is_none());
        }
    }
}
