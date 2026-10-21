use rentemester_client::{ApiClient, BankOriginalMutation, Error};
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
async fn original_read_and_mutation_endpoints_preserve_full_envelopes_and_raw_assertions() {
    let suggestions = json!({"ok":true,"data":{"count":1,"rows":[{"bankTransactionId":7,"suggestions":[{"confidence":1.4}]}]},"errors":[]});
    let (origin, t) = server(200, suggestions.clone());
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.bank_original_suggestions("synthetic", Some(7), Some(3))
            .await
            .unwrap(),
        suggestions
    );
    let (header, body) = t.join().unwrap();
    assert!(header.starts_with(
        "GET /api/companies/synthetic/bank/original/suggestions?bankTransactionId=7&max=3 "
    ));
    assert!(body.is_null());
    let plan = json!({"ok":true,"data":{"plan":{"planHash":"server-proof","sourceProof":{"retained":true}}},"errors":[]});
    let (origin, t) = server(200, plan.clone());
    assert_eq!(
        ApiClient::new(&origin, true, None)
            .unwrap()
            .bank_original_correction_plan("synthetic", 7, 9)
            .await
            .unwrap(),
        plan
    );
    assert!(t.join().unwrap().0.starts_with("GET /api/companies/synthetic/bank/original/correction-plan?bankTransactionId=7&replacementJournalEntryId=9 "));
    for (operation, action, input) in [
        (
            BankOriginalMutation::LinkJournal,
            "link-journal",
            json!({"confirm":true,"bankTransactionId":7,"journalEntryId":8,"matchMethod":"source-reference","sourceReference":" exact-ref "}),
        ),
        (
            BankOriginalMutation::CorrectionApply,
            "correction-apply",
            json!({"confirm":true,"bankTransactionId":7,"replacementJournalEntryId":9,"expectedReconciliationId":"current","planHash":"a".repeat(64),"reason":" raw reason ","idempotencyKey":" raw key ","expectedPrincipal":{"kind":"service-account","subjectId":"service-7"}}),
        ),
    ] {
        let value = json!({"ok":true,"data":{"id":8,"idempotent":false,"receipt":{"server":true}},"errors":[],"appliedRules":["DK-BOOKKEEPING-RECONCILIATION-001"]});
        let (origin, t) = server(200, value.clone());
        assert_eq!(
            ApiClient::new(&origin, true, None)
                .unwrap()
                .bank_original_mutation("synthetic", operation, input.to_string().into_bytes())
                .await
                .unwrap(),
            value
        );
        let (header, body) = t.join().unwrap();
        assert!(header.starts_with(&format!(
            "POST /api/companies/synthetic/bank/original/{action} "
        )));
        assert_eq!(body, input);
    }
}
#[tokio::test]
async fn business_refusal_and_uncertain_write_never_become_success_or_retry() {
    let input = json!({"confirm":true,"bankTransactionId":7,"journalEntryId":8,"matchMethod":"manual-review"});
    for (status, value) in [
        (
            200,
            json!({"ok":false,"code":"stale","errors":["server failure"],"appliedRules":["rule"]}),
        ),
        (403, json!({"ok":false})),
        (503, json!({"ok":false})),
        (200, json!({"ok":false,"data":{},"errors":["bad"]})),
    ] {
        let (origin, t) = server(status, value.clone());
        let outcome = ApiClient::new(&origin, true, None)
            .unwrap()
            .bank_original_mutation(
                "synthetic",
                BankOriginalMutation::LinkJournal,
                input.to_string().into_bytes(),
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
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for bad in [
        json!({"confirm":false,"bankTransactionId":7,"journalEntryId":8,"matchMethod":"manual-review"}),
        json!({"confirm":true,"bankTransactionId":7,"journalEntryId":8,"matchMethod":"manual-review","actor":"spoof"}),
        json!({"confirm":true,"bankTransactionId":7,"journalEntryId":8,"matchMethod":"manual-review","note":null}),
    ] {
        assert!(matches!(
            api.bank_original_mutation(
                "synthetic",
                BankOriginalMutation::LinkJournal,
                bad.to_string().into_bytes()
            )
            .await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.bank_original_suggestions("synthetic", None, Some(0))
            .await,
        Err(Error::InvalidInput(_))
    ));
}
