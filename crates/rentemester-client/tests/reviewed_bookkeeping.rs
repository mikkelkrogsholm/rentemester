mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error, ReviewedMutation as Op, ReviewedRead};
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
                "POST /api/companies/synthetic/accounting-drafts/bank-one/submit HTTP/1.1"
            ));
            assert!(wire.ends_with("{ \"expectedEventHash\" : \"server-returned-hash\" }\n"));
            assert!(!wire.contains("idempotency"));
        } else {
            assert!(wire.starts_with("GET /api/companies/synthetic/reviewed-bank HTTP/1.1"));
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
#[tokio::test]
async fn exact_submit_body_is_not_confirmed_rewritten_or_retried() {
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
            .mutate_reviewed_bookkeeping(
                "synthetic",
                Op::Submit("bank-one"),
                b"{ \"expectedEventHash\" : \"server-returned-hash\" }\n".to_vec(),
                true,
            )
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
async fn queue_is_a_read_even_on_server_failure() {
    let (origin, task) = http("503 Unavailable");
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.reviewed_bookkeeping("synthetic", ReviewedRead::Queue)
            .await
            .unwrap_err(),
        Error::Refused { status: 503 }
    );
    assert_eq!(task.join().unwrap(), 1);
}
#[tokio::test]
async fn paths_extra_actor_keys_payload_and_confirmation_fail_before_transport() {
    let api = ApiClient::new("http://127.0.0.1:9", true, Some("session=synthetic".into())).unwrap();
    for (op, body, confirmed) in [
        (Op::Submit("../other"), r#"{"expectedEventHash":"h"}"#, true),
        (
            Op::Submit("bank-one?x=1"),
            r#"{"expectedEventHash":"h"}"#,
            true,
        ),
        (
            Op::Submit("bank-one"),
            r#"{"expectedEventHash":"h"}"#,
            false,
        ),
        (
            Op::ApprovePost("bank-one"),
            r#"{"expectedEventHash":"h","confirm":true,"actor":"reviewer"}"#,
            true,
        ),
        (
            Op::ApprovePost("bank-one"),
            r#"{"expectedEventHash":"h","confirm":true,"payload":{}}"#,
            true,
        ),
        (
            Op::ApprovePost("bank-one"),
            r#"{"expectedEventHash":"h","confirm":true,"idempotencyKey":"new"}"#,
            true,
        ),
        (Op::Reject("bank-one"), r#"{"expectedEventHash":"h"}"#, true),
        (
            Op::CreateBankDraft,
            r#"{"bankTransactionId":1,"draftId":"bank-one","accountNo":"8000","expectedSourceHash":"h","confirm":false}"#,
            true,
        ),
    ] {
        assert!(matches!(
            api.mutate_reviewed_bookkeeping("synthetic", op, body.as_bytes().to_vec(), confirmed)
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
}
