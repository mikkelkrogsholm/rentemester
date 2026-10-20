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
async fn oss_preserves_server_report_and_sends_exact_dated_read() {
    let reply = r#"{"ok":true,"ossConsumerSalesBase":-1000,"entryCount":1,"submission":false,"warnings":[],"errors":[]}"#;
    let (origin, wire, task) = server("200 OK", reply, "application/json");
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.oss_report("synthetic", "2026-01-01", "2026-06-30")
            .await
            .unwrap(),
        serde_json::from_str::<Value>(reply).unwrap()
    );
    let request = String::from_utf8(wire.recv().unwrap()).unwrap();
    assert!(request.starts_with(
        "GET /api/companies/synthetic/oss-report?from=2026-01-01&to=2026-06-30 HTTP/1.1"
    ));
    assert!(request.to_lowercase().contains("cookie: session=synthetic"));
    assert_eq!(request.split("\r\n\r\n").nth(1).unwrap(), "");
    assert_eq!(task.join().unwrap(), 1);
}
#[tokio::test]
async fn invalid_oss_dates_refuse_before_network() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for (from, to) in [("2026-02-30", "2026-03-01"), ("2026-07-01", "2026-01-01")] {
        assert!(matches!(
            api.oss_report("synthetic", from, to).await,
            Err(Error::InvalidInput(_))
        ));
    }
}
