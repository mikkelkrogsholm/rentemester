use rentemester_client::{
    ApiClient, Error, OriginalInvoiceClaimKind as Kind, validate_invoice_claims_original,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};
fn server(status: u16, value: Value) -> (String, thread::JoinHandle<(String, Value)>) {
    let response = value.to_string().into_bytes();
    let length = response.len();
    raw_server(status, response, length)
}
fn raw_server(
    status: u16,
    response: Vec<u8>,
    response_length: usize,
) -> (String, thread::JoinHandle<(String, Value)>) {
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
        if status != 0 {
            write!(s, "HTTP/1.1 {status} Mock\r\nContent-Length:{response_length}\r\nConnection:close\r\n\r\n").unwrap();
            // Oversized responses may be rejected before the server finishes writing.
            let _ = s.write_all(&response);
        }
        drop(s);
        l.set_nonblocking(true).unwrap();
        for _ in 0..10 {
            assert!(
                matches!(l.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock),
                "unexpected retry"
            );
            thread::sleep(std::time::Duration::from_millis(5));
        }
        (header, body)
    });
    (origin, t)
}

fn body(kind: Kind) -> Value {
    use Kind::*;
    let mut value = match kind {
        Remind => {
            json!({"documentId":7,"invoiceNumber":" fallback ","date":"2026-02-03","note":" raw note "})
        }
        PostReminder => {
            json!({"invoiceNumber":"INV","receivableAccountNo":"debtor","reminderIncomeAccountNo":"reminder-income"})
        }
        Compensation => json!({"documentId":7,"asOf":"2026-02-03","amountDkk":250}),
        ClaimCompensation => json!({"documentId":7,"asOf":"2026-02-03","note":" raw note "}),
        PostCompensation => {
            json!({"documentId":7,"date":"2026-02-03","receivableAccountNo":"debtor","compensationIncomeAccountNo":"comp-income"})
        }
        Interest => json!({"documentId":7,"asOf":"2026-02-03"}),
        ClaimInterest => {
            json!({"documentId":7,"asOf":"2026-02-03","referenceRate":2.65,"note":" raw note "})
        }
        PostInterest => {
            json!({"documentId":7,"claimId":9,"receivableAccountNo":"debtor","interestIncomeAccountNo":"interest-income"})
        }
        InterestCorrection => json!({"documentId":7}),
        PostInterestCorrection => json!({"documentId":7,"reason":" reviewed reason "}),
        SettleClaimBank => {
            json!({"payload":{"invoiceDocumentId":7,"invoiceNumber":" fallback ","bankTransactionReference":" raw bank ref ","paymentDate":"2026-02-03","amount":12.34,"bankAccountNo":"bank","receivableAccountNo":"historical-debtor"}})
        }
    };
    if !kind.is_read_only() {
        value["confirm"] = json!(true);
    }
    value
}

