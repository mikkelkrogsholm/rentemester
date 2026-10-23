mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, CfoAnalyticsQuery, Error};
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

fn query() -> CfoAnalyticsQuery {
    serde_json::from_value(json!({"scope":"portfolio","from":"2025-01-01","to":"2026-01-02","companySlugs":["left","right"],"party":"party/ø &","dimension":"department:sales","currency":"DKK","aggregate":"sum","cursor":"page+/=","limit":1})).unwrap()
}
#[tokio::test]
async fn cfo_exact_get_preserves_money_evidence_and_query() {
    let reply = r#"{"ok":true,"status":"ready","rows":[{"amount":"9007199254740993.01","sourceHash":"retained","partyLinkProvenance":{"planHash":"exact"}}],"totals":{"DKK":"-0.01"},"nextCursor":"next","future":true}"#;
    let (origin, wire, task) = server("200 OK", reply, "application/json");
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.cfo_analytics(&query()).await.unwrap(),
        serde_json::from_str::<Value>(reply).unwrap()
    );
    let bytes = wire.recv().unwrap();
    let request = String::from_utf8(bytes).unwrap();
    assert!(request.starts_with("GET /api/cfo-analytics?scope=portfolio&from=2025-01-01&to=2026-01-02&companySlug=left&companySlug=right&party=party%2F%C3%B8+%26&dimension=department%3Asales&currency=DKK&aggregate=sum&cursor=page%2B%2F%3D&limit=1 HTTP/1.1"));
    assert!(!request.to_lowercase().contains("idempotency-key"));
    assert!(request.ends_with("\r\n\r\n"));
    assert_eq!(task.join().unwrap(), 1);
}
#[tokio::test]
async fn cfo_partial_and_known_unavailable_are_not_rewritten_or_uncertain() {
    for (status, reply) in [
        (
            "200 OK",
            r#"{"ok":true,"status":"partial","totals":null,"rows":[],"hidden":null}"#,
        ),
        (
            "503 Service Unavailable",
            r#"{"ok":false,"code":"cfo_archive_unavailable","errors":["source required"]}"#,
        ),
    ] {
        let (origin, wire, task) = server(status, reply, "application/json");
        let api = ApiClient::new(&origin, true, None).unwrap();
        assert_eq!(
            api.cfo_analytics(&query()).await.unwrap(),
            serde_json::from_str::<Value>(reply).unwrap()
        );
        wire.recv().unwrap();
        assert_eq!(task.join().unwrap(), 1);
    }
    let (origin, wire, task) = server("503 Service Unavailable", "{}", "application/json");
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert!(matches!(
        api.cfo_analytics(&query()).await,
        Err(Error::Refused { status: 503 })
    ));
    wire.recv().unwrap();
    task.join().unwrap();
}
#[tokio::test]
async fn cfo_invalid_shape_never_reaches_transport() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    let mut q = query();
    q.limit = Some(201);
    assert!(matches!(
        api.cfo_analytics(&q).await,
        Err(Error::InvalidInput(_))
    ));
    for extra in ["confirm", "body", "actor", "endpoint"] {
        let mut v = json!({"scope":"company","from":"2025-01-01","to":"2025-12-31"});
        v[extra] = json!(true);
        assert!(serde_json::from_value::<CfoAnalyticsQuery>(v).is_err());
    }
}
