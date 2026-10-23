mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error, validate_original_asset};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
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

fn registration() -> Value {
    json!({"name":"Synthetic asset","category":"Equipment","acquisitionDate":"2026-01-01","cost":125,"usefulLifeMonths":3,"documentId":7})
}
fn writeoff() -> Value {
    json!({"name":"Synthetic purchase","category":"Equipment","acquisitionDate":"2026-01-01","cost":250,"documentId":8,"expenseAccount":"expense","date":"2026-01-02","thresholdRuleSource":"Explicit advisor assessment","confirmImmediateWriteOff":false})
}
fn registered() -> Value {
    json!({"ok":true,"assetId":4,"totalPeriods":3,"periodAmount":41.66,"appliedRules":["DK-ASSET-DEPR-001"],"errors":[]})
}
#[test]
fn strict_source_inputs_including_false_inner_confirmation() {
    assert!(validate_original_asset(&registration(), "register").is_ok());
    assert!(validate_original_asset(&writeoff(), "write-off").is_ok());
    for (key, value) in [
        ("note", Value::Null),
        ("documentId", json!(0)),
        ("documentId", json!(9007199254740992u64)),
        ("cost", json!(1.001)),
        ("cost", json!("125")),
        ("method", json!("linear")),
        ("principal", json!("forged")),
        ("idempotencyKey", json!("key")),
        ("acquisitionDate", json!("2026-02-30")),
    ] {
        let mut v = registration();
        v[key] = value;
        assert!(validate_original_asset(&v, "register").is_err(), "{v}");
    }
    for v in [
        json!({"period":1}),
        json!({"period":1,"date":"2026-01-01","amount":1}),
        json!({"periodIndex":1,"date":"2026-01-01"}),
    ] {
        assert!(validate_original_asset(&v, "depreciate").is_err());
    }
    let mut v = writeoff();
    v["confirmImmediateWriteOff"] = json!("false");
    assert!(validate_original_asset(&v, "write-off").is_err());
}
#[tokio::test]
async fn source_registration_route_no_defaults_no_extra_output() {
    let (api, t) = server(200, registered());
    assert_eq!(
        api.original_asset_register("synthetic", &registration(), true, Some("user:synthetic"))
            .await
            .unwrap(),
        registered()
    );
    let (header, body) = t.join().unwrap();
    assert!(header.starts_with("POST /api/companies/synthetic/asset/original/register "));
    let mut expected = registration();
    expected["confirm"] = json!(true);
    expected["expectedActor"] = json!("user:synthetic");
    assert_eq!(body, expected);
    for key in ["schedule", "entryId", "company"] {
        let mut v = registered();
        v[key] = json!(1);
        let (api, t) = server(200, v);
        assert_eq!(
            api.original_asset_register("synthetic", &registration(), true, None)
                .await,
            Err(Error::OutcomeUnknown)
        );
        t.join().unwrap();
    }
}
#[tokio::test]
async fn definite_422_false_is_exact_and_status_authority_wins() {
    let core = json!({"ok":false,"appliedRules":["DK-ASSET-WRITEOFF-001"],"errors":["Immediate write-off requires explicit confirmation"]});
    let (api, t) = server(422, core.clone());
    assert_eq!(
        api.original_asset_write_off("synthetic", &writeoff(), true, None)
            .await
            .unwrap(),
        core
    );
    let (_, body) = t.join().unwrap();
    assert_eq!(body["confirm"], true);
    assert_eq!(body["confirmImmediateWriteOff"], false);
    for status in [401, 403, 500, 408] {
        let (api, t) = server(status, core.clone());
        let r = api
            .original_asset_write_off("synthetic", &writeoff(), true, None)
            .await;
        assert_eq!(
            r,
            Err(if status == 401 || status == 403 {
                Error::Refused { status }
            } else {
                Error::OutcomeUnknown
            })
        );
        t.join().unwrap();
    }
    let (api, t) = server(
        422,
        json!({"ok":false,"appliedRules":["DK-ASSET-WRITEOFF-001"],"errors":["refused"],"exceptionId":1}),
    );
    assert_eq!(
        api.original_asset_write_off("synthetic", &writeoff(), true, None)
            .await,
        Err(Error::Refused { status: 422 })
    );
    t.join().unwrap();
}
#[tokio::test]
async fn depreciation_receipt_and_report_financial_validation() {
    let receipt = json!({"ok":true,"entryId":1,"entryNo":"2026-0001","entryHash":"a".repeat(64),"appliedRules":["DK-ASSET-DEPR-001"],"errors":[],"assetId":4,"periodIndex":2,"periodAmount":41.66});
    let (api, t) = server(200, receipt.clone());
    assert_eq!(
        api.original_asset_depreciate(
            "synthetic",
            4,
            &json!({"period":2,"date":"2026-02-01"}),
            true,
            None
        )
        .await
        .unwrap(),
        receipt
    );
    assert!(
        t.join()
            .unwrap()
            .0
            .starts_with("POST /api/companies/synthetic/asset/original/4/depreciate ")
    );
    let report = json!({"ok":true,"assets":[{"assetId":4,"name":"Synthetic","category":"Equipment","acquisitionDate":"2026-01-01","cost":125,"usefulLifeMonths":3,"postedPeriods":3,"accumulatedDepreciation":125,"netBookValue":0}],"totals":{"cost":125,"accumulatedDepreciation":125,"netBookValue":0},"errors":[]});
    let (api, t) = server(200, report.clone());
    assert_eq!(
        api.original_asset_register_report("synthetic")
            .await
            .unwrap(),
        report
    );
    assert!(
        t.join()
            .unwrap()
            .0
            .starts_with("GET /api/companies/synthetic/asset/original/register-report ")
    );
    let mut invalid = report;
    invalid["totals"]["cost"] = json!(126);
    let (api, t) = server(200, invalid);
    assert_eq!(
        api.original_asset_register_report("synthetic").await,
        Err(Error::Protocol)
    );
    t.join().unwrap();
}

#[tokio::test]
async fn writeoff_success_exact_and_zero_registration_slice_is_valid() {
    let core = json!({"ok":true,"entryId":2,"entryNo":"2026-0002","entryHash":"b".repeat(64),"writeOffId":1,"cost":250,"thresholdDkk":36000,"appliedRules":["DK-ASSET-WRITEOFF-001"],"errors":[]});
    let mut input = writeoff();
    input["confirmImmediateWriteOff"] = json!(true);
    let (api, t) = server(200, core.clone());
    assert_eq!(
        api.original_asset_write_off("synthetic", &input, true, None)
            .await
            .unwrap(),
        core
    );
    t.join().unwrap();
    let mut input = registration();
    input["cost"] = json!(0.01);
    input["usefulLifeMonths"] = json!(2);
    let mut core = registered();
    core["totalPeriods"] = json!(2);
    core["periodAmount"] = json!(0);
    let (api, t) = server(200, core.clone());
    assert_eq!(
        api.original_asset_register("synthetic", &input, true, None)
            .await
            .unwrap(),
        core
    );
    t.join().unwrap();
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    assert!(matches!(
        api.original_asset_register("synthetic", &input, false, None)
            .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.original_asset_depreciate(
            "synthetic",
            0,
            &json!({"period":1,"date":"2026-01-01"}),
            true,
            None
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
}
