use rentemester_client::{
    ApiClient, Error, MAX_OWNERSHIP_BODY, MAX_OWNERSHIP_PROPOSE_BODY, OwnershipMutation,
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
        let reply = r#"{"ok":true,"futureEvidence":{"retained":true},"comparison":[{"budget":null,"variance":null}],"warnings":["native advisory"]}"#;
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

const PROPOSE: &str = r#"{ "snapshotId":"ownership-synthetic","source":"Synthetic source brief","observedAt":"2026-01-01","facts":[{"owner":{"kind":"party","partyId":"party-synthetic"},"ownedCompanySlug":"synthetic","validFrom":"2000-01-01","economicBasisPoints":1000,"votingBasisPoints":null,"economicIntervalBasisPoints":null,"controlType":"equity","shareClass":null,"jurisdiction":"DK","evidenceRefs":["synthetic-human-reviewed-brief"]}],"confirm":true }
"#;
#[tokio::test]
async fn exact_native_snapshot_lifecycle_and_full_asof_history_reads() {
    for (action, path, bytes) in [
        (
            OwnershipMutation::Propose,
            "POST /api/companies/synthetic/ownership/propose HTTP/1.1",
            PROPOSE.to_owned(),
        ),
        (
            OwnershipMutation::Review,
            "POST /api/companies/synthetic/ownership/review HTTP/1.1",
            r#"{ "snapshotId":"ownership-synthetic","decision":"approved","confirm":true }"#.into(),
        ),
        (
            OwnershipMutation::Apply,
            "POST /api/companies/synthetic/ownership/apply HTTP/1.1",
            format!(
                r#"{{ "snapshotId":"ownership-synthetic","snapshotHash":"{}","diffHash":"{}","confirm":true }}"#,
                "a".repeat(64),
                "b".repeat(64)
            ),
        ),
    ] {
        let (origin, t) = server(path, &bytes, "200 OK");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        assert_eq!(
            api.mutate_ownership("synthetic", action, bytes.into_bytes(), true)
                .await
                .unwrap()["futureEvidence"]["retained"],
            true
        );
        t.join().unwrap();
    }
    for (path, history) in [
        (
            "GET /api/companies/synthetic/ownership?asOf=2026-06-01 HTTP/1.1",
            false,
        ),
        (
            "GET /api/companies/synthetic/ownership/history?snapshotId=ownership%2Bsynthetic&asOf=2026-06-01 HTTP/1.1",
            true,
        ),
    ] {
        let (origin, t) = server(path, "", "200 OK");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let v = if history {
            api.ownership_history("synthetic", Some("ownership+synthetic"), Some("2026-06-01"))
                .await
        } else {
            api.ownership("synthetic", "2026-06-01").await
        };
        assert_eq!(v.unwrap()["futureEvidence"]["retained"], true);
        t.join().unwrap();
    }
    let large = format!("{PROPOSE}{}", " ".repeat(1_100_000));
    let (origin, t) = server(
        "POST /api/companies/synthetic/ownership/propose HTTP/1.1",
        &large,
        "200 OK",
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert!(
        api.mutate_ownership(
            "synthetic",
            OwnershipMutation::Propose,
            large.into_bytes(),
            true
        )
        .await
        .is_ok()
    );
    t.join().unwrap();
}
#[tokio::test]
async fn native_state_permission_and_unknown_results_send_once() {
    for (status, unknown) in [
        ("403 Forbidden", false),
        ("404 Not Found", false),
        ("422 Unprocessable Entity", false),
        ("503 Service Unavailable", true),
    ] {
        let (origin, t) = server(
            "POST /api/companies/synthetic/ownership/propose HTTP/1.1",
            PROPOSE,
            status,
        );
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        assert_eq!(
            matches!(
                api.mutate_ownership(
                    "synthetic",
                    OwnershipMutation::Propose,
                    PROPOSE.as_bytes().to_vec(),
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
        "GET /api/companies/synthetic/ownership/history HTTP/1.1",
        "",
        "503 Service Unavailable",
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.ownership_history("synthetic", None, None)
            .await
            .unwrap_err(),
        Error::Refused { status: 503 }
    );
    t.join().unwrap();
}
#[tokio::test]
async fn invalid_confirm_scope_attribution_and_resources_refuse_before_transport() {
    let api = ApiClient::new("http://127.0.0.1:1", true, Some("session=synthetic".into())).unwrap();
    for body in [
        PROPOSE.replace("\"confirm\":true", "\"confirm\":false"),
        PROPOSE.replace("\"source\":", "\"actor\":"),
        PROPOSE.replace("\"observedAt\":", "\"principal\":"),
        PROPOSE.replace("\"partyId\":", "\"hiddenOverride\":"),
        PROPOSE.replace("\"facts\":[", "\"inventedFacts\":["),
        " ".repeat(MAX_OWNERSHIP_PROPOSE_BODY + 1),
    ] {
        assert!(matches!(
            api.mutate_ownership(
                "synthetic",
                OwnershipMutation::Propose,
                body.into_bytes(),
                true
            )
            .await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.mutate_ownership(
            "synthetic",
            OwnershipMutation::Propose,
            PROPOSE.as_bytes().to_vec(),
            false
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
    for action in [OwnershipMutation::Review, OwnershipMutation::Apply] {
        assert!(matches!(
            api.mutate_ownership(
                "synthetic",
                action,
                vec![b' '; MAX_OWNERSHIP_BODY + 1],
                true
            )
            .await,
            Err(Error::InvalidInput(_))
        ));
    }
    for date in [
        "",
        "2026",
        "2026-1-01",
        "2026-01-01T00:00:00Z",
        "202x-01-01",
    ] {
        assert!(matches!(
            api.ownership("synthetic", date).await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.ownership_history("synthetic", Some(&"x".repeat(81)), None)
            .await,
        Err(Error::InvalidInput(_))
    ));
}
