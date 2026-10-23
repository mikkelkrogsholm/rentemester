mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error, MAX_DOCUMENT_INGEST_BODY};
use serde_json::json;
use std::{
    io::{Read, Write},
    thread,
    time::Duration,
};
fn http(
    status: &'static str,
    mime: &'static str,
    response: Vec<u8>,
    expected: Vec<u8>,
) -> (String, thread::JoinHandle<usize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut data = vec![];
        loop {
            let mut chunk = [0; 8192];
            let n = stream.read(&mut chunk).unwrap();
            data.extend_from_slice(&chunk[..n]);
            if n == 0 {
                break;
            }
            if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                let h = String::from_utf8_lossy(&data[..end]);
                let size = h
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length: ")
                            .map(|s| s.parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                if data.len() >= end + 4 + size {
                    break;
                }
            }
        }
        let h = String::from_utf8_lossy(&data);
        assert!(h.to_lowercase().contains("cookie: session=synthetic"));
        if expected.is_empty() {
            assert!(h.starts_with("GET /api/companies/synthetic/documents/7/file HTTP/1.1"));
        } else {
            assert!(h.starts_with("POST /api/companies/synthetic/documents/ingest HTTP/1.1"));
            let end = data.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
            assert_eq!(&data[end..], expected);
        }
        write!(stream,"HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection:close\r\n\r\n",response.len()).unwrap();
        stream.write_all(&response).unwrap();
        drop(stream);
        listener.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(70));
        let mut n = 1;
        while listener.accept().is_ok() {
            n += 1;
        }
        n
    });
    (origin, task)
}
#[tokio::test]
async fn exact_native_upload_keeps_metadata_server_owned_and_never_retries() {
    let body=b"{ \"confirm\":true,\"fileName\":\"synthetic.txt\",\"fileBase64\":\"YQ==\",\"metadata\":{\"source\":\"\",\"nativeUnknownField\":true} }\n";
    for (status, error) in [
        ("200 OK", None),
        (
            "422 Unprocessable Entity",
            Some(Error::Refused { status: 422 }),
        ),
        ("503 Unavailable", Some(Error::OutcomeUnknown)),
    ] {
        let (origin, task) = http(
            status,
            "application/json",
            br#"{"ok":true,"document":{"id":7,"documentNo":"SYN-1"}}"#.to_vec(),
            body.to_vec(),
        );
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let result = api.ingest_document("synthetic", body.to_vec(), true).await;
        if let Some(e) = error {
            assert_eq!(result.unwrap_err(), e);
        } else {
            assert_eq!(result.unwrap()["document"]["id"], 7);
        }
        assert_eq!(task.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn original_preserves_binary_bytes_and_actual_mime_read_failures_are_not_unknown() {
    for (status, mime, expected) in [
        ("200 OK", "image/png", None),
        ("200 OK", "text/html", Some(Error::Protocol)),
        (
            "403 Forbidden",
            "application/json",
            Some(Error::Refused { status: 403 }),
        ),
        (
            "503 Unavailable",
            "application/json",
            Some(Error::Refused { status: 503 }),
        ),
    ] {
        let bytes = b"\x89PNG\r\n\x1a\n\xff\0synthetic".to_vec();
        let (origin, task) = http(status, mime, bytes.clone(), vec![]);
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let result = api.document_original("synthetic", 7).await;
        if let Some(e) = expected {
            assert_eq!(result.unwrap_err(), e);
        } else {
            assert_eq!(
                result.unwrap(),
                json!({"ok":true,"original":{"contentType":"image/png","bytes":bytes}})
            );
        }
        assert_eq!(task.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn upload_confirmation_envelope_and_size_are_bounded_before_transport() {
    let api = ApiClient::new("http://127.0.0.1:9", true, Some("session=synthetic".into())).unwrap();
    let body = json!({"confirm":true,"fileName":"receipt.txt","fileBase64":"YQ==","metadata":{"source":"synthetic"}});
    assert!(matches!(
        api.ingest_document("synthetic", body.to_string().into_bytes(), false)
            .await,
        Err(Error::InvalidInput(_))
    ));
    for invalid in [
        json!({"confirm":true,"fileName":"receipt.txt","fileBase64":"YQ==","metadata":{},"actor":"other"}),
        json!({"confirm":true,"fileName":"receipt.txt","fileBase64":"YQ==","metadata":{},"force":true}),
        json!({"confirm":true,"fileName":"receipt.txt","fileBase64":"YQ==","metadata":{},"vendorId":7}),
    ] {
        assert!(matches!(
            api.ingest_document("synthetic", invalid.to_string().into_bytes(), true)
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.ingest_document("synthetic", vec![b' '; MAX_DOCUMENT_INGEST_BODY + 1], true)
            .await,
        Err(Error::InvalidInput(_))
    ));
    for id in [0, -1, 9_007_199_254_740_992] {
        assert!(matches!(
            api.document_original("synthetic", id).await,
            Err(Error::InvalidInput(_))
        ));
    }
}

#[tokio::test]
async fn original_content_length_above_native_bound_is_refused_before_allocation() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        assert!(stream.read(&mut request).unwrap() > 0);
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: 26214401\r\nConnection:close\r\n\r\n").unwrap();
    });
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.document_original("synthetic", 7).await.unwrap_err(),
        Error::Protocol
    );
    task.join().unwrap();
}
