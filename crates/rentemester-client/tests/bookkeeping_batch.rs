use rentemester_client::{
    ApiClient, BookkeepingBatchMutation as Op, Error, MAX_BOOKKEEPING_BATCH_BODY,
};
use serde_json::{Value, json};
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

fn scope() -> Value {
    json!({"accountingFrom":"2026-01-01","accountingTo":"2026-12-31","bankFrom":"2026-01-01","bankTo":"2026-12-31"})
}
fn request(op: Op) -> Vec<u8> {
    let v = if op == Op::Persist {
        let mut v = scope();
        v["runKey"] = json!("chosen-synthetic-run");
        v["expectedPlanHash"] = json!("a".repeat(64));
        v["confirm"] = json!(true);
        v
    } else {
        json!({"runId":7,"planHash":"a".repeat(64),"confirm":true})
    };
    format!(" {v}\n").into_bytes()
}
#[tokio::test]
async fn five_routes_exact_body_scope_and_lossless_canonical_receipts() {
    const JSON: &str = r#"{"ok":true,"state":{"future":{"amount":"90071992547409.99"}},"results":[{"receipt":{"entryId":3}}]}"#;
    for action in ["plan", "status", "persist", "approve", "apply"] {
        let (origin, rx, t) = server("200 OK", JSON, "application/json");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let op = match action {
            "persist" => Some(Op::Persist),
            "approve" => Some(Op::Approve),
            "apply" => Some(Op::Apply),
            _ => None,
        };
        let bytes = op.map(request);
        let v = match op {
            Some(op) => {
                api.mutate_bookkeeping_batch("synthetic", op, bytes.clone().unwrap(), true)
                    .await
            }
            None if action == "plan" => api.bookkeeping_batch_plan("synthetic", &scope()).await,
            None => api.bookkeeping_batch_status("synthetic", 7).await,
        }
        .unwrap();
        assert_eq!(v, serde_json::from_str::<Value>(JSON).unwrap());
        let got = rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let h = String::from_utf8_lossy(&got);
        let first = h.lines().next().unwrap();
        assert!(first.starts_with(if op.is_some() { "POST " } else { "GET " }));
        if action == "plan" {
            assert!(first.contains("/api/companies/synthetic/bookkeeping-batch?"));
            for (k, v) in scope().as_object().unwrap() {
                assert!(first.contains(&format!("{k}={}", v.as_str().unwrap())));
            }
        } else {
            let suffix = if action == "status" { "runs/7" } else { action };
            assert!(first.contains(&format!(
                "/api/companies/synthetic/bookkeeping-batch/{suffix} "
            )));
        }
        let end = got.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        assert_eq!(&got[end..], bytes.as_deref().unwrap_or(&[]));
        assert_eq!(t.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn invalid_native_shapes_refuse_before_http() {
    let api = ApiClient::new("http://127.0.0.1:9", true, Some("session=synthetic".into())).unwrap();
    for bytes in [b"{}".to_vec(),vec![b' ';MAX_BOOKKEEPING_BATCH_BODY+1],br#"{"runId":7,"planHash":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","confirm":true}"#.to_vec(),br#"{"runId":7,"planHash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","confirm":false}"#.to_vec(),br#"{"runId":7,"planHash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","confirm":true,"actor":"user:forged"}"#.to_vec()]{assert!(matches!(api.mutate_bookkeeping_batch("synthetic",Op::Apply,bytes,true).await,Err(Error::InvalidInput(_))));}
    assert!(matches!(
        api.mutate_bookkeeping_batch("synthetic", Op::Apply, request(Op::Apply), false)
            .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.bookkeeping_batch_status("synthetic", 0).await,
        Err(Error::InvalidInput(_))
    ));
    let mut s = scope();
    s["bankTo"] = json!("2025-01-01");
    assert!(matches!(
        api.bookkeeping_batch_plan("synthetic", &s).await,
        Err(Error::InvalidInput(_))
    ));
}
#[tokio::test]
async fn known_refusal_keeps_details_and_uncertain_response_never_retries() {
    for (status, reply, unknown) in [
        (
            "422 Unprocessable Entity",
            r#"{"ok":false,"code":"batch_stale","checks":[{"ok":false}],"future":{"amount":"0.01"}}"#,
            false,
        ),
        (
            "503 Service Unavailable",
            r#"{"ok":false,"code":"outcome_unknown"}"#,
            true,
        ),
        (
            "408 Request Timeout",
            r#"{"ok":false,"code":"timeout"}"#,
            true,
        ),
    ] {
        let (origin, rx, t) = server(status, reply, "application/json");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let got = api
            .mutate_bookkeeping_batch("synthetic", Op::Apply, request(Op::Apply), true)
            .await;
        if unknown {
            assert!(matches!(got, Err(Error::OutcomeUnknown)));
        } else {
            assert_eq!(got.unwrap(), serde_json::from_str::<Value>(reply).unwrap());
        }
        rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(t.join().unwrap(), 1);
    }
}

#[tokio::test]
async fn unknown_persist_can_read_exact_run_key_without_resending() {
    let (origin, rx, t) = server(
        "200 OK",
        r#"{"ok":true,"state":{"run":{"id":7}}}"#,
        "application/json",
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.bookkeeping_batch_status_by_key("synthetic", "chosen key/one")
            .await
            .unwrap()["state"]["run"]["id"],
        7
    );
    let wire = rx.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(wire.starts_with(
        b"GET /api/companies/synthetic/bookkeeping-batch/runs?runKey=chosen+key%2Fone HTTP/1.1"
    ));
    assert_eq!(t.join().unwrap(), 1);
}
