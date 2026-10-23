use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    process::Command,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn exact_missing_input_and_pure_help() {
    for op in ["post-eu-service-purchase", "post-representation-purchase"] {
        for tail in [
            vec![],
            vec!["--input"],
            vec!["--input="],
            vec!["--input", " "],
        ] {
            let out = Command::new(BIN)
                .args(["vat", op])
                .args(tail)
                .output()
                .unwrap();
            assert_eq!(out.status.code(), Some(2));
            assert_eq!(
                String::from_utf8(out.stderr).unwrap(),
                "Missing required --input <file.json>\n"
            );
        }
        let out = Command::new(BIN)
            .args([
                "vat",
                op,
                "--help",
                "--local-profile",
                "/not/opened",
                "--cookie-file",
                "/not/opened",
            ])
            .output()
            .unwrap();
        assert!(out.status.success());
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("REPRESENTATION_SPECIAL"));
        assert!(text.contains("current-credential assertion"));
        for flag in ["--confirm", "--idempotency-key", "--output"] {
            let out = Command::new(BIN)
                .args(["vat", op, flag, "yes"])
                .output()
                .unwrap();
            assert_eq!(out.status.code(), Some(2));
        }
    }
}
#[test]
fn fixed_write_receipt_source_false_and_unresolved_invoice_usage() {
    let root = std::env::temp_dir().join(format!("vat-purchase-cli-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let cookie = root.join("cookie");
    std::fs::write(&cookie, "session=synthetic").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&cookie, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    for (op, core, exit) in [
        (
            "post-eu-service-purchase",
            json!({"ok":true,"entryId":8,"entryNo":"2026-8","entryHash":"a".repeat(64),"errors":[],"appliedRules":["DK-VAT-REVERSE-CHARGE-001"]}),
            0,
        ),
        (
            "post-representation-purchase",
            json!({"ok":false,"errors":["mapping missing; supply expenseAccountNo"],"appliedRules":[]}),
            1,
        ),
        (
            "post-eu-service-purchase",
            json!({"ok":false,"errors":["Could not resolve document for invoiceNo INV"],"appliedRules":[]}),
            2,
        ),
    ] {
        let input = if op == "post-eu-service-purchase" {
            json!({"transactionDate":"2026-01-01","text":"synthetic","netAmount":100,"expenseAccountNo":"synthetic","invoiceNo":" INV ","documentId":3})
        } else {
            json!({"transactionDate":"2026-01-01","text":"synthetic","netAmount":100,"documentId":3})
        };
        let file = root.join("input.json");
        std::fs::write(&file, input.to_string()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let response = core.clone();
        let thread = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let end = loop {
                let mut b = [0; 4096];
                let n = stream.read(&mut b).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&b[..n]);
                if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    break i + 4;
                }
            };
            let header = String::from_utf8_lossy(&bytes[..end]).to_string();
            let len: usize = header
                .lines()
                .find_map(|l| {
                    l.to_lowercase()
                        .strip_prefix("content-length:")
                        .map(|s| s.trim().parse().unwrap())
                })
                .unwrap();
            while bytes.len() < end + len {
                let mut b = [0; 4096];
                let n = stream.read(&mut b).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&b[..n]);
            }
            let body: Value = serde_json::from_slice(&bytes[end..end + len]).unwrap();
            let text = response.to_string();
            write!(
                stream,
                "HTTP/1.1 {} Mock\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                if response["ok"] == true { 200 } else { 422 },
                text.len()
            )
            .unwrap();
            (header, body)
        });
        let out = Command::new(BIN)
            .args([
                "--company=synthetic",
                "vat",
                op,
                "--server",
                &origin,
                "--allow-loopback-http",
                "--cookie-file",
            ])
            .arg(&cookie)
            .arg("--input")
            .arg(&file)
            .args(["--actor", "user:synthetic", "--json"])
            .env("USER", "forged")
            .output()
            .unwrap();
        let (header, body) = thread.join().unwrap();
        assert!(header.starts_with("POST /api/companies/synthetic/original/vat/"));
        assert_eq!(
            body,
            json!({"payload":input,"confirm":true,"actor":"user:synthetic"})
        );
        assert_eq!(
            out.status.code(),
            Some(exit),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        if exit == 2 {
            assert_eq!(
                String::from_utf8_lossy(&out.stderr),
                "Could not resolve document for invoiceNo INV\n"
            );
        } else {
            assert_eq!(serde_json::from_slice::<Value>(&out.stdout).unwrap(), core);
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
