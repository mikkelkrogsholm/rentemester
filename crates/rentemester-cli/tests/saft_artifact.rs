use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    process::{Command, Output},
    thread,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
const ROOT: &str = "saft-export-synthetic";
fn package(stamp: &str, explicit: bool) -> Vec<u8> {
    let manifest = json!({"manifestVersion":2,"packageType":"saft_export","profileId":"rentemester-dk-saft-v3-ledger-sales-purchases-masterfiles","periodStart":"2026-01-01","periodEnd":"2026-12-31","generatedAt":stamp,"generatedAtSource":if explicit{"caller_supplied"}else{"period_end"},"generatedAtExplicit":explicit,"sourceCompanyRootName":"synthetic","appliedRules":["DK-BOOKKEEPING-SAFT-EXPORT-001"],"files":{"saftXml":"saft.xml","readme":"README.txt"},"counts":{"accounts":0,"journalEntries":0,"salesInvoices":0,"purchaseInvoices":0,"vatSummaryCodes":0,"documentReferences":0,"customers":0,"suppliers":0},"outputs":[{"path":"README.txt","sizeBytes":25,"sha256":"50485207c90dea133753892737e99c4b140eee241425693746b56e14729e30f2"},{"path":"saft.xml","sizeBytes":32,"sha256":"90092b7a463f58a9916395c440a2b9895c48bd4f006caa9a855d54854419e778"}],"sourceContext":{"journalEntries":[],"documents":[],"auditLog":[],"trialBalance":{"ok":true,"balanced":true,"accounts":[],"totalDebit":"0.00","totalCredit":"0.00"},"journalHashChain":{"ok":true,"entriesChecked":0,"errors":[]}}});
    rentemester_storage::accountant::tar(BTreeMap::from([
        (
            format!("{ROOT}/saft.xml"),
            b"<AuditFile>synthetic</AuditFile>".to_vec(),
        ),
        (
            format!("{ROOT}/README.txt"),
            b"Synthetic bounded profile".to_vec(),
        ),
        (
            format!("{ROOT}/manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        ),
    ]))
    .unwrap()
}
fn server(bytes: Vec<u8>) -> (String, thread::JoinHandle<String>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = vec![];
        loop {
            let mut b = [0; 4096];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            request.extend(&b[..n]);
            if let Some(p) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&request[..p]).to_lowercase();
                let len: usize = head
                    .lines()
                    .find_map(|s| s.strip_prefix("content-length:"))
                    .unwrap()
                    .trim()
                    .parse()
                    .unwrap();
                if request.len() >= p + 4 + len {
                    break;
                }
            }
        }
        write!(s,"HTTP/1.1 200 OK\r\nContent-Type:application/x-tar\r\nContent-Length:{}\r\nConnection:close\r\n\r\n",bytes.len()).unwrap();
        s.write_all(&bytes).unwrap();
        String::from_utf8(request).unwrap()
    });
    (origin, t)
}

fn call(origin: &str, dir: &Path, extra: &[&str]) -> Output {
    Command::new(BIN)
        .args([
            "system",
            "export-saft",
            "--server",
            origin,
            "--allow-loopback-http",
            "--cookie-file",
            dir.join("cookie").to_str().unwrap(),
            "--company",
            "synthetic",
            "--from",
            "2026-01-01",
            "--to",
            "2026-12-31",
            "--out",
            dir.join("out").to_str().unwrap(),
            "--json",
        ])
        .args(extra)
        .output()
        .unwrap()
}
#[test]
fn original_command_forwards_generation_choice_and_materializes_exact_flat_metadata() {
    for explicit in [false, true] {
        let dir = std::env::temp_dir().join(format!("rm137-cli-{}-{explicit}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(dir.join("cookie"))
            .unwrap()
            .write_all(b"session=synthetic")
            .unwrap();
        let stamp = if explicit {
            "2027-01-01T00:00:00.123Z"
        } else {
            "2026-12-31T23:59:59.000Z"
        };
        let extra = if explicit {
            vec!["--generated-at", "2027-01-01T01:00:00.123456+01:00"]
        } else {
            vec![]
        };
        let bytes = package(stamp, explicit);
        let (origin, task) = server(bytes.clone());
        let output = call(&origin, &dir, &extra);
        assert_eq!(
            output.status.code(),
            Some(0),
            "{} {}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(output.stderr.is_empty());
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        let target = std::fs::canonicalize(&dir).unwrap().join("out").join(ROOT);
        assert_eq!(
            result,
            json!({"ok":true,"exportDir":target,"manifestPath":target.join("manifest.json"),"saftXmlPath":target.join("saft.xml"),"generatedAt":stamp,"periodStart":"2026-01-01","periodEnd":"2026-12-31","journalEntryCount":0,"salesInvoiceCount":0,"purchaseInvoiceCount":0,"customerCount":0,"supplierCount":0,"appliedRules":["DK-BOOKKEEPING-SAFT-EXPORT-001"],"errors":[]})
        );
        assert_eq!(
            std::fs::read(target.join("saft.xml")).unwrap(),
            b"<AuditFile>synthetic</AuditFile>"
        );
        let request = task.join().unwrap();
        assert!(request.starts_with("POST /api/companies/synthetic/saft-export HTTP/1.1"));
        let body: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
        let mut expected = json!({"periodStart":"2026-01-01","periodEnd":"2026-12-31","confirm":true,"generationMode":if explicit{"caller_supplied"}else{"period_end"}});
        if explicit {
            expected["generatedAt"] = json!(extra[1]);
        }
        assert_eq!(body, expected);
        let (origin, task) = server(bytes);
        let output = call(&origin, &dir, &extra);
        assert_eq!(output.status.code(), Some(1));
        let refused: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(refused["ok"], false);
        assert!(!refused["errors"].as_array().unwrap().is_empty());
        task.join().unwrap();
        assert!(target.join("manifest.json").is_file());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
#[test]
fn missing_selectors_are_usage_two_and_help_succeeds() {
    let output = Command::new(BIN)
        .args(["system", "export-saft", "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
    let output = Command::new(BIN)
        .args(["system", "export-saft", "--help"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
}
