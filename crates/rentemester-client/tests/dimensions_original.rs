mod common;
use common::TcpListener;
use rentemester_client::{
    ApiClient, DimensionsOriginalOperation as Op, Error, original_dimensions_schema,
    validate_original_dimensions,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    thread,
};
const OPS: [Op; 14] = [
    Op::Define,
    Op::Member,
    Op::DefinitionLifecycle,
    Op::MemberLifecycle,
    Op::Definitions,
    Op::Members,
    Op::Plan,
    Op::Apply,
    Op::Replace,
    Op::Supersede,
    Op::List,
    Op::BudgetPlan,
    Op::BudgetApply,
    Op::Budgets,
];
#[test]
fn frozen_schema_and_strict_inputs() {
    for op in OPS {
        let s = original_dimensions_schema(op);
        assert_eq!(s["additionalProperties"], false);
        assert!(s["required"].is_array());
        for k in ["company", "actor", "principal"] {
            assert!(s["properties"].get(k).is_none());
            assert!(validate_original_dimensions(op, &json!({k:"forged"})).is_err());
        }
    }
    assert!(validate_original_dimensions(Op::Definitions, &json!({})).is_ok());
    let mut large = assignment();
    large["allocations"] = json!([]);
    let allocation = json!({"dimensionId":"d".repeat(64),"memberId":"m".repeat(64),"amountMinor":1,"currency":"DKK"});
    while serde_json::to_vec(&large).unwrap().len() <= 65536 {
        large["allocations"]
            .as_array_mut()
            .unwrap()
            .push(allocation.clone());
    }
    assert!(serde_json::to_vec(&large).unwrap().len() < 66560);
    assert!(validate_original_dimensions(Op::Plan, &large).is_err());
    assert!(
        validate_original_dimensions(Op::List, &json!({"journalLineId":9007199254740992u64}))
            .is_err()
    );
    assert!(
        validate_original_dimensions(
            Op::Define,
            &json!({"dimensionId":"project","kind":"project","name":"Projects","confirm":false})
        )
        .is_err()
    );
    for amount in [json!(0.001), json!(-1), json!("12.00")] {
        assert!(validate_original_dimensions(Op::BudgetPlan,&json!({"accountNo":"1000","period":"2026-01","sourceRef":"review","allocations":[{"dimensionId":"project","memberId":"alpha","amount":amount}]})).is_err());
    }
}
fn server(status: u16, reply: Value, target: &'static str) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 4096];
            let n = stream.read(&mut chunk).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let h = String::from_utf8_lossy(&bytes[..end]);
                assert!(h.starts_with(target), "{h}");
                let len = h
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|n| n.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + len {
                    break;
                }
            }
        }
        let body = reply.to_string();
        write!(
            stream,
            "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        listener.set_nonblocking(true).unwrap();
        thread::sleep(std::time::Duration::from_millis(100));
        assert!(listener.accept().is_err(), "adapter must not retry");
    });
    (origin, handle)
}

