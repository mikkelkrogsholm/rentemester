mod common;
use common::TcpListener;
use rentemester_client::{
    ApiClient, Error, HostedRecurringMutation as Hosted, MAX_HOSTED_RECURRING_BODY,
    MAX_SERVICE_PRINCIPAL_BODY, ServicePrincipalMutation as Service,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    sync::mpsc,
    thread,
    time::Duration,
};
fn server(
    status: &'static str,
    reply: &'static str,
) -> (String, mpsc::Receiver<Vec<u8>>, thread::JoinHandle<usize>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut bytes = vec![];
        loop {
            let mut b = [0; 4096];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let h = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                let len = h
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length: "))
                    .map(|n| n.parse::<usize>().unwrap())
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + len {
                    break;
                }
            }
        }
        tx.send(bytes).unwrap();
        write!(
            s,
            "HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{reply}",
            reply.len()
        )
        .unwrap();
        drop(s);
        l.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(60));
        1 + l.incoming().take_while(Result::is_ok).count()
    });
    (origin, rx, t)
}

const UUID: &str = "12345678-1234-4123-8123-123456789abc";
fn service_body(op: Service) -> Value {
    let mut value = json!({"confirm":true,"operationId":UUID});
    match op {
        Service::Create => {
            value["displayName"] = json!(" Synthetic executor ");
            value["expiresInSeconds"] = json!(86400);
        }
        Service::Rotate | Service::Revoke => {
            value["serviceAccountId"] = json!("service-synthetic");
            value["credentialId"] = json!("credential-synthetic");
            if op == Service::Rotate {
                value["expiresInSeconds"] = json!(31536000);
            }
        }
        Service::Recover => {}
    }
    value
}
fn hosted_body(op: Hosted) -> Value {
    let mut value =
        json!({"templateId":7,"reason":" explicit hosted source review ","confirm":true});
    match op {
        Hosted::Consent => {
            value["planHash"] = json!("a".repeat(64));
            value["serviceAccountId"] = json!("service-synthetic");
            value["credentialId"] = json!("credential-synthetic");
        }
        Hosted::Revoke => value["expectedStateHash"] = json!("b".repeat(64)),
    }
    value
}
#[tokio::test]
async fn lifecycle_and_hosted_exact_routes_bytes_raw_receipts_and_no_uncertain_replay() {
    const REPLY: &str = r#"{"ok":true,"operationId":"12345678-1234-4123-8123-123456789abc","secret":"synthetic-one-time-receipt","future":{"status":"canonical","amount":"90071992547409.99"}}"#;
    for status in ["201 Created", "422 Refused", "503 Unavailable"] {
        for op in [
            Service::Create,
            Service::Rotate,
            Service::Revoke,
            Service::Recover,
        ] {
            let (origin, rx, t) = server(status, REPLY);
            let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
            let bytes = format!(" {}\n", service_body(op)).into_bytes();
            let out = api.mutate_service_principal(op, bytes.clone(), true).await;
            if status.starts_with("201") {
                assert_eq!(out.unwrap(), serde_json::from_str::<Value>(REPLY).unwrap());
            } else if status.starts_with("422") {
                assert_eq!(out, Err(Error::Refused { status: 422 }));
            } else {
                assert_eq!(out, Err(Error::OutcomeUnknown));
            }
            let wire = rx.recv().unwrap();
            assert!(
                wire.starts_with(
                    format!(
                        "POST /api/workspace/service-principals{} HTTP/1.1",
                        op.suffix()
                    )
                    .as_bytes()
                )
            );
            assert!(wire.ends_with(&bytes));
            assert_eq!(t.join().unwrap(), 1);
        }
        for op in [Hosted::Consent, Hosted::Revoke] {
            let (origin, rx, t) = server(status, REPLY);
            let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
            let bytes = format!(" {}\n", hosted_body(op)).into_bytes();
            let out = api
                .mutate_hosted_recurring("synthetic", op, bytes.clone(), true)
                .await;
            if status.starts_with("201") {
                assert_eq!(out.unwrap(), serde_json::from_str::<Value>(REPLY).unwrap());
            } else if status.starts_with("422") {
                assert_eq!(out, Err(Error::Refused { status: 422 }));
            } else {
                assert_eq!(out, Err(Error::OutcomeUnknown));
            }
            let wire = rx.recv().unwrap();
            assert!(
                wire.starts_with(
                    format!(
                        "POST /api/companies/synthetic/scheduled-recurring/{} HTTP/1.1",
                        op.suffix()
                    )
                    .as_bytes()
                )
            );
            assert!(wire.ends_with(&bytes));
            assert_eq!(t.join().unwrap(), 1);
        }
    }
}
#[tokio::test]
async fn readonly_metadata_and_hosted_plan_are_exact_nonconsuming_gets() {
    const REPLY: &str = r#"{"ok":true,"hostedPlan":{"template":{"frozen":true},"executor":{"serviceAccountId":"synthetic","credentialId":"original"},"policy":"normal_due_only"},"principals":[{"credentials":[{"id":"retained"}]}]}"#;
    for hosted in [false, true] {
        let (origin, rx, t) = server("200 OK", REPLY);
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let out = if hosted {
            api.hosted_recurring_plan("synthetic", 7).await
        } else {
            api.service_principals().await
        };
        assert_eq!(out.unwrap(), serde_json::from_str::<Value>(REPLY).unwrap());
        let wire = String::from_utf8(rx.recv().unwrap()).unwrap();
        assert!(wire.starts_with(if hosted {
            "GET /api/companies/synthetic/scheduled-recurring/7/hosted-plan HTTP/1.1"
        } else {
            "GET /api/workspace/service-principals HTTP/1.1"
        }));
        assert!(!wire.contains("confirm"));
        assert_eq!(t.join().unwrap(), 1);
    }
}
#[tokio::test]
async fn lifecycle_exact_optional_fields_native_uuid_expiry_and_unknown_secret_admission() {
    let api = ApiClient::new("http://127.0.0.1:9", true, Some("session=synthetic".into())).unwrap();
    for (op, patch) in [
        (Service::Create, json!({"secret":"must-not-print"})),
        (Service::Create, json!({"displayName":" \t"})),
        (Service::Create, json!({"displayName":"x".repeat(121)})),
        (Service::Create, json!({"operationId":null})),
        (Service::Create, json!({"operationId":"bad"})),
        (Service::Rotate, json!({"expiresInSeconds":86400.0})),
        (Service::Rotate, json!({"expiresInSeconds":86399})),
        (Service::Revoke, json!({"expiresInSeconds":86400})),
        (Service::Recover, json!({"credentialId":"wrong"})),
        (Service::Rotate, json!({"credentialId":"\n"})),
    ] {
        let mut v = service_body(op);
        for (k, value) in patch.as_object().unwrap() {
            v[k] = value.clone();
        }
        assert!(
            matches!(
                api.mutate_service_principal(op, serde_json::to_vec(&v).unwrap(), true)
                    .await,
                Err(Error::InvalidInput(_))
            ),
            "{op:?}"
        );
    }
    let mut recover = service_body(Service::Recover);
    recover.as_object_mut().unwrap().remove("operationId");
    assert!(matches!(
        api.mutate_service_principal(
            Service::Recover,
            serde_json::to_vec(&recover).unwrap(),
            true
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.mutate_service_principal(
            Service::Create,
            serde_json::to_vec(&service_body(Service::Create)).unwrap(),
            false
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.mutate_service_principal(
            Service::Create,
            vec![b' '; MAX_SERVICE_PRINCIPAL_BODY + 1],
            true
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
}
#[tokio::test]
async fn hosted_exact_executor_hash_safe_id_reason_and_confirmation_guards() {
    let api = ApiClient::new("http://127.0.0.1:9", true, Some("session=synthetic".into())).unwrap();
    for patch in [
        json!({"serviceKey":"must-not-print"}),
        json!({"reason":" \u{feff} "}),
        json!({"reason":"🧾".repeat(501)}),
        json!({"templateId":9007199254740992u64}),
        json!({"templateId":7.0}),
        json!({"planHash":"A".repeat(64)}),
        json!({"credentialId":null}),
        json!({"serviceAccountId":"\n"}),
        json!({"confirm":false}),
        json!({"periodIndex":0}),
    ] {
        let mut v = hosted_body(Hosted::Consent);
        for (k, value) in patch.as_object().unwrap() {
            v[k] = value.clone();
        }
        assert!(matches!(
            api.mutate_hosted_recurring(
                "synthetic",
                Hosted::Consent,
                serde_json::to_vec(&v).unwrap(),
                true
            )
            .await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.mutate_hosted_recurring(
            "synthetic",
            Hosted::Consent,
            vec![b' '; MAX_HOSTED_RECURRING_BODY + 1],
            true
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.mutate_hosted_recurring(
            "synthetic",
            Hosted::Consent,
            serde_json::to_vec(&hosted_body(Hosted::Consent)).unwrap(),
            false
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
    for id in [0, -1, 9007199254740992] {
        assert!(matches!(
            api.hosted_recurring_plan("synthetic", id).await,
            Err(Error::InvalidInput(_))
        ));
    }
}
#[tokio::test]
async fn native_trimmed_unicode_display_and_reason_boundaries_preserve_wire_bytes() {
    for hosted in [false, true] {
        let (origin, rx, t) = server("200 OK", r#"{"ok":true}"#);
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        let mut v = if hosted {
            hosted_body(Hosted::Consent)
        } else {
            service_body(Service::Create)
        };
        v[if hosted { "reason" } else { "displayName" }] = json!(format!(
            "  {}  ",
            "🧾".repeat(if hosted { 500 } else { 120 })
        ));
        if !hosted {
            v["operationId"] = json!(format!(" {} ", UUID.to_uppercase()));
        }
        let bytes = serde_json::to_vec(&v).unwrap();
        if hosted {
            api.mutate_hosted_recurring("synthetic", Hosted::Consent, bytes.clone(), true)
                .await
                .unwrap();
        } else {
            api.mutate_service_principal(Service::Create, bytes.clone(), true)
                .await
                .unwrap();
        }
        assert!(rx.recv().unwrap().ends_with(&bytes));
        assert_eq!(t.join().unwrap(), 1);
    }
}
