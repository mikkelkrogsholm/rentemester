use rentemester_client::{ApiClient, Error, MAX_PRIVACY_BODY};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};
fn server(
    path: &'static str,
    body: &str,
    status: &'static str,
) -> (String, thread::JoinHandle<()>) {
    let body = body.to_owned();
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut bytes = vec![];
        loop {
            let mut chunk = [0; 4096];
            let n = s.read(&mut chunk).unwrap();
            bytes.extend_from_slice(&chunk[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let h = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let size = h
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .map(|s| s.parse::<usize>().unwrap())
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + size {
                    break;
                }
            }
        }
        let wire = String::from_utf8(bytes).unwrap();
        assert!(wire.starts_with(path));
        assert!(wire.ends_with(&body));
        assert!(wire.to_lowercase().contains("cookie: session=synthetic"));
        let reply = r#"{"ok":true,"coverage":{"complete":false,"unknown":["backups"]},"records":[{"retentionUnknown":true,"retainUntil":null,"erasable":false}],"refused":[{"reason":"native retention"}]}"#;
        write!(
            s,
            "HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
            reply.len()
        )
        .unwrap();
        drop(s);
        l.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(70));
        assert!(l.accept().is_err(), "must never retry");
    });
    (origin, t)
}
#[tokio::test]
async fn export_and_overlay_exact_confirmed_bytes_once_with_typed_outcomes() {
    for (erase, path, body) in [
        (
            false,
            "POST /api/companies/synthetic/gdpr/export HTTP/1.1",
            "{ \"cvr\":\"  SYN-SUBJECT  \",\"name\":\"Synthetic ø subject\",\"asOf\":\"2026-01-31\",\"confirm\":true }\n",
        ),
        (
            true,
            "POST /api/companies/synthetic/gdpr/erase HTTP/1.1",
            "{ \"cvr\":\"SYN-SUBJECT\",\"confirm\":true }\n",
        ),
    ] {
        for status in ["200 OK", "422 Unprocessable Entity", "503 Unavailable"] {
            let (origin, t) = server(path, body, status);
            let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
            let out = if erase {
                api.gdpr_erase("synthetic", body.as_bytes().to_vec(), true)
                    .await
            } else {
                api.gdpr_export("synthetic", body.as_bytes().to_vec(), true)
                    .await
            };
            match status {
                "200 OK" => {
                    let v = out.unwrap();
                    assert_eq!(v["coverage"]["complete"], false);
                    assert!(v["records"][0]["retainUntil"].is_null());
                    assert_eq!(v["records"][0]["retentionUnknown"], true);
                    assert_eq!(v["refused"][0]["reason"], "native retention");
                }
                "422 Unprocessable Entity" => {
                    assert_eq!(out.unwrap_err(), Error::Refused { status: 422 })
                }
                _ => assert_eq!(out.unwrap_err(), Error::OutcomeUnknown),
            }
            t.join().unwrap();
        }
    }
}
#[tokio::test]
async fn retention_is_queryless_read_with_known_failure() {
    let (origin, t) = server(
        "GET /api/companies/synthetic/retention HTTP/1.1",
        "",
        "503 Unavailable",
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.retention("synthetic").await.unwrap_err(),
        Error::Refused { status: 503 }
    );
    t.join().unwrap();
}
#[tokio::test]
async fn erase_never_accepts_caller_date_keys_actor_or_missing_scope() {
    let api = ApiClient::new("http://127.0.0.1:9", true, Some("session=synthetic".into())).unwrap();
    for body in [
        r#"{"cvr":"SYN","asOf":null,"confirm":true}"#.to_string(),
        r#"{"cvr":"SYN","asOf":"9999-12-31","confirm":true}"#.into(),
        r#"{"cvr":"SYN","actor":"user:other","confirm":true}"#.into(),
        r#"{"cvr":"SYN","idempotencyKey":"invented","confirm":true}"#.into(),
        r#"{"name":" ","confirm":true}"#.into(),
        r#"{"confirm":true}"#.into(),
        r#"{"cvr":"SYN","confirm":false}"#.into(),
        " ".repeat(MAX_PRIVACY_BODY + 1),
    ] {
        assert!(matches!(
            api.gdpr_erase("synthetic", body.into_bytes(), true).await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.gdpr_export(
            "synthetic",
            br#"{"cvr":"SYN","confirm":true}"#.to_vec(),
            false
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
}
