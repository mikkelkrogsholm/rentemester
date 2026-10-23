mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error, KnowledgeMutation, MAX_KNOWLEDGE_BODY};
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

const PROPOSAL: &str = r#"{ "predicate":"markets","value":{"channels":["DK",3.25,true,null]},"source":{"kind":"user","ref":"synthetic brief"},"validFrom":"2026-01-01","confirm":true }
"#;
#[tokio::test]
async fn exact_proposal_and_full_context_once() {
    let (origin, t) = server(
        "POST /api/companies/synthetic/knowledge/propose HTTP/1.1",
        PROPOSAL,
        "200 OK",
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    let v = api
        .mutate_company_knowledge(
            "synthetic",
            KnowledgeMutation::Propose,
            PROPOSAL.as_bytes().to_vec(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(v["futureEvidence"]["retained"], true);
    t.join().unwrap();
    let (origin, t) = server(
        "GET /api/companies/synthetic/knowledge?includeProposed=true&asOf=2026-01-01 HTTP/1.1",
        "",
        "200 OK",
    );
    let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
    assert!(
        api.company_knowledge("synthetic", Some("2026-01-01"), true)
            .await
            .is_ok()
    );
    t.join().unwrap();
}
#[tokio::test]
async fn definite_refusal_and_uncertain_write_never_retry() {
    for (status, unknown) in [
        ("422 Unprocessable Entity", false),
        ("503 Service Unavailable", true),
    ] {
        let (origin, t) = server(
            "POST /api/companies/synthetic/knowledge/propose HTTP/1.1",
            PROPOSAL,
            status,
        );
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let e = api
            .mutate_company_knowledge(
                "synthetic",
                KnowledgeMutation::Propose,
                PROPOSAL.as_bytes().to_vec(),
                true,
            )
            .await
            .unwrap_err();
        assert_eq!(matches!(e, Error::OutcomeUnknown), unknown);
        t.join().unwrap();
    }
}
#[tokio::test]
async fn schema_refuses_authority_override_missing_value_and_no_confirmation() {
    let api = ApiClient::new("http://127.0.0.1:1", true, Some("session=synthetic".into())).unwrap();
    for bytes in [
        PROPOSAL.replace("\"value\":", "\"actor\":"),
        PROPOSAL.replace("\"confirm\":true", "\"confirm\":false"),
        PROPOSAL.replace("\"source\":", "\"companySlug\":"),
        "x".repeat(MAX_KNOWLEDGE_BODY + 1),
    ] {
        assert!(matches!(
            api.mutate_company_knowledge(
                "synthetic",
                KnowledgeMutation::Propose,
                bytes.into_bytes(),
                true
            )
            .await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.mutate_company_knowledge(
            "synthetic",
            KnowledgeMutation::Propose,
            PROPOSAL.as_bytes().to_vec(),
            false
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
}
