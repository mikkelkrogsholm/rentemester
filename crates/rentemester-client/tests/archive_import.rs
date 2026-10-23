mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error};
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

fn body(apply: bool) -> Vec<u8> {
    let mut v = json!({"fileName":"source.json","contentBase64":"eyJvcmlnaW5hbCI6dHJ1ZX0="});
    if apply {
        v["planHash"] = json!("a".repeat(64));
        v["idempotencyKey"] = json!("caller-exact-archive-key");
        v["confirm"] = json!(true);
    }
    serde_json::to_vec(&v).unwrap()
}
#[tokio::test]
async fn archive_review_apply_preserve_exact_source_key_and_canonical_result() {
    for apply in [false, true] {
        let reply = r#"{"ok":true,"archiveImport":{"importId":42,"sourceSha256":"retained","years":[{"fiscalYear":2024}]},"future":true}"#;
        let (origin, wire, task) = server("200 OK", reply, "application/json");
        let api = ApiClient::new(&origin, true, None).unwrap();
        let bytes = if apply {
            body(true)
        } else {
            serde_json::to_vec(
                &json!({"fileName":"source.json","contentBase64":"A".repeat(400_000)}),
            )
            .unwrap()
        };
        assert_eq!(
            api.archive_import("synthetic", bytes.clone(), apply, apply)
                .await
                .unwrap(),
            serde_json::from_str::<Value>(reply).unwrap()
        );
        let request = wire.recv().unwrap();
        assert!(
            request.starts_with(
                format!(
                    "POST /api/companies/synthetic/archive-import/{} HTTP/1.1",
                    if apply { "apply" } else { "plan" }
                )
                .as_bytes()
            )
        );
        assert!(request.ends_with(&bytes));
        assert_eq!(task.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn archive_unknown_write_readonly_refusal_and_original_bytes_are_distinct() {
    for apply in [false, true] {
        let (origin, wire, task) = server("503 Service Unavailable", "{}", "application/json");
        let api = ApiClient::new(&origin, true, None).unwrap();
        let result = api
            .archive_import("synthetic", body(apply), apply, apply)
            .await;
        assert!(if apply {
            matches!(result, Err(Error::OutcomeUnknown))
        } else {
            matches!(result, Err(Error::Refused { status: 503 }))
        });
        wire.recv().unwrap();
        assert_eq!(task.join().unwrap(), 1);
    }
    let reply = "{ \"source\": \"exact bytes\" }\n";
    let (origin, wire, task) = server("200 OK", reply, "application/json");
    let api = ApiClient::new(&origin, true, None).unwrap();
    let value = api.archive_import_original("synthetic", 42).await.unwrap();
    assert_eq!(
        value,
        json!({"ok":true,"contentType":"application/json","bytes":reply.as_bytes()})
    );
    assert!(
        wire.recv()
            .unwrap()
            .starts_with(b"GET /api/companies/synthetic/archive-import/42/original HTTP/1.1")
    );
    task.join().unwrap();
}
#[tokio::test]
async fn archive_detail_preserves_raw_precision_and_invalid_controls_never_connect() {
    let reply = r#"{"ok":true,"years":[{"fiscalYear":2024,"postings":[{"sourceAmount":"-0.005000","amount":"-0.01"}],"balances":[]}]}"#;
    let (origin, wire, task) = server("200 OK", reply, "application/json");
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.archives("synthetic", Some(2024)).await.unwrap(),
        serde_json::from_str::<Value>(reply).unwrap()
    );
    assert!(
        wire.recv()
            .unwrap()
            .starts_with(b"GET /api/companies/synthetic/archives/2024 HTTP/1.1")
    );
    task.join().unwrap();
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    assert!(matches!(
        api.archive_import("synthetic", body(true), true, false)
            .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.archive_import("synthetic", body(false), false, true)
            .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.archives("synthetic", Some(999)).await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.archive_import_original("synthetic", 0).await,
        Err(Error::InvalidInput(_))
    ));
}
