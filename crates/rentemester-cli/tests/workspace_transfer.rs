use common::TcpListener;
#[path = "../../rentemester-client/tests/common/mod.rs"]
mod common;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    thread,
    time::Duration,
};
const BIN: &str = env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn transfer_discovery_exact_workspace_company_scope_and_confirmation() {
    let out = Command::new(BIN)
        .args(["remote", "--help"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    for name in [
        "workspace-transfer-status",
        "workspace-transfer-freeze",
        "workspace-transfer-cancel",
        "workspace-transfer-export",
        "workspace-transfer-release",
        "workspace-transfer-activate",
        "restored-receipts",
        "reconcile-restored-receipt",
    ] {
        assert!(v["commands"].as_array().unwrap().contains(&json!(name)));
    }
    assert_eq!(
        v["workspaceTransfer"]["bodySchemas"]["workspace-transfer-export"]["required"],
        json!(["expectedStateHash", "confirm"])
    );
    assert_eq!(
        v["workspaceTransfer"]["bodySchemas"]["workspace-transfer-release"]["properties"]["challenge"]
            ["properties"]["companies"]["maxItems"],
        64
    );
    for args in [
        vec!["workspace-transfer-status", "--confirm"],
        vec!["workspace-transfer-status", "--company", "synthetic"],
        vec!["workspace-transfer-freeze"],
        vec![
            "workspace-transfer-export",
            "--confirm",
            "--company",
            "synthetic",
        ],
        vec!["restored-receipts"],
        vec!["reconcile-restored-receipt", "--company", "synthetic"],
        vec!["workspace-transfer-activate", "--confirm", "--id", "1"],
    ] {
        assert_eq!(
            Command::new(BIN)
                .arg("remote")
                .args(args)
                .output()
                .unwrap()
                .status
                .code(),
            Some(2)
        );
    }
}
#[cfg(unix)]
#[test]
fn transfer_cli_preserves_canonical_refusal_export_and_selected_receipt_mapping() {
    use std::os::unix::fs::OpenOptionsExt;
    let cookie =
        std::env::temp_dir().join(format!("rm-transfer-cli-{}.cookie", std::process::id()));
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&cookie)
        .unwrap()
        .write_all(b"session=synthetic-private")
        .unwrap();
    for (command, path, body, status, mime, response, exit, company) in [
        (
            "workspace-transfer-freeze",
            "/api/workspace/transfer/freeze",
            json!({"expectedStateHash":"a".repeat(64),"destination":"https://synthetic-target.example.invalid","reason":" explicit source review ","confirm":true}),
            "409 Conflict",
            "application/json",
            r#"{"ok":false,"code":"source_already_moved","transfer":{"mode":"moved","release":"retained"}}"#,
            1,
            false,
        ),
        (
            "workspace-transfer-export",
            "/api/workspace/transfer/export",
            json!({"expectedStateHash":"a".repeat(64),"confirm":true}),
            "200 OK",
            "application/vnd.rentemester.native-backup+json",
            "signed\r\nbytes",
            0,
            false,
        ),
        (
            "reconcile-restored-receipt",
            "/api/companies/synthetic/restore/receipts/reconcile",
            json!({"receiptId":7,"expectedStateHash":"a".repeat(64),"reason":" exact source receipt review ","confirm":true}),
            "200 OK",
            "application/json",
            r#"{"ok":true,"restoredReceipts":{"rows":[{"receiptId":7,"mapped":true,"future":"kept"}]}}"#,
            0,
            true,
        ),
        (
            "workspace-transfer-cancel",
            "/api/workspace/transfer/cancel",
            json!({"expectedStateHash":"a".repeat(64),"reason":" explicit pre-offer cancel ","confirm":true}),
            "503 Unavailable",
            "application/json",
            r#"{"secret":"must-not-echo"}"#,
            1,
            false,
        ),
    ] {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", l.local_addr().unwrap());
        let bytes = format!(" {body}\n");
        let expected = bytes.clone();
        let t = thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut wire = vec![];
            loop {
                let mut b = [0; 4096];
                let n = s.read(&mut b).unwrap();
                assert!(n > 0);
                wire.extend_from_slice(&b[..n]);
                if wire.ends_with(expected.as_bytes()) {
                    break;
                }
            }
            assert!(wire.starts_with(format!("POST {path} HTTP/1.1").as_bytes()));
            write!(s,"HTTP/1.1 {status}\r\nContent-Type:{mime}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{response}",response.len()).unwrap();
            drop(s);
            let l = l.into_inner();
            l.set_nonblocking(true).unwrap();
            thread::sleep(Duration::from_millis(60));
            assert!(l.accept().is_err());
        });
        let mut args = vec![
            "remote",
            command,
            "--server",
            &origin,
            "--allow-loopback-http",
            "--cookie-file",
            cookie.to_str().unwrap(),
            "--confirm",
        ];
        if company {
            args.extend(["--company", "synthetic"]);
        }
        let mut child = Command::new(BIN)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(bytes.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert_eq!(out.status.code(), Some(exit));
        assert!(out.stderr.is_empty());
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(!v.to_string().contains("synthetic-private"));
        assert!(!v.to_string().contains("must-not-echo"));
        if command.ends_with("export") {
            assert_eq!(v["data"]["backup"]["bytes"], json!(response.as_bytes()));
        } else if status.starts_with("503") {
            assert_eq!(v["code"], "outcome_unknown");
        } else {
            assert_eq!(v["data"], serde_json::from_str::<Value>(response).unwrap());
        }
        t.join().unwrap();
    }
    std::fs::remove_file(cookie).unwrap();
}
