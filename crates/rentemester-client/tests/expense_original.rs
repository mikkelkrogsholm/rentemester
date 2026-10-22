use rentemester_client::{ApiClient, Error, validate_original_expense_book};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};
fn input() -> Value {
    json!({"documentId":3,"bankTransactionId":7,"expenseAccount":"synthetic-expense"})
}
fn snapshot(ok: bool) -> Value {
    json!({"ok":ok,"documentId":3,"classification":"EU","cached":ok,"evidenceExpiresAt":if ok {json!("2027-01-01T00:00:00Z")}else{Value::Null},"wouldCallProvider":!ok,"errors":if ok {json!([])}else{json!(["fresh EU VAT validation evidence is required"])},"derivedRegion":"EU","requiredValidation":"EU VAT provider validation","cache":{"reused":ok,"freshUntil":if ok {json!("2027-01-01T00:00:00Z")}else{Value::Null}},"applyWouldCallProvider":!ok,"evidence":[{"id":2,"event_type":"preflight_failed","classification":"EU","provider_status":"invalid","evidence_expires_at":null,"created_at":"2026-10-22T00:00:00Z","actor":"service_account:synthetic"}],"exception":{"id":1,"status":"open","severity":"error","message":"EU VAT validation failed","requiredAction":"retry explicitly","createdAt":"2026-10-22T00:00:00Z"}})
}
fn server(status: u16, value: Value) -> (ApiClient, thread::JoinHandle<(String, Value)>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let api = ApiClient::new(
        &format!("http://{}", listener.local_addr().unwrap()),
        true,
        None,
    )
    .unwrap();
    let t = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(4)))
            .unwrap();
        let mut bytes = vec![];
        let end = loop {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let header = String::from_utf8_lossy(&bytes[..end]).to_string();
        let len: usize = header
            .lines()
            .find_map(|l| {
                l.to_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse().unwrap())
            })
            .unwrap_or(0);
        while bytes.len() < end + len {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
        }
        let body = if len == 0 {
            Value::Null
        } else {
            serde_json::from_slice(&bytes[end..end + len]).unwrap()
        };
        let text = value.to_string();
        write!(
            s,
            "HTTP/1.1 {status} Mock\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        )
        .unwrap();
        (header, body)
    });
    (api, t)
}
#[test]
fn strict_local_boundaries_and_six_vs_five_treatments() {
    for treatment in [
        "standard",
        "reverse_charge",
        "eu_goods_acquisition",
        "representation",
        "exempt",
        "non_deductible",
    ] {
        let mut v = input();
        v["vatTreatment"] = json!(treatment);
        assert!(validate_original_expense_book(&v, false).is_ok());
        assert_eq!(
            validate_original_expense_book(&v, true).is_ok(),
            treatment != "eu_goods_acquisition"
        );
    }
    for (key, v) in [
        ("actor", json!("forged")),
        ("expectedActor", Value::Null),
        ("vatTreatment", Value::Null),
        ("documentId", json!(9007199254740992u64)),
        ("idempotencyKey", json!(" ")),
        ("unknown", json!(true)),
    ] {
        let mut args = input();
        args[key] = v;
        assert!(validate_original_expense_book(&args, false).is_err());
    }
    let rt = tokio::runtime::Runtime::new().unwrap();
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    rt.block_on(async {
        assert!(matches!(
            api.original_expense_vat_preflight("synthetic", 0).await,
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            api.original_expense_vat_preflight_apply("synthetic", 3, false, None)
                .await,
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            api.original_expense_book("synthetic", &input(), true, Some(" forged "))
                .await,
            Err(Error::InvalidInput(_))
        ));
    });
}
#[test]
fn exact_paths_definite_failed_snapshot_and_optional_caller_key() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        for apply in [false,true] {
            let value=snapshot(false);let(api,t)=server(200,value.clone());
            let result=if apply {api.original_expense_vat_preflight_apply("synthetic",3,true,Some("user:synthetic")).await}else{api.original_expense_vat_preflight("synthetic",3).await};
            assert_eq!(result.unwrap(),value);let(header,body)=t.join().unwrap();assert!(header.starts_with(if apply {"POST "}else{"GET "}));assert!(header.contains("/api/companies/synthetic/expense/original/vat-preflight/3"));
            assert_eq!(body,if apply {json!({"confirm":true,"expectedActor":"user:synthetic"})}else{Value::Null});
        }
        for key in [None,Some(" raw caller key ")] {
            let mut args=input();let mut value=json!({"ok":true,"entryId":8,"entryNo":"synthetic-8","entryHash":"a".repeat(64),"documentId":3,"bankTransactionId":7,"appliedRules":["synthetic-rule"],"errors":[]});
            if let Some(key)=key {args["idempotencyKey"]=json!(key);value["idempotency"]=json!({"replayed":true,"receiptId":2,"createdAt":"2026-10-22T00:00:00Z","expiresAt":"2027-10-22T00:00:00Z"});}
            let(api,t)=server(200,value.clone());assert_eq!(api.original_expense_book("synthetic",&args,true,None).await.unwrap(),value);let(header,body)=t.join().unwrap();assert!(header.contains("/expense/original/book"));assert_eq!(body.get("idempotencyKey"),args.get("idempotencyKey"));assert_eq!(body["confirm"],true);assert!(body.get("expectedActor").is_none());
        }
    });
}
#[test]
fn authority_wins_and_malformed_or_lost_mutations_are_unknown() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        for (status, mut value, expected) in [
            (403, snapshot(true), Error::Refused { status: 403 }),
            (500, snapshot(false), Error::OutcomeUnknown),
            (408, snapshot(false), Error::OutcomeUnknown),
            (200, snapshot(false), Error::OutcomeUnknown),
        ] {
            if status == 200 {
                value["evidence"][0]["detail_json"] = json!("private provider response");
            }
            let (api, t) = server(status, value);
            assert_eq!(
                api.original_expense_vat_preflight_apply("synthetic", 3, true, None)
                    .await
                    .unwrap_err(),
                expected
            );
            t.join().unwrap();
        }
        let mut value = snapshot(false);
        value["exception"]["privatePath"] = json!("/private/source");
        let (api, t) = server(200, value);
        assert_eq!(
            api.original_expense_vat_preflight("synthetic", 3)
                .await
                .unwrap_err(),
            Error::Protocol
        );
        t.join().unwrap();
    });
}
