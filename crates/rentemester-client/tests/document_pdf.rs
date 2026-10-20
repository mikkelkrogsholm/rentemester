use rentemester_client::{ApiClient, Error};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};

#[tokio::test]
async fn parsing_is_confirmed_once_and_ambiguous_writes_are_not_retried() {
    for (status, expected) in [
        ("200 OK", None),
        ("503 Unavailable", Some(Error::OutcomeUnknown)),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let api = ApiClient::new(
            &format!("http://{}", listener.local_addr().unwrap()),
            true,
            Some("session=synthetic".into()),
        )
        .unwrap();
        let task = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut data = Vec::new();
            loop {
                let mut b = [0; 1024];
                let n = stream.read(&mut b).unwrap();
                assert!(n > 0);
                data.extend_from_slice(&b[..n]);
                if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&data[..end]);
                    let size = headers
                        .lines()
                        .find_map(|s| {
                            s.to_lowercase()
                                .strip_prefix("content-length: ")
                                .map(|s| s.parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if data.len() >= end + 4 + size {
                        assert!(headers.starts_with(
                            "POST /api/companies/synthetic/documents/7/parse HTTP/1.1"
                        ));
                        assert!(headers.to_lowercase().contains("cookie: session=synthetic"));
                        assert_eq!(&data[end + 4..], br#"{"confirm":true}"#);
                        break;
                    }
                }
            }
            let body = r#"{"ok":true,"parse":{"cached":false}}"#;
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            drop(stream);
            listener.set_nonblocking(true).unwrap();
            thread::sleep(Duration::from_millis(70));
            assert!(listener.accept().is_err());
        });
        let result = api.document_parse("synthetic", 7, true).await;
        if let Some(error) = expected {
            assert_eq!(result, Err(error));
        } else {
            assert_eq!(result.unwrap()["parse"]["cached"], false);
        }
        task.join().unwrap();
    }
}

#[tokio::test]
async fn invalid_selectors_and_missing_confirmation_refuse_before_transport() {
    let api = ApiClient::new("http://127.0.0.1:1", true, None).unwrap();
    for result in [
        api.document_parse("synthetic", 7, false).await,
        api.document_parse_status("synthetic", 0).await,
        api.document_parsed_text("synthetic", 7, 0, 11).await,
        api.document_parsed_text("synthetic", 7, u64::MAX, 10).await,
        api.documents_parse_pending("synthetic", 0, 101, true).await,
        api.documents_parse_pending("synthetic", 0, 1, false).await,
    ] {
        assert!(matches!(result, Err(Error::InvalidInput(_))));
    }
}

#[tokio::test]
async fn retained_evidence_tamper_code_survives_transport() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let api = ApiClient::new(
        &format!("http://{}", listener.local_addr().unwrap()),
        true,
        None,
    )
    .unwrap();
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut bytes = [0; 4096];
        let n = stream.read(&mut bytes).unwrap();
        assert!(
            String::from_utf8_lossy(&bytes[..n])
                .starts_with("GET /api/companies/synthetic/documents/7/parse-status HTTP/1.1")
        );
        let body = r#"{"ok":false,"code":"PDF_EVIDENCE_TAMPERED","errors":["PDF evidence integrity verification failed"]}"#;
        write!(
            stream,
            "HTTP/1.1 409 Conflict\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
    });
    let result = api.document_parse_status("synthetic", 7).await.unwrap();
    assert_eq!(result["ok"], false);
    assert_eq!(result["code"], "PDF_EVIDENCE_TAMPERED");
    task.join().unwrap();
}
