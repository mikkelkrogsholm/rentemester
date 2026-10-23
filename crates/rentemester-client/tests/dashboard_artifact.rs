mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, DASHBOARD_CONTENT_TYPE, Error};
use std::{
    io::{Read, Write},
    thread,
};
fn server(reply: &'static [u8], mime: &'static str) -> (String, thread::JoinHandle<String>) {
    server_response(
        reply,
        mime,
        "dashboard-synthetic-2024-02-29.html",
        "0.25",
        "200 OK",
        None,
    )
}
fn server_response(
    reply: &'static [u8],
    mime: &'static str,
    filename: &'static str,
    render_ms: &'static str,
    status: &'static str,
    declared: Option<usize>,
) -> (String, thread::JoinHandle<String>) {
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
        write!(s,"HTTP/1.1 {status}\r\nContent-Length:{}\r\nContent-Type:{mime}\r\nContent-Disposition:attachment; filename=\"{filename}\"\r\nx-rentemester-as-of:2024-02-29\r\nx-rentemester-generated-at:2024-02-29T12:00:00Z\r\nx-rentemester-commit-sha:unknown\r\nx-rentemester-rule-bundle-version:synthetic1\r\nx-rentemester-render-ms:{render_ms}\r\nConnection:close\r\n\r\n",declared.unwrap_or(reply.len())).unwrap();
        let _ = s.write_all(reply);
        String::from_utf8(bytes).unwrap()
    });
    (origin, t)
}

#[tokio::test]
async fn exact_html_cookie_selectors_and_metadata() {
    let html = "<!doctype html><p>Ærlig &amp; dansk</p>".as_bytes();
    let (origin, t) = server(html, DASHBOARD_CONTENT_TYPE);
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    let v = api
        .dashboard_export("synthetic", Some("2024-02-29"))
        .await
        .unwrap();
    assert_eq!(v["bytes"], serde_json::json!(html));
    assert_eq!(v["filename"], "dashboard-synthetic-2024-02-29.html");
    assert_eq!(v["asOfDate"], "2024-02-29");
    assert_eq!(v["renderMs"], 0.25);
    let wire = t.join().unwrap();
    assert!(wire.starts_with(
        "GET /api/companies/synthetic/dashboard/export?format=html&asOf=2024-02-29 HTTP/1.1"
    ));
    assert!(wire.to_lowercase().contains("cookie: session=synthetic"));
    assert!(matches!(
        api.dashboard_export("synthetic", Some("2024-02-30")).await,
        Err(Error::InvalidInput(_))
    ));
}
#[tokio::test]
async fn default_date_stays_server_owned_and_bad_mime_is_refused() {
    for mime in [DASHBOARD_CONTENT_TYPE, "text/html"] {
        let (origin, t) = server(b"<p>html</p>", mime);
        let api = ApiClient::new(&origin, true, None).unwrap();
        let r = api.dashboard_export("synthetic", None).await;
        if mime == DASHBOARD_CONTENT_TYPE {
            assert_eq!(r.unwrap()["asOfDate"], "2024-02-29");
        } else {
            assert_eq!(r, Err(Error::Protocol));
        }
        assert!(
            t.join()
                .unwrap()
                .starts_with("GET /api/companies/synthetic/dashboard/export?format=html HTTP/1.1")
        );
    }
}

#[tokio::test]
async fn unsafe_filename_oversize_and_invalid_metadata_are_refused() {
    for (filename, ms, declared) in [
        ("../escape.html", "0.25", None),
        ("safe.html", "NaN", None),
        ("safe.html", "-1", None),
        (
            "safe.html",
            "0.25",
            Some(rentemester_client::MAX_DASHBOARD_BYTES + 1),
        ),
    ] {
        let (origin, t) = server_response(
            b"<p>ok</p>",
            DASHBOARD_CONTENT_TYPE,
            filename,
            ms,
            "200 OK",
            declared,
        );
        let api = ApiClient::new(&origin, true, None).unwrap();
        assert_eq!(
            api.dashboard_export("synthetic", None).await,
            Err(Error::Protocol)
        );
        t.join().unwrap();
    }
    let refusal = br#"{"ok":false,"code":"permission_denied","details":{"retained":true}}"#;
    let (origin, t) = server_response(
        refusal,
        "application/json",
        "unused",
        "0",
        "403 Forbidden",
        None,
    );
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.dashboard_export("synthetic", None).await.unwrap(),
        serde_json::from_slice::<serde_json::Value>(refusal).unwrap()
    );
    t.join().unwrap();
}
