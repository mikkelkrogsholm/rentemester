use rentemester_client::{ApiClient, Error, JournalMutation};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};
const BODY: &[u8] = br#"{ "confirm":true,"input":{"idempotencyKey":"synthetic-fixed","amount":"9007199254740993.01"}}"#;
fn manifest() -> Value {
    json!({"format":"rentemester-http-protocol/1","protocolMajor":1,"manifestId":"a".repeat(64),"wireProfiles":["rust-http-v1"],"operations":[{"method":"POST","path":"/api/companies/{company}/manual-journal","capability":"POST /api/companies/{company}/manual-journal"}]})
}
fn read(stream: &mut impl Read) -> Vec<u8> {
    let mut bytes = vec![];
    loop {
        let mut b = [0; 4096];
        let n = stream.read(&mut b).unwrap();
        assert!(n > 0);
        bytes.extend_from_slice(&b[..n]);
        if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&bytes[..end]);
            let len = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length: "))
                .map(|n| n.parse::<usize>().unwrap())
                .unwrap_or(0);
            if bytes.len() >= end + 4 + len {
                return bytes;
            }
        }
    }
}
fn server(
    status: u16,
    body: String,
    business: Option<&'static str>,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let wire = String::from_utf8(read(&mut s)).unwrap().to_lowercase();
        assert!(wire.starts_with("get /api/protocol http/1.1\r\n"));
        for header in [
            "cookie:",
            "authorization:",
            "x-rentemester-service-key:",
            "x-rentemester-protocol:",
        ] {
            assert!(!wire.contains(header));
        }
        write!(
            s,
            "HTTP/1.1 {status} Response\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        drop(s);
        if let Some(reply) = business {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let bytes = read(&mut s);
            assert!(bytes.ends_with(BODY));
            let wire = String::from_utf8(bytes).unwrap().to_lowercase();
            assert!(wire.starts_with("post /api/companies/synthetic/manual-journal http/1.1\r\n"));
            for header in [
                "x-rentemester-protocol: 1".into(),
                format!("x-rentemester-manifest: {}", "a".repeat(64)),
                "x-rentemester-wire-profile: rust-http-v1".into(),
            ] {
                assert_eq!(wire.lines().filter(|line| *line == header).count(), 1);
            }
            assert!(wire.contains("x-rentemester-service-key: private-synthetic"));
            s.write_all(reply.as_bytes()).unwrap();
        }
        listener.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(100));
        assert!(
            listener.accept().is_err(),
            "no business dispatch after preflight failure; no write retry"
        );
    });
    (origin, task)
}
async fn call(status: u16, body: String, business: Option<&'static str>) -> Result<Value, Error> {
    let (origin, task) = server(status, body, business);
    let api = ApiClient::with_service_key(&origin, true, "private-synthetic".into()).unwrap();
    let result = api
        .manual_journal("synthetic", JournalMutation::Post, BODY.to_vec())
        .await;
    task.join().unwrap();
    result
}
#[tokio::test]
async fn admission_preserves_exact_write_and_binds_headers() {
    assert_eq!(
        call(
            200,
            manifest().to_string(),
            Some("HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}")
        )
        .await
        .unwrap()["ok"],
        true
    );
}
#[tokio::test]
async fn incompatible_manifest_never_dispatches_post() {
    for body in ["{}".into(), "not json".into(), "x".repeat(1024 * 1024 + 1)] {
        assert_eq!(call(200, body, None).await, Err(Error::Protocol));
    }
    for status in [404, 503] {
        assert_eq!(
            call(status, manifest().to_string(), None).await,
            Err(Error::Protocol)
        );
    }
    let base = manifest();
    for (field, value) in [
        ("protocolMajor", json!(2)),
        ("wireProfiles", json!(["legacy-react-v1"])),
        ("wireProfiles", json!(["unknown"])),
        ("operations", json!([])),
        ("manifestId", json!("A".repeat(64))),
    ] {
        let mut m = base.clone();
        m[field] = value;
        assert_eq!(call(200, m.to_string(), None).await, Err(Error::Protocol));
    }
    for path in [
        "/api/companies/{company}/manual-journal/extra",
        "/api/companies/*",
        "/api/companies/{company}/manual-journal?x=1",
    ] {
        let mut m = base.clone();
        m["operations"][0]["path"] = json!(path);
        m["operations"][0]["capability"] = json!(format!("POST {path}"));
        assert_eq!(call(200, m.to_string(), None).await, Err(Error::Protocol));
    }
    let mut m = base.clone();
    m["operations"]
        .as_array_mut()
        .unwrap()
        .push(base["operations"][0].clone());
    assert_eq!(call(200, m.to_string(), None).await, Err(Error::Protocol));
}
#[tokio::test]
async fn stale_admission_is_safe_refusal_without_retry_and_lost_write_stays_unknown() {
    assert_eq!(
        call(
            200,
            manifest().to_string(),
            Some("HTTP/1.1 409 Conflict\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        )
        .await,
        Err(Error::Refused { status: 409 })
    );
    assert_eq!(
        call(200, manifest().to_string(), Some("")).await,
        Err(Error::OutcomeUnknown)
    );
}
