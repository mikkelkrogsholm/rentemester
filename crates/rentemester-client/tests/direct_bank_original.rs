mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
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

fn plan() -> Value {
    json!({"documentId":3,"bankTransactionId":7,"billDate":"2026-01-01","dueDate":"2026-01-31","expenseAccountNo":"expense","note":" raw note "})
}
fn apply() -> Value {
    let mut value = plan();
    value["confirm"] = json!(true);
    value["planHash"] = json!("a".repeat(64));
    value["reason"] = json!(" raw reason ");
    value["idempotencyKey"] = json!(" raw key ");
    value["expectedPrincipal"] = json!({"kind":"service-account","subjectId":"service-7"});
    value
}
#[tokio::test]
async fn original_plan_and_apply_preserve_full_proof_raw_key_principal_and_replay_receipt() {
    for (write, input, data) in [
        (
            false,
            plan(),
            json!({"plan":{"planHash":"native-proof","sourceProof":{"preserved":true}}}),
        ),
        (
            true,
            apply(),
            json!({"id":4,"idempotent":false,"sourcePrincipal":{"kind":"service-account","subjectId":"service-7"},"originalJournalEntryId":8,"reversalJournalEntryId":9,"idempotency":{"replayed":true,"receiptId":"server"}}),
        ),
    ] {
        let value = json!({"ok":true,"data":data,"errors":[]});
        let (origin, t) = server(200, value.clone());
        assert_eq!(
            ApiClient::new(&origin, true, None)
                .unwrap()
                .direct_bank_original("synthetic", input.to_string().into_bytes(), write)
                .await
                .unwrap(),
            value
        );
        let (header, body) = t.join().unwrap();
        assert!(header.starts_with(&format!(
            "POST /api/companies/synthetic/bank/original/direct-payable-{} ",
            if write { "apply" } else { "plan" }
        )));
        assert_eq!(body, input);
    }
}
#[tokio::test]
async fn strict_shapes_business_failures_and_write_unknown_do_not_launder_source_refusals() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for (key, bad) in [
        ("note", Value::Null),
        ("vendorId", Value::Null),
        ("vatTreatment", Value::Null),
        (
            "expectedPrincipal",
            json!({"kind":"user","subjectId":"forged"}),
        ),
        ("confirm", json!(true)),
        ("actor", json!("forged")),
    ] {
        let mut body = plan();
        body[key] = bad;
        assert!(matches!(
            api.direct_bank_original("synthetic", body.to_string().into_bytes(), false)
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
    for (status, write, value) in [
        (
            200,
            false,
            json!({"ok":false,"errors":["server-owned business"],"code":"business"}),
        ),
        (
            403,
            true,
            json!({"ok":false,"errors":["principal mismatch"]}),
        ),
        (
            503,
            false,
            json!({"ok":false,"errors":["source unavailable"],"code":"source"}),
        ),
        (503, true, json!({"ok":false,"code":"source"})),
        (408, true, json!({"ok":false})),
        (
            200,
            true,
            json!({"ok":false,"data":{},"errors":["invalid reply"]}),
        ),
    ] {
        let (origin, t) = server(status, value.clone());
        let outcome = ApiClient::new(&origin, true, None)
            .unwrap()
            .direct_bank_original(
                "synthetic",
                if write { apply() } else { plan() }
                    .to_string()
                    .into_bytes(),
                write,
            )
            .await;
        if status == 200 && !write {
            assert_eq!(outcome.unwrap(), value);
        } else if write && (status == 503 || status == 408 || status == 200) {
            assert!(matches!(outcome, Err(Error::OutcomeUnknown)));
        } else {
            assert!(matches!(outcome, Err(Error::Refused { .. })));
        }
        t.join().unwrap();
    }
}
