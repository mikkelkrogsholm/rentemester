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
async fn queue_selectors_and_response_are_exact_read_only_server_data() {
    for (status, archived, query) in [
        (None, false, "includeArchived=false"),
        (Some("all"), true, "includeArchived=true&status=all"),
        (Some("open"), false, "includeArchived=false&status=open"),
        (
            Some("resolved"),
            true,
            "includeArchived=true&status=resolved",
        ),
    ] {
        let reply = r#"{"ok":true,"exceptions":{"slug":"synthetic","status":"all","count":1,"rows":[{"id":7,"archived":true,"sourceEvidence":{"documentId":3},"postingPreview":null}],"bySeverity":{"warning":1}}}"#;
        let (origin, wire, task) = server("200 OK", reply, "application/json");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        assert_eq!(
            api.exceptions("synthetic", status, archived).await.unwrap(),
            serde_json::from_str::<Value>(reply).unwrap()
        );
        let request = String::from_utf8(wire.recv().unwrap()).unwrap();
        assert!(request.starts_with(&format!(
            "GET /api/companies/synthetic/exceptions?{query} HTTP/1.1"
        )));
        assert!(request.to_lowercase().contains("cookie: session=synthetic"));
        assert_eq!(request.split("\r\n\r\n").nth(1).unwrap(), "");
        assert_eq!(task.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn resolution_sends_exact_note_only_body_once_and_preserves_unknown_outcome() {
    let body = "{ \"note\":\"  Synthetic review — bank remains unmatched  \" }\n";
    for (status, expected) in [
        ("200 OK", None),
        (
            "422 Unprocessable Entity",
            Some(Error::Refused { status: 422 }),
        ),
        ("503 Unavailable", Some(Error::OutcomeUnknown)),
    ] {
        let reply = r#"{"ok":true,"exception":{"id":7,"resolved":false}}"#;
        let (origin, wire, task) = server(status, reply, "application/json");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let result = api
            .resolve_exception("synthetic", 7, body.as_bytes().to_vec(), true)
            .await;
        if let Some(error) = expected {
            assert_eq!(result.unwrap_err(), error);
        } else {
            assert_eq!(
                result.unwrap(),
                serde_json::from_str::<Value>(reply).unwrap()
            );
        }
        let request = String::from_utf8(wire.recv().unwrap()).unwrap();
        assert!(request.starts_with("POST /api/companies/synthetic/exceptions/7/resolve HTTP/1.1"));
        assert_eq!(request.split("\r\n\r\n").nth(1).unwrap(), body);
        assert!(request.to_lowercase().contains("cookie: session=synthetic"));
        assert_eq!(task.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn read_failure_does_not_claim_uncertain_mutation() {
    let (origin, _wire, task) = server("503 Unavailable", "{}", "application/json");
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.exceptions("synthetic", Some("all"), false)
            .await
            .unwrap_err(),
        Error::Refused { status: 503 }
    );
    assert_eq!(task.join().unwrap(), 1);
}
#[tokio::test]
async fn invalid_selectors_bodies_and_missing_confirmation_refuse_before_transport() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for status in ["", "closed", "ALL", "open&includeArchived=true"] {
        assert!(matches!(
            api.exceptions("synthetic", Some(status), false).await,
            Err(Error::InvalidInput(_))
        ));
    }
    for (id, body, confirm) in [
        (1, b"{}".to_vec(), false),
        (0, b"{}".to_vec(), true),
        (-1, b"{}".to_vec(), true),
        (9_007_199_254_740_992, b"{}".to_vec(), true),
        (1, b"{\"confirm\":true}".to_vec(), true),
        (1, b"{\"actor\":\"agent:caller\"}".to_vec(), true),
        (1, b"{\"note\":1}".to_vec(), true),
        (1, b"[]".to_vec(), true),
        (
            1,
            serde_json::to_vec(&serde_json::json!({"note":"x".repeat(4097)})).unwrap(),
            true,
        ),
        (
            1,
            vec![b' '; rentemester_client::MAX_EXCEPTION_BODY + 1],
            true,
        ),
    ] {
        assert!(matches!(
            api.resolve_exception("synthetic", id, body, confirm).await,
            Err(Error::InvalidInput(_))
        ));
    }
}

#[tokio::test]
async fn optional_note_is_not_fabricated_when_absent_or_null() {
    for body in ["{}", "{\"note\":null}"] {
        let (origin, wire, task) = server("200 OK", "{\"ok\":true}", "application/json");
        let api = ApiClient::new(&origin, true, None).unwrap();
        api.resolve_exception("synthetic", 7, body.as_bytes().to_vec(), true)
            .await
            .unwrap();
        let request = String::from_utf8(wire.recv().unwrap()).unwrap();
        assert_eq!(request.split("\r\n\r\n").nth(1).unwrap(), body);
        assert_eq!(task.join().unwrap(), 1);
    }
}
