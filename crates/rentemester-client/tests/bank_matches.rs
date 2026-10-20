use rentemester_client::{ApiClient, Error};
use serde_json::Value;
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

#[tokio::test]
async fn canonical_suggestions_scores_and_selectors_pass_through_without_writes() {
    let reply = r#"{"ok":true,"count":1,"rows":[{"bankTransactionId":7,"date":"2026-10-20","text":"Synthetic reference","amount":125,"currency":"DKK","reference":"synthetic","suggestions":[{"kind":"issued_invoice","documentId":3,"invoiceNo":"SYN-3","customerName":"Synthetic buyer","confidence":1.2,"reasons":["Exact invoice number"]},{"kind":"supplier_credit_refund","documentId":4,"invoiceNo":"SYN-4","supplierName":"Synthetic supplier","confidence":0.85,"reasons":["Refund cue"]}],"unsafeMatchReason":null}],"errors":[]}"#;
    for (id, max, query) in [
        (None, None, String::new()),
        (Some(7), Some(1), "?bankTransactionId=7&max=1".into()),
        (None, Some(usize::MAX), format!("?max={}", usize::MAX)),
    ] {
        let (origin, wire, task) = server("200 OK", reply, "application/json");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        assert_eq!(
            api.bank_suggest_matches("synthetic", id, max)
                .await
                .unwrap(),
            serde_json::from_str::<Value>(reply).unwrap()
        );
        let request = String::from_utf8(wire.recv().unwrap()).unwrap();
        assert!(request.starts_with(&format!(
            "GET /api/companies/synthetic/bank/match-suggestions{query} HTTP/1.1"
        )));
        assert!(request.to_lowercase().contains("cookie: session=synthetic"));
        assert_eq!(request.split("\r\n\r\n").nth(1).unwrap(), "");
        assert_eq!(task.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn unknown_or_matched_selector_keeps_domain_failure_and_read_errors_are_not_retried() {
    let reply = r#"{"ok":false,"count":0,"rows":[],"errors":["unmatched bank transaction 7 does not exist"]}"#;
    let (origin, wire, task) = server("200 OK", reply, "application/json");
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.bank_suggest_matches("synthetic", Some(7), None)
            .await
            .unwrap(),
        serde_json::from_str::<Value>(reply).unwrap()
    );
    wire.recv().unwrap();
    assert_eq!(task.join().unwrap(), 1);
    for (status, code) in [("403 Forbidden", 403), ("503 Service Unavailable", 503)] {
        let (origin, wire, task) = server(status, "{}", "application/json");
        let api = ApiClient::new(&origin, true, None).unwrap();
        assert_eq!(
            api.bank_suggest_matches("synthetic", None, None).await,
            Err(Error::Refused { status: code })
        );
        wire.recv().unwrap();
        assert_eq!(task.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn invalid_positive_selectors_fail_before_network() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for (id, max) in [
        (Some(0), None),
        (Some(-1), None),
        (Some(9_007_199_254_740_992), None),
        (None, Some(0)),
    ] {
        assert!(matches!(
            api.bank_suggest_matches("synthetic", id, max).await,
            Err(Error::InvalidInput(_))
        ));
    }
}
