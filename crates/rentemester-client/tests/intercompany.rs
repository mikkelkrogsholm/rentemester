mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error, IntercompanyMutation, MAX_INTERCOMPANY_BODY};
use std::{
    io::{Read, Write},
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
        let reply = r#"{"ok":true,"scope":"intercompany-reconciliation","rows":[{"left":{"balance":"125.50","sourceRefs":[{"amount":"125.50"}]},"difference":"0.00"}],"futureEvidence":{"retained":true}}"#;
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

const MANIFEST: &str = r#"{ "version":1,"groups":[{"id":"synthetic-group","name":"Synthetic Group","memberships":[{"id":"left-member","companySlug":"synthetic-left","validFrom":"2026-01-01"},{"id":"right-member","companySlug":"synthetic-right","validFrom":"2026-01-01"}],"ownership":[]}],"confirm":true }
"#;
const MAPPING: &str = r#"{ "id":"synthetic-map","groupId":"synthetic-group","leftCompanySlug":"synthetic-left","rightCompanySlug":"synthetic-right","leftAccountNos":["intercompany"],"rightAccountNos":["intercompany"],"leftPosition":"receivable","rightPosition":"payable","evidenceRefs":["reviewed-source"],"validFrom":"2026-01-01","confirm":true }
"#;
fn exact() -> String {
    format!(
        r#"{{ "mappingId":"synthetic-map","mappingHash":"{}","confirm":true }}"#,
        "a".repeat(64)
    )
}
#[tokio::test]
async fn six_exact_paths_bodies_and_full_decimal_string_projections() {
    for (action, path, bytes) in [
        (
            IntercompanyMutation::Manifest,
            "POST /api/companies/synthetic-left/intercompany/manifest HTTP/1.1",
            MANIFEST.to_string(),
        ),
        (
            IntercompanyMutation::Propose,
            "POST /api/companies/synthetic-left/intercompany/propose HTTP/1.1",
            MAPPING.to_string(),
        ),
        (
            IntercompanyMutation::Approve,
            "POST /api/companies/synthetic-left/intercompany/approve HTTP/1.1",
            exact(),
        ),
        (
            IntercompanyMutation::Revoke,
            "POST /api/companies/synthetic-left/intercompany/revoke HTTP/1.1",
            exact(),
        ),
    ] {
        let (origin, t) = server(path, &bytes, "200 OK");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let v = api
            .mutate_intercompany("synthetic-left", action, bytes.into_bytes(), true)
            .await
            .unwrap();
        assert_eq!(v["rows"][0]["left"]["balance"], "125.50");
        assert_eq!(v["rows"][0]["difference"], "0.00");
        assert_eq!(v["rows"][0]["left"]["sourceRefs"][0]["amount"], "125.50");
        assert_eq!(v["futureEvidence"]["retained"], true);
        t.join().unwrap();
    }
    for (global, path) in [
        (
            false,
            "GET /api/companies/synthetic-left/intercompany HTTP/1.1",
        ),
        (
            true,
            "GET /api/group-reconciliation?asOf=2026-06-01 HTTP/1.1",
        ),
    ] {
        let (origin, t) = server(path, "", "200 OK");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let v = if global {
            api.group_reconciliation("2026-06-01").await
        } else {
            api.intercompany("synthetic-left").await
        }
        .unwrap();
        assert_eq!(v["futureEvidence"]["retained"], true);
        t.join().unwrap();
    }
}
#[tokio::test]
async fn current_authority_refusal_and_unknown_write_are_once_without_keys_or_retries() {
    for (status, unknown) in [
        ("403 Forbidden", false),
        ("422 Unprocessable Entity", false),
        ("503 Service Unavailable", true),
    ] {
        let bytes = exact();
        let (origin, t) = server(
            "POST /api/companies/synthetic-left/intercompany/approve HTTP/1.1",
            &bytes,
            status,
        );
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        assert_eq!(
            matches!(
                api.mutate_intercompany(
                    "synthetic-left",
                    IntercompanyMutation::Approve,
                    bytes.into_bytes(),
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
        "GET /api/group-reconciliation?asOf=2026-06-01 HTTP/1.1",
        "",
        "503 Service Unavailable",
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.group_reconciliation("2026-06-01").await.unwrap_err(),
        Error::Refused { status: 503 }
    );
    t.join().unwrap();
}
#[tokio::test]
async fn source_text_is_preserved_and_domain_validation_stays_server_owned() {
    let bytes = MANIFEST.replace("Synthetic Group", &"æ".repeat(160));
    let (origin, t) = server(
        "POST /api/companies/synthetic-left/intercompany/manifest HTTP/1.1",
        &bytes,
        "200 OK",
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    api.mutate_intercompany(
        "synthetic-left",
        IntercompanyMutation::Manifest,
        bytes.into_bytes(),
        true,
    )
    .await
    .unwrap();
    t.join().unwrap();
    let bytes = MAPPING.replace(
        "\"rightPosition\":\"payable\"",
        "\"rightPosition\":\"receivable\"",
    );
    let (origin, t) = server(
        "POST /api/companies/synthetic-left/intercompany/propose HTTP/1.1",
        &bytes,
        "422 Unprocessable Entity",
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.mutate_intercompany(
            "synthetic-left",
            IntercompanyMutation::Propose,
            bytes.into_bytes(),
            true
        )
        .await
        .unwrap_err(),
        Error::Refused { status: 422 }
    );
    t.join().unwrap();
}
#[tokio::test]
async fn scope_confirmation_unknown_fields_and_size_refuse_before_transport() {
    let api = ApiClient::new("http://127.0.0.1:1", true, Some("session=synthetic".into())).unwrap();
    for date in [
        "",
        "2026-1-01",
        "2026-01-01&asOf=2025-01-01",
        "2026-01-01 ",
        "2026/01/01",
    ] {
        assert!(matches!(
            api.group_reconciliation(date).await,
            Err(Error::InvalidInput(_))
        ));
    }
    for bytes in [
        MANIFEST.replace("true", "false"),
        MANIFEST.replace("\"version\":1", "\"version\":1,\"actor\":\"forged\""),
        MANIFEST.replace("\"name\":", "\"arbitrary\":"),
        format!("{MANIFEST}{}", " ".repeat(MAX_INTERCOMPANY_BODY)),
    ] {
        assert!(matches!(
            api.mutate_intercompany(
                "synthetic-left",
                IntercompanyMutation::Manifest,
                bytes.into_bytes(),
                true
            )
            .await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.mutate_intercompany(
            "synthetic-left",
            IntercompanyMutation::Manifest,
            MANIFEST.as_bytes().to_vec(),
            false
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.intercompany("../outside").await,
        Err(Error::InvalidInput(_))
    ));
    let schema = rentemester_client::intercompany_body_schema(IntercompanyMutation::Propose);
    assert_eq!(schema["additionalProperties"], false);
    assert!(schema["properties"].get("idempotencyKey").is_none());
    assert_eq!(
        schema["properties"]["leftPosition"]["enum"],
        serde_json::json!(["receivable", "payable"])
    );
}
