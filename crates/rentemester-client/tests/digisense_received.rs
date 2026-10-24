//! Real HTTP SDK fixture: protocol first, exact reviewed bytes once, no retry.
use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};
fn read(s: &mut std::net::TcpStream) -> Vec<u8> {
    s.set_read_timeout(Some(Duration::from_secs(4))).unwrap();
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
                .map(|s| s.parse::<usize>().unwrap())
                .unwrap_or(0);
            if bytes.len() >= end + 4 + len {
                return bytes;
            }
        }
    }
}
fn reply(s: &mut std::net::TcpStream, status: &str, body: &[u8]) {
    write!(s,"HTTP/1.1 {status}\r\nContent-Type:application/json\r\nContent-Length:{}\r\nConnection:close\r\n\r\n",body.len()).unwrap();
    s.write_all(body).unwrap();
}
#[tokio::test]
async fn digisense_received_exact_http_protocol_and_no_replay() {
    for workspace in [false, true] {
        for uncertain in [false, true] {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            let origin = format!("http://{}", l.local_addr().unwrap());
            let bytes = if workspace {
                br#"{ "confirm" : true }"#.to_vec()
            } else {
                br#"{ "limit":2, "confirm":true, "maxPages":1 }"#.to_vec()
            };
            let expected = bytes.clone();
            let t = thread::spawn(move || {
                let (mut s, _) = l.accept().unwrap();
                let discovery = String::from_utf8(read(&mut s)).unwrap().to_lowercase();
                assert!(discovery.starts_with("get /api/protocol "));
                assert!(!discovery.contains("cookie:"));
                let manifest = include_bytes!("../../../protocol/http-v1.json");
                reply(&mut s, "200 OK", manifest);
                drop(s);
                let (mut s, _) = l.accept().unwrap();
                let wire = read(&mut s);
                let end = wire.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
                let path = if workspace {
                    "/api/workspace/efaktura/modtag"
                } else {
                    "/api/companies/synthetic/efaktura/modtag"
                };
                assert!(wire.starts_with(format!("POST {path} HTTP/1.1").as_bytes()));
                assert_eq!(&wire[end + 4..], expected);
                let h = String::from_utf8_lossy(&wire[..end]).to_lowercase();
                assert!(h.contains("x-rentemester-protocol: 1"));
                assert!(h.contains("cookie: session=synthetic"));
                if !uncertain {
                    reply(&mut s, "200 OK", br#"{"ok":true,"documentsIngested":1}"#);
                }
                drop(s);
                thread::sleep(Duration::from_millis(70));
                l.set_nonblocking(true).unwrap();
                assert!(l.accept().is_err(), "no automatic retry");
            });
            let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
            let result = if workspace {
                api.digisense_received_workspace(bytes, true).await
            } else {
                api.digisense_received("synthetic", bytes, true).await
            };
            if uncertain {
                assert_eq!(result, Err(Error::OutcomeUnknown));
            } else {
                assert_eq!(result.unwrap(), json!({"ok":true,"documentsIngested":1}));
            }
            t.join().unwrap();
        }
    }
}
#[tokio::test]
async fn digisense_received_missing_protocol_capability_prevents_write() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let wire = read(&mut s);
        assert!(wire.starts_with(b"GET /api/protocol "));
        let mut manifest: Value =
            serde_json::from_slice(include_bytes!("../../../protocol/http-v1.json")).unwrap();
        manifest["operations"]
            .as_array_mut()
            .unwrap()
            .retain(|o| !o["path"].as_str().unwrap().ends_with("efaktura/modtag"));
        reply(&mut s, "200 OK", &serde_json::to_vec(&manifest).unwrap());
        drop(s);
        thread::sleep(Duration::from_millis(70));
        l.set_nonblocking(true).unwrap();
        assert!(l.accept().is_err());
    });
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.digisense_received("synthetic", br#"{"confirm":true}"#.to_vec(), true)
            .await,
        Err(Error::Protocol)
    );
    t.join().unwrap();
}
