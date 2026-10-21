use rentemester_client::{ApiClient, Error, InvoiceLifecycleKind};
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

fn issue() -> Value {
    json!({"invoiceType":"full","issueDate":"2026-01-01","lines":[{"description":"Synthetic","quantity":2,"unitPriceExVat":50,"lineTotalExVat":100}],"totals":{"netAmount":100,"vatRate":0.25,"vatAmount":25,"grossAmount":125}})
}
#[tokio::test]
async fn all_seven_routes_keep_structured_payloads_computed_fields_account_and_server_receipts() {
    for (kind, route, body) in [
        (
            InvoiceLifecycleKind::Issue,
            "issue",
            json!({"confirm":true,"customerId":2,"payload":issue()}),
        ),
        (
            InvoiceLifecycleKind::Create,
            "create",
            json!({"confirm":true,"payload":{"issueDate":"2026-01-01","vatRatePercent":25,"lines":[{"description":"Synthetic","quantity":2,"unitPriceExVat":50}]}}),
        ),
        (
            InvoiceLifecycleKind::Render,
            "render",
            json!({"confirm":true,"documentId":7,"invoiceNumber":"fallback"}),
        ),
        (
            InvoiceLifecycleKind::Post,
            "post",
            json!({"confirm":true,"documentId":7,"revenueAccountNo":"synthetic-income"}),
        ),
        (
            InvoiceLifecycleKind::SettleBank,
            "settle-bank",
            json!({"confirm":true,"payload":{"invoiceNumber":"INV-7","bankTransactionReference":"incoming-ref","amount":50,"bankAccountNo":"bank","receivableAccountNo":"debtor"}}),
        ),
        (
            InvoiceLifecycleKind::CreditNote,
            "credit-note",
            json!({"confirm":true,"payload":{"originalInvoiceDocumentId":7,"issueDate":"2026-01-02","reason":" raw credit ","grossAmount":25}}),
        ),
        (
            InvoiceLifecycleKind::RefundBank,
            "refund-bank",
            json!({"confirm":true,"payload":{"invoiceDocumentId":7,"bankTransactionId":11,"paymentDate":"2026-01-03","refundDate":"2026-01-03","amount":25}}),
        ),
    ] {
        let value = json!({"ok":true,"data":{"documentId":7,"entryId":8,"entryNo":9,"sha256":"server-source","pdfStoredPath":"/api/original.pdf","computed":{"net":100,"vat":25,"gross":125}},"errors":[],"appliedRules":["server-rule"]});
        let (origin, t) = server(200, value.clone());
        assert_eq!(
            ApiClient::new(&origin, true, None)
                .unwrap()
                .invoice_lifecycle("synthetic", kind, body.to_string().into_bytes())
                .await
                .unwrap(),
            value
        );
        let (header, actual) = t.join().unwrap();
        assert!(header.starts_with(&format!(
            "POST /api/companies/synthetic/invoices/original/{route} "
        )));
        assert_eq!(actual, body);
    }
}
#[tokio::test]
async fn strict_source_payloads_confirm_business_account_selection_and_unknown_writes() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for body in [
        json!({"confirm":false,"payload":issue()}),
        json!({"confirm":true,"payload":"{}"}),
        json!({"confirm":true,"payload":{"invoiceType":"full","lines":[{"description":"Synthetic","quantity":null}]}}),
        json!({"confirm":true,"payload":{"invoiceType":"full","createdBy":"forged"}}),
        json!({"confirm":true,"payload":issue(),"idempotencyKey":"key"}),
        json!({"confirm":true,"payload":issue(),"actor":"forged"}),
    ] {
        assert!(matches!(
            api.invoice_lifecycle(
                "synthetic",
                InvoiceLifecycleKind::Issue,
                body.to_string().into_bytes()
            )
            .await,
            Err(Error::InvalidInput(_))
        ));
    }
    for (status, kind, value) in [
        (
            200,
            InvoiceLifecycleKind::Post,
            json!({"ok":false,"code":"account_selection_required","errors":["Select revenue account"]}),
        ),
        (403, InvoiceLifecycleKind::Post, json!({"ok":false})),
        (
            503,
            InvoiceLifecycleKind::Post,
            json!({"ok":false,"code":"source"}),
        ),
        (
            503,
            InvoiceLifecycleKind::Render,
            json!({"ok":false,"code":"source"}),
        ),
        (
            200,
            InvoiceLifecycleKind::Render,
            json!({"ok":false,"data":{},"errors":["invalid"]}),
        ),
    ] {
        let (origin, t) = server(status, value.clone());
        let out = ApiClient::new(&origin, true, None)
            .unwrap()
            .invoice_lifecycle(
                "synthetic",
                kind,
                json!({"confirm":true,"documentId":7})
                    .to_string()
                    .into_bytes(),
            )
            .await;
        if status == 200 && matches!(kind, InvoiceLifecycleKind::Post) {
            assert_eq!(out.unwrap(), value);
        } else if status == 503 && matches!(kind, InvoiceLifecycleKind::Post) {
            assert!(matches!(out, Err(Error::OutcomeUnknown)));
        } else if status == 200 {
            assert!(matches!(out, Err(Error::Protocol)));
        } else {
            assert!(matches!(out, Err(Error::Refused { .. })));
        }
        t.join().unwrap();
    }
}
