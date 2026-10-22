use rentemester_client::{
    ApiClient, Error, OriginalRecurringKind as Kind, validate_recurring_original,
};
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

fn template() -> Value {
    json!({"name":"Synthetic quarterly service","interval":"quarterly","intervalCount":2,"deliveryChannel":"email","firstIssueDate":"2024-01-31","invoice":{"invoiceType":"full","vatTreatment":"standard","seller":{"name":"Synthetic Seller","address":"Synthetic1","vatOrCvr":"87654321"},"buyer":{"name":"Synthetic Buyer","address":"Synthetic2","vatOrCvr":"DK12345678","eanNumber":"5790000000074","publicRecipient":true,"buyerReference":" raw reference "},"lines":[{"description":"Synthetic VAT service","quantity":2,"unitPriceExVat":40,"lineTotalExVat":80,"taxClassification":"standard","vatRate":0.25,"unitCode":"H87"}],"totals":{"netAmount":80,"vatRate":0.25,"vatAmount":20,"grossAmount":100},"currency":"DKK"},"paymentTermsDays":30,"deliveryPeriodMode":"interval_window","notes":" raw notes ","confirm":true})
}
#[tokio::test]
async fn original_recurring_fixed_routes_preserve_complete_source_and_flat_results() {
    for (kind, body, result) in [
        (
            Kind::Create,
            template(),
            json!({"ok":true,"templateId":7,"appliedRules":["DK-RECURRING-INVOICE-TEMPLATE-001"],"errors":[]}),
        ),
        (
            Kind::Generate,
            json!({"templateId":7,"asOfDate":"2024-07-31","confirm":true}),
            json!({"ok":true,"created":true,"templateId":7,"periodIndex":1,"documentId":11,"invoiceNumber":"2024-0001","issueDate":"2024-07-31","dueDate":"2024-08-30","deliveryPeriodStart":"2024-07-31","deliveryPeriodEnd":"2025-01-31","appliedRules":["DK-RECURRING-INVOICE-GENERATE-001","DK-INVOICE-FULL-001"],"errors":[]}),
        ),
    ] {
        let (origin, t) = server(200, result.clone());
        let actual = ApiClient::new(&origin, true, None)
            .unwrap()
            .recurring_original("synthetic", kind, body.to_string().into_bytes())
            .await
            .unwrap();
        assert_eq!(actual, result);
        let (header, sent) = t.join().unwrap();
        assert!(header.starts_with(&format!(
            "POST /api/companies/synthetic/recurring-invoices/original/{} ",
            kind.suffix()
        )));
        assert_eq!(sent, body);
    }
    for include in [false, true] {
        let expected = json!({"ok":true,"count":1,"rows":[{"id":7,"name":"Synthetic quarterly service","interval":"quarterly","intervalCount":2,"deliveryChannel":"email","firstIssueDate":"2024-01-31","nextIssueDate":"2025-01-31","paymentTermsDays":30,"deliveryPeriodMode":"interval_window","notes":"raw notes","active":!include,"createdAt":"2024-01-01 00:00:00"}],"errors":[]});
        let (origin, t) = server(200, expected.clone());
        assert_eq!(
            ApiClient::new(&origin, true, None)
                .unwrap()
                .recurring_original_list("synthetic", include)
                .await
                .unwrap(),
            expected
        );
        let (header, body) = t.join().unwrap();
        assert!(header.starts_with(&format!("GET /api/companies/synthetic/recurring-invoices/original/list?includeInactive={include} ")));
        assert_eq!(body, Value::Null);
    }
}
#[tokio::test]
async fn original_recurring_refusals_and_unknown_never_look_like_success() {
    let valid = template();
    for body in [
        json!({"confirm":true}),
        {
            let mut v = valid.clone();
            v["actor"] = json!("forged");
            v
        },
        {
            let mut v = valid.clone();
            v["confirm"] = json!(false);
            v
        },
        {
            let mut v = valid.clone();
            v["invoice"]["buyer"]["actor"] = json!("forged");
            v
        },
        {
            let mut v = valid.clone();
            v["invoice"]["lines"][0]
                .as_object_mut()
                .unwrap()
                .remove("description");
            v
        },
    ] {
        assert!(matches!(
            validate_recurring_original(Kind::Create, &body.to_string().into_bytes()),
            Err(Error::InvalidInput(_))
        ));
    }
    let refusal = json!({"ok":false,"appliedRules":["DK-RECURRING-INVOICE-TEMPLATE-001"],"errors":["invoice assertions differ"]});
    let (origin, t) = server(200, refusal.clone());
    assert_eq!(
        ApiClient::new(&origin, true, None)
            .unwrap()
            .recurring_original("synthetic", Kind::Create, valid.to_string().into_bytes())
            .await
            .unwrap(),
        refusal
    );
    t.join().unwrap();
    for (status, result, error) in [
        (503, json!({}), Error::OutcomeUnknown),
        (408, json!({}), Error::OutcomeUnknown),
        (
            200,
            json!({"ok":true,"template":{"id":7}}),
            Error::OutcomeUnknown,
        ),
        (403, json!({}), Error::Refused { status: 403 }),
    ] {
        let (origin, t) = server(status, result);
        assert_eq!(
            ApiClient::new(&origin, true, None)
                .unwrap()
                .recurring_original("synthetic", Kind::Create, valid.to_string().into_bytes())
                .await,
            Err(error)
        );
        t.join().unwrap();
    }
}

