mod common;
use common::TcpListener;
use rentemester_client::{
    ApiClient, Error, MAX_WORKSPACE_BACKUP_BYTES, WORKSPACE_BACKUP_CONTENT_TYPE,
};
use std::{
    io::{Read, Write},
    thread,
    time::Duration,
};
fn serve(
    status: &'static str,
    mime: &str,
    length: usize,
    data: Vec<u8>,
) -> (String, thread::JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let mime = mime.to_owned();
    let task = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut request = vec![];
        loop {
            let mut b = [0; 1024];
            let n = socket.read(&mut b).unwrap();
            assert!(n > 0);
            request.extend_from_slice(&b[..n]);
            if request.ends_with(br#"{"confirm":true}"#) {
                break;
            }
        }
        let head = String::from_utf8_lossy(&request).to_lowercase();
        assert!(head.starts_with("post /api/workspace/backup http/1.1\r\n"));
        assert!(head.contains("cookie: session=synthetic"));
        assert!(head.contains("origin: http://127.0.0.1:"));
        assert!(head.contains("content-type: application/json"));
        let split = request.windows(4).position(|v| v == b"\r\n\r\n").unwrap() + 4;
        assert_eq!(&request[split..], br#"{"confirm":true}"#);
        write!(socket,"HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n").unwrap();
        let _ = socket.write_all(&data);
        drop(socket);
        listener.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(40));
        assert!(listener.accept().is_err(), "no retry");
        request
    });
    (origin, task)
}
#[tokio::test]
async fn workspace_backup_exact_wire_bytes_and_current_failure_envelopes() {
    let original = vec![0, 255, 123, 34, 10, 13, 125];
    for (status, mime, length, data, expected) in [
        (
            "200 OK",
            WORKSPACE_BACKUP_CONTENT_TYPE,
            original.len(),
            original.clone(),
            None,
        ),
        (
            "403 Forbidden",
            WORKSPACE_BACKUP_CONTENT_TYPE,
            6,
            b"SECRET".to_vec(),
            Some(Error::Refused { status: 403 }),
        ),
        (
            "422 Unprocessable Entity",
            WORKSPACE_BACKUP_CONTENT_TYPE,
            6,
            b"SECRET".to_vec(),
            Some(Error::Refused { status: 422 }),
        ),
        (
            "503 Unavailable",
            WORKSPACE_BACKUP_CONTENT_TYPE,
            6,
            b"SECRET".to_vec(),
            Some(Error::OutcomeUnknown),
        ),
        (
            "200 OK",
            "application/json",
            original.len(),
            original.clone(),
            Some(Error::OutcomeUnknown),
        ),
        (
            "200 OK",
            WORKSPACE_BACKUP_CONTENT_TYPE,
            0,
            vec![],
            Some(Error::OutcomeUnknown),
        ),
        (
            "200 OK",
            WORKSPACE_BACKUP_CONTENT_TYPE,
            original.len() + 1,
            original.clone(),
            Some(Error::OutcomeUnknown),
        ),
        (
            "200 OK",
            WORKSPACE_BACKUP_CONTENT_TYPE,
            MAX_WORKSPACE_BACKUP_BYTES + 1,
            vec![],
            Some(Error::OutcomeUnknown),
        ),
    ] {
        let (origin, task) = serve(status, mime, length, data);
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let result = api.workspace_backup(true).await;
        if let Some(error) = expected {
            assert_eq!(result.unwrap_err(), error);
        } else {
            let result = result.unwrap();
            assert_eq!(
                result,
                serde_json::json!({"ok":true,"backup":{"contentType":WORKSPACE_BACKUP_CONTENT_TYPE,"bytes":original}})
            );
        }
        task.join().unwrap();
    }
}
#[tokio::test]
async fn workspace_backup_confirmation_precedes_transport() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    assert!(matches!(
        api.workspace_backup(false).await,
        Err(Error::InvalidInput(_))
    ));
}
#[tokio::test]
async fn workspace_backup_actual_stream_is_bounded_without_content_length() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut request = vec![];
        loop {
            let mut b = [0; 1024];
            let n = socket.read(&mut b).unwrap();
            assert!(n > 0);
            request.extend_from_slice(&b[..n]);
            if request.ends_with(br#"{"confirm":true}"#) {
                break;
            }
        }
        write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: {WORKSPACE_BACKUP_CONTENT_TYPE}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").unwrap();
        let chunk = [b'x'; 16384];
        for _ in 0..=MAX_WORKSPACE_BACKUP_BYTES / chunk.len() {
            if socket
                .write_all(b"4000\r\n")
                .and_then(|_| socket.write_all(&chunk))
                .and_then(|_| socket.write_all(b"\r\n"))
                .is_err()
            {
                break;
            }
        }
        let _ = socket.write_all(b"0\r\n\r\n");
    });
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.workspace_backup(true).await.unwrap_err(),
        Error::OutcomeUnknown
    );
    task.join().unwrap();
}
