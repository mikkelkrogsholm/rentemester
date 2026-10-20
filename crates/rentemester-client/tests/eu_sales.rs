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
async fn eu_report_and_vies_use_canonical_server_without_claiming_validity_or_retrying() {
    for lookup in [false, true] {
        let reply = r#"{"ok":true,"validation":{"valid":false},"totalValue":-745,"sources":[],"warnings":[]}"#;
        let (origin, wire, task) = server("200 OK", reply, "application/json");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let value = if lookup {
            api.vies_validate("synthetic", "DE123456789").await
        } else {
            api.eu_sales("synthetic", "2025-09-01", "2025-10-12").await
        };
        assert_eq!(
            value.unwrap(),
            serde_json::from_str::<Value>(reply).unwrap()
        );
        let bytes = wire.recv().unwrap();
        let request = String::from_utf8(bytes).unwrap();
        if lookup {
            assert!(request.starts_with("POST /api/companies/synthetic/vies/validate HTTP/1.1"));
            let body = request.split("\r\n\r\n").nth(1).unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(body).unwrap(),
                serde_json::json!({"vatOrCvr":"DE123456789"})
            );
        } else {
            assert!(request.starts_with(
                "GET /api/companies/synthetic/eu-sales?from=2025-09-01&to=2025-10-12 HTTP/1.1"
            ));
        }
        assert_eq!(task.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn eu_invalid_selectors_refuse_before_network() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for (from, to) in [("2025-02-30", "2025-03-01"), ("2025-10-12", "2025-09-01")] {
        assert!(matches!(
            api.eu_sales("synthetic", from, to).await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.vies_validate("synthetic", "").await,
        Err(Error::InvalidInput(_))
    ));
}