#[tokio::test]
async fn all_eleven_source_contracts_preserve_route_fields_omissions_and_canonical_results() {
    for kind in Kind::ALL {
        let body = body(kind);
        let value = json!({"ok":true,"data":{"sourceOperation":kind.suffix(),"claimOpenBalance":null,"principalCurrency":"EUR","claimBalanceDkk":17,"accruedInterestAmount":1.23,"priorClaimedInterest":2,"hasProposal":true,"canPost":false,"requiresRefund":true,"proposedLines":[{"accountNo":"historical-income"}]},"errors":[],"appliedRules":["source-rule"]});
        let (origin, t) = server(200, value.clone());
        assert_eq!(
            ApiClient::new(&origin, true, None)
                .unwrap()
                .invoice_claims_original("synthetic", kind, body.to_string().into_bytes())
                .await
                .unwrap(),
            value
        );
        let (header, actual) = t.join().unwrap();
        assert!(header.starts_with(&format!(
            "POST /api/companies/synthetic/invoices/original/{} ",
            kind.suffix()
        )));
        assert_eq!(actual, body);
    }
}
#[tokio::test]
async fn original_claims_strict_shapes_business_refusals_and_read_write_transport_safety() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for (kind, body) in [
        (
            Kind::Remind,
            json!({"documentId":7,"date":"2026-01-01","confirm":false}),
        ),
        (Kind::Interest, json!({"documentId":7,"asOf":null})),
        (
            Kind::Interest,
            json!({"documentId":7,"asOf":"2026-01-01","confirm":true}),
        ),
        (
            Kind::PostInterestCorrection,
            json!({"documentId":7,"confirm":true}),
        ),
        (Kind::PostInterest, json!({"documentId":7,"confirm":true})),
        (
            Kind::SettleClaimBank,
            json!({"payload":{"invoiceDocumentId":7,"actor":"forged"},"confirm":true}),
        ),
    ] {
        assert!(matches!(
            api.invoice_claims_original("synthetic", kind, body.to_string().into_bytes())
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
    validate_invoice_claims_original(
        Kind::Interest,
        br#"{"documentId":7,"asOf":"2026-02-30","referenceRate":-1}"#,
    )
    .unwrap();
    for (kind, status, value) in [
        (
            Kind::Interest,
            200,
            json!({"ok":false,"code":"unsupported_foreign_interest","errors":["unsupported foreign interest"]}),
        ),
        (Kind::Remind, 403, json!({"ok":false})),
        (Kind::Remind, 503, json!({"ok":false})),
        (Kind::Interest, 503, json!({"ok":false})),
        (Kind::Remind, 200, json!({"broken":true})),
        (Kind::Interest, 200, json!({"broken":true})),
    ] {
        let (origin, t) = server(status, value.clone());
        let result = ApiClient::new(&origin, true, None)
            .unwrap()
            .invoice_claims_original("synthetic", kind, body(kind).to_string().into_bytes())
            .await;
        if value.get("code").is_some() {
            assert_eq!(result.unwrap(), value);
        } else if status == 403 || kind.is_read_only() && status == 503 {
            assert_eq!(result.unwrap_err(), Error::Refused { status });
        } else if kind.is_read_only() {
            assert_eq!(result.unwrap_err(), Error::Protocol);
        } else {
            assert_eq!(result.unwrap_err(), Error::OutcomeUnknown);
        }
        t.join().unwrap();
    }
}

#[tokio::test]
async fn every_claim_preserves_exact_422_and_legacy_codeless_200_refusals() {
    for kind in Kind::ALL {
        for status in [200, 422] {
            let mut value = json!({"ok":false,"errors":[" exact server error ","second error"],"appliedRules":["source-rule"]});
            if status == 422 {
                value["code"] = json!(" server_claim_gate ");
            }
            let (origin, t) = server(status, value.clone());
            assert_eq!(
                ApiClient::new(&origin, true, None)
                    .unwrap()
                    .invoice_claims_original("synthetic", kind, body(kind).to_string().into_bytes())
                    .await
                    .unwrap(),
                value
            );
            t.join().unwrap();
        }
    }
}

#[tokio::test]
async fn every_claim_rejects_untrusted_or_inconsistent_error_envelopes_generically() {
    let invalid = [
        json!({"ok":true,"data":{},"code":"remote-secret","errors":[]}),
        json!({"ok":false,"data":{},"code":"remote-secret","errors":["remote-secret"]}),
        json!({"ok":false,"errors":["remote-secret"]}),
        json!({"ok":false,"code":"","errors":["remote-secret"]}),
        json!({"ok":false,"code":" \t ","errors":["remote-secret"]}),
        json!({"ok":false,"code":12,"errors":["remote-secret"]}),
        json!({"ok":false,"code":"remote-secret","errors":[]}),
        json!({"ok":false,"code":"remote-secret","errors":[12]}),
        json!({"ok":false,"code":"remote-secret","errors":["remote-secret"],"appliedRules":[12]}),
        json!({"ok":false,"code":"remote-secret","errors":["remote-secret"],"appliedRules":null}),
    ];
    for kind in Kind::ALL {
        for value in &invalid {
            let (origin, t) = server(422, value.clone());
            let error = ApiClient::new(&origin, true, None)
                .unwrap()
                .invoice_claims_original("synthetic", kind, body(kind).to_string().into_bytes())
                .await
                .unwrap_err();
            assert_eq!(error, Error::Refused { status: 422 });
            assert!(!error.to_string().contains("remote-secret"));
            t.join().unwrap();
        }
        for status in [401, 403, 409] {
            let (origin, t) = server(
                status,
                json!({"ok":false,"code":"remote-secret","errors":["remote-secret"]}),
            );
            assert_eq!(
                ApiClient::new(&origin, true, None)
                    .unwrap()
                    .invoice_claims_original("synthetic", kind, body(kind).to_string().into_bytes())
                    .await
                    .unwrap_err(),
                Error::Refused { status }
            );
            t.join().unwrap();
        }
    }
}

#[tokio::test]
async fn bounded_complete_receipts_and_unknown_writes_never_retry_and_reads_stay_read_only() {
    for kind in Kind::ALL {
        let oversized =
            json!({"ok":false,"code":"gate","errors":["x".repeat(16 * 1024 * 1024 + 1)]})
                .to_string()
                .into_bytes();
        for (status, response, extra) in [
            (422, b"not json remote-secret".to_vec(), 0),
            (422, b"{\"ok\":false".to_vec(), 100),
            (422, oversized, 0),
            (408, vec![], 0),
            (503, vec![], 0),
            (0, vec![], 0),
            (200, b"{\"ok\":true".to_vec(), 100),
        ] {
            let length = response.len() + extra;
            let (origin, t) = raw_server(status, response, length);
            let error = ApiClient::new(&origin, true, None)
                .unwrap()
                .invoice_claims_original("synthetic", kind, body(kind).to_string().into_bytes())
                .await
                .unwrap_err();
            let expected = if status == 422 {
                Error::Refused { status }
            } else if !kind.is_read_only() {
                Error::OutcomeUnknown
            } else if status == 0 || status == 200 {
                Error::Transport
            } else {
                Error::Refused { status }
            };
            assert_eq!(error, expected, "{} HTTP{status}", kind.suffix());
            let (_, actual) = t.join().unwrap();
            assert_eq!(actual, body(kind));
            if kind.is_read_only() {
                assert!(actual.get("confirm").is_none());
            }
        }
    }
}