#[tokio::test]
async fn workspace_recurring_preserves_partial_runs_and_never_retries_unknown() {
    let body = json!({"workspace":"/configured/workspace","asOfDate":"2024-07-31","maxGenerations":2,"confirm":true});
    let partial = json!({"ok":false,"companies":[{"slug":"archived","status":"skipped","reason":"archived"},{"slug":"failed","status":"failed","reason":"run failed"},{"slug":"next","status":"processed","generated":2,"attempted":1,"hasMore":true,"remainingGenerations":4,"continuation":{"remainingGenerations":4}}],"errors":["recurring run failed for company failed"],"hasMore":true,"remainingGenerations":4,"continuation":{"remainingGenerations":4,"companies":[{"slug":"next","remainingGenerations":4}]}});
    for core in [
        partial,
        json!({"ok":true,"companies":[{"slug":"next","status":"processed","generated":1,"attempted":0,"hasMore":false,"remainingGenerations":0}],"errors":[],"hasMore":false,"remainingGenerations":0}),
    ] {
        let (origin, t) = server(200, core.clone());
        let actual = ApiClient::new(&origin, true, Some("session=synthetic".into()))
            .unwrap()
            .recurring_run_workspace(body.to_string().into_bytes())
            .await
            .unwrap();
        assert_eq!(actual, core);
        let (headers, sent) = t.join().unwrap();
        assert!(headers.starts_with("POST /api/recurring-invoices/run-workspace "));
        assert_eq!(sent, body);
    }
    for (status, result) in [
        (503, json!({})),
        (408, json!({})),
        (200, json!({"ok":true,"companies":[],"errors":[]})),
    ] {
        let (origin, t) = server(status, result);
        assert_eq!(
            ApiClient::new(&origin, true, None)
                .unwrap()
                .recurring_run_workspace(body.to_string().into_bytes())
                .await,
            Err(Error::OutcomeUnknown)
        );
        t.join().unwrap();
    }
    let api =
        ApiClient::with_service_key("http://127.0.0.1:9", true, "synthetic-key".into()).unwrap();
    assert!(matches!(
        api.recurring_run_workspace(body.to_string().into_bytes())
            .await,
        Err(Error::InvalidInput(_))
    ));
    for invalid in [
        json!({"workspace":"w","asOfDate":"2024-01-01","confirm":false}),
        json!({"workspace":"w","asOfDate":"2024-01-01","maxGenerations":5001,"confirm":true}),
        json!({"workspace":"w","asOfDate":"2024-01-01","company":"forged","confirm":true}),
        json!({"workspace":"w","asOfDate":"2024-01-01","actor":"forged","confirm":true}),
    ] {
        assert!(
            rentemester_client::validate_recurring_workspace(&invalid.to_string().into_bytes())
                .is_err()
        );
    }
}

