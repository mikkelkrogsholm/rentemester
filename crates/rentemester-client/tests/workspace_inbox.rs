use rentemester_client::{ApiClient, Error, MAX_WORKSPACE_INBOX_BODY, WorkspaceInboxMutation};
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

const INGEST: &str = r#"{ "confirm":true,"sourceId":"inbox-synthetic","idempotencyKey":" caller-exact-key ","bytesBase64":"U3ludGhldGlj","filename":"source.txt","mimeType":"text/plain","transport":"upload","receivedAt":"2026-01-01","metadata":{"source":"Synthetic","futureFactualField":{"unknown":null}},"candidates":[{"companySlug":"target","evidenceKind":"user_selected","evidence":"Synthetic operator selection"}] }
"#;
#[tokio::test]
async fn exact_native_ingress_handoff_and_canonical_reads() {
    for (action, path, bytes) in [
        (
            WorkspaceInboxMutation::Ingest,
            "POST /api/companies/synthetic/workspace-inbox HTTP/1.1",
            INGEST,
        ),
        (
            WorkspaceInboxMutation::Assign("inbox+synthetic".into()),
            "POST /api/companies/synthetic/workspace-inbox/inbox%2Bsynthetic/assign HTTP/1.1",
            r#"{ "companySlug":"target", "confirm":true }"#,
        ),
        (
            WorkspaceInboxMutation::Complete("inbox-synthetic".into()),
            "POST /api/companies/synthetic/workspace-inbox/inbox-synthetic/complete HTTP/1.1",
            r#"{ "companySlug":"target", "confirm":true }"#,
        ),
    ] {
        let (origin, t) = server(path, bytes, "201 Created");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        assert_eq!(
            api.mutate_workspace_inbox("synthetic", action, bytes.as_bytes().to_vec(), true)
                .await
                .unwrap()["futureEvidence"]["retained"],
            true
        );
        t.join().unwrap();
    }
    for (path, inspect) in [
        (
            "GET /api/companies/synthetic/workspace-inbox?cursor=2&limit=10 HTTP/1.1",
            false,
        ),
        (
            "GET /api/companies/synthetic/workspace-inbox/inbox%2Bsynthetic HTTP/1.1",
            true,
        ),
    ] {
        let (origin, t) = server(path, "", "200 OK");
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let result = if inspect {
            api.workspace_inbox_source("synthetic", "inbox+synthetic")
                .await
        } else {
            api.workspace_inbox("synthetic", 2, 10).await
        };
        assert_eq!(result.unwrap()["futureEvidence"]["retained"], true);
        t.join().unwrap();
    }
}
#[tokio::test]
async fn native_refusals_and_unknown_handoff_send_once() {
    for (status, unknown) in [
        ("403 Forbidden", false),
        ("404 Not Found", false),
        ("422 Unprocessable Entity", false),
        ("503 Service Unavailable", true),
    ] {
        let (origin, t) = server(
            "POST /api/companies/synthetic/workspace-inbox HTTP/1.1",
            INGEST,
            status,
        );
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let e = api
            .mutate_workspace_inbox(
                "synthetic",
                WorkspaceInboxMutation::Ingest,
                INGEST.as_bytes().to_vec(),
                true,
            )
            .await
            .unwrap_err();
        assert_eq!(matches!(e, Error::OutcomeUnknown), unknown);
        t.join().unwrap();
    }
    let (origin, t) = server(
        "GET /api/companies/synthetic/workspace-inbox?cursor=0&limit=25 HTTP/1.1",
        "",
        "503 Service Unavailable",
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert_eq!(
        api.workspace_inbox("synthetic", 0, 25).await.unwrap_err(),
        Error::Refused { status: 503 }
    );
    t.join().unwrap();
}
#[tokio::test]
async fn confirmation_envelope_path_and_resource_refusals_before_network() {
    let api = ApiClient::new("http://127.0.0.1:1", true, Some("session=synthetic".into())).unwrap();
    for body in [
        INGEST.replace("\"confirm\":true", "\"confirm\":false"),
        INGEST.replace("\"sourceId\":", "\"actor\":"),
        INGEST.replace("\"upload\"", "\"mail\""),
        INGEST.replace("\"evidenceKind\":", "\"companyOverride\":"),
        INGEST.replace(
            "\"metadata\":{\"source\":\"Synthetic\",\"futureFactualField\":{\"unknown\":null}}",
            "\"metadata\":[]",
        ),
        " ".repeat(MAX_WORKSPACE_INBOX_BODY + 1),
    ] {
        assert!(matches!(
            api.mutate_workspace_inbox(
                "synthetic",
                WorkspaceInboxMutation::Ingest,
                body.into_bytes(),
                true
            )
            .await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.mutate_workspace_inbox(
            "synthetic",
            WorkspaceInboxMutation::Ingest,
            INGEST.as_bytes().to_vec(),
            false
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
    for id in ["", ".", "..", "\n", &"x".repeat(101)] {
        assert!(matches!(
            api.workspace_inbox_source("synthetic", id).await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.workspace_inbox("synthetic", 0, 101).await,
        Err(Error::InvalidInput(_))
    ));
    for body in [
        r#"{"companySlug":"target","confirm":true,"idempotencyKey":"invented"}"#,
        r#"{"confirm":true}"#,
        r#"{"companySlug":"target","confirm":false}"#,
    ] {
        assert!(matches!(
            api.mutate_workspace_inbox(
                "synthetic",
                WorkspaceInboxMutation::Complete("source".into()),
                body.as_bytes().to_vec(),
                true
            )
            .await,
            Err(Error::InvalidInput(_))
        ));
    }
}

#[tokio::test]
async fn reviewed_mail_metadata_stays_exact_and_hash_bound() {
    let body = format!(
        r#"{{ "companySlug":"target", "sourceHash":"{}", "metadata":{{"documentType":"cash_register_receipt","sender":{{"name":"Synthetic source"}}}}, "confirm":true }}"#,
        "a".repeat(64)
    );
    let (origin, task) = server(
        "POST /api/companies/synthetic/workspace-inbox/source%2Bid/review HTTP/1.1",
        &body,
        "200 OK",
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    api.mutate_workspace_inbox(
        "synthetic",
        WorkspaceInboxMutation::Review("source+id".into()),
        body.into_bytes(),
        true,
    )
    .await
    .unwrap();
    task.join().unwrap();
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for value in [
        serde_json::json!({"companySlug":"target","sourceHash":"A".repeat(64),"metadata":{},"confirm":true}),
        serde_json::json!({"companySlug":"target","sourceHash":"a".repeat(64),"metadata":{},"confirm":false}),
        serde_json::json!({"companySlug":"target","sourceHash":"a".repeat(64),"metadata":{},"confirm":true,"ledgerUuid":"forbidden"}),
    ] {
        assert!(matches!(
            api.mutate_workspace_inbox(
                "synthetic",
                WorkspaceInboxMutation::Review("source".into()),
                value.to_string().into_bytes(),
                true
            )
            .await,
            Err(Error::InvalidInput(_))
        ));
    }
}
