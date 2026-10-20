use rentemester_client::{ApiClient, Error};
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
fn input() -> Value {
    json!({"cutOverDate":"2024-07-01","lines":[{"accountNo":"A","debitAmount":"100.01"},{"accountNo":"E","creditAmount":"100.01"}],"note":" Exact original review æ ","sourceArchiveImportIds":[42]})
}
fn body(apply: bool) -> Vec<u8> {
    if apply {
        serde_json::to_vec(&json!({"input":input(),"planHash":"a".repeat(64),"idempotencyKey":" caller-original-key ","confirm":true})).unwrap()
    } else {
        serde_json::to_vec(&input()).unwrap()
    }
}
#[tokio::test]
async fn opening_read_plan_apply_exact_wire_and_full_canonical_receipt() {
    for op in ["read", "plan", "apply"] {
        let reply = r#"{"ok":true,"openingBalance":{"cutOverDate":"2024-07-01","actor":"user:original","sourceBasis":{"kind":"operator_supplied"}},"idempotency":{"replayed":true},"future":["exact"]}"#;
        let (origin, wire, task) = server("200 OK", reply, "application/json");
        let api = ApiClient::new(&origin, true, None).unwrap();
        let bytes = body(op == "apply");
        let result = match op {
            "read" => api.opening_balance("synthetic").await,
            "plan" => api.opening_balance_plan("synthetic", bytes.clone()).await,
            _ => {
                api.opening_balance_apply("synthetic", bytes.clone(), true)
                    .await
            }
        };
        assert_eq!(
            result.unwrap(),
            serde_json::from_str::<Value>(reply).unwrap()
        );
        let request = wire.recv().unwrap();
        let path = if op == "read" {
            "opening-balance".to_owned()
        } else {
            format!("opening-balance/{op}")
        };
        assert!(
            request.starts_with(
                format!(
                    "{} /api/companies/synthetic/{path} HTTP/1.1",
                    if op == "read" { "GET" } else { "POST" }
                )
                .as_bytes()
            )
        );
        if op != "read" {
            assert!(request.ends_with(&bytes));
        }
        assert_eq!(task.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn opening_plan_is_readonly_unknown_apply_never_retries_and_known_refusal_survives() {
    for apply in [false, true] {
        let (origin, wire, task) = server("503 Service Unavailable", "{}", "application/json");
        let api = ApiClient::new(&origin, true, None).unwrap();
        let error = if apply {
            api.opening_balance_apply("synthetic", body(true), true)
                .await
                .unwrap_err()
        } else {
            api.opening_balance_plan("synthetic", body(false))
                .await
                .unwrap_err()
        };
        assert_eq!(
            error,
            if apply {
                Error::OutcomeUnknown
            } else {
                Error::Transport
            }
        );
        wire.recv().unwrap();
        assert_eq!(task.join().unwrap(), 1);
    }
    let reply = r#"{"ok":false,"code":"opening_balance_refused","errors":["plan changed"],"original":"retained"}"#;
    let (origin, wire, task) = server("422 Unprocessable Entity", reply, "application/json");
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.opening_balance_apply("synthetic", body(true), true)
            .await
            .unwrap(),
        serde_json::from_str::<Value>(reply).unwrap()
    );
    wire.recv().unwrap();
    assert_eq!(task.join().unwrap(), 1);
}
#[tokio::test]
async fn opening_bad_controls_size_duplicate_fields_and_extra_authority_never_connect() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    assert!(matches!(
        api.opening_balance_apply("synthetic", body(true), false)
            .await,
        Err(Error::InvalidInput(_))
    ));
    for bytes in [
        b"{}".to_vec(),
        vec![b' '; 65_537],
        br#"{"cutOverDate":"2024-07-01","cutOverDate":"2024-07-02","lines":[]}"#.to_vec(),
        serde_json::to_vec(&{
            let mut i = input();
            i["actor"] = json!("user:fake");
            i
        })
        .unwrap(),
        serde_json::to_vec(&{
            let mut i = input();
            i["lines"][0]["debitAmount"] = Value::Null;
            i
        })
        .unwrap(),
        serde_json::to_vec(&{
            let mut i = input();
            i["lines"][0]["debitAmount"] = json!(100);
            i
        })
        .unwrap(),
    ] {
        assert!(matches!(
            api.opening_balance_plan("synthetic", bytes).await,
            Err(Error::InvalidInput(_))
        ));
    }
    let mut wrong: Value = serde_json::from_slice(&body(true)).unwrap();
    wrong["confirm"] = json!(false);
    assert!(matches!(
        api.opening_balance_apply("synthetic", serde_json::to_vec(&wrong).unwrap(), true)
            .await,
        Err(Error::InvalidInput(_))
    ));
    let schema = rentemester_client::opening_balance_body_schema(true);
    assert_eq!(schema["properties"]["input"]["additionalProperties"], false);
    assert_eq!(schema["properties"]["confirm"]["const"], true);
    assert_eq!(
        schema["properties"]["input"]["properties"]["lines"]["items"]["properties"]["debitAmount"]
            ["type"],
        "string"
    );
}
