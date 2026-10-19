use rentemester_client::{
    ApiClient, ConsolidationMutation, Error, MAX_CONSOLIDATION_BODY, MAX_CONSOLIDATION_PROPOSE_BODY,
};
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
        let reply = r#"{"ok":true,"scope":"native-consolidation","rows":[{"left":{"balance":"125.50","sourceRefs":[{"amount":"125.50"}]},"difference":"0.00"}],"futureEvidence":{"retained":true}}"#;
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

fn proposal() -> String {
    serde_json::json!({"profile":{"version":1,"id":"synthetic-profile","groupId":"synthetic-group","currency":"DKK","validFrom":"2026-01-01","reportingLines":[{"id":"assets","label":"Ægte kilde","section":"asset","displayOrder":0},{"id":"liabilities","label":"Liabilities","section":"liability","displayOrder":1},{"id":"equity","label":"Equity","section":"equity","displayOrder":2,"role":"current-result"},{"id":"income","label":"Income","section":"income","displayOrder":3},{"id":"expense","label":"Expense","section":"expense","displayOrder":4}],"accountMappings":[{"id":"map","companySlug":"synthetic-left","accountNo":"source-account","reportingLineId":"assets","validFrom":"2026-01-01","validToExclusive":null}],"evidenceRefs":["reviewed-source"]},"confirm":true}).to_string()
}
fn approve() -> String {
    format!(
        r#"{{ "profileId":"synthetic-profile","profileHash":"{}","confirm":true }}
"#,
        "a".repeat(64)
    )
}
#[tokio::test]
async fn six_exact_native_routes_preserve_request_and_full_decimal_source_projection() {
    for (action, path, bytes) in [
        (
            ConsolidationMutation::Propose,
            "POST /api/companies/synthetic-left/consolidation/propose HTTP/1.1",
            proposal(),
        ),
        (
            ConsolidationMutation::Approve,
            "POST /api/companies/synthetic-left/consolidation/approve HTTP/1.1",
            approve(),
        ),
        (
            ConsolidationMutation::Revoke,
            "POST /api/companies/synthetic-left/consolidation/revoke HTTP/1.1",
            r#"{ "profileId":"synthetic-profile","confirm":true }"#.into(),
        ),
    ] {
        let (origin, t) = server(path, &bytes, "200 OK");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let v = api
            .mutate_consolidation("synthetic-left", action, bytes.into_bytes(), true)
            .await
            .unwrap();
        assert_eq!(v["rows"][0]["left"]["balance"], "125.50");
        assert_eq!(v["rows"][0]["difference"], "0.00");
        assert_eq!(v["futureEvidence"]["retained"], true);
        t.join().unwrap();
    }
    for (kind, path) in [
        (
            0,
            "GET /api/companies/synthetic-left/consolidation?asOf=2026-06-01 HTTP/1.1",
        ),
        (1, "GET /api/group-report-profiles?asOf=2026-06-01 HTTP/1.1"),
        (
            2,
            "GET /api/group-consolidated-report?profileId=synthetic-profile&from=2026-01-01&asOf=2026-06-01 HTTP/1.1",
        ),
    ] {
        let (origin, t) = server(path, "", "200 OK");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let v = match kind {
            0 => api.consolidation("synthetic-left", "2026-06-01").await,
            1 => api.group_report_profiles("2026-06-01").await,
            _ => {
                api.group_consolidated_report("synthetic-profile", "2026-01-01", "2026-06-01")
                    .await
            }
        }
        .unwrap();
        assert_eq!(v["rows"][0]["left"]["sourceRefs"][0]["amount"], "125.50");
        assert_eq!(v["futureEvidence"]["retained"], true);
        t.join().unwrap();
    }
}
#[tokio::test]
async fn exact_review_refusals_and_unknown_mutation_are_once_no_retries() {
    for (status, unknown) in [
        ("403 Forbidden", false),
        ("422 Unprocessable Entity", false),
        ("503 Service Unavailable", true),
    ] {
        let bytes = approve();
        let (origin, t) = server(
            "POST /api/companies/synthetic-left/consolidation/approve HTTP/1.1",
            &bytes,
            status,
        );
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        assert_eq!(
            matches!(
                api.mutate_consolidation(
                    "synthetic-left",
                    ConsolidationMutation::Approve,
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
        "GET /api/group-report-profiles?asOf=2026-06-01 HTTP/1.1",
        "",
        "503 Service Unavailable",
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.group_report_profiles("2026-06-01").await.unwrap_err(),
        Error::Refused { status: 503 }
    );
    t.join().unwrap();
}
#[tokio::test]
async fn confirmation_source_scope_unknown_fields_and_operation_body_limits_refuse_before_http() {
    let api = ApiClient::new("http://127.0.0.1:1", true, None).unwrap();
    for d in ["", "2026-1-01", "2026-01-01&asOf=2025-01-01", "2026/01/01"] {
        assert!(matches!(
            api.group_report_profiles(d).await,
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            api.consolidation("synthetic-left", d).await,
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            api.group_consolidated_report("p", d, "2026-01-01").await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.consolidation("../other", "2026-01-01").await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.group_consolidated_report("", "2026-01-01", "2026-01-01")
            .await,
        Err(Error::InvalidInput(_))
    ));
    for (action, bytes, confirm) in [
        (ConsolidationMutation::Propose, proposal(), false),
        (
            ConsolidationMutation::Propose,
            proposal().replace("\"confirm\":true", "\"confirm\":false"),
            true,
        ),
        (
            ConsolidationMutation::Propose,
            proposal().replacen("\"profile\":", "\"actor\":\"forged\",\"profile\":", 1),
            true,
        ),
        (
            ConsolidationMutation::Propose,
            " ".repeat(MAX_CONSOLIDATION_PROPOSE_BODY + 1),
            true,
        ),
        (
            ConsolidationMutation::Approve,
            " ".repeat(MAX_CONSOLIDATION_BODY + 1),
            true,
        ),
        (ConsolidationMutation::Revoke, approve(), true),
    ] {
        assert!(matches!(
            api.mutate_consolidation("synthetic-left", action, bytes.into_bytes(), confirm)
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
}
#[tokio::test]
async fn large_utf8_proposal_is_sent_exactly_domain_date_and_coverage_validation_remains_server_owned()
 {
    let mut value: serde_json::Value =
        serde_json::from_str(&proposal().replace("Ægte kilde", &"æ".repeat(160))).unwrap();
    let row = value["profile"]["accountMappings"][0].clone();
    value["profile"]["accountMappings"] = serde_json::Value::Array(
        (0..2048)
            .map(|i| {
                let mut row = row.clone();
                row["id"] = serde_json::json!(format!("map-{i}"));
                row
            })
            .collect(),
    );
    let bytes = value.to_string();
    assert!(bytes.len() > 262144 && bytes.len() < MAX_CONSOLIDATION_PROPOSE_BODY);
    let (origin, t) = server(
        "POST /api/companies/synthetic-left/consolidation/propose HTTP/1.1",
        &bytes,
        "422 Unprocessable Entity",
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.mutate_consolidation(
            "synthetic-left",
            ConsolidationMutation::Propose,
            bytes.into_bytes(),
            true
        )
        .await
        .unwrap_err(),
        Error::Refused { status: 422 }
    );
    t.join().unwrap();
}
