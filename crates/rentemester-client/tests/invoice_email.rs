mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error, MAX_INVOICE_EMAIL_BODY};
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
            "HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
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
fn send() -> Value {
    json!({"invoiceDocumentId":7,"to":"buyer@example.invalid","kind":"invoice","confirm":true})
}
fn retry() -> Value {
    json!({"messageId":format!("<{}@rentemester.local>","a".repeat(32)),"expectedStateHash":"b".repeat(64),"reason":" Reviewed SMTP refusal ","confirm":true})
}
fn reconcile() -> Value {
    let mut v = retry();
    v["resolution"] = json!("not_accepted");
    v["evidence"] = json!(" SMTP log reference synthetic-001 ");
    v
}
#[tokio::test]
async fn fixed_routes_exact_bytes_full_canonical_refusals_and_single_attempt() {
    const ACCEPTED: &str = r#"{"ok":true,"delivery":{"state":"accepted","duplicate":true,"history":[{"future":"kept"}],"amount":"90071992547409.99"}}"#;
    const UNKNOWN: &str = r#"{"ok":false,"code":"delivery_unknown","delivery":{"state":"unknown","messageId":"canonical","history":[{"future":"kept"}]}}"#;
    const REFUSED: &str = r#"{"ok":false,"code":"delivery_refused","delivery":{"state":"refused","duplicate":false,"history":[]}}"#;
    for (action, path, status, reply) in [
        (0, "invoices/send-email", "200 OK", ACCEPTED),
        (0, "invoices/send-email", "409 Conflict", UNKNOWN),
        (
            2,
            "invoices/email-deliveries/retry",
            "422 Unprocessable",
            REFUSED,
        ),
        (1, "invoices/email-deliveries/reconcile", "200 OK", ACCEPTED),
    ] {
        let (origin, rx, t) = server(status, reply);
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let v = match action {
            0 => send(),
            1 => reconcile(),
            _ => retry(),
        };
        let bytes = format!(" {v}\n").into_bytes();
        let out = match action {
            0 => {
                api.invoice_email_send("synthetic", bytes.clone(), true)
                    .await
            }
            1 => {
                api.invoice_email_reconcile("synthetic", bytes.clone(), true)
                    .await
            }
            _ => {
                api.invoice_email_retry("synthetic", bytes.clone(), true)
                    .await
            }
        }
        .unwrap();
        assert_eq!(out, serde_json::from_str::<Value>(reply).unwrap());
        let wire = rx.recv().unwrap();
        assert!(
            wire.starts_with(format!("POST /api/companies/synthetic/{path} HTTP/1.1").as_bytes())
        );
        assert!(wire.ends_with(&bytes));
        assert_eq!(t.join().unwrap(), 1);
    }
    let (origin, rx, t) = server(
        "200 OK",
        r#"{"ok":true,"invoiceEmail":{"configured":false,"rows":[]}}"#,
    );
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.invoice_email_status("synthetic").await.unwrap()["invoiceEmail"]["configured"],
        false
    );
    let wire = rx.recv().unwrap();
    assert!(wire.starts_with(b"GET /api/companies/synthetic/invoices/email-deliveries HTTP/1.1"));
    assert_eq!(t.join().unwrap(), 1);
}
#[tokio::test]
async fn ambiguous_transport_status_and_body_do_not_become_safe_retry_or_leak() {
    for (status, reply) in [
        ("503 Unavailable", r#"{"credentials":"must-not-leak"}"#),
        ("200 OK", "truncated"),
        ("200 OK", r#"{"ok":true}"#),
        ("409 Conflict", r#"{"ok":false,"code":"delivery_unknown"}"#),
    ] {
        let (origin, rx, t) = server(status, reply);
        let api = ApiClient::new(&origin, true, None).unwrap();
        assert_eq!(
            api.invoice_email_send("synthetic", send().to_string().into_bytes(), true)
                .await,
            Err(Error::OutcomeUnknown)
        );
        rx.recv().unwrap();
        assert_eq!(t.join().unwrap(), 1);
    }
    let (origin, rx, t) = server(
        "422 Unprocessable",
        r#"{"ok":false,"code":"stale_source","private":"must-not-leak"}"#,
    );
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.invoice_email_retry("synthetic", retry().to_string().into_bytes(), true)
            .await,
        Err(Error::Refused { status: 422 })
    );
    rx.recv().unwrap();
    assert_eq!(t.join().unwrap(), 1);
}
#[tokio::test]
async fn explicit_confirm_identity_injection_hash_and_utf8_bounds_refuse_before_http() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for v in [
        json!({"invoiceDocumentId":7,"to":"buyer@example.invalid","confirm":false}),
        json!({"invoiceDocumentId":7,"to":"buyer@example.invalid\r\nBcc: hidden@example.invalid","confirm":true}),
        json!({"invoiceDocumentId":9007199254740992u64,"to":"buyer@example.invalid","confirm":true}),
        json!({"invoiceDocumentId":7,"to":"buyer@example.invalid","smtpPassword":"must-not-echo","confirm":true}),
    ] {
        assert!(matches!(
            api.invoice_email_send("synthetic", v.to_string().into_bytes(), true)
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
    for (key, value) in [
        ("reason", json!("short")),
        ("reason", json!("ø".repeat(1001))),
        ("reason", json!("reviewed\nSMTP failure")),
        ("messageId", json!("arbitrary")),
        ("expectedStateHash", json!("A".repeat(64))),
    ] {
        let mut v = retry();
        v[key] = value;
        assert!(matches!(
            api.invoice_email_retry("synthetic", v.to_string().into_bytes(), true)
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
    let mut v = reconcile();
    v["evidence"] = json!("ø".repeat(2001));
    assert!(matches!(
        api.invoice_email_reconcile("synthetic", v.to_string().into_bytes(), true)
            .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.invoice_email_send("synthetic", send().to_string().into_bytes(), false)
            .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.invoice_email_send("synthetic", vec![b' '; MAX_INVOICE_EMAIL_BODY + 1], true)
            .await,
        Err(Error::InvalidInput(_))
    ));
}
