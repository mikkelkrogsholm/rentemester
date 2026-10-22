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
