mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    thread,
};
fn success() -> Value {
    json!({"ok":true,"entryId":17,"entryNo":"JE-000017","entryHash":"a".repeat(64),"errors":[],"appliedRules":["DK-BOOKKEEPING-APPEND-ONLY-001"],"idempotency":{"replayed":false,"receiptId":81,"createdAt":"2026-10-22T14:00:00Z","expiresAt":"2026-10-29T14:00:00Z"}})
}
fn body() -> Vec<u8> {
    serde_json::to_vec(&json!({"input":{"transactionDate":"2026-10-22","text":"Synthetic original journal","lines":[{"accountNo":"1000","debitAmount":10.5},{"accountNo":"2000","creditAmount":10.5}]},"confirm":true,"idempotencyKey":"caller-key"})).unwrap()
}
async fn request(core: Value, status: &'static str, reverse: bool) -> Result<Value, Error> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let api = ApiClient::new(
        &format!("http://{}", listener.local_addr().unwrap()),
        true,
        Some("session=private".into()),
    )
    .unwrap();
    let mut wire = body();
    if reverse {
        wire=serde_json::to_vec(&json!({"entryNo":"JE-000016","transactionDate":"2026-10-22","reason":"Synthetic correction","confirm":true,"idempotencyKey":"caller-key"})).unwrap();
    }
    let expected = wire.clone();
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut b = [0; 4096];
            let n = socket.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(i) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..i]).to_lowercase();
                let len = headers
                    .lines()
                    .find_map(|s| s.strip_prefix("content-length: "))
                    .unwrap()
                    .parse::<usize>()
                    .unwrap();
                if bytes.len() >= i + 4 + len {
                    assert_eq!(&bytes[i + 4..], expected);
                    break;
                }
            }
        }
        assert!(String::from_utf8_lossy(&bytes).starts_with(if reverse {
            "POST /api/companies/synthetic/journal/original/keyed-reverse HTTP/1.1"
        } else {
            "POST /api/companies/synthetic/journal/original/keyed-post HTTP/1.1"
        }));
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{core}",
            core.to_string().len()
        );
        socket.write_all(response.as_bytes()).unwrap();
        drop(socket);
        listener.set_nonblocking(true).unwrap();
        let until = std::time::Instant::now() + std::time::Duration::from_millis(100);
        while std::time::Instant::now() < until {
            assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
            thread::sleep(std::time::Duration::from_millis(5));
        }
    });
    let outcome = if reverse {
        api.journal_original_keyed_reverse("synthetic", wire, true)
            .await
    } else {
        api.journal_original_keyed_post("synthetic", wire, true)
            .await
    };
    server.join().unwrap();
    outcome
}
#[tokio::test]
async fn journal_original_keyed_success_replay_receipts_and_reverse() {
    for replayed in [false, true] {
        let mut core = success();
        core["idempotency"]["replayed"] = json!(replayed);
        assert_eq!(request(core.clone(), "200 OK", false).await.unwrap(), core);
    }
    let mut core = success();
    core["originalEntryId"] = json!(16);
    assert_eq!(request(core.clone(), "200 OK", true).await.unwrap(), core);
}
#[tokio::test]
async fn journal_original_keyed_strict_metadata_and_no_retry_unknown() {
    for (field, value) in [
        ("receiptId", json!(9007199254740992_u64)),
        ("receiptId", json!("81")),
        ("replayed", json!("false")),
        ("createdAt", json!("invented")),
        ("expiresAt", json!("2026-10-20T14:00:00Z")),
        ("extra", json!(true)),
    ] {
        let mut core = success();
        core["idempotency"][field] = value;
        assert_eq!(
            request(core, "200 OK", false).await,
            Err(Error::OutcomeUnknown)
        );
    }
    let mut core = success();
    core.as_object_mut().unwrap().remove("idempotency");
    assert_eq!(
        request(core, "200 OK", false).await,
        Err(Error::OutcomeUnknown)
    );
    assert_eq!(
        request(json!({"private":"secret"}), "503 Unavailable", false).await,
        Err(Error::OutcomeUnknown)
    );
    assert_eq!(
        request(json!({"private":"secret"}), "403 Forbidden", false).await,
        Err(Error::Refused { status: 403 })
    );
}
#[tokio::test]
async fn journal_original_keyed_refusals_preserve_code_errors_rules_without_receipts() {
    for code in [
        "IDEMPOTENCY_CONFLICT",
        "RESTORED_KEY_RECONCILIATION_REQUIRED",
        "journal_selection_usage",
    ] {
        let core = json!({"ok":false,"code":code,"errors":["Synthetic refusal"],"appliedRules":["native-guard"]});
        assert_eq!(
            request(core.clone(), "422 Unprocessable Entity", true)
                .await
                .unwrap(),
            core
        );
        let mut bad = core;
        bad["idempotency"] = success()["idempotency"].clone();
        assert_eq!(
            request(bad, "422 Unprocessable Entity", true).await,
            Err(Error::Refused { status: 422 })
        );
    }
}
#[tokio::test]
async fn journal_original_keyed_required_key_and_confirmation_never_send() {
    let api = ApiClient::new("http://127.0.0.1:1", true, Some("session=private".into())).unwrap();
    assert!(matches!(
        api.journal_original_keyed_post("synthetic", body(), false)
            .await,
        Err(Error::InvalidInput(_))
    ));
    for key in [
        None,
        Some(json!("")),
        Some(json!("  ")),
        Some(json!("😀".repeat(65))),
        Some(json!(1)),
    ] {
        let mut input: Value = serde_json::from_slice(&body()).unwrap();
        input.as_object_mut().unwrap().remove("idempotencyKey");
        if let Some(key) = key {
            input["idempotencyKey"] = key;
        }
        assert!(matches!(
            api.journal_original_keyed_post("synthetic", serde_json::to_vec(&input).unwrap(), true)
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
}
