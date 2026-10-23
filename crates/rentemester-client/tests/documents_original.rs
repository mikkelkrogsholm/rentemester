mod common;
use common::TcpListener;
use rentemester_client::{
    ApiClient, DocumentsOriginalMutation as Mutation, Error, documents_original_file,
    documents_original_schema,
};
use serde_json::json;
use std::{
    fs,
    io::{Read, Write},
    thread,
    time::{Duration, Instant},
};
#[test]
fn documents_original_schema_is_operation_input_not_transport() {
    let schema = documents_original_schema(Mutation::Ingest);
    assert_eq!(schema["required"], json!(["metadata"]));
    assert_eq!(schema["properties"]["force"]["type"], "boolean");
    assert!(schema["properties"].get("confirm").is_none());
    assert_eq!(
        documents_original_schema(Mutation::CompanyContext(1))["required"],
        json!(["sourceReference", "businessUseReason"])
    );
}
#[test]
fn documents_original_local_snapshot_rejects_symlinks_directories_devices_and_bounds() {
    let root = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(format!("documents-original-sdk-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("synthetic.pdf");
    fs::write(&path, b"%PDF-synthetic").unwrap();
    assert_eq!(
        documents_original_file(&path).unwrap(),
        ("synthetic.pdf".into(), b"%PDF-synthetic".to_vec())
    );
    assert!(documents_original_file(&root).is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&path, root.join("link.pdf")).unwrap();
        assert!(documents_original_file(&root.join("link.pdf")).is_err());
        assert!(documents_original_file(std::path::Path::new("/dev/null")).is_err());
        let fifo = root.join("fifo.pdf");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        assert!(documents_original_file(&fifo).is_err());
    }
    fs::File::create(&path)
        .unwrap()
        .set_len(25 * 1024 * 1024 + 1)
        .unwrap();
    assert!(documents_original_file(&path).is_err());
    fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn documents_original_confirm_and_payload_validation_precede_http() {
    let api = ApiClient::new("http://127.0.0.1:1", true, None).unwrap();
    for (body, confirmed) in [
        (json!({}), false),
        (
            json!({"confirm":true,"metadata":{"source":"x","actor":"invented"}}),
            true,
        ),
        (
            json!({"confirm":true,"metadata":{"source":"x","purchaseVatLines":[]}}),
            true,
        ),
        (
            json!({"confirm":true,"metadata":{"source":"x"},"fileName":"../bad.pdf","fileBase64":"YQ=="}),
            true,
        ),
    ] {
        assert!(matches!(
            api.documents_original_mutation(
                "synthetic",
                Mutation::Ingest,
                serde_json::to_vec(&body).unwrap(),
                confirmed
            )
            .await,
            Err(Error::InvalidInput(_))
        ));
    }
}
#[tokio::test]
async fn documents_original_service_transport_flat_refusal_and_no_retry() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let api = ApiClient::with_service_key(
        &format!("http://{}", listener.local_addr().unwrap()),
        true,
        "rm_svc_synthetic_key".into(),
    )
    .unwrap();
    let task = thread::spawn(move || {
        let start = Instant::now();
        let (mut socket, _) = loop {
            match listener.accept() {
                Ok(v) => break v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(start.elapsed() < Duration::from_secs(5));
                    thread::sleep(Duration::from_millis(5));
                }
                Err(e) => panic!("{e}"),
            }
        };
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut b = [0; 4096];
            let n = socket.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let len = headers
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length: ")
                            .map(|v| v.parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + len {
                    assert!(
                        headers.starts_with(
                            "POST /api/companies/synthetic/documents/original/7/enrich "
                        )
                    );
                    assert!(
                        headers
                            .to_lowercase()
                            .contains("x-rentemester-service-key: rm_svc_synthetic_key")
                    );
                    assert!(!headers.to_lowercase().contains("cookie:"));
                    break;
                }
            }
        }
        let body =
            r#"{"ok":false,"code":"DOCUMENT_INGEST_BLOCKED","errors":["Actual domain refusal"]}"#;
        write!(
            socket,
            "HTTP/1.1 422 Refused\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        drop(socket);
        let until = Instant::now() + Duration::from_millis(150);
        while Instant::now() < until {
            assert!(listener.accept().is_err());
            thread::sleep(Duration::from_millis(5));
        }
    });
    let result = api
        .documents_original_mutation(
            "synthetic",
            Mutation::Enrich(7),
            serde_json::to_vec(&json!({"confirm":true,"metadata":{"source":"synthetic"}})).unwrap(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(result["errors"], json!(["Actual domain refusal"]));
    task.join().unwrap();
}
#[tokio::test]
async fn documents_original_pdf_service_uses_native_transport_and_strict_status() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let api = ApiClient::with_service_key(
        &format!("http://{}", listener.local_addr().unwrap()),
        true,
        "synthetic-service".into(),
    )
    .unwrap();
    let task = thread::spawn(move || {
        let start = Instant::now();
        let (mut socket, _) = loop {
            match listener.accept() {
                Ok(v) => break v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(start.elapsed() < Duration::from_secs(5));
                    thread::sleep(Duration::from_millis(5));
                }
                Err(e) => panic!("{e}"),
            }
        };
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut data = Vec::new();
        while !data.windows(4).any(|w| w == b"\r\n\r\n") {
            let mut b = [0; 1024];
            let n = socket.read(&mut b).unwrap();
            assert!(n > 0);
            data.extend_from_slice(&b[..n]);
        }
        let headers = String::from_utf8(data).unwrap().to_lowercase();
        assert!(headers.starts_with("get /api/companies/synthetic/documents/7/parse-status "));
        assert!(headers.contains("x-rentemester-service-key: synthetic-service"));
        assert!(!headers.contains("cookie:"));
        let body = r#"{"ok":true,"parse":null}"#;
        write!(
            socket,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        drop(socket);
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(150) {
            assert!(listener.accept().is_err());
            thread::sleep(Duration::from_millis(5));
        }
    });
    assert_eq!(
        api.document_parse_status("synthetic", 7).await.unwrap(),
        json!({"ok":true,"parse":null})
    );
    task.join().unwrap();
}

#[tokio::test]
async fn documents_original_auth_refusals_and_strict_422_never_release_untrusted_business_text() {
    for pdf in [false, true] {
        for (status, code) in [
            (401, Some("AUTH_DETAIL")),
            (403, Some("AUTH_DETAIL")),
            (422, None),
            (422, Some("  ")),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let api = ApiClient::new(
                &format!("http://{}", listener.local_addr().unwrap()),
                true,
                None,
            )
            .unwrap();
            let mut body = json!({"ok":false,"errors":["private authentication or proxy detail"]});
            if let Some(code) = code {
                body["code"] = json!(code);
            }
            let worker = thread::spawn(move || {
                let start = Instant::now();
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(start.elapsed() < Duration::from_secs(5));
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("{error}"),
                    }
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let mut bytes = [0; 1024];
                    let count = socket.read(&mut bytes).unwrap();
                    assert!(count > 0);
                    request.extend_from_slice(&bytes[..count]);
                }
                let body = body.to_string();
                write!(socket, "HTTP/1.1 {status} Refused\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                drop(socket);
                let until = Instant::now() + Duration::from_millis(150);
                while Instant::now() < until {
                    assert!(listener.accept().is_err());
                    thread::sleep(Duration::from_millis(5));
                }
            });
            let result = if pdf {
                api.document_parse_status("synthetic", 7).await
            } else {
                api.documents_original_list("synthetic").await
            };
            let error = result.unwrap_err();
            assert!(!error.to_string().contains("private authentication"));
            assert!(matches!(error, Error::Refused { .. } | Error::Protocol));
            worker.join().unwrap();
        }
    }
}
