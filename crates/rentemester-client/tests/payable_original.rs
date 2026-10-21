use rentemester_client::{ApiClient, Error, PayableOriginalMutation, PayableOriginalQuery};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};
fn server(status: u16, value: Value) -> (String, thread::JoinHandle<(String, Value)>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = vec![];
        let end = loop {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(i) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let header = String::from_utf8_lossy(&bytes[..end]).to_string();
        let length: usize = header
            .lines()
            .find_map(|l| {
                l.to_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse().unwrap())
            })
            .unwrap_or(0);
        while bytes.len() < end + length {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
        }
        let body = if length == 0 {
            Value::Null
        } else {
            serde_json::from_slice(&bytes[end..end + length]).unwrap()
        };
        let response = value.to_string();
        write!(
            s,
            "HTTP/1.1 {status} Mock\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{response}",
            response.len()
        )
        .unwrap();
        (header, body)
    });
    (origin, t)
}

#[tokio::test]
async fn original_filters_alias_priority_payload_names_rules_and_raw_receipts_are_unchanged() {
    let value = json!({"ok":true,"data":{"count":0,"status":"overdue","asOfDate":"2026-04-01","totalOpenBalance":0,"overdueOpenBalance":0,"notYetDueOpenBalance":0,"rows":[]},"errors":[]});
    let (origin, t) = server(200, value.clone());
    let query = PayableOriginalQuery {
        status: Some("overdue".into()),
        as_of: Some("2026-04-01".into()),
        as_of_date: Some("1970-01-01".into()),
        supplier: Some(" SYN & Co ".into()),
        vendor_id: Some(3),
        from: Some("2026-03-01".into()),
        to: Some("2026-01-01".into()),
        min_days: Some(0),
    };
    assert_eq!(
        ApiClient::new(&origin, true, None)
            .unwrap()
            .payable_original_list("synthetic", &query)
            .await
            .unwrap(),
        value
    );
    let (header, body) = t.join().unwrap();
    assert!(header.starts_with("GET /api/companies/synthetic/payables/original/list?status=overdue&asOf=2026-04-01&supplier=+SYN+%26+Co+&from=2026-03-01&to=2026-01-01&vendorId=3&minDays=0 "));
    assert!(!header.contains("1970") && !header.contains("asOfDate"));
    assert!(body.is_null());
    for (operation, action, input, data, rule) in [
        (
            PayableOriginalMutation::Register,
            "register",
            json!({"confirm":true,"documentId":4,"billDate":"2026-01-01","dueDate":"2026-01-31","expenseAccount":"expense","idempotencyKey":" raw register ","note":" raw note "}),
            json!({"payableId":7,"entryId":8,"entryNo":9,"idempotency":{"replayed":true,"receiptId":"server-register"}}),
            "DK-PAYABLE-001",
        ),
        (
            PayableOriginalMutation::Pay,
            "pay",
            json!({"confirm":true,"payableId":7,"bankTransactionId":11,"amount":50.25,"date":"2026-02-01","paymentAccount":"bank","idempotencyKey":" raw payment "}),
            json!({"paymentId":12,"journalEntryId":13,"openBalance":74.75,"idempotency":{"replayed":false,"receiptId":"server-pay"}}),
            "DK-PAYABLE-PAYMENT-001",
        ),
    ] {
        let value = json!({"ok":true,"data":data,"errors":[],"appliedRules":[rule]});
        let (origin, t) = server(200, value.clone());
        assert_eq!(
            ApiClient::new(&origin, true, None)
                .unwrap()
                .payable_original_mutation("synthetic", operation, input.to_string().into_bytes())
                .await
                .unwrap(),
            value
        );
        let (header, body) = t.join().unwrap();
        assert!(header.starts_with(&format!(
            "POST /api/companies/synthetic/payables/original/{action} "
        )));
        assert_eq!(body, input);
    }
}
#[tokio::test]
async fn optional_null_native_alias_identity_and_uncertain_outcomes_refuse_safely() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    let base = json!({"confirm":true,"payableId":7,"bankTransactionId":11});
    for (key, bad) in [
        ("note", Value::Null),
        ("date", Value::Null),
        ("paymentAccount", Value::Null),
        ("amount", json!(0)),
        ("actor", json!("forged")),
        ("paymentDate", json!("2026-01-01")),
        ("idempotencyKey", json!("x".repeat(129))),
        ("confirm", json!(false)),
    ] {
        let mut input = base.clone();
        input[key] = bad;
        assert!(matches!(
            api.payable_original_mutation(
                "synthetic",
                PayableOriginalMutation::Pay,
                input.to_string().into_bytes()
            )
            .await,
            Err(Error::InvalidInput(_))
        ));
    }
    for (status, value) in [
        (
            200,
            json!({"ok":false,"code":"business","errors":["server-owned refusal"],"appliedRules":["rule"]}),
        ),
        (403, json!({"ok":false})),
        (503, json!({"ok":false,"code":"source"})),
        (
            200,
            json!({"ok":false,"data":{},"errors":["invalid response"]}),
        ),
    ] {
        let (origin, t) = server(status, value.clone());
        let outcome = ApiClient::new(&origin, true, None)
            .unwrap()
            .payable_original_mutation(
                "synthetic",
                PayableOriginalMutation::Pay,
                base.to_string().into_bytes(),
            )
            .await;
        if status == 403 {
            assert!(matches!(outcome, Err(Error::Refused { status: 403 })));
        } else if status == 503 || value.get("data").is_some() {
            assert!(matches!(outcome, Err(Error::OutcomeUnknown)));
        } else {
            assert_eq!(outcome.unwrap(), value);
        }
        t.join().unwrap();
    }
    let value = json!({"ok":false,"errors":["asOfDate must be YYYY-MM-DD when present"]});
    let (origin, t) = server(200, value.clone());
    assert_eq!(
        ApiClient::new(&origin, true, None)
            .unwrap()
            .payable_original_list(
                "synthetic",
                &PayableOriginalQuery {
                    as_of_date: Some("2026-02-30".into()),
                    ..Default::default()
                }
            )
            .await
            .unwrap(),
        value
    );
    assert!(t.join().unwrap().0.contains("asOf=2026-02-30"));
}
