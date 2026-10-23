mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error, ReminderClaimMutation as Op};
use std::{
    io::{Read, Write},
    thread,
    time::Duration,
};
fn server(
    path: &'static str,
    body: &str,
    status: &'static str,
) -> (String, thread::JoinHandle<()>) {
    let body = body.to_owned();
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
        assert!(wire.ends_with(&body));
        assert!(wire.to_lowercase().contains("cookie: session=synthetic"));
        let reply = r#"{"ok":true,"company":{"payment":null},"accounts":{"accountRoles":{"status":"incomplete","missing":["bank"],"proposals":[]}},"resolution":{"ok":true,"role":"input_vat","accountNo":"V","version":2,"confirmationSource":"explicit"}}"#;
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
async fn exact_reminder_bodies_transport_refusal_unknown_and_no_retry() {
    for (op, path, body) in [
        (
            Op::Register,
            "POST /api/companies/synthetic/invoices/reminders/register HTTP/1.1",
            r#"{ "invoiceDocumentId":7,"reminderDate":"2026-01-03","feeAmount":50,"note":" reviewed ","confirm":true }"#,
        ),
        (
            Op::Post,
            "POST /api/companies/synthetic/invoices/reminders/post HTTP/1.1",
            r#"{"invoiceDocumentId":7,"reminderId":2,"receivableAccountNo":"R","reminderIncomeAccountNo":"F","confirm":true}"#,
        ),
        (
            Op::Settle,
            "POST /api/companies/synthetic/invoices/claims/settle HTTP/1.1",
            r#"{"invoiceDocumentId":7,"bankTransactionId":8,"confirm":true}"#,
        ),
    ] {
        for status in ["200 OK", "422 Unprocessable Entity", "503 Unavailable"] {
            let (origin, t) = server(path, body, status);
            let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
            let out = api
                .reminder_claim_mutation("synthetic", op, body.as_bytes().to_vec(), true)
                .await;
            match status {
                "200 OK" => {
                    assert!(out.unwrap()["company"]["payment"].is_null());
                }
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
async fn reads_are_not_unknown_mutations_and_malformed_confirmation_does_not_send() {
    let (origin, t) = server(
        "GET /api/companies/synthetic/invoices/7/reminder-claims HTTP/1.1",
        "",
        "503 Unavailable",
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.invoice_reminder_claims("synthetic", 7)
            .await
            .unwrap_err(),
        Error::Refused { status: 503 }
    );
    t.join().unwrap();
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    assert!(matches!(
        api.invoice_reminder_claims("synthetic", 0).await,
        Err(Error::InvalidInput(_))
    ));
    for bytes in [br#"{"invoiceDocumentId":7,"bankTransactionId":8,"confirm":false}"#.to_vec(),br#"{"invoiceDocumentId":7,"bankTransactionId":8,"bankTransactionReference":"mixed","confirm":true}"#.to_vec(),vec![b' ';16385]]{assert!(matches!(api.reminder_claim_mutation("synthetic",Op::Settle,bytes,true).await,Err(Error::InvalidInput(_))));}
}
