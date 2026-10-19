use rentemester_client::{
    ApiClient, CorporateRecordMutation, Error, MAX_CORPORATE_BODY, MAX_CORPORATE_INGEST_BODY,
    MAX_CORPORATE_ORIGINAL_JSON,
};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::Duration,
};
const INGEST: &str = r#"{ "type":"articles","recordId":"record-synthetic","bytesBase64":"AP8=","filename":"Synthetic.txt","source":"Synthetic brief","receivedAt":"2026-01-01","uploader":"Synthetic selected uploader","sensitivity":null,"links":[{"type":"company","id":"synthetic"}],"recordDate":null,"retentionUntil":null,"confirm":true }
"#;
fn server(path: &str, body: &str, status: &str, reply: String) -> (String, thread::JoinHandle<()>) {
    let path = path.to_owned();
    let body = body.to_owned();
    let status = status.to_owned();
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut bytes = vec![];
        loop {
            let mut b = [0; 4096];
            let n = s.read(&mut b).unwrap();
            assert_ne!(n, 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let h = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let len = h
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .map(|s| s.parse::<usize>().unwrap())
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + len {
                    assert_eq!(&bytes[end + 4..], body.as_bytes());
                    break;
                }
            }
        }
        assert!(bytes.starts_with(path.as_bytes()));
        assert!(
            String::from_utf8_lossy(&bytes)
                .to_lowercase()
                .contains("cookie: session=synthetic")
        );
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
const REPLY: &str =
    r#"{"ok":true,"record":{"futureSourceEvidence":true},"warnings":["server advisory"]}"#;
#[tokio::test]
async fn exact_native_writes_and_full_canonical_reads() {
    for (action, path, body) in [
        (
            CorporateRecordMutation::Ingest,
            "POST /api/companies/synthetic/corporate-records HTTP/1.1",
            INGEST,
        ),
        (
            CorporateRecordMutation::Link("record+synthetic".into()),
            "POST /api/companies/synthetic/corporate-records/record%2Bsynthetic/link HTTP/1.1",
            r#"{ "recordId":"record+synthetic","type":"company","id":"other","confirm":true }"#,
        ),
        (
            CorporateRecordMutation::Enrich("record-synthetic".into()),
            "POST /api/companies/synthetic/corporate-records/record-synthetic/enrich HTTP/1.1",
            r#"{ "assertion":"Explicit reviewed source assertion","confirm":true }"#,
        ),
        (
            CorporateRecordMutation::Supersede("record-synthetic".into()),
            "POST /api/companies/synthetic/corporate-records/record-synthetic/supersede HTTP/1.1",
            r#"{ "replacementRecordId":"record-other","reason":"Explicit reviewed replacement","confirm":true }"#,
        ),
    ] {
        let (origin, t) = server(path, body, "200 OK", REPLY.into());
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        assert_eq!(
            api.mutate_corporate_record("synthetic", action, body.as_bytes().to_vec(), true)
                .await
                .unwrap()["record"]["futureSourceEvidence"],
            true
        );
        t.join().unwrap();
    }
    for (path, inspect) in [
        (
            "GET /api/companies/synthetic/corporate-records?cursor=2&limit=10 HTTP/1.1",
            false,
        ),
        (
            "GET /api/companies/synthetic/corporate-records/record%2Bsynthetic HTTP/1.1",
            true,
        ),
    ] {
        let (origin, t) = server(path, "", "200 OK", REPLY.into());
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let v = if inspect {
            api.corporate_record("synthetic", "record+synthetic").await
        } else {
            api.corporate_records("synthetic", 2, 10).await
        };
        assert_eq!(v.unwrap()["record"]["futureSourceEvidence"], true);
        t.join().unwrap();
    }
    let body = INGEST.replace(
        r#""links":[{"type":"company","id":"synthetic"}]"#,
        r#""links":null"#,
    );
    let (origin, t) = server(
        "POST /api/companies/synthetic/corporate-records HTTP/1.1",
        &body,
        "201 Created",
        REPLY.into(),
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert!(
        api.mutate_corporate_record(
            "synthetic",
            CorporateRecordMutation::Ingest,
            body.into_bytes(),
            true
        )
        .await
        .is_ok()
    );
    t.join().unwrap();
}
#[tokio::test]
async fn original_json_exceeds_generic_response_bound_without_invented_envelope() {
    let reply = serde_json::json!({"ok":true,"bytesBase64":"YWFh".repeat(4_500_000),"sha256":"a".repeat(64),"futureEvidence":{"retained":true}});
    let (origin, t) = server(
        "GET /api/companies/synthetic/corporate-records/record-synthetic/download HTTP/1.1",
        "",
        "200 OK",
        reply.to_string(),
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.corporate_record_original("synthetic", "record-synthetic")
            .await
            .unwrap(),
        reply
    );
    t.join().unwrap();
    let (origin, t) = server(
        "GET /api/companies/synthetic/corporate-records/record-synthetic/download HTTP/1.1",
        "",
        "200 OK",
        "not-json".into(),
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.corporate_record_original("synthetic", "record-synthetic")
            .await
            .unwrap_err(),
        Error::Protocol
    );
    t.join().unwrap();
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        let mut b = [0; 4096];
        assert!(s.read(&mut b).unwrap() > 0);
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length:{}\r\nConnection:close\r\n\r\n",
            MAX_CORPORATE_ORIGINAL_JSON + 1
        )
        .unwrap();
    });
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.corporate_record_original("synthetic", "record-synthetic")
            .await
            .unwrap_err(),
        Error::Protocol
    );
    t.join().unwrap();
}
#[tokio::test]
async fn refused_and_unknown_outcomes_are_distinct_and_never_retried() {
    for (status, unknown) in [
        ("403 Forbidden", false),
        ("404 Not Found", false),
        ("422 Unprocessable Entity", false),
        ("503 Service Unavailable", true),
    ] {
        let (origin, t) = server(
            "POST /api/companies/synthetic/corporate-records HTTP/1.1",
            INGEST,
            status,
            REPLY.into(),
        );
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        assert_eq!(
            matches!(
                api.mutate_corporate_record(
                    "synthetic",
                    CorporateRecordMutation::Ingest,
                    INGEST.as_bytes().to_vec(),
                    true
                )
                .await
                .unwrap_err(),
                Error::OutcomeUnknown
            ),
            unknown
        );
        t.join().unwrap();
    }
    let (origin, t) = server(
        "GET /api/companies/synthetic/corporate-records/record-synthetic/download HTTP/1.1",
        "",
        "503 Service Unavailable",
        REPLY.into(),
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.corporate_record_original("synthetic", "record-synthetic")
            .await
            .unwrap_err(),
        Error::Refused { status: 503 }
    );
    t.join().unwrap();
}
#[tokio::test]
async fn invalid_confirmation_shape_path_and_limits_refuse_before_network() {
    let api = ApiClient::new("http://127.0.0.1:1", true, Some("session=synthetic".into())).unwrap();
    for body in [
        INGEST.replace("\"confirm\":true", "\"confirm\":false"),
        INGEST.replace("\"source\":", "\"actor\":"),
        INGEST.replace("\"uploader\":", "\"idempotencyKey\":"),
        INGEST.replace(
            "\"links\":[{\"type\":\"company\",\"id\":\"synthetic\"}]",
            "\"links\":{}",
        ),
        " ".repeat(MAX_CORPORATE_INGEST_BODY + 1),
    ] {
        assert!(matches!(
            api.mutate_corporate_record(
                "synthetic",
                CorporateRecordMutation::Ingest,
                body.into_bytes(),
                true
            )
            .await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.mutate_corporate_record(
            "synthetic",
            CorporateRecordMutation::Ingest,
            INGEST.as_bytes().to_vec(),
            false
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.mutate_corporate_record(
            "synthetic",
            CorporateRecordMutation::Enrich("record".into()),
            vec![b' '; MAX_CORPORATE_BODY + 1],
            true
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
    for id in ["", ".", "..", "\n", &"x".repeat(65)] {
        assert!(matches!(
            api.corporate_record_original("synthetic", id).await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.corporate_records("synthetic", 0, 101).await,
        Err(Error::InvalidInput(_))
    ));
}