#[tokio::test]
async fn receipts_status_and_no_retry() {
    for (op, input, status, reply, expected) in [
        (
            Op::Definitions,
            json!({}),
            200,
            json!({"ok":true,"definitions":[]}),
            None,
        ),
        (
            Op::Definitions,
            json!({}),
            200,
            json!({"ok":true,"definitions":[{"id":1,"dimension_id":"project","kind":"project","name":"Projects","status":"active","event_type":"defined","supersedes_dimension_id":null,"actor":"agent:synthetic","principal":"user:reviewer","created_at":"2026-01-01T00:00:00Z"}]}),
            None,
        ),
        (
            Op::Definitions,
            json!({}),
            200,
            json!({"ok":true}),
            Some(Error::Protocol),
        ),
        (
            Op::Define,
            json!({"dimensionId":"project","kind":"project","name":"Projects","confirm":true}),
            200,
            json!({"ok":true,"id":1,"idempotent":false}),
            None,
        ),
        (
            Op::Define,
            json!({"dimensionId":"project","kind":"project","name":"Projects","confirm":true}),
            200,
            json!({"ok":true,"id":0,"idempotent":false}),
            Some(Error::OutcomeUnknown),
        ),
        (
            Op::Define,
            json!({"dimensionId":"project","kind":"project","name":"Projects","confirm":true}),
            503,
            json!({}),
            Some(Error::OutcomeUnknown),
        ),
        (
            Op::Definitions,
            json!({}),
            401,
            json!({}),
            Some(Error::Refused { status: 401 }),
        ),
        (
            Op::Definitions,
            json!({}),
            403,
            json!({}),
            Some(Error::Refused { status: 403 }),
        ),
        (
            Op::Definitions,
            json!({}),
            400,
            json!({}),
            Some(Error::Refused { status: 400 }),
        ),
        (
            Op::Definitions,
            json!({}),
            422,
            json!({"ok":false,"errors":["actual refusal"]}),
            None,
        ),
    ] {
        let target = if op == Op::Define {
            "POST /api/companies/synthetic/original/dimensions/define "
        } else {
            "GET /api/companies/synthetic/original/dimensions/definitions "
        };
        let (origin, h) = server(status, reply.clone(), target);
        let result = ApiClient::new(&origin, true, None)
            .unwrap()
            .original_dimensions("synthetic", op, &input, None, None)
            .await;
        if let Some(e) = expected {
            assert_eq!(result, Err(e));
        } else {
            assert_eq!(result.unwrap(), reply);
        }
        h.join().unwrap();
    }
}
fn assignment() -> Value {
    json!({"journalLineId":1,"allocations":[{"dimensionId":"project","memberId":"alpha","amountMinor":1250,"currency":"DKK"}]})
}
#[tokio::test]
async fn exact_native_plan_and_receipt_binding() {
    use sha2::{Digest, Sha256};
    for tamper in 0..4 {
        let mut input = assignment();
        if tamper == 3 {
            input["allocations"][0]["currency"] = json!("dkk");
        }
        let mut p = json!({"schemaVersion":"rentemester-accounting-dimensions-v1","journalLineId":1,"journalEntryId":2,"journalEntryHash":"b".repeat(64),"lineCurrency":"DKK","lineAmountMinor":1250,"allocations":input["allocations"],"source":"reviewed","sourceRef":null});
        p["allocations"][0]["currency"] = json!("DKK");
        p["planHash"] = json!(format!("{:x}", Sha256::digest(p.to_string().as_bytes())));
        if tamper == 1 {
            p["journalLineId"] = json!(3);
        }
        if tamper == 2 {
            p["allocations"][0]["amountMinor"] = json!(999);
        }
        let reply = json!({"ok":true,"plan":p});
        let (origin, h) = server(
            200,
            reply.clone(),
            "POST /api/companies/synthetic/original/dimensions/plan ",
        );
        let result = ApiClient::new(&origin, true, None)
            .unwrap()
            .original_dimensions("synthetic", Op::Plan, &input, None, None)
            .await;
        if tamper == 0 || tamper == 3 {
            assert_eq!(result.unwrap(), reply);
        } else {
            assert_eq!(result, Err(Error::Protocol));
        }
        h.join().unwrap();
    }
    let mut input = assignment();
    input["confirm"] = json!(true);
    input["planHash"] = json!("a".repeat(64));
    let (origin, h) = server(
        200,
        json!({"ok":true,"id":1,"idempotent":true,"planHash":"b".repeat(64)}),
        "POST /api/companies/synthetic/original/dimensions/apply ",
    );
    assert_eq!(
        ApiClient::new(&origin, true, None)
            .unwrap()
            .original_dimensions(
                "synthetic",
                Op::Apply,
                &input,
                Some("user:owner"),
                Some("user:owner")
            )
            .await,
        Err(Error::OutcomeUnknown)
    );
    h.join().unwrap();
}

// Independent canonical fixture: integer numeric spelling (not serde float 20.0).
#[tokio::test]
async fn budget_plan_uses_exact_source_canonical_numbers_and_requested_set() {
    let input = json!({"accountNo":"1000","period":"2026-01","allocations":[{"dimensionId":"project","memberId":"alpha","amount":12.0},{"dimensionId":"project","memberId":"beta","amount":8.0}],"sourceRef":"review"});
    let base = json!({"ok":true,"plan":{"schemaVersion":"rentemester-dimension-budget-v1","accountNo":"1000","period":"2026-01","accountBudget":20.0,"allocations":input["allocations"],"sourceRef":"review","planHash":"65c8a2f0cc7a6d12ba9aa2cdd4c0c82557728cad96f1f24157fde83d7ff46fdb"}});
    for tamper in 0..3 {
        let mut reply = base.clone();
        if tamper == 1 {
            reply["plan"]["sourceRef"] = json!("other");
        }
        if tamper == 2 {
            reply["plan"]["accountBudget"] = json!(21);
        }
        let (origin, h) = server(
            200,
            reply.clone(),
            "POST /api/companies/synthetic/original/dimensions/budget-plan ",
        );
        let result = ApiClient::new(&origin, true, None)
            .unwrap()
            .original_dimensions("synthetic", Op::BudgetPlan, &input, None, None)
            .await;
        if tamper == 0 {
            assert_eq!(result.unwrap(), reply);
        } else {
            assert_eq!(result, Err(Error::Protocol));
        }
        h.join().unwrap();
    }
}
