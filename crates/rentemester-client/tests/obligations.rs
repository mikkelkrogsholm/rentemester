//! The obligations adapter forwards scope and raw canonical facts; it never writes.
mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    thread,
    time::Duration,
};
fn fixture(path: &str, status: u16, value: Value) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let expected = path.to_owned();
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
            let mut chunk = [0; 1024];
            let n = stream.read(&mut chunk).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            assert!(bytes.len() <= 8192);
        }
        let wire = String::from_utf8(bytes).unwrap();
        assert_eq!(
            wire.lines().next().unwrap(),
            format!("GET {expected} HTTP/1.1")
        );
        assert!(wire.to_lowercase().contains("cookie: session=synthetic"));
        assert!(wire.ends_with("\r\n\r\n"));
        assert!(!wire.to_lowercase().contains("content-type:"));
        let body = value.to_string();
        write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
    });
    (origin, task)
}
#[tokio::test]
async fn exact_optional_company_query_and_full_obligation_evidence_are_preserved() {
    for (year, path) in [
        (None, "/api/companies/synthetic_owner/obligations"),
        (
            Some("2024/25"),
            "/api/companies/synthetic_owner/obligations?year=2024%2F25",
        ),
        (
            Some("2024/25 & review=1"),
            "/api/companies/synthetic_owner/obligations?year=2024%2F25+%26+review%3D1",
        ),
    ] {
        let expected = json!({"ok":true,"obligations":{"slug":"synthetic_owner","selectedYear":year,"coverage":{"status":"partial","reason":"source_gap"},"tasks":[{"status":"blocked","deadline":null,"amount":"100000000000000.01","sourceRefs":[{"documentId":7}],"blockers":["unreviewed_original"],"nextAction":"Review original"}],"submitted":false}});
        let (origin, server) = fixture(path, 200, expected.clone());
        let client = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        assert_eq!(
            client.obligations("synthetic_owner", year).await.unwrap(),
            expected
        );
        server.join().unwrap();
    }
}
#[tokio::test]
async fn read_refusals_remain_refusals_without_invented_empty_or_success() {
    for status in [403, 404, 422, 503] {
        let (origin, server) = fixture(
            "/api/companies/synthetic/obligations?year=2025",
            status,
            json!({"ok":false,"code":"fixture_refusal"}),
        );
        let client = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        assert!(
            matches!(client.obligations("synthetic",Some("2025")).await,Err(Error::Refused{status:actual})if actual==status)
        );
        server.join().unwrap();
    }
}
#[tokio::test]
async fn invalid_scope_or_native_query_bounds_refuse_before_transport() {
    let client =
        ApiClient::new("http://127.0.0.1:9", true, Some("session=synthetic".into())).unwrap();
    for company in ["", "../other", "synthetic/other", "other?year=2024"] {
        assert!(matches!(
            client.obligations(company, None).await,
            Err(Error::InvalidInput(_))
        ));
    }
    for year in [
        "",
        " 2024",
        "2024 ",
        "2024\n",
        "2024\u{0000}",
        "2024\u{fffd}",
        &"x".repeat(129),
        &"Æ".repeat(65),
    ] {
        assert!(matches!(
            client.obligations("synthetic", Some(year)).await,
            Err(Error::InvalidInput(_))
        ));
    }
}
