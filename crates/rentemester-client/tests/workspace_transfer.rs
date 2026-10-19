use rentemester_client::{
    ApiClient, Error, MAX_WORKSPACE_TRANSFER_BODY, MAX_WORKSPACE_TRANSFER_PROOF_BODY,
    WORKSPACE_BACKUP_CONTENT_TYPE,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
    time::Duration,
};
fn server(
    status: &'static str,
    reply: &'static str,
    mime: &'static str,
) -> (String, mpsc::Receiver<Vec<u8>>, thread::JoinHandle<usize>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut bytes = vec![];
        loop {
            let mut b = [0; 4096];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let h = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let len = h
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .map(|n| n.parse::<usize>().unwrap())
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + len {
                    break;
                }
            }
        }
        tx.send(bytes).unwrap();
        write!(
            s,
            "HTTP/1.1 {status}\r\nContent-Type:{mime}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
            reply.len()
        )
        .unwrap();
        drop(s);
        l.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(60));
        1 + l.incoming().take_while(Result::is_ok).count()
    });
    (origin, rx, t)
}
fn request(action: &str) -> Value {
    let mut v = json!({"expectedStateHash":"a".repeat(64),"confirm":true});
    if action != "export" {
        v["reason"] = json!(" Explicit transfer review ");
    }
    if action == "freeze" {
        v["destination"] = json!("https://synthetic-destination.example.invalid");
    }
    if action == "release" {
        v["challenge"] = json!({"version":1,"transferId":"1".repeat(32),"sourceInstanceId":"2".repeat(32),"workspaceId":"synthetic-workspace","archiveSha256":"b".repeat(64),"destinationInstanceId":"3".repeat(32),"destination":"https://synthetic-destination.example.invalid","companies":[{"slug":"synthetic","ledgerUuid":"4".repeat(32)}]});
    }
    if action == "activate" {
        v["release"] = json!(r#"{"signed":"opaque-retained-source-release"}"#);
    }
    if action == "receipt" {
        v["receiptId"] = json!(7);
    }
    v
}
#[tokio::test]
async fn all_eight_fixed_routes_preserve_exact_scopes_bytes_and_canonical_state() {
    const JSON: &str = r#"{"ok":true,"transfer":{"mode":"staged","stateHash":"canonical","future":{"amount":"90071992547409.99"}},"restoredReceipts":{"rows":[{"sourcePrincipal":"historical","receiptId":7}]}}"#;
    for (action, path, write) in [
        ("status", "/api/workspace/transfer", false),
        ("freeze", "/api/workspace/transfer/freeze", true),
        ("cancel", "/api/workspace/transfer/cancel", true),
        ("release", "/api/workspace/transfer/release", true),
        ("activate", "/api/workspace/transfer/activate", true),
        (
            "receipts",
            "/api/companies/synthetic/restore/receipts",
            false,
        ),
        (
            "receipt",
            "/api/companies/synthetic/restore/receipts/reconcile",
            true,
        ),
        ("export", "/api/workspace/transfer/export", true),
    ] {
        let binary = action == "export";
        let (origin, rx, t) = server(
            "200 OK",
            if binary { "signed\r\nexact" } else { JSON },
            if binary {
                WORKSPACE_BACKUP_CONTENT_TYPE
            } else {
                "application/json"
            },
        );
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let bytes = format!(" {}\n", request(action)).into_bytes();
        let value = match action {
            "status" => api.workspace_transfer_status().await,
            "freeze" => api.workspace_transfer_freeze(bytes.clone(), true).await,
            "cancel" => api.workspace_transfer_cancel(bytes.clone(), true).await,
            "release" => api.workspace_transfer_release(bytes.clone(), true).await,
            "activate" => api.workspace_transfer_activate(bytes.clone(), true).await,
            "export" => api.workspace_transfer_export(bytes.clone(), true).await,
            "receipts" => api.restored_receipts("synthetic").await,
            _ => {
                api.reconcile_restored_receipt("synthetic", bytes.clone(), true)
                    .await
            }
        }
        .unwrap();
        if binary {
            assert_eq!(
                value["backup"]["contentType"],
                WORKSPACE_BACKUP_CONTENT_TYPE
            );
            assert_eq!(value["backup"]["bytes"], json!(b"signed\r\nexact".to_vec()));
        } else {
            assert_eq!(value, serde_json::from_str::<Value>(JSON).unwrap());
        }
        let wire = rx.recv().unwrap();
        assert!(wire.starts_with(
            format!("{} {path} HTTP/1.1", if write { "POST" } else { "GET" }).as_bytes()
        ));
        if write {
            assert!(wire.ends_with(&bytes));
        }
        assert_eq!(t.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn refusal_codes_survive_but_unknown_write_never_becomes_success_or_replay() {
    const REFUSAL: &str = r#"{"ok":false,"code":"source_already_moved","transfer":{"mode":"moved","release":"retained-signed-receipt","history":[{"future":"preserved"}]}}"#;
    let (origin, rx, t) = server("409 Conflict", REFUSAL, "application/json");
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.workspace_transfer_release(request("release").to_string().into_bytes(), true)
            .await
            .unwrap(),
        serde_json::from_str::<Value>(REFUSAL).unwrap()
    );
    rx.recv().unwrap();
    assert_eq!(t.join().unwrap(), 1);
    for (status, reply, mime) in [
        (
            "503 Unavailable",
            r#"{"secret":"not-for-errors"}"#,
            "application/json",
        ),
        ("200 OK", "truncated", "application/json"),
        (
            "200 OK",
            r#"{"ok":false,"code":"outcome_unconfirmed"}"#,
            "application/json",
        ),
    ] {
        let (origin, rx, t) = server(status, reply, mime);
        let api = ApiClient::new(&origin, true, None).unwrap();
        assert_eq!(
            api.workspace_transfer_freeze(request("freeze").to_string().into_bytes(), true)
                .await,
            Err(Error::OutcomeUnknown)
        );
        rx.recv().unwrap();
        assert_eq!(t.join().unwrap(), 1);
    }
    let (origin, rx, t) = server("200 OK", "signed-but-wrong-mime", "text/plain");
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.workspace_transfer_export(request("export").to_string().into_bytes(), true)
            .await,
        Err(Error::OutcomeUnknown)
    );
    rx.recv().unwrap();
    assert_eq!(t.join().unwrap(), 1);
}
#[tokio::test]
async fn input_guards_preserve_source_proof_and_reject_implicit_authority_before_http() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for (mut v, key, value) in [
        (request("freeze"), "confirm", json!(false)),
        (request("freeze"), "reason", json!("short")),
        (request("freeze"), "destination", json!("short")),
        (
            request("freeze"),
            "expectedStateHash",
            json!("A".repeat(64)),
        ),
        (request("freeze"), "reason", json!("ø".repeat(1001))),
        (request("freeze"), "privateKey", json!("never-echo")),
    ] {
        v[key] = value;
        assert!(matches!(
            api.workspace_transfer_freeze(v.to_string().into_bytes(), true)
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
    let mut v = request("receipt");
    v["receiptId"] = json!(9007199254740992u64);
    assert!(matches!(
        api.reconcile_restored_receipt("synthetic", v.to_string().into_bytes(), true)
            .await,
        Err(Error::InvalidInput(_))
    ));
    let mut v = request("activate");
    v["release"] = json!("x".repeat(16385));
    assert!(matches!(
        api.workspace_transfer_activate(v.to_string().into_bytes(), true)
            .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.workspace_transfer_freeze(request("freeze").to_string().into_bytes(), false)
            .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.workspace_transfer_export(vec![b' '; MAX_WORKSPACE_TRANSFER_BODY + 1], true)
            .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.workspace_transfer_release(vec![b' '; MAX_WORKSPACE_TRANSFER_PROOF_BODY + 1], true)
            .await,
        Err(Error::InvalidInput(_))
    ));
    // A signed release can exceed the ordinary16KiB JSON body once escaped; only
    // activation's32KiB envelope applies. Client does not verify or rewrite it.
    let mut v = request("activate");
    v["release"] = json!("\"".repeat(10000));
    let bytes = v.to_string().into_bytes();
    assert!(
        bytes.len() > MAX_WORKSPACE_TRANSFER_BODY
            && bytes.len() < MAX_WORKSPACE_TRANSFER_PROOF_BODY
    );
    let (origin, rx, t) = server(
        "200 OK",
        r#"{"ok":true,"transfer":{"mode":"active"}}"#,
        "application/json",
    );
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.workspace_transfer_activate(bytes.clone(), true)
            .await
            .unwrap()["transfer"]["mode"],
        "active"
    );
    assert!(rx.recv().unwrap().ends_with(&bytes));
    assert_eq!(t.join().unwrap(), 1);
}
