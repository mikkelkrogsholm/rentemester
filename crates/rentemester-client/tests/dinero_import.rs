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
        v["idempotencyKey"] = json!("caller-exact-dinero-key");
        v["confirm"] = json!(true);
    }
    serde_json::to_vec(&v).unwrap()
}
#[tokio::test]
async fn dinero_exact_sources_confirmation_and_no_retry() {
    for apply in [false, true] {
        let reply = r#"{"ok":true,"dineroImport":{"attemptId":42,"sourceSha256":"retained"}}"#;
        let (origin, wire, task) = server("200 OK", reply, "application/json");
        let api = ApiClient::new(&origin, true, None).unwrap();
        let input = body(apply);
        assert_eq!(
            api.dinero_import("synthetic", input.clone(), apply, apply)
                .await
                .unwrap(),
            serde_json::from_str::<Value>(reply).unwrap()
        );
        let request = wire.recv().unwrap();
        assert!(
            request.starts_with(
                format!(
                    "POST /api/companies/synthetic/imports/dinero/{} HTTP/1.1",
                    if apply { "apply" } else { "plan" }
                )
                .as_bytes()
            )
        );
        assert!(request.ends_with(&input));
        assert_eq!(task.join().unwrap(), 1);
    }
    let (origin, wire, task) = server(
        "503 Unavailable",
        r#"{"ok":false,"code":"internal"}"#,
        "application/json",
    );
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.dinero_import_apply("synthetic", body(true), true)
            .await
            .unwrap_err(),
        Error::OutcomeUnknown
    );
    wire.recv().unwrap();
    assert_eq!(task.join().unwrap(), 1);
}
#[tokio::test]
async fn dinero_read_original_and_explicit_asof() {
    let (origin, wire, task) = server("200 OK", "{\"raw\":true}", "application/json");
    let api = ApiClient::new(&origin, true, None).unwrap();
    let actual = api.dinero_import_original("synthetic", 42).await.unwrap();
    assert_eq!(
        actual,
        json!({"ok":true,"original":{"contentType":"application/json","bytes":b"{\"raw\":true}"}})
    );
    assert!(
        String::from_utf8(wire.recv().unwrap())
            .unwrap()
            .starts_with("GET /api/companies/synthetic/imports/dinero/42/original ")
    );
    assert_eq!(task.join().unwrap(), 1);
    let (origin, wire, task) = server(
        "200 OK",
        r#"{"ok":true,"importedReceivables":{"rows":[],"totalOpen":0}}"#,
        "application/json",
    );
    let api = ApiClient::new(&origin, true, None).unwrap();
    api.imported_receivables("synthetic", "2025-12-31")
        .await
        .unwrap();
    assert!(
        String::from_utf8(wire.recv().unwrap())
            .unwrap()
            .starts_with(
                "GET /api/companies/synthetic/invoices/imported-receivables?asOf=2025-12-31 "
            )
    );
    assert_eq!(task.join().unwrap(), 1);
}
#[tokio::test]
async fn dinero_refuses_ambiguous_authority_and_invalid_selectors_before_network() {
    let api = ApiClient::new("http://127.0.0.1:1", true, None).unwrap();
    assert!(matches!(
        api.dinero_import_apply("synthetic", body(true), false)
            .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.dinero_import_plan("synthetic", body(true)).await,
        Err(Error::InvalidInput(_))
    ));
    let mut v: Value = serde_json::from_slice(&body(true)).unwrap();
    v["attemptId"] = json!(3);
    assert!(matches!(
        api.dinero_import_apply("synthetic", serde_json::to_vec(&v).unwrap(), true)
            .await,
        Err(Error::InvalidInput(_))
    ));
    for date in ["2025-02-29", "2025-1-01", "", "0000-01-01"] {
        assert!(matches!(
            api.imported_receivables("synthetic", date).await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.dinero_import_original("synthetic", 0).await,
        Err(Error::InvalidInput(_))
    ));
}
