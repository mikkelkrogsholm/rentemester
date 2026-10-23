mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, DispositionAction as Action, Error, MAX_DISPOSITION_BODY};
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

fn request(op: Action) -> Vec<u8> {
    let mut v = if matches!(op, Action::Plan | Action::Propose) {
        json!({"disposition":{"dispositionId":"synthetic/ø?A#B","type":"loan","economicDate":"2026-01-10","amount":12.34,"currency":"DKK","partyIds":["synthetic-party"],"evidenceRecordIds":["synthetic-evidence"],"left":{"companySlug":"synthetic-left","role":"lender","expectedSide":"receivable"},"right":{"companySlug":"synthetic-right","role":"borrower","expectedSide":"payable"}}})
    } else {
        json!({"dispositionId":"synthetic/ø?A#B","payloadHash":"a".repeat(64)})
    };
    match op {
        Action::Link => {
            v["side"] = json!("left");
            v["journalEntryId"] = json!(3);
            v["ledgerHeadHash"] = json!("b".repeat(64));
        }
        Action::Settle => {
            v["settlementEvidenceRecordIds"] = json!(["actual-record"]);
        }
        Action::Supersede => {
            v["replacementDispositionId"] = json!("synthetic-replacement");
            v["reason"] = json!("Explicit synthetic replacement");
        }
        Action::Reopen => {
            v["reason"] = json!("Explicit synthetic reopen");
        }
        _ => {}
    }
    if op.is_write() {
        v["confirm"] = json!(true);
    }
    format!(" {v}\n").into_bytes()
}
#[tokio::test]
async fn seven_actions_preserve_exact_payload_and_full_canonical_state() {
    const JSON: &str = r#"{"ok":true,"status":"settled","balanced":false,"scope":"intercompany-disposition-status","events":[{"principal_id":"original-human"}],"future":{"amount":"90071992547409.99"}}"#;
    for op in [
        Action::Plan,
        Action::Propose,
        Action::Approve,
        Action::Link,
        Action::Settle,
        Action::Supersede,
        Action::Reopen,
    ] {
        let (origin, rx, t) = server("200 OK", JSON, "application/json");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let bytes = request(op);
        assert_eq!(
            api.intercompany_disposition("synthetic-left", op, bytes.clone(), op.is_write())
                .await
                .unwrap(),
            serde_json::from_str::<Value>(JSON).unwrap()
        );
        let wire = rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(String::from_utf8_lossy(&wire).starts_with(&format!(
            "POST /api/companies/synthetic-left/group-dispositions/{} HTTP/1.1",
            op.suffix()
        )));
        let end = wire.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        assert_eq!(&wire[end..], bytes);
        assert_eq!(t.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn global_status_encodes_exact_id_as_one_segment_without_company() {
    let (origin, rx, t) = server(
        "200 OK",
        r#"{"ok":true,"scope":"intercompany-disposition-status","balanced":false}"#,
        "application/json",
    );
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.group_disposition_status("synthetic/ø?A#B", Some("2026-01-10"))
            .await
            .unwrap()["balanced"],
        false
    );
    let wire = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(wire.starts_with(
        b"GET /api/group-dispositions/synthetic%2F%C3%B8%3FA%23B?asOf=2026-01-10 HTTP/1.1"
    ));
    assert_eq!(t.join().unwrap(), 1);
}
#[tokio::test]
async fn refused_transition_retains_details_and_uncertain_write_is_sent_once() {
    for (status, reply, op, unknown) in [
        (
            "422 Unprocessable Entity",
            r#"{"ok":false,"code":"disposition_refused","errors":["head changed"],"future":{"head":"retained"}}"#,
            Action::Link,
            false,
        ),
        (
            "503 Service Unavailable",
            r#"{"ok":false,"code":"outcome_unknown"}"#,
            Action::Link,
            true,
        ),
        (
            "503 Service Unavailable",
            r#"{"ok":false,"code":"unavailable"}"#,
            Action::Plan,
            false,
        ),
    ] {
        let (origin, rx, t) = server(status, reply, "application/json");
        let api = ApiClient::new(&origin, true, None).unwrap();
        let result = api
            .intercompany_disposition("synthetic-left", op, request(op), op.is_write())
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
async fn confirmation_scope_body_bounds_and_caller_authority_fields_reject_before_io() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for (op, bytes, confirmed) in [
        (Action::Plan, request(Action::Plan), true),
        (Action::Link, request(Action::Link), false),
        (Action::Approve, vec![b' '; MAX_DISPOSITION_BODY + 1], true),
        (Action::Approve, b"{}".to_vec(), true),
    ] {
        assert!(matches!(
            api.intercompany_disposition("synthetic-left", op, bytes, confirmed)
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
    let mut v: Value = serde_json::from_slice(&request(Action::Approve)).unwrap();
    v["actor"] = json!("forged");
    assert!(matches!(
        api.intercompany_disposition(
            "synthetic-left",
            Action::Approve,
            serde_json::to_vec(&v).unwrap(),
            true
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
}
