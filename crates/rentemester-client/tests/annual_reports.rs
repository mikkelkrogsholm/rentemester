use rentemester_client::{ApiClient, Error};
use serde_json::json;
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};
#[tokio::test]
async fn annual_read_preserves_full_preparation_and_blocker_responses() {
    for prepared in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let value = json!({"ok":true,"annualReport":{"report":{"ok":prepared,"errors":if prepared {vec![]}else{vec!["open year"]},"comparison":{"equity":123.45},"notes":[]}}});
        let expected = value.clone();
        let task = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut data = [0; 4096];
            let n = stream.read(&mut data).unwrap();
            let wire = String::from_utf8_lossy(&data[..n]);
            assert!(wire.starts_with("GET /api/companies/synthetic/annual-report?fiscalYearStart=2024-07-01&fiscalYearEnd=2025-06-30 HTTP/1.1"));
            assert!(wire.to_lowercase().contains("cookie: session=synthetic"));
            let body = value.to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        assert_eq!(
            api.annual_report("synthetic", "2024-07-01", "2025-06-30")
                .await
                .unwrap(),
            expected
        );
        task.join().unwrap();
    }
}
#[tokio::test]
async fn annual_dates_and_company_are_checked_before_transport() {
    let api = ApiClient::new("http://127.0.0.1:9", true, Some("session=synthetic".into())).unwrap();
    for (start, end) in [
        ("2025-02-29", "2025-12-31"),
        ("2025-12-31", "2025-01-01"),
        ("2025-1-01", "2025-12-31"),
        ("2025-01-01&x=1", "2025-12-31"),
    ] {
        assert!(matches!(
            api.annual_report("synthetic", start, end).await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.annual_report("../other", "2025-01-01", "2025-12-31")
            .await,
        Err(Error::InvalidInput(_))
    ));
}
