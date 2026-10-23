mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error, MAX_BANK_CORRECTION_BODY};
use std::{
    io::{Read, Write},
    thread,
    time::Duration,
};
fn http(status: &'static str) -> (String, thread::JoinHandle<usize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut data = Vec::new();
        loop {
            let mut chunk = [0; 4096];
            let n = stream.read(&mut chunk).unwrap();
            data.extend_from_slice(&chunk[..n]);
            if n == 0 {
                break;
            }
            if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&data[..end]);
                let size = headers
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length: ")
                            .map(|s| s.parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                if data.len() >= end + 4 + size {
                    break;
                }
            }
        }
        let wire = String::from_utf8(data).unwrap();
        if wire.starts_with("POST") {
            assert!(wire.starts_with(
                "POST /api/companies/synthetic/bank/reconciliation-correction HTTP/1.1"
            ));
            assert!(wire.ends_with(BODY));
            assert!(wire.contains("  nøgle-🚀  "));
        } else {
            assert!(wire.starts_with("GET /api/companies/synthetic/bank/reconciliation-correction-plan?bankTransactionId=7&replacementJournalEntryId=8 HTTP/1.1"));
        }
        assert!(wire.to_lowercase().contains("cookie: session=synthetic"));
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Length: 11\r\nConnection:close\r\n\r\n{{\"ok\":true}}"
        )
        .unwrap();
        drop(stream);
        listener.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(100));
        let mut count = 1;
        while listener.accept().is_ok() {
            count += 1;
        }
        count
    });
    (origin, task)
}

const BODY: &str = r#"{ "bankTransactionId":7,"replacementJournalEntryId":8,"expectedReconciliationId":"append-only:1","planHash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","reason":"  Synthetic correction  ","idempotencyKey":"  nøgle-🚀  ","confirm":true }
"#;
#[tokio::test]
async fn exact_confirmed_link_is_sent_once_and_refusal_differs_from_unknown() {
    for (status, expected) in [
        ("200 OK", None),
        ("503 Unavailable", Some(Error::OutcomeUnknown)),
        (
            "422 Unprocessable Entity",
            Some(Error::Refused { status: 422 }),
        ),
    ] {
        let (origin, task) = http(status);
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let result = api
            .correct_bank_reconciliation("synthetic", BODY.as_bytes().to_vec(), true)
            .await;
        if let Some(error) = expected {
            assert_eq!(result.unwrap_err(), error);
        } else {
            assert!(result.is_ok());
        }
        assert_eq!(task.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn candidate_query_is_exact_and_read_errors_never_claim_unknown_mutation() {
    for status in ["200 OK", "503 Unavailable"] {
        let (origin, task) = http(status);
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let result = api
            .bank_reconciliation_correction_plan("synthetic", 7, 8)
            .await;
        if status.starts_with("200") {
            assert!(result.is_ok());
        } else {
            assert_eq!(result.unwrap_err(), Error::Refused { status: 503 });
        }
        assert_eq!(task.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn selectors_envelope_confirmation_and_native_size_fail_before_transport() {
    let api = ApiClient::new("http://127.0.0.1:9", true, Some("session=synthetic".into())).unwrap();
    for id in [0, -1, 9007199254740992] {
        assert!(matches!(
            api.bank_reconciliation_correction_plan("synthetic", id, 8)
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.correct_bank_reconciliation("synthetic", BODY.as_bytes().to_vec(), false)
            .await,
        Err(Error::InvalidInput(_))
    ));
    let mut body: serde_json::Value = serde_json::from_str(BODY).unwrap();
    for key in ["actor", "envelope", "amount", "transactionDate"] {
        body[key] = serde_json::json!("forbidden");
        assert!(matches!(
            api.correct_bank_reconciliation("synthetic", body.to_string().into_bytes(), true)
                .await,
            Err(Error::InvalidInput(_))
        ));
        body.as_object_mut().unwrap().remove(key);
    }
    body.as_object_mut().unwrap().remove("idempotencyKey");
    assert!(matches!(
        api.correct_bank_reconciliation("synthetic", body.to_string().into_bytes(), true)
            .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.correct_bank_reconciliation(
            "synthetic",
            vec![b' '; MAX_BANK_CORRECTION_BODY + 1],
            true
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
}
