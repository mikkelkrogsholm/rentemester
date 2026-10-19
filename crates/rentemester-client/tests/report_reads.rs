use rentemester_client::{ApiClient, Error, Report};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};
fn server(reply: &'static [u8], mime: &'static str) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = vec![];
        while !bytes.ends_with(b"\r\n\r\n") {
            let mut buf = [0; 4096];
            let n = s.read(&mut buf).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buf[..n]);
        }
        write!(s,"HTTP/1.1 200 OK\r\nContent-Length:{}\r\nContent-Type:{mime}\r\nConnection:close\r\n\r\n",reply.len()).unwrap();
        s.write_all(reply).unwrap();
        String::from_utf8(bytes).unwrap()
    });
    (origin, t)
}
#[tokio::test]
async fn all_five_reads_preserve_explicit_source_and_exact_server_values() {
    const REPLY:&[u8]=br#"{"ok":true,"amount":"9007199254740993.01","prior":null,"source":"archive","future":true}"#;
    for (report, path) in [
        (Report::Journal, "journal"),
        (Report::TrialBalance, "trial-balance"),
        (Report::IncomeStatement, "income-statement"),
        (Report::Balance, "balance"),
        (Report::Overview, "overview"),
    ] {
        let (origin, t) = server(REPLY, "application/json");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        assert_eq!(
            api.report_source("synthetic", report, Some("2024"), None, Some("archive"))
                .await
                .unwrap(),
            serde_json::from_slice::<serde_json::Value>(REPLY).unwrap()
        );
        assert!(t.join().unwrap().starts_with(&format!(
            "GET /api/companies/synthetic/{path}?year=2024&source=archive HTTP/1.1"
        )));
    }
}
#[tokio::test]
async fn export_preserves_binary_bytes_and_rejects_unexpected_mime_and_source() {
    const BYTES: &[u8] = b"%PDF-1.4\n\x00\xff\n%%EOF";
    let (origin, t) = server(BYTES, "application/pdf");
    let api = ApiClient::new(&origin, true, None).unwrap();
    let value = api
        .report_export(
            "synthetic",
            Report::Journal,
            Some("2024"),
            Some("1000 &x=1"),
            Some("archive"),
            "pdf",
        )
        .await
        .unwrap();
    assert_eq!(value["bytes"], serde_json::json!(BYTES));
    assert_eq!(value["contentType"], "application/pdf");
    assert!(t.join().unwrap().starts_with("GET /api/companies/synthetic/journal/export?year=2024&account=1000+%26x%3D1&source=archive&format=pdf HTTP/1.1"));
    let (origin, t) = server(BYTES, "text/html");
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.report_export("synthetic", Report::Balance, None, None, None, "pdf")
            .await,
        Err(Error::Protocol)
    );
    t.join().unwrap();
    assert!(matches!(
        api.report_source("synthetic", Report::Balance, None, None, Some("guessed"))
            .await,
        Err(Error::InvalidInput(_))
    ));
}
