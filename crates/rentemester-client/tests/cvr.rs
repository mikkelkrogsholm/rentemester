mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error};
use serde_json::Value;
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

#[tokio::test]
async fn status_extracts_only_presence_without_provider_effects() {
    let reply = r#"{"ok":true,"cvrStatus":{"configured":false}}"#;
    let (origin, wire, task) = server("200 OK", reply, "application/json");
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.cvr_status().await.unwrap(),
        serde_json::json!({"configured":false})
    );
    let request = String::from_utf8(wire.recv().unwrap()).unwrap();
    assert!(request.starts_with("GET /api/system/cvr-status HTTP/1.1"));
    assert_eq!(request.split("\r\n\r\n").nth(1).unwrap(), "");
    assert_eq!(task.join().unwrap(), 1);
}
#[tokio::test]
async fn lookup_preserves_original_text_and_exact_inner_success_or_failure_once() {
    for reply in [
        r#"{"ok":true,"cvr":{"ok":true,"cached":true,"fetchedAt":"2026-01-01T00:00:00.000Z","company":{"cvr":"12345678","name":"Synthetic CVR customer","address":null},"errors":[]}}"#,
        r#"{"ok":true,"cvr":{"ok":false,"cached":false,"company":null,"errors":["Synthetic missing credentials"]}}"#,
    ] {
        let (origin, wire, task) = server("200 OK", reply, "application/json");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        assert_eq!(
            api.cvr_lookup("synthetic", " dk 12 34 56 78 ", true)
                .await
                .unwrap(),
            serde_json::from_str::<Value>(reply).unwrap()["cvr"]
        );
        let request = String::from_utf8(wire.recv().unwrap()).unwrap();
        assert!(
            request.starts_with(
                "GET /api/companies/synthetic/cvr-lookup?cvr=+dk+12+34+56+78+ HTTP/1.1"
            )
        );
        assert!(request.to_lowercase().contains("cookie: session=synthetic"));
        assert_eq!(request.split("\r\n\r\n").nth(1).unwrap(), "");
        assert_eq!(task.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn sync_uses_empty_object_post_and_keeps_business_failure_or_fiscal_warning() {
    for reply in [
        r#"{"ok":true,"sync":{"ok":true,"cvr":"87654321","company":{"name":"Synthetic Company"},"cached":false,"updatedFields":["name","address"],"fiscalYearStartMonth":{"current":1,"cvr":7,"matches":false},"errors":[]}}"#,
        r#"{"ok":true,"sync":{"ok":false,"errors":["Synthetic CVR not configured"]}}"#,
    ] {
        let (origin, wire, task) = server("200 OK", reply, "application/json");
        let api = ApiClient::new(&origin, true, None).unwrap();
        assert_eq!(
            api.sync_company_cvr("synthetic", true).await.unwrap(),
            serde_json::from_str::<Value>(reply).unwrap()["sync"]
        );
        let request = String::from_utf8(wire.recv().unwrap()).unwrap();
        assert!(request.starts_with("POST /api/companies/synthetic/sync-cvr HTTP/1.1"));
        assert_eq!(request.split("\r\n\r\n").nth(1).unwrap(), "{}");
        assert_eq!(task.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn unconfirmed_or_excessive_inputs_fail_before_any_effect() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    assert!(matches!(
        api.cvr_lookup("synthetic", "12345678", false).await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.sync_company_cvr("synthetic", false).await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.cvr_lookup("synthetic", &"x".repeat(129), true).await,
        Err(Error::InvalidInput(_))
    ));
}
#[tokio::test]
async fn lookup_and_sync_uncertainty_do_not_retry_and_current_refusals_remain_refusals() {
    for sync in [false, true] {
        for (status, expected) in [
            ("503 Service Unavailable", Error::OutcomeUnknown),
            ("403 Forbidden", Error::Refused { status: 403 }),
            ("200 OK", Error::OutcomeUnknown),
        ] {
            let (origin, wire, task) = server(status, "{}", "application/json");
            let api = ApiClient::new(&origin, true, None).unwrap();
            let result = if sync {
                api.sync_company_cvr("synthetic", true).await
            } else {
                api.cvr_lookup("synthetic", "12345678", true).await
            };
            assert_eq!(result, Err(expected));
            wire.recv().unwrap();
            assert_eq!(task.join().unwrap(), 1);
        }
    }
    let (origin, wire, task) = server("200 OK", "{}", "application/json");
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(api.cvr_status().await, Err(Error::Protocol));
    wire.recv().unwrap();
    assert_eq!(task.join().unwrap(), 1);
}
