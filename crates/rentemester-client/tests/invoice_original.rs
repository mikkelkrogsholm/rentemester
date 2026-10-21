use rentemester_client::{ApiClient, Error, InvoiceOriginalKind, InvoiceOriginalQuery};
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
async fn four_read_routes_preserve_selectors_negative_money_blank_filters_and_canonical_data() {
    for (kind, query, path, contains, data) in [
        (
            InvoiceOriginalKind::List,
            InvoiceOriginalQuery {
                status: Some("all".into()),
                from: Some("2025-01-01".into()),
                to: Some("2026-12-31".into()),
                customer_cvr: Some(" dk123 ".into()),
                customer: Some(" ".into()),
                invoice_number: Some(" Exact ".into()),
                min_amount: Some(serde_json::from_str("-123.45").unwrap()),
                max_amount: Some(serde_json::from_str("0").unwrap()),
                ..Default::default()
            },
            "list",
            vec![
                "from=2025-01-01",
                "customerCvr=+dk123+",
                "customer=+",
                "invoiceNumber=+Exact+",
                "minAmount=-123.45",
                "maxAmount=0",
            ],
            json!({"count":0,"rows":[],"asOfDate":"server-copenhagen"}),
        ),
        (
            InvoiceOriginalKind::Find,
            InvoiceOriginalQuery {
                query: Some("literal % & words".into()),
                min_amount: Some(serde_json::from_str("-999").unwrap()),
                max_amount: Some(serde_json::from_str("999").unwrap()),
                amount: Some(serde_json::from_str("-12.34").unwrap()),
                as_of: Some("2020-01-01".into()),
                ..Default::default()
            },
            "find",
            vec![
                "query=literal+%25+%26+words",
                "minAmount=-999",
                "maxAmount=999",
                "amount=-12.34",
            ],
            json!({"count":1,"rows":[{"invoiceNumber":"future-but-current"}]}),
        ),
        (
            InvoiceOriginalKind::Status,
            InvoiceOriginalQuery {
                document_id: Some(7),
                invoice_number: Some("ignored-but-forwarded".into()),
                ..Default::default()
            },
            "status",
            vec!["documentId=7", "invoiceNumber=ignored-but-forwarded"],
            json!({"invoiceDocumentId":7,"currency":"EUR","claimOpenBalance":null,"principalCurrency":"EUR","claimCurrency":"DKK","registeredClaimBalanceDkk":100,"payments":[{"amount":12,"currency":"EUR"}]}),
        ),
        (
            InvoiceOriginalKind::Overdue,
            InvoiceOriginalQuery {
                as_of: Some("2026-04-01".into()),
                min_days: Some(0),
                ..Default::default()
            },
            "overdue",
            vec!["asOf=2026-04-01", "minDays=0"],
            json!({"count":0,"rows":[]}),
        ),
    ] {
        let value = json!({"ok":true,"data":data,"errors":[]});
        let (origin, t) = server(200, value.clone());
        assert_eq!(
            ApiClient::new(&origin, true, None)
                .unwrap()
                .invoice_original("synthetic", kind, &query)
                .await
                .unwrap(),
            value
        );
        let (header, body) = t.join().unwrap();
        assert!(header.starts_with(&format!(
            "GET /api/companies/synthetic/invoices/original/{path}?"
        )));
        assert!(contains.into_iter().all(|s| header.contains(s)));
        assert!(body.is_null());
    }
}
#[tokio::test]
async fn business_missing_current_source_and_protocol_errors_preserve_read_only_policy() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    assert!(matches!(
        api.invoice_original(
            "synthetic",
            InvoiceOriginalKind::Overdue,
            &InvoiceOriginalQuery {
                customer: Some("irrelevant".into()),
                ..Default::default()
            }
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
    for (status, value) in [
        (
            200,
            json!({"ok":false,"errors":["invoice document 999 does not exist"]}),
        ),
        (403, json!({"ok":false})),
        (503, json!({"ok":false,"code":"source"})),
        (200, json!({"ok":false,"errors":["invalid"],"data":{}})),
    ] {
        let (origin, t) = server(status, value.clone());
        let outcome = ApiClient::new(&origin, true, None)
            .unwrap()
            .invoice_original(
                "synthetic",
                InvoiceOriginalKind::Status,
                &InvoiceOriginalQuery {
                    document_id: Some(999),
                    ..Default::default()
                },
            )
            .await;
        if status != 200 {
            assert!(matches!(outcome, Err(Error::Refused { .. })));
        } else if value.get("data").is_some() {
            assert!(matches!(outcome, Err(Error::Protocol)));
        } else {
            assert_eq!(outcome.unwrap(), value);
        }
        t.join().unwrap();
    }
}
