use rentemester_client::{ApiClient, Error, validate_original_accrual};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};
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

fn input() -> Value {
    json!({"accrualType":"prepaid_expense","description":"Synthetic prepayment","totalAmount":125.09,"recognitionPeriods":3,"firstRecognitionDate":"2026-01-31","resultAccountNo":"expense"})
}
fn receipt() -> Value {
    json!({"ok":true,"entryId":9,"entryNo":"2026-0009","entryHash":"a".repeat(64),"appliedRules":["DK-BOOKKEEPING-ACCRUAL-001","DK-BOOKKEEPING-JOURNAL-001"],"errors":[],"accrualId":4,"accrualType":"prepaid_expense","totalPeriods":3,"periodAmount":41.69})
}
#[test]
fn strict_inputs_dates_money_null_unknown_and_no_local_ordering_policy() {
    assert!(validate_original_accrual(&input(), true).is_ok());
    let mut valid = input();
    valid["registrationDate"] = json!("2026-02-01");
    assert!(validate_original_accrual(&valid, true).is_ok());
    valid["totalAmount"] = json!(0.01);
    valid["recognitionPeriods"] = json!(2);
    assert!(validate_original_accrual(&valid, true).is_ok());
    for (k, v) in [
        ("totalAmount", json!(1.001)),
        ("totalAmount", json!("1.00")),
        ("documentId", json!(9007199254740992u64)),
        ("documentId", Value::Null),
        ("note", Value::Null),
        ("firstRecognitionDate", json!("2026-02-30")),
        ("firstRecognitionDate", json!("0000-01-01")),
        ("actor", json!("forged")),
        ("idempotencyKey", json!("key")),
        ("unknown", json!(true)),
        ("recognitionPeriods", json!(0)),
    ] {
        let mut invalid = input();
        invalid[k] = v;
        assert!(
            validate_original_accrual(&invalid, true).is_err(),
            "{invalid}"
        );
    }
    assert!(validate_original_accrual(&json!({"period":1,"date":"2026-01-01"}), false).is_ok());
    for v in [
        json!({"periodIndex":1}),
        json!({"period":1,"amount":1}),
        json!({"period":1,"date":null}),
    ] {
        assert!(validate_original_accrual(&v, false).is_err());
    }
}
#[tokio::test]
async fn writes_flat_receipt_defaults_and_confirmation_before_http() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for (confirm, actor) in [(false, None), (true, Some(" forged "))] {
        assert!(matches!(
            api.original_accrual_register("synthetic", &input(), confirm, actor)
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.original_accrual_recognize("synthetic", 0, &json!({"period":1}), true, None)
            .await,
        Err(Error::InvalidInput(_))
    ));
    let expected = receipt();
    let (api, t) = server(200, expected.clone());
    assert_eq!(
        api.original_accrual_register(
            "synthetic",
            &input(),
            true,
            Some("service_account:synthetic")
        )
        .await
        .unwrap(),
        expected
    );
    let (headers, body) = t.join().unwrap();
    assert!(headers.starts_with("POST /api/companies/synthetic/accrual/original/register "));
    let mut expected = input();
    expected["confirm"] = json!(true);
    expected["expectedActor"] = json!("service_account:synthetic");
    assert_eq!(body, expected);
    assert!(body.get("balanceAccountNo").is_none());
    assert!(body.get("documentId").is_none());
    let rec = json!({"ok":true,"entryId":10,"entryNo":"2026-0010","entryHash":"b".repeat(64),"appliedRules":["DK-BOOKKEEPING-ACCRUAL-001"],"errors":[],"accrualId":4,"periodIndex":2,"periodAmount":41.69,"recognizedPeriods":1,"totalPeriods":3,"fullyRecognized":false});
    let (api, t) = server(200, rec.clone());
    assert_eq!(
        api.original_accrual_recognize(
            "synthetic",
            4,
            &json!({"period":2,"date":"2026-01-01"}),
            true,
            None
        )
        .await
        .unwrap(),
        rec
    );
    assert!(
        t.join()
            .unwrap()
            .0
            .starts_with("POST /api/companies/synthetic/accrual/original/4/recognize ")
    );
}
#[tokio::test]
async fn uncertainty_authority_status_and_safe_refusals_no_retries() {
    for status in [408, 500, 503] {
        let (api, t) = server(status, receipt());
        assert_eq!(
            api.original_accrual_register("synthetic", &input(), true, None)
                .await,
            Err(Error::OutcomeUnknown)
        );
        t.join().unwrap();
    }
    for status in [401, 403] {
        let (api, t) = server(status, receipt());
        assert_eq!(
            api.original_accrual_register("synthetic", &input(), true, None)
                .await,
            Err(Error::Refused { status })
        );
        t.join().unwrap();
    }
    for (key, value) in [
        ("entryHash", json!("not-a-hash")),
        ("entryId", json!(0)),
        ("schedule", json!([])),
        ("periodAmount", json!(41.691)),
        ("accrualType", json!("deferred_revenue")),
        ("totalPeriods", json!(2)),
        ("path", json!("/private/secret")),
        ("appliedRules", json!(["/private/sql"])),
    ] {
        let mut response = receipt();
        response[key] = value;
        let (api, t) = server(200, response);
        assert_eq!(
            api.original_accrual_register("synthetic", &input(), true, None)
                .await,
            Err(Error::OutcomeUnknown),
            "{key}"
        );
        t.join().unwrap();
    }
    let (api, t) = server(
        422,
        json!({"ok":false,"errors":["SQL /private/secret"],"appliedRules":["DK-BOOKKEEPING-ACCRUAL-001"]}),
    );
    let safe = api
        .original_accrual_register("synthetic", &input(), true, None)
        .await
        .unwrap();
    assert_eq!(safe["ok"], false);
    assert!(!safe.to_string().contains("private"));
    t.join().unwrap();
    let (api, t) = server(422, receipt());
    assert_eq!(
        api.original_accrual_register("synthetic", &input(), true, None)
            .await,
        Err(Error::Refused { status: 422 })
    );
    t.join().unwrap();
}
fn report() -> Value {
    json!({"ok":true,"accruals":[{"accrualId":4,"accrualType":"prepaid_expense","description":"Synthetic prepayment","totalAmount":125.09,"recognitionPeriods":3,"recognizedPeriods":1,"recognizedAmount":41.69,"remainingAmount":83.40,"fullyRecognized":false,"balanceAccountNo":"asset","resultAccountNo":"expense","firstRecognitionDate":"2026-01-31","periodStepMonths":1}],"totals":{"totalAmount":125.09,"recognizedAmount":41.69,"remainingAmount":83.40},"errors":[]})
}
#[tokio::test]
async fn exact_report_and_exposure_consistency() {
    let expected = report();
    let (api, t) = server(200, expected.clone());
    assert_eq!(
        api.original_accrual_register_report("synthetic")
            .await
            .unwrap(),
        expected
    );
    let (header, body) = t.join().unwrap();
    assert!(header.starts_with("GET /api/companies/synthetic/accrual/original/register-report "));
    assert!(body.is_null());
    for response in [
        json!({"ok":true,"company":"synthetic","accruals":[],"totals":{"totalAmount":0,"recognizedAmount":0,"remainingAmount":0},"errors":[]}),
        {
            let mut v = report();
            v["totals"]["totalAmount"] = json!(12509);
            v
        },
        {
            let mut v = report();
            v["accruals"][0]["recognizedPeriods"] = json!(4);
            v
        },
        {
            let mut v = report();
            v["accruals"][0]["remainingAmount"] = json!(83.401);
            v
        },
        {
            let mut v = report();
            v["accruals"][0]["firstRecognitionDate"] = json!("2026-02-30");
            v
        },
    ] {
        let (api, t) = server(200, response);
        assert_eq!(
            api.original_accrual_register_report("synthetic").await,
            Err(Error::Protocol)
        );
        t.join().unwrap();
    }
}