#[tokio::test]
async fn original_recurring_bounded_422_receipts_for_reads_and_writes() {
    let receipt = json!({"ok":false,"code":"SERVER_RECURRING_POLICY","errors":["template does not exist","period refused"],"appliedRules":["recurring-policy"]});
    for mutation in [false, true] {
        for (status, body, expected) in [
            (422, receipt.clone(), Ok(receipt.clone())),
            (200, receipt.clone(), Ok(receipt.clone())),
            (403, receipt.clone(), Err(Error::Refused { status: 403 })),
            (
                422,
                json!({"ok":true,"code":"BAD","errors":[]}),
                Err(Error::Refused { status: 422 }),
            ),
            (
                422,
                json!({"ok":false,"errors":["private marker"]}),
                Err(Error::Refused { status: 422 }),
            ),
            (
                422,
                json!({"ok":false,"code":"  ","errors":["private marker"]}),
                Err(Error::Refused { status: 422 }),
            ),
            (
                422,
                json!({"ok":false,"code":42,"errors":["private marker"]}),
                Err(Error::Refused { status: 422 }),
            ),
            (
                422,
                json!({"ok":false,"code":"BAD","errors":[]}),
                Err(Error::Refused { status: 422 }),
            ),
            (
                422,
                json!({"ok":false,"code":"BAD","errors":[42]}),
                Err(Error::Refused { status: 422 }),
            ),
            (
                422,
                json!({"ok":false,"code":"BAD","errors":["private marker"],"appliedRules":[42]}),
                Err(Error::Refused { status: 422 }),
            ),
            (
                422,
                json!({"ok":false,"code":"BAD","errors":["private marker"],"data":{}}),
                Err(Error::Refused { status: 422 }),
            ),
            (
                422,
                json!({"ok":false,"code":"BAD","errors":["private marker"],"rows":[],"count":0}),
                Err(Error::Refused { status: 422 }),
            ),
            (
                422,
                json!({"ok":false,"code":"BAD","errors":["private marker"],"created":true,"documentId":7}),
                Err(Error::Refused { status: 422 }),
            ),
        ] {
            let (origin, t) = server(status, body);
            let api = ApiClient::new(&origin, true, None).unwrap();
            let actual = if mutation {
                api.recurring_original(
                    "synthetic",
                    Kind::Create,
                    template().to_string().into_bytes(),
                )
                .await
            } else {
                api.recurring_original_list("synthetic", false).await
            };
            assert_eq!(actual, expected);
            let (headers, body) = t.join().unwrap();
            assert!(headers.starts_with(if mutation { "POST " } else { "GET " }));
            if !mutation {
                assert_eq!(body, Value::Null);
            }
        }
    }
}

// This fixture keeps accepting after the response to detect even a silent retry.
fn broken_server(status: u16, body: String, declared: usize) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut buf = [0; 4096];
            let n = stream.read(&mut buf).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&buf[..n]);
            if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse().unwrap())
                    })
                    .unwrap_or(0);
                if bytes.len() >= end + 4 + length {
                    break;
                }
            }
        }
        if status != 0 {
            let _ = write!(
                stream,
                "HTTP/1.1 {status} Mock\r\nContent-Length:{declared}\r\nConnection:close\r\n\r\n{body}"
            );
        }
        drop(stream);
        listener.set_nonblocking(true).unwrap();
        let until = std::time::Instant::now() + std::time::Duration::from_millis(150);
        while std::time::Instant::now() < until {
            match listener.accept() {
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(std::time::Duration::from_millis(5))
                }
                _ => panic!("unexpected retry"),
            }
        }
    });
    (origin, t)
}

#[tokio::test]
async fn original_recurring_incomplete_and_oversized_responses_never_retry() {
    for mutation in [false, true] {
        for (status, body, declared, expected) in [
            (
                0,
                String::new(),
                0,
                if mutation {
                    Error::OutcomeUnknown
                } else {
                    Error::Transport
                },
            ),
            (
                200,
                "{".into(),
                100,
                if mutation {
                    Error::OutcomeUnknown
                } else {
                    Error::Transport
                },
            ),
            (422, "{".into(), 100, Error::Refused { status: 422 }),
            (422, "not json".into(), 8, Error::Refused { status: 422 }),
            (
                422,
                " ".repeat(16 * 1024 * 1024 + 1),
                16 * 1024 * 1024 + 1,
                Error::Refused { status: 422 },
            ),
            (
                200,
                " ".repeat(16 * 1024 * 1024 + 1),
                16 * 1024 * 1024 + 1,
                if mutation {
                    Error::OutcomeUnknown
                } else {
                    Error::Protocol
                },
            ),
            (
                408,
                String::new(),
                0,
                if mutation {
                    Error::OutcomeUnknown
                } else {
                    Error::Refused { status: 408 }
                },
            ),
            (
                503,
                String::new(),
                0,
                if mutation {
                    Error::OutcomeUnknown
                } else {
                    Error::Refused { status: 503 }
                },
            ),
        ] {
            let (origin, t) = broken_server(status, body, declared);
            let api = ApiClient::new(&origin, true, None).unwrap();
            let actual = if mutation {
                api.recurring_original(
                    "synthetic",
                    Kind::Create,
                    template().to_string().into_bytes(),
                )
                .await
            } else {
                api.recurring_original_list("synthetic", false).await
            };
            assert_eq!(actual, Err(expected));
            t.join().unwrap();
        }
    }
}
