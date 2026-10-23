mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error, MAX_PARTY_BODY, PartyMutation};
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

const CREATE: &str = r#"{ "kind":"organization","name":"Synthetic Client Party","aliases":[],"identifiers":[{"country":"DK","identifier":"12345678","identifierKind":"dk_cvr"}],"source":"synthetic-brief","observedAt":"2026-01-01","reviewAssertion":"explicitly reviewed","role":"supplier","defaults":{"currency":"DKK"},"confirm":true }
"#;
#[tokio::test]
async fn exact_native_party_mutations_and_full_reads() {
    for (action, path, body) in [
        (
            PartyMutation::Create,
            "POST /api/companies/synthetic/workspace-parties HTTP/1.1",
            CREATE,
        ),
        (
            PartyMutation::Role("party-synthetic".into()),
            "POST /api/companies/synthetic/workspace-parties/party-synthetic/role HTTP/1.1",
            r#"{ "role":"customer","defaults":{"paymentTermsDays":14},"confirm":true }"#,
        ),
        (
            PartyMutation::Assertion("party-synthetic".into()),
            "POST /api/companies/synthetic/workspace-parties/party-synthetic/assertion HTTP/1.1",
            r#"{ "field":"name","value":"Synthetic revised","source":"registry","observedAt":"2026-02-01","reviewState":"approved","confirm":true }"#,
        ),
        (
            PartyMutation::Alias("party-synthetic".into()),
            "POST /api/companies/synthetic/workspace-parties/party-synthetic/alias HTTP/1.1",
            r#"{ "alias":"Synthetic new alias","source":"user","observedAt":"2026-02-01","reviewState":"proposed","confirm":true }"#,
        ),
    ] {
        let (origin, t) = server(path, body, "200 OK");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        assert_eq!(
            api.mutate_workspace_party("synthetic", action, body.as_bytes().to_vec(), true)
                .await
                .unwrap()["futureEvidence"]["retained"],
            true
        );
        t.join().unwrap();
    }
    let (origin, t) = server(
        "GET /api/companies/synthetic/workspace-parties?cursor=0&limit=25&query=Synthetic HTTP/1.1",
        "",
        "200 OK",
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert!(
        api.workspace_parties("synthetic", Some("Synthetic"), 0, 25)
            .await
            .is_ok()
    );
    t.join().unwrap();
    let (origin, t) = server(
        "GET /api/companies/synthetic/workspace-parties/party%2Bsynthetic HTTP/1.1",
        "",
        "200 OK",
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert!(
        api.workspace_party("synthetic", "party+synthetic")
            .await
            .is_ok()
    );
    t.join().unwrap();
}
#[tokio::test]
async fn refusal_and_uncertainty_send_once_without_retry() {
    for (status, unknown) in [
        ("403 Forbidden", false),
        ("422 Unprocessable Entity", false),
        ("503 Service Unavailable", true),
    ] {
        let (origin, t) = server(
            "POST /api/companies/synthetic/workspace-parties HTTP/1.1",
            CREATE,
            status,
        );
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        assert_eq!(
            matches!(
                api.mutate_workspace_party(
                    "synthetic",
                    PartyMutation::Create,
                    CREATE.as_bytes().to_vec(),
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
}
#[tokio::test]
async fn authority_overrides_missing_confirm_bad_paths_and_body_limits_refuse_before_transport() {
    let api = ApiClient::new("http://127.0.0.1:1", true, Some("session=synthetic".into())).unwrap();
    for text in [
        CREATE.replace("\"confirm\":true", "\"confirm\":false"),
        CREATE.replace("\"name\":", "\"actor\":"),
        CREATE.replace("\"currency\":", "\"arbitraryDefault\":"),
        "x".repeat(MAX_PARTY_BODY + 1),
    ] {
        assert!(matches!(
            api.mutate_workspace_party("synthetic", PartyMutation::Create, text.into_bytes(), true)
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
    for id in ["", "..", "party\nforged"] {
        assert!(matches!(
            api.workspace_party("synthetic", id).await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(
        api.mutate_workspace_party(
            "synthetic",
            PartyMutation::Create,
            CREATE.as_bytes().to_vec(),
            false
        )
        .await
        .is_err()
    );
}
