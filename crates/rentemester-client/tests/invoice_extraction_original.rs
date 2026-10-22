use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};

fn endpoint(
    status: &'static str,
    body: &'static str,
    method: &'static str,
    suffix: &'static str,
) -> (ApiClient, thread::JoinHandle<()>) {
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
        let mut bytes = vec![];
        loop {
            let mut chunk = [0; 1024];
            let n = stream.read(&mut chunk).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            assert!(bytes.len() < 8192);
            if let Some(end) = bytes.windows(4).position(|x| x == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let size = headers
                    .lines()
                    .find_map(|s| {
                        s.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .map(|n| n.parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + size {
                    assert!(headers.starts_with(&format!(
                        "{method} /api/companies/synthetic/documents/original/7/{suffix} HTTP/1.1"
                    )));
                    assert!(
                        headers
                            .to_ascii_lowercase()
                            .contains("cookie: session=synthetic")
                    );
                    if method == "POST" {
                        assert_eq!(&bytes[end + 4..], br#"{"confirm":true}"#);
                    } else {
                        assert_eq!(size, 0);
                    }
                    break;
                }
            }
        }
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        drop(stream);
        listener.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(60));
        assert!(
            listener.accept().is_err(),
            "provider operation must never be retried by SDK"
        );
    });
    (api, task)
}

#[tokio::test]
async fn flat_null_read_and_retained_needs_resolution_write() {
    let (api, task) = endpoint(
        "200 OK",
        r#"{"ok":true,"extraction":null}"#,
        "GET",
        "invoice-extraction",
    );
    assert_eq!(
        api.original_invoice_extraction("synthetic", 7)
            .await
            .unwrap(),
        json!({"ok":true,"extraction":null})
    );
    task.join().unwrap();
    let body = r#"{"ok":true,"extraction":{"attemptId":1,"documentId":7,"status":"needs_resolution","originalHash":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","fields":[{"key":"grossAmount","value":12.34,"confidence":0.79,"page":1,"sourceText":"Total 12.34","box":{"x":1,"y":2,"width":3,"height":4}}],"resolutions":[{"fieldKey":"provider_failure","value":{"code":"EXTRACTION_PROVIDER_UNAVAILABLE"}}],"conflicts":["EXTRACTION_PROVIDER_UNAVAILABLE"],"exceptionState":{"id":1,"status":"open"}}}"#;
    let (api, task) = endpoint("200 OK", body, "POST", "extract-invoice");
    assert_eq!(
        api.original_extract_invoice("synthetic", 7, true, None)
            .await
            .unwrap(),
        serde_json::from_str::<Value>(body).unwrap()
    );
    task.join().unwrap();
}
#[tokio::test]
async fn status_precedes_forged_body_and_writes_never_retry() {
    for status in ["401 Unauthorized", "403 Forbidden"] {
        for body in [
            r#"{"ok":true,"extraction":null}"#,
            r#"{"ok":false,"errors":["EXTRACTION_FAILED"]}"#,
        ] {
            let (api, task) = endpoint(status, body, "POST", "extract-invoice");
            assert!(matches!(
                api.original_extract_invoice("synthetic", 7, true, None)
                    .await,
                Err(Error::Refused { .. })
            ));
            task.join().unwrap();
        }
    }
    for (status, body, expected) in [
        (
            "503 Unavailable",
            r#"{"ok":false,"errors":["EXTRACTION_PROVIDER_UNAVAILABLE"]}"#,
            None,
        ),
        (
            "422 Domain",
            r#"{"ok":false,"errors":["EXTRACTION_SNAPSHOT_INVALID"]}"#,
            None,
        ),
        (
            "422 Domain",
            r#"{"ok":false,"errors":["EXTRACTION_ADMISSION_CHANGED"]}"#,
            None,
        ),
        (
            "422 Domain",
            r#"{"ok":false,"errors":["private /server/secret"]}"#,
            Some(Error::Refused { status: 422 }),
        ),
        (
            "503 Unavailable",
            r#"{"ok":false,"errors":["EXTRACTION_FAILED"]}"#,
            Some(Error::OutcomeUnknown),
        ),
        ("408 Timeout", "{}", Some(Error::OutcomeUnknown)),
        ("200 OK", "not-json", Some(Error::OutcomeUnknown)),
        (
            "200 OK",
            r#"{"ok":true,"extraction":null,"cookie":"secret"}"#,
            Some(Error::OutcomeUnknown),
        ),
        (
            "200 OK",
            r#"{"ok":true,"extraction":{"context":"private"}}"#,
            Some(Error::OutcomeUnknown),
        ),
    ] {
        let (api, task) = endpoint(status, body, "POST", "extract-invoice");
        let got = api
            .original_extract_invoice("synthetic", 7, true, None)
            .await;
        if let Some(e) = expected {
            assert_eq!(got, Err(e));
        } else {
            assert_eq!(got.unwrap(), serde_json::from_str::<Value>(body).unwrap());
        }
        task.join().unwrap();
    }
}
#[tokio::test]
async fn usage_refuses_before_http() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let api = ApiClient::new(
        &format!("http://{}", listener.local_addr().unwrap()),
        true,
        None,
    )
    .unwrap();
    for id in [0, -1, 9_007_199_254_740_992] {
        assert!(matches!(
            api.original_invoice_extraction("synthetic", id).await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.original_extract_invoice("synthetic", 7, false, None)
            .await,
        Err(Error::InvalidInput(_))
    ));
    for actor in ["", "   ", " user:forged", "bad\nactor"] {
        assert!(matches!(
            api.original_extract_invoice("synthetic", 7, true, Some(actor))
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
    listener.set_nonblocking(true).unwrap();
    assert!(listener.accept().is_err());
}
