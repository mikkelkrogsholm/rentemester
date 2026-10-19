use rentemester_client::{ApiClient, Error, JournalMutation, Report};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
    time::Duration,
};

fn server(response: &'static str) -> (String, mpsc::Receiver<Vec<u8>>, thread::JoinHandle<usize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (sender, receiver) = mpsc::channel();
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 4096];
            let size = stream.read(&mut chunk).unwrap();
            if size == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..size]);
            if let Some(header_end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..header_end]).to_lowercase();
                let content_length = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .map(|n| n.parse::<usize>().unwrap())
                    .unwrap_or(0);
                if bytes.len() >= header_end + 4 + content_length {
                    break;
                }
            }
        }
        sender.send(bytes).unwrap();
        stream.write_all(response.as_bytes()).unwrap();
        drop(stream);
        listener.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(180));
        let mut requests = 1;
        while listener.accept().is_ok() {
            requests += 1;
        }
        requests
    });
    (origin, receiver, task)
}
const OK: &str = "HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}";
const BODY: &[u8] = br#"{ "confirm":true, "input":{"idempotencyKey":"fixed-key","request":{"debit":"9007199254740993.01"}} }"#;

#[tokio::test]
async fn authenticated_report_has_exact_origin_host_and_encoded_query() {
    let (origin, request, task) = server(OK);
    let api = ApiClient::new(&origin, true, Some("session=secret".into())).unwrap();
    assert_eq!(
        api.report(
            "demo",
            Report::TrialBalance,
            Some("2026"),
            Some("1000 &x=2")
        )
        .await
        .unwrap()["ok"],
        true
    );
    let wire = String::from_utf8(request.recv().unwrap()).unwrap();
    let lower = wire.to_lowercase();
    assert!(wire.starts_with(
        "GET /api/companies/demo/trial-balance?year=2026&account=1000+%26x%3D2 HTTP/1.1"
    ));
    assert!(lower.contains(&format!(
        "\r\nhost: {}\r\n",
        origin.strip_prefix("http://").unwrap()
    )));
    assert!(lower.contains(&format!("\r\norigin: {origin}\r\n")));
    assert!(lower.contains("\r\ncookie: session=secret\r\n"));
    assert_eq!(task.join().unwrap(), 1);
}
#[tokio::test]
async fn writes_send_exact_bytes_and_do_not_retry_uncertain_response() {
    let (origin, request, task) = server(
        "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    let api = ApiClient::new(&origin, true, Some("session=secret".into())).unwrap();
    assert_eq!(
        api.manual_journal("demo", JournalMutation::Post, BODY.to_vec())
            .await,
        Err(Error::OutcomeUnknown)
    );
    let wire = request.recv().unwrap();
    assert!(wire.starts_with(b"POST /api/companies/demo/manual-journal HTTP/1.1"));
    assert!(wire.ends_with(BODY));
    assert_eq!(task.join().unwrap(), 1);
}
#[tokio::test]
async fn reversal_and_explicit_http_refusal() {
    let (origin, request, task) = server(
        "HTTP/1.1 422 Unprocessable Entity\r\nContent-Length: 6\r\nConnection: close\r\n\r\nsecret",
    );
    let api = ApiClient::new(&origin, true, None).unwrap();
    let error = api
        .manual_journal("demo", JournalMutation::Reverse, BODY.to_vec())
        .await
        .unwrap_err();
    assert_eq!(error, Error::Refused { status: 422 });
    assert!(!format!("{error:?} {error}").contains("secret"));
    assert!(
        request
            .recv()
            .unwrap()
            .starts_with(b"POST /api/companies/demo/manual-journal-reversal ")
    );
    assert_eq!(task.join().unwrap(), 1);
}
#[tokio::test]
async fn redirects_never_follow_or_forward_credentials() {
    let (origin, request, task) = server(
        "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/stolen\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    let api = ApiClient::new(&origin, true, Some("session=secret".into())).unwrap();
    assert_eq!(api.me().await, Err(Error::Refused { status: 302 }));
    request.recv().unwrap();
    assert_eq!(task.join().unwrap(), 1);
}
#[tokio::test]
async fn truncated_successful_write_is_unknown() {
    let (origin, request, task) =
        server("HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{}");
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.manual_journal("demo", JournalMutation::Post, BODY.to_vec())
            .await,
        Err(Error::OutcomeUnknown)
    );
    request.recv().unwrap();
    assert_eq!(task.join().unwrap(), 1);
}
#[test]
fn fail_closed_configuration_and_secret_redaction() {
    for origin in [
        "http://example.com",
        "http://localhost:8080",
        "https://u:password@example.com",
        "https://example.com/path",
        "https://example.com?token=secret",
        "https://example.com#secret",
        "file:///tmp/x",
    ] {
        let error = ApiClient::new(origin, true, None).err().unwrap();
        assert!(!format!("{error:?} {error}").contains("password"));
        assert!(!format!("{error:?} {error}").contains("secret"));
    }
    assert!(ApiClient::new("http://127.0.0.1:8080", false, None).is_err());
    assert!(ApiClient::new("http://[::1]:8080", true, None).is_ok());
    assert!(ApiClient::new("https://example.com", false, Some("secret\r\nx: y".into())).is_err());
}
#[tokio::test]
async fn mutation_requires_key_confirmation_and_safe_company_before_network() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    assert!(matches!(
        api.manual_journal("demo", JournalMutation::Post, b"{}".to_vec())
            .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.manual_journal("../elsewhere", JournalMutation::Post, BODY.to_vec())
            .await,
        Err(Error::InvalidInput(_))
    ));
}
