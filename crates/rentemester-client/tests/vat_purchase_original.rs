use rentemester_client::{ApiClient, Error, validate_original_vat_purchase};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};
fn input() -> Value {
    json!({"transactionDate":"2026-01-01","text":"synthetic","netAmount":100,"expenseAccountNo":"synthetic-expense","invoiceNo":" INV "})
}
fn receipt() -> Value {
    json!({"ok":true,"entryId":8,"entryNo":"2026-8","entryHash":"a".repeat(64),"appliedRules":["synthetic-rule"],"errors":[]})
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
fn strict_payload_boundaries_and_domain_semantics() {
    assert!(validate_original_vat_purchase(&input(), false).is_ok());
    let mut representation = input();
    representation.as_object_mut().unwrap().remove("invoiceNo");
    representation
        .as_object_mut()
        .unwrap()
        .remove("expenseAccountNo");
    representation["documentId"] = json!(3);
    assert!(validate_original_vat_purchase(&representation, true).is_ok());
    for (key, value) in [
        ("actor", json!("forged")),
        ("program", json!("x")),
        ("confirm", json!(true)),
        ("documentId", json!(9007199254740992u64)),
        ("sourceBankTransactionId", json!(0)),
        ("purchaseVatLines", json!([])),
        ("idempotencyKey", json!("x")),
    ] {
        let mut v = input();
        v[key] = value;
        assert!(validate_original_vat_purchase(&v, false).is_err(), "{key}");
    }
    let mut v = input();
    v["transactionDate"] = json!("");
    v["text"] = json!("");
    v["netAmount"] = json!(-1);
    assert!(validate_original_vat_purchase(&v, false).is_ok());
    v["text"] = json!("x".repeat(16 * 1024 * 1024));
    assert!(validate_original_vat_purchase(&v, false).is_err());
}
#[test]
fn fixed_wire_and_real_false_preserved() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
    for rep in [false,true] {
        let mut payload=input();if rep {payload.as_object_mut().unwrap().remove("invoiceNo");payload["documentId"]=json!(3);}
        for core in [receipt(),json!({"ok":false,"errors":["Could not resolve document for invoiceNo INV"],"appliedRules":[]})] {
            let(api,t)=server(if core["ok"]==true {200}else{422},core.clone());
            let result=if rep {api.original_vat_representation_purchase("synthetic",&payload,Some("user:synthetic")).await} else {api.original_vat_eu_service_purchase("synthetic",&payload,Some("user:synthetic")).await};
            assert_eq!(result.unwrap(),core);let(header,body)=t.join().unwrap();
            assert!(header.starts_with(&format!("POST /api/companies/synthetic/original/vat/{} ",if rep {"representation-purchase"}else{"eu-service-purchase"})));
            assert_eq!(body,json!({"payload":payload,"confirm":true,"actor":"user:synthetic"}));
        }
    }
 });
}
#[test]
fn malformed_receipt_is_unknown_and_authority_is_not_reclassified() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let mut cases = vec![
            (403, receipt(), Error::Refused { status: 403 }),
            (503, receipt(), Error::OutcomeUnknown),
            (408, receipt(), Error::OutcomeUnknown),
        ];
        for (key, value) in [
            ("entryId", json!(0)),
            ("entryId", json!(9007199254740992u64)),
            ("entryNo", json!(" ")),
            ("entryHash", json!("g".repeat(64))),
            ("entryHash", json!("a".repeat(63))),
            ("errors", json!(["error"])),
            ("appliedRules", json!([5])),
            ("privatePath", json!("/secret")),
        ] {
            let mut v = receipt();
            v[key] = value;
            cases.push((200, v, Error::OutcomeUnknown));
        }
        for (status, value, error) in cases {
            let (api, t) = server(status, value);
            assert_eq!(
                api.original_vat_eu_service_purchase("synthetic", &input(), None)
                    .await
                    .unwrap_err(),
                error
            );
            t.join().unwrap();
        }
        let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
        assert!(matches!(
            api.original_vat_eu_service_purchase("synthetic", &input(), Some(" forged "))
                .await,
            Err(Error::InvalidInput(_))
        ));
        assert_eq!(
            api.original_vat_eu_service_purchase("synthetic", &input(), None)
                .await
                .unwrap_err(),
            Error::OutcomeUnknown
        );
    });
}
