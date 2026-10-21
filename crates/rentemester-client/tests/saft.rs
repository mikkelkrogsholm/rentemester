use rentemester_client::{ApiClient, Error};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};
const BODY: &str =
    "{ \"periodStart\":\"2026-07-01\",\"periodEnd\":\"2027-06-30\",\"confirm\":true }\n";
fn server(
    status: &'static str,
    mime: &'static str,
    truncate: bool,
) -> (String, thread::JoinHandle<()>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut b = vec![];
        loop {
            let mut chunk = [0; 4096];
            let n = s.read(&mut chunk).unwrap();
            assert!(n > 0);
            b.extend_from_slice(&chunk[..n]);
            if b.ends_with(BODY.as_bytes()) {
                break;
            }
        }
        let h = String::from_utf8_lossy(&b).to_lowercase();
        assert!(h.starts_with("post /api/companies/synthetic/saft-export http/1.1"));
        assert!(h.contains("cookie: session=synthetic"));
        assert!(h.contains("origin: http://127.0.0.1:"));
        write!(s,"HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length:1024\r\nConnection:close\r\n\r\n").unwrap();
        s.write_all(&vec![7; if truncate { 16 } else { 1024 }])
            .unwrap();
        drop(s);
        l.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(60));
        assert!(l.accept().is_err(), "export must never automatically retry");
    });
    (origin, t)
}
#[tokio::test]
async fn exact_audited_post_binary_refusal_and_uncertain_outcomes() {
    for (status, mime, truncate) in [
        ("200 OK", "application/x-tar", false),
        ("422 Unprocessable Entity", "application/x-tar", false),
        ("503 Unavailable", "application/x-tar", false),
        ("200 OK", "text/plain", false),
        ("200 OK", "application/x-tar", true),
    ] {
        let (origin, t) = server(status, mime, truncate);
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let out = api
            .saft_export("synthetic", BODY.as_bytes().to_vec(), true)
            .await;
        match (status, mime, truncate) {
            ("200 OK", "application/x-tar", false) => {
                let v = out.unwrap();
                assert_eq!(v["export"]["contentType"], "application/x-tar");
                assert_eq!(v["export"]["bytes"], serde_json::json!(vec![7; 1024]));
            }
            ("422 Unprocessable Entity", _, _) => {
                assert_eq!(out.unwrap_err(), Error::Refused { status: 422 })
            }
            _ => assert_eq!(out.unwrap_err(), Error::OutcomeUnknown),
        }
        t.join().unwrap();
    }
}
#[tokio::test]
async fn malformed_confirmation_scope_or_body_refuses_before_network() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for(company,bytes,confirmed)in[("synthetic",BODY.as_bytes().to_vec(),false),("../escape",BODY.as_bytes().to_vec(),true),("synthetic",br#"{"periodStart":"2026-01-01","periodEnd":"2026-12-31","confirm":true,"generatedAt":"forged"}"#.to_vec(),true),("synthetic",vec![b' ';4097],true)]{assert!(matches!(api.saft_export(company,bytes,confirmed).await,Err(Error::InvalidInput(_))));}
}

#[tokio::test]
async fn explicit_generation_modes_forward_exact_bytes_and_server_owns_calendar_validation() {
    for extra in [
        serde_json::json!({"generationMode":"period_end"}),
        serde_json::json!({"generationMode":"caller_supplied","generatedAt":"2027-01-01T00:59:59.123456+01:00"}),
        serde_json::json!({"generationMode":"caller_supplied","generatedAt":"2026-02-30T12:00:00Z"}),
    ] {
        let mut value =
            serde_json::json!({"periodStart":"2026-01-01","periodEnd":"2026-12-31","confirm":true});
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let bytes = serde_json::to_vec(&value).unwrap();
        let expected = bytes.clone();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let task = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut request = vec![];
            while !request.ends_with(&expected) {
                let mut b = [0; 4096];
                let n = s.read(&mut b).unwrap();
                assert!(n > 0);
                request.extend(&b[..n]);
            }
            s.write_all(b"HTTP/1.1 422 Unprocessable Entity\r\nContent-Length:0\r\nConnection:close\r\n\r\n").unwrap();
        });
        let api = ApiClient::new(&origin, true, None).unwrap();
        assert_eq!(
            api.saft_export("synthetic", bytes, true).await.unwrap_err(),
            Error::Refused { status: 422 }
        );
        task.join().unwrap();
    }
}
#[tokio::test]
async fn incompatible_generation_mode_combinations_refuse_before_http() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for extra in [
        serde_json::json!({"generatedAt":"2027-01-01T00:00:00Z"}),
        serde_json::json!({"generationMode":"period_end","generatedAt":"2027-01-01T00:00:00Z"}),
        serde_json::json!({"generationMode":"caller_supplied"}),
        serde_json::json!({"generationMode":"unknown"}),
        serde_json::json!({"generationMode":null}),
        serde_json::json!({"generationMode":"caller_supplied","generatedAt":null}),
    ] {
        let mut value =
            serde_json::json!({"periodStart":"2026-01-01","periodEnd":"2026-12-31","confirm":true});
        value
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        assert!(matches!(
            api.saft_export("synthetic", serde_json::to_vec(&value).unwrap(), true)
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
}
