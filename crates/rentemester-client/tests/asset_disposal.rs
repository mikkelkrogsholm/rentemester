mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    thread,
};
fn server(status: u16, value: Value) -> (ApiClient, thread::JoinHandle<(String, Value)>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let api = ApiClient::new(
        &format!("http://{}", listener.local_addr().unwrap()),
        true,
        None,
    )
    .unwrap();
    let t = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(4)))
            .unwrap();
        let mut bytes = vec![];
        let end = loop {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let header = String::from_utf8_lossy(&bytes[..end]).to_string();
        let len: usize = header
            .lines()
            .find_map(|l| {
                l.to_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse().unwrap())
            })
            .unwrap_or(0);
        while bytes.len() < end + len {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
        }
        let body = if len == 0 {
            Value::Null
        } else {
            serde_json::from_slice(&bytes[end..end + len]).unwrap()
        };
        let text = value.to_string();
        write!(
            s,
            "HTTP/1.1 {status} Mock\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        )
        .unwrap();
        (header, body)
    });
    (api, t)
}

fn input() -> Value {
    json!({"transactionDate":"2026-10-20","documentId":7,"reason":"Retired"})
}
fn receipt() -> Value {
    json!({"ok":true,"disposal":{"disposalId":1,"assetId":3,"transactionDate":"2026-10-20","documentId":7,"reason":"Retired","cost":120,"accumulatedDepreciation":20,"carryingValueWrittenOff":100,"remainingCarryingValue":0,"journalEntryId":5,"journalEntryNo":"2026-5","journalEntryHash":"b".repeat(64),"replayed":false}})
}
#[tokio::test]
async fn asset_disposal_apply_and_readback() {
    let mut i = input();
    i["expectedPlanHash"] = json!("a".repeat(64));
    i["idempotencyKey"] = json!("synthetic-key");
    i["confirm"] = json!(true);
    let (api, t) = server(200, receipt());
    assert!(api.asset_dispose("synthetic", 3, &i, true).await.is_ok());
    let (h, b) = t.join().unwrap();
    assert!(h.starts_with("POST /api/companies/synthetic/assets/3/dispose "));
    assert_eq!(b, i);
    let (api, t) = server(200, receipt());
    assert!(
        api.asset_disposal_receipt("synthetic", 3, "synthetic-key")
            .await
            .is_ok()
    );
    let (h, b) = t.join().unwrap();
    assert!(
        h.starts_with("GET /api/companies/synthetic/assets/3/disposal-receipt?key=synthetic-key ")
    );
    assert!(b.is_null());
}
#[tokio::test]
async fn asset_disposal_malformed_write_is_unknown() {
    let mut i = input();
    i["expectedPlanHash"] = json!("a".repeat(64));
    i["idempotencyKey"] = json!("synthetic-key");
    i["confirm"] = json!(true);
    let (api, t) = server(200, json!({"ok":true,"disposal":{}}));
    assert!(matches!(
        api.asset_dispose("synthetic", 3, &i, true).await,
        Err(Error::OutcomeUnknown)
    ));
    t.join().unwrap();
}
#[tokio::test]
async fn asset_disposal_authority_refusal_and_missing_receipt() {
    let (api, t) = server(403, json!({"ok":false}));
    assert!(matches!(
        api.asset_disposal_preview("synthetic", 3, &input()).await,
        Err(Error::Refused { status: 403 })
    ));
    t.join().unwrap();
    let (api, t) = server(404, json!({"ok":false}));
    assert!(matches!(
        api.asset_disposal_receipt("synthetic", 3, "unknown-key")
            .await,
        Err(Error::Refused { status: 404 })
    ));
    t.join().unwrap();
}
