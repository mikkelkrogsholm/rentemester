mod common;
use common::TcpListener;
use rentemester_client::{
    ApiClient, DocumentPartyOriginalOperation as Op, Error, document_party_original_schema,
    document_party_original_validate,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    thread,
    time::{Duration, Instant},
};
fn input(op: Op) -> Value {
    match op {
        Op::Plan => json!({"documentId":7,"role":"supplier"}),
        Op::Apply => {
            json!({"documentId":7,"role":"supplier","planHash":"a".repeat(64),"confirm":true})
        }
        Op::Supersede => {
            json!({"documentId":7,"role":"supplier","planHash":"a".repeat(64),"reason":"reviewed correction","confirm":true})
        }
        Op::Internal => {
            json!({"documentId":7,"reason":"synthetic internal voucher","confirm":true})
        }
        Op::InternalSupersede => {
            json!({"documentId":7,"decisionHash":"a".repeat(64),"reason":"reviewed correction","confirm":true})
        }
    }
}
#[test]
fn document_party_original_full_body_schema_and_all_eighteen_roles() {
    for op in [
        Op::Plan,
        Op::Apply,
        Op::Supersede,
        Op::Internal,
        Op::InternalSupersede,
    ] {
        let schema = document_party_original_schema(op);
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!("documentId"))
        );
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(
            schema["properties"].get("confirm").is_some(),
            op != Op::Plan
        );
        let good = input(op);
        assert!(document_party_original_validate(op, &serde_json::to_vec(&good).unwrap()).is_ok());
        for (key, value) in [
            ("documentId", json!(0)),
            ("documentId", json!(9007199254740992u64)),
            ("actor", json!("forged")),
            ("workspace", json!("/private/no")),
            ("expectedActor", Value::Null),
        ] {
            let mut bad = good.clone();
            bad[key] = value;
            assert!(
                document_party_original_validate(op, &serde_json::to_vec(&bad).unwrap()).is_err(),
                "{op:?} {key}"
            );
        }
    }
    let schema = document_party_original_schema(Op::Plan);
    let roles = schema["properties"]["role"]["enum"].as_array().unwrap();
    assert_eq!(roles.len(), 18);
    for role in roles {
        let mut v = input(Op::Plan);
        v["role"] = role.clone();
        assert!(
            document_party_original_validate(Op::Plan, &serde_json::to_vec(&v).unwrap()).is_ok()
        );
    }
    for (op, key, value) in [
        (Op::Apply, "planHash", json!("x".repeat(64))),
        (Op::Internal, "reason", json!("  ")),
        (Op::Plan, "confirm", json!(true)),
        (Op::Internal, "confirm", json!(false)),
        (Op::Plan, "role", json!("fuzzy_name")),
    ] {
        let mut v = input(op);
        v[key] = value;
        assert!(document_party_original_validate(op, &serde_json::to_vec(&v).unwrap()).is_err());
    }
}
fn mock(status: u16, body: Value, expected: &str) -> (ApiClient, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let api = ApiClient::with_service_key(
        &format!("http://{}", listener.local_addr().unwrap()),
        true,
        "rm_svc_synthetic".into(),
    )
    .unwrap();
    let expected = expected.to_owned();
    let task = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut socket = loop {
            match listener.accept() {
                Ok((s, _)) => break s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "bounded mock accept");
                    thread::yield_now();
                }
                Err(e) => panic!("{e}"),
            }
        };
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut buf = [0; 4096];
            let n = socket.read(&mut buf).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buf[..n]);
            if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let len = headers
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length: ")
                            .map(|n| n.parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + len {
                    assert!(headers.starts_with(&expected), "{headers}");
                    assert!(
                        headers
                            .to_lowercase()
                            .contains("x-rentemester-service-key: rm_svc_synthetic")
                    );
                    break;
                }
            }
            assert!(bytes.len() < 20000);
        }
        let body = body.to_string();
        write!(socket,"HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        drop(socket);
        assert!(listener.accept().is_err(), "SDK must not retry");
    });
    (api, task)
}
#[tokio::test]
async fn document_party_original_read_plan_and_ambiguous_write_distinction() {
    for (op, expected) in [
        (Op::Plan, Error::Protocol),
        (Op::Apply, Error::OutcomeUnknown),
    ] {
        let suffix = if op == Op::Plan { "plan" } else { "apply" };
        let (api, t) = mock(
            200,
            json!({"ok":true,"privateKey":"must never escape"}),
            &format!("POST /api/companies/synthetic/documents/original/party-links/{suffix} "),
        );
        assert_eq!(
            api.document_party_original(
                "synthetic",
                op,
                serde_json::to_vec(&input(op)).unwrap(),
                true
            )
            .await,
            Err(expected)
        );
        t.join().unwrap();
        let (api, t) = mock(
            503,
            json!({"ok":true}),
            &format!("POST /api/companies/synthetic/documents/original/party-links/{suffix} "),
        );
        assert_eq!(
            api.document_party_original(
                "synthetic",
                op,
                serde_json::to_vec(&input(op)).unwrap(),
                true
            )
            .await,
            Err(if op == Op::Plan {
                Error::Transport
            } else {
                Error::OutcomeUnknown
            })
        );
        t.join().unwrap();
    }
}
#[tokio::test]
async fn document_party_original_authority_never_accepts_forged_payloads_and_curated_422() {
    for status in [401, 403, 409, 422, 200] {
        let body = json!({"ok":false,"errors":["PLAN_HASH_MISMATCH"]});
        let (api, t) = mock(
            status,
            body.clone(),
            "POST /api/companies/synthetic/documents/original/party-links/plan ",
        );
        let r = api
            .document_party_original(
                "synthetic",
                Op::Plan,
                serde_json::to_vec(&input(Op::Plan)).unwrap(),
                false,
            )
            .await;
        if matches!(status, 200 | 422) {
            assert_eq!(r.unwrap(), body);
        } else {
            assert_eq!(r, Err(Error::Refused { status }));
        }
        t.join().unwrap();
    }
    let (api, t) = mock(
        422,
        json!({"ok":false,"errors":["untrusted arbitrary failure"]}),
        "POST /api/companies/synthetic/documents/original/party-links/apply ",
    );
    assert_eq!(
        api.document_party_original(
            "synthetic",
            Op::Apply,
            serde_json::to_vec(&input(Op::Apply)).unwrap(),
            true
        )
        .await,
        Err(Error::Refused { status: 422 })
    );
    t.join().unwrap();
}
#[tokio::test]
async fn document_party_original_exact_snake_rows_and_private_fields_refused() {
    let row = json!({"id":7,"document_no":"SYN-7","sha256_hash":"a".repeat(64),"linked":0,"resolution_state":"unresolved"});
    let body = json!({"ok":true,"links":[row]});
    let (api, t) = mock(
        200,
        body.clone(),
        "GET /api/companies/synthetic/documents/original/party-links?status=unlinked ",
    );
    assert_eq!(
        api.document_party_original_list("synthetic", Some("unlinked"))
            .await
            .unwrap(),
        body
    );
    t.join().unwrap();
    // A retained source row may predate document numbering; never reject NULL.
    let mut legacy = body.clone();
    legacy["links"][0]["document_no"] = Value::Null;
    let (api, t) = mock(
        200,
        legacy.clone(),
        "GET /api/companies/synthetic/documents/original/party-links ",
    );
    assert_eq!(
        api.document_party_original_list("synthetic", None)
            .await
            .unwrap(),
        legacy
    );
    t.join().unwrap();
    for bad in [
        json!({"ok":true,"links":[{"id":7,"documentNo":"SYN-7"}]}),
        {
            let mut b = body.clone();
            b["links"][0]["private"] = json!("secret");
            b
        },
    ] {
        let (api, t) = mock(
            200,
            bad,
            "GET /api/companies/synthetic/documents/original/party-links ",
        );
        assert_eq!(
            api.document_party_original_list("synthetic", None).await,
            Err(Error::Protocol)
        );
        t.join().unwrap();
    }
}
#[tokio::test]
async fn document_party_original_confirmation_and_input_rejections_precede_connection() {
    let api = ApiClient::new("http://127.0.0.1:1", true, None).unwrap();
    assert!(matches!(
        api.document_party_original(
            "synthetic",
            Op::Apply,
            serde_json::to_vec(&input(Op::Apply)).unwrap(),
            false
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.document_party_original_inspect("synthetic", 0).await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.document_party_original_list("synthetic", Some("invented"))
            .await,
        Err(Error::InvalidInput(_))
    ));
}
#[tokio::test]
async fn document_party_original_valid_plan_inspect_and_four_receipts() {
    let h = "a".repeat(64);
    let snapshot = json!({"partyId":"synthetic-party","kind":"organization","name":"Synthetic Party","identifiers":[{"country":"DK","identifierKind":"dk_cvr","identifier":"12345678"}],"roles":[{"companySlug":"synthetic","role":"supplier","defaults":{}}],"history":[{"event_type":"created","payload_hash":h,"actor":"user:synthetic","created_at":"2026-10-22T00:00:00Z"}]});
    let plan = json!({"ok":true,"plan":{"documentId":7,"documentSha256":h,"documentPayloadSha256":h,"partyId":"synthetic-party","partySnapshot":snapshot,"role":"supplier","evidence":{"kind":"exact_identifier","jurisdiction":"DK","identifierKind":"dk_cvr","identifier":"12345678"},"planHash":h}});
    let (api, t) = mock(
        200,
        plan.clone(),
        "POST /api/companies/synthetic/documents/original/party-links/plan ",
    );
    assert_eq!(
        api.document_party_original(
            "synthetic",
            Op::Plan,
            serde_json::to_vec(&input(Op::Plan)).unwrap(),
            false
        )
        .await
        .unwrap(),
        plan
    );
    t.join().unwrap();
    for (op, suffix, result) in [
        (
            Op::Apply,
            "apply",
            json!({"ok":true,"id":1,"idempotent":false,"planHash":h}),
        ),
        (
            Op::Supersede,
            "supersede",
            json!({"ok":true,"id":null,"idempotent":true}),
        ),
        (
            Op::Internal,
            "internal",
            json!({"ok":true,"id":2,"idempotent":true,"decisionHash":h}),
        ),
        (
            Op::InternalSupersede,
            "internal-supersede",
            json!({"ok":true,"id":3,"idempotent":false}),
        ),
    ] {
        let (api, t) = mock(
            200,
            result.clone(),
            &format!("POST /api/companies/synthetic/documents/original/party-links/{suffix} "),
        );
        assert_eq!(
            api.document_party_original(
                "synthetic",
                op,
                serde_json::to_vec(&input(op)).unwrap(),
                true
            )
            .await
            .unwrap(),
            result
        );
        t.join().unwrap();
    }
    let row = json!({"id":1,"party_id":"synthetic-party","party_role":"supplier","event_type":"linked","evidence_kind":"exact_identifier","evidence_json":"{}","document_sha256":h,"document_payload_sha256":h,"party_snapshot_json":"{}","plan_hash":h,"reason":null,"actor":"user:synthetic","principal":"user:synthetic","created_at":"2026-10-22T00:00:00Z"});
    let result = json!({"ok":true,"links":[row]});
    let (api, t) = mock(
        200,
        result.clone(),
        "GET /api/companies/synthetic/documents/original/party-links/7 ",
    );
    assert_eq!(
        api.document_party_original_inspect("synthetic", 7)
            .await
            .unwrap(),
        result
    );
    t.join().unwrap();
    let mut private = result;
    private["links"][0]["idempotency_key"] = json!("private");
    let (api, t) = mock(
        200,
        private,
        "GET /api/companies/synthetic/documents/original/party-links/7 ",
    );
    assert_eq!(
        api.document_party_original_inspect("synthetic", 7).await,
        Err(Error::Protocol)
    );
    t.join().unwrap();
}
