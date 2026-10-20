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
                        "{method} /api/companies/synthetic/documents/7/{suffix} HTTP/1.1"
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
async fn exact_confirmed_request_preserves_retained_conflict_and_citation_evidence() {
    let (api, task) = endpoint(
        "200 OK",
        r#"{"ok":true,"extraction":{"attemptId":"a","documentId":"d","status":"needs_resolution","originalHash":"synthetic-sha","fields":[{"key":"grossAmount","value":"9007199254740993.01","confidence":0.9,"page":1,"sourceText":"Total 9007199254740993.01"}],"resolutions":[],"conflicts":["AMOUNT_MISMATCH"],"exceptionState":"needs_resolution"}}"#,
        "POST",
        "extract-invoice",
    );
    let result = api
        .extract_document_invoice("synthetic", 7, true)
        .await
        .unwrap();
    assert_eq!(result["extraction"]["status"], "needs_resolution");
    assert_eq!(
        result["extraction"]["fields"][0]["value"],
        "9007199254740993.01"
    );
    assert_eq!(
        result["extraction"]["fields"][0]["sourceText"],
        "Total 9007199254740993.01"
    );
    task.join().unwrap();
}

#[tokio::test]
async fn disabled_provider_is_a_known_refusal_but_other_ambiguous_writes_are_not_retried() {
    for (status, body, known) in [
        (
            "503 Unavailable",
            r#"{"ok":false,"code":"EXTRACTION_PROVIDER_UNAVAILABLE","errors":["Extraction provider unavailable"]}"#,
            true,
        ),
        (
            "503 Unavailable",
            r#"{"ok":false,"code":"temporarily_unavailable"}"#,
            false,
        ),
        ("200 OK", "{", false),
    ] {
        let (api, task) = endpoint(status, body, "POST", "extract-invoice");
        let result = api.extract_document_invoice("synthetic", 7, true).await;
        if known {
            assert_eq!(result.unwrap()["code"], "EXTRACTION_PROVIDER_UNAVAILABLE");
        } else {
            assert_eq!(result, Err(Error::OutcomeUnknown));
        }
        task.join().unwrap();
    }
}

#[tokio::test]
async fn canonical_reads_preserve_absence_and_current_access_refusals() {
    for (status, body, expected) in [
        (
            "200 OK",
            r#"{"ok":true,"extraction":null}"#,
            json!({"ok":true,"extraction":null}),
        ),
        (
            "403 Forbidden",
            r#"{"ok":false,"code":"access_denied"}"#,
            json!({"ok":false,"code":"access_denied"}),
        ),
    ] {
        let (api, task) = endpoint(status, body, "GET", "invoice-extraction");
        assert_eq!(
            api.document_invoice_extraction("synthetic", 7)
                .await
                .unwrap(),
            expected
        );
        task.join().unwrap();
    }
}

#[tokio::test]
async fn invalid_scope_or_missing_confirmation_never_reaches_transport() {
    let api = ApiClient::new("http://127.0.0.1:1", true, None).unwrap();
    let results: [Result<Value, Error>; 4] = [
        api.extract_document_invoice("synthetic", 7, false).await,
        api.extract_document_invoice("synthetic", 0, true).await,
        api.document_invoice_extraction("../other", 7).await,
        api.document_invoice_extraction("synthetic", 9_007_199_254_740_992)
            .await,
    ];
    for result in results {
        assert!(matches!(result, Err(Error::InvalidInput(_))));
    }
}
