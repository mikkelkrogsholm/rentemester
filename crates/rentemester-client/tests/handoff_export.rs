mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error, write_handoff_package};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    thread,
};
fn tar(files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut out = vec![];
    for (name, data) in files {
        let mut h = [0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
        h[148..156].fill(b' ');
        h[156] = b'0';
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        let sum: usize = h.iter().map(|b| *b as usize).sum();
        h[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        out.extend(h);
        out.extend(data);
        out.resize(out.len().next_multiple_of(512), 0);
    }
    out.resize(out.len() + 1024, 0);
    out
}
const ROOT: &str = "myndigheds-eksport-2026";
fn fixture() -> BTreeMap<String, Vec<u8>> {
    let bytes = b"original receipt".to_vec();
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let docs = json!([{"id":1,"sha256":hash,"storedPathRelativeToCompany":"documents/source.pdf","exportedReadablePath":"documents-readable/receipt.pdf"}]);
    let mut entries = BTreeMap::from([
        ("README.txt".to_string(), b"Synthetic export".to_vec()),
        ("documents.json".into(), serde_json::to_vec(&docs).unwrap()),
        ("documents-readable/receipt.pdf".into(), bytes),
    ]);
    let outputs:Vec<_>=entries.iter().map(|(p,b)|json!({"path":p,"sizeBytes":b.len(),"sha256":format!("{:x}",Sha256::digest(b))})).collect();
    let manifest = json!({"manifestVersion":2,"packageType":"authority_export","periodStart":"2026-01-01","periodEnd":"2026-12-31","generatedAt":"2027-01-01T12:00:00Z","deadlineAt":null,"appliedRules":["DK-BOOKKEEPING-AUTHORITY-EXPORT-001"],"counts":{"journalEntries":2,"bankTransactions":1,"documents":1,"copiedReadableDocuments":1},"files":{"documents":"documents.json","readme":"README.txt","readableDocumentsDir":"documents-readable"},"outputs":outputs,"copiedDocuments":[{"documentId":1,"sourcePathRelativeToCompany":"documents/source.pdf","exportedPath":"documents-readable/receipt.pdf","sha256":hash,"sizeBytes":16}]});
    entries.insert(
        "manifest.json".into(),
        serde_json::to_vec(&manifest).unwrap(),
    );
    entries
        .into_iter()
        .map(|(p, b)| (format!("{ROOT}/{p}"), b))
        .collect()
}
fn envelope(bytes: Vec<u8>) -> Value {
    json!({"ok":true,"export":{"contentType":"application/x-tar","bytes":bytes}})
}
#[test]
fn materializes_verified_sources_in_fresh_root_preserving_parent_and_never_overwrites() {
    let out = std::env::temp_dir().join(format!("rm136-valid-{}", std::process::id()));
    std::fs::create_dir(&out).unwrap();
    std::fs::write(out.join("existing"), b"keep").unwrap();
    let v = envelope(tar(&fixture()));
    let result = write_handoff_package(&v, &out).unwrap();
    assert_eq!(result["documentCount"], 1);
    assert_eq!(result["generatedAt"], "2027-01-01T12:00:00Z");
    assert_eq!(
        std::fs::read(out.join(ROOT).join("documents-readable/receipt.pdf")).unwrap(),
        b"original receipt"
    );
    assert!(write_handoff_package(&v, &out).is_err());
    assert_eq!(std::fs::read(out.join("existing")).unwrap(), b"keep");
    std::fs::remove_dir_all(out).unwrap();
}
#[test]
fn rejects_unsafe_archive_and_manifest_before_any_local_effect() {
    let original = fixture();
    let mut cases = vec![];
    let mut bad = tar(&original);
    bad[0] ^= 1;
    cases.push(bad);
    let mut bad = tar(&original);
    bad[156] = b'2';
    cases.push(bad);
    let mut bad = tar(&original);
    bad.truncate(bad.len() - 512);
    cases.push(bad);
    let mut bad = tar(&original);
    bad[528] = 1; // Nonzero regular-file padding.
    cases.push(bad);
    let mut bad = tar(&original);
    let first = bad[..1024].to_vec();
    bad.splice(0..0, first); // Duplicate regular-file header with its original valid checksum.
    cases.push(bad);

    for kind in 0..5 {
        let mut files = original.clone();
        match kind {
            0 => {
                files.insert("../escape".into(), vec![]);
            }
            1 => {
                files.insert("other/extra".into(), vec![]);
            }
            2 => {
                files.insert(format!("{ROOT}/undeclared"), vec![]);
            }
            _ => {
                let name = format!("{ROOT}/manifest.json");
                let mut m: Value = serde_json::from_slice(&files[&name]).unwrap();
                if kind == 3 {
                    m["outputs"][0]["sha256"] = json!("forged");
                } else {
                    m["copiedDocuments"][0]["sourcePathRelativeToCompany"] =
                        json!("documents/wrong.pdf");
                }
                files.insert(name, serde_json::to_vec(&m).unwrap());
            }
        }
        cases.push(tar(&files));
    }
    for (i, bytes) in cases.into_iter().enumerate() {
        let out = std::env::temp_dir().join(format!("rm136-invalid-{}-{i}", std::process::id()));
        assert!(
            write_handoff_package(&envelope(bytes), &out).is_err(),
            "case {i}"
        );
        assert!(!out.exists());
    }
}
#[tokio::test]
async fn exact_confirmed_post_and_uncertain_response_never_retry() {
    for (profile, route, status, mime) in [
        (
            "authority",
            "authority-export",
            "200 OK",
            "application/x-tar",
        ),
        (
            "accountant_handoff",
            "accountant-export",
            "503 Unavailable",
            "application/x-tar",
        ),
        ("authority", "authority-export", "200 OK", "text/plain"),
    ] {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", l.local_addr().unwrap());
        let body=br#"{"periodStart":"2026-01-01","periodEnd":"2026-12-31","confirm":true,"requester":"Synthetic agent","requestedAt":"2027-01-01T12:00:00+01:00"}"#.to_vec();
        let expected = body.clone();
        let t = thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = vec![];
            while !request.ends_with(&expected) {
                let mut b = [0; 4096];
                let n = s.read(&mut b).unwrap();
                assert!(n > 0);
                request.extend(&b[..n]);
            }
            let text = String::from_utf8_lossy(&request);
            assert!(text.starts_with(&format!("POST /api/companies/synthetic/{route} HTTP/1.1")));
            assert!(text.to_lowercase().contains("cookie: session=synthetic"));
            write!(s,"HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length:1024\r\nConnection:close\r\n\r\n").unwrap();
            s.write_all(&[0; 1024]).unwrap();
            drop(s);
            l.set_nonblocking(true).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(30));
            assert!(l.accept().is_err());
        });
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let result = api.handoff_export("synthetic", profile, body, true).await;
        if status == "200 OK" && mime == "application/x-tar" {
            assert_eq!(
                result.unwrap()["export"]["bytes"].as_array().unwrap().len(),
                1024
            );
        } else {
            assert_eq!(result.unwrap_err(), Error::OutcomeUnknown);
        }
        t.join().unwrap();
    }
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    assert!(matches!(
        api.handoff_export("synthetic", "authority", vec![], false)
            .await,
        Err(Error::InvalidInput(_))
    ));
}
