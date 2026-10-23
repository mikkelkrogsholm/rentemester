mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error, MAX_RECURRING_BODY, RecurringInvoiceMutation as Op};
use std::{
    io::{Read, Write},
    thread,
    time::Duration,
};
const CREATE: &str = r#"{ "name":"Synthetic recurring", "interval":"monthly","firstIssueDate":"2026-01-01","paymentTermsDays":0,"vatRatePercent":25,"buyer":{"name":"Synthetic buyer"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":12.34}],"confirm":true }
"#;
fn server(
    path: &'static str,
    body: &'static str,
    status: &'static str,
) -> (String, thread::JoinHandle<()>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut bytes = vec![];
        loop {
            let mut chunk = [0; 4096];
            let n = s.read(&mut chunk).unwrap();
            bytes.extend_from_slice(&chunk[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let h = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let size = h
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .map(|s| s.parse::<usize>().unwrap())
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + size {
                    break;
                }
            }
        }
        let wire = String::from_utf8(bytes).unwrap();
        assert!(wire.starts_with(path));
        assert!(wire.ends_with(body));
        assert!(wire.to_lowercase().contains("cookie: session=synthetic"));
        let reply = r#"{"ok":true,"generation":{"created":false,"documentId":7,"additionalFutureEvidence":{"retained":true}}}"#;
        write!(
            s,
            "HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
            reply.len()
        )
        .unwrap();
        drop(s);
        l.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(70));
        assert!(l.accept().is_err(), "must never retry");
    });
    (origin, t)
}
#[tokio::test]
async fn all_mutations_forward_exact_body_once_with_typed_outcomes() {
    for (op, path, body) in [
        (
            Op::Create,
            "POST /api/companies/synthetic/recurring-invoices HTTP/1.1",
            CREATE,
        ),
        (
            Op::Generate(7),
            "POST /api/companies/synthetic/recurring-invoices/7/generate HTTP/1.1",
            "{ \"asOfDate\":\"2026-01-01\",\"confirm\":true }\n",
        ),
        (
            Op::Retire(7),
            "POST /api/companies/synthetic/recurring-invoices/7/retire HTTP/1.1",
            "{\"reason\":\"  Reviewed retire  \",\"confirm\":true}",
        ),
    ] {
        for status in ["200 OK", "422 Unprocessable Entity", "503 Unavailable"] {
            let (origin, t) = server(path, body, status);
            let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
            let out = api
                .recurring_invoice_mutation("synthetic", op, body.as_bytes().to_vec(), true)
                .await;
            match status {
                "200 OK" => assert_eq!(
                    out.unwrap()["generation"]["additionalFutureEvidence"]["retained"],
                    true
                ),
                "422 Unprocessable Entity" => {
                    assert_eq!(out.unwrap_err(), Error::Refused { status: 422 })
                }
                _ => assert_eq!(out.unwrap_err(), Error::OutcomeUnknown),
            }
            t.join().unwrap();
        }
    }
}
#[tokio::test]
async fn list_has_no_query_and_read_refusal_is_known() {
    let (origin, t) = server(
        "GET /api/companies/synthetic/recurring-invoices HTTP/1.1",
        "",
        "503 Unavailable",
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.recurring_invoices("synthetic").await.unwrap_err(),
        Error::Refused { status: 503 }
    );
    t.join().unwrap();
}
#[tokio::test]
async fn malformed_confirmation_key_selector_and_size_refuse_without_network() {
    let api = ApiClient::new("http://127.0.0.1:9", true, Some("session=synthetic".into())).unwrap();
    for (op, body, confirm) in [
        (Op::Create, CREATE.to_owned(), false),
        (
            Op::Generate(0),
            "{\"asOfDate\":\"2026-01-01\",\"confirm\":true}".into(),
            true,
        ),
        (
            Op::Generate(7),
            "{\"asOfDate\":\"2026-01-01\",\"confirm\":true,\"idempotencyKey\":\"invented\"}".into(),
            true,
        ),
        (Op::Retire(7), "{\"confirm\":false}".into(), true),
        (Op::Create, " ".repeat(MAX_RECURRING_BODY + 1), true),
    ] {
        assert!(matches!(
            api.recurring_invoice_mutation("synthetic", op, body.into_bytes(), confirm)
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
}
