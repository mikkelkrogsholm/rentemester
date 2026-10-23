mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error, MAX_PAYABLE_CORRECTION_BODY};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
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

fn request(apply: bool) -> Vec<u8> {
    let mut v = json!({"documentId":1,"bankTransactionId":2,"billDate":"2026-01-10","dueDate":"2026-02-10","expenseAccountNo":"3000","vatTreatment":null,"vendorId":null,"note":"synthetic explicit note"});
    if apply {
        for (k, value) in [
            ("planHash", json!("a".repeat(64))),
            ("reason", json!("synthetic correction")),
            ("idempotencyKey", json!("caller original key")),
            ("confirm", json!(true)),
        ] {
            v[k] = value;
        }
    }
    format!(" {v}\n").into_bytes()
}
#[tokio::test]
async fn readonly_and_apply_send_exact_body_and_keep_full_canonical_result() {
    const JSON: &str = r#"{"ok":true,"correction":{"id":7,"idempotent":true,"future":{"amount":"90071992547409.99"}},"idempotency":{"replayed":true}}"#;
    for apply in [false, true] {
        let (origin, rx, t) = server("200 OK", JSON, "application/json");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let bytes = request(apply);
        assert_eq!(
            api.direct_bank_payable_correction("synthetic", bytes.clone(), apply, apply)
                .await
                .unwrap(),
            serde_json::from_str::<Value>(JSON).unwrap()
        );
        let wire = rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let end = wire.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        assert_eq!(&wire[end..], bytes);
        assert!(String::from_utf8_lossy(&wire).starts_with(&format!(
            "POST /api/companies/synthetic/payables/direct-bank-correction/{} HTTP/1.1",
            if apply { "apply" } else { "plan" }
        )));
        assert_eq!(t.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn uncertain_apply_never_retries_and_refusal_retains_details() {
    for (status, reply, unknown) in [
        (
            "422 Unprocessable Entity",
            r#"{"ok":false,"code":"plan_stale","errors":["PLAN_HASH_MISMATCH"],"detail":{"field":"source"}}"#,
            false,
        ),
        (
            "503 Service Unavailable",
            r#"{"ok":false,"code":"outcome_unknown"}"#,
            true,
        ),
        (
            "408 Request Timeout",
            r#"{"ok":false,"code":"timeout"}"#,
            true,
        ),
    ] {
        let (origin, rx, t) = server(status, reply, "application/json");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let result = api
            .direct_bank_payable_correction("synthetic", request(true), true, true)
            .await;
        if unknown {
            assert_eq!(result, Err(Error::OutcomeUnknown));
        } else {
            assert_eq!(
                result.unwrap(),
                serde_json::from_str::<Value>(reply).unwrap()
            );
        }
        rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(t.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn malformed_or_unconfirmed_requests_are_rejected_before_io() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for bytes in [b"{}".to_vec(), vec![b' '; MAX_PAYABLE_CORRECTION_BODY + 1]] {
        assert!(matches!(
            api.direct_bank_payable_correction("synthetic", bytes, true, true)
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.direct_bank_payable_correction("synthetic", request(true), true, false)
            .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.direct_bank_payable_correction("synthetic", request(false), false, true)
            .await,
        Err(Error::InvalidInput(_))
    ));
    let mut v: Value = serde_json::from_slice(&request(false)).unwrap();
    v["actor"] = json!("user:forged");
    assert!(matches!(
        api.direct_bank_payable_correction(
            "synthetic",
            serde_json::to_vec(&v).unwrap(),
            false,
            false
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
}
