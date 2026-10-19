use rentemester_client::{ApiClient, Error, JournalMutation, Report};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
    time::Duration,
};

fn server(response: &'static str) -> (String, mpsc::Receiver<Vec<u8>>, thread::JoinHandle<usize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (sender, receiver) = mpsc::channel();
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut bytes = Vec::new();
        loop {
            let mut chunk = [0; 4096];
            let size = stream.read(&mut chunk).unwrap();
            if size == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..size]);
            if let Some(header_end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..header_end]).to_lowercase();
                let content_length = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .map(|n| n.parse::<usize>().unwrap())
                    .unwrap_or(0);
                if bytes.len() >= header_end + 4 + content_length {
                    break;
                }
            }
        }
        sender.send(bytes).unwrap();
        stream.write_all(response.as_bytes()).unwrap();
        drop(stream);
        listener.set_nonblocking(true).unwrap();
        thread::sleep(Duration::from_millis(180));
        let mut requests = 1;
        while listener.accept().is_ok() {
            requests += 1;
        }
        requests
    });
    (origin, receiver, task)
}
const OK: &str = "HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}";
const BODY: &[u8] = br#"{ "confirm":true, "input":{"idempotencyKey":"fixed-key","request":{"debit":"9007199254740993.01"}} }"#;

#[tokio::test]
async fn authenticated_report_has_exact_origin_host_and_encoded_query() {
    let (origin, request, task) = server(OK);
    let api = ApiClient::new(&origin, true, Some("session=secret".into())).unwrap();
    assert_eq!(
        api.report(
            "demo",
            Report::TrialBalance,
            Some("2026"),
            Some("1000 &x=2")
        )
        .await
        .unwrap()["ok"],
        true
    );
    let wire = String::from_utf8(request.recv().unwrap()).unwrap();
    let lower = wire.to_lowercase();
    assert!(wire.starts_with(
        "GET /api/companies/demo/trial-balance?year=2026&account=1000+%26x%3D2 HTTP/1.1"
    ));
    assert!(lower.contains(&format!(
        "\r\nhost: {}\r\n",
        origin.strip_prefix("http://").unwrap()
    )));
    assert!(lower.contains(&format!("\r\norigin: {origin}\r\n")));
    assert!(lower.contains("\r\ncookie: session=secret\r\n"));
    assert_eq!(task.join().unwrap(), 1);
}
#[tokio::test]
async fn writes_send_exact_bytes_and_do_not_retry_uncertain_response() {
    let (origin, request, task) = server(
        "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    let api = ApiClient::new(&origin, true, Some("session=secret".into())).unwrap();
    assert_eq!(
        api.manual_journal("demo", JournalMutation::Post, BODY.to_vec())
            .await,
        Err(Error::OutcomeUnknown)
    );
    let wire = request.recv().unwrap();
    assert!(wire.starts_with(b"POST /api/companies/demo/manual-journal HTTP/1.1"));
    assert!(wire.ends_with(BODY));
    assert_eq!(task.join().unwrap(), 1);
}
#[tokio::test]
async fn reversal_and_explicit_http_refusal() {
    let (origin, request, task) = server(
        "HTTP/1.1 422 Unprocessable Entity\r\nContent-Length: 6\r\nConnection: close\r\n\r\nsecret",
    );
    let api = ApiClient::new(&origin, true, None).unwrap();
    let error = api
        .manual_journal("demo", JournalMutation::Reverse, BODY.to_vec())
        .await
        .unwrap_err();
    assert_eq!(error, Error::Refused { status: 422 });
    assert!(!format!("{error:?} {error}").contains("secret"));
    assert!(
        request
            .recv()
            .unwrap()
            .starts_with(b"POST /api/companies/demo/manual-journal-reversal ")
    );
    assert_eq!(task.join().unwrap(), 1);
}
#[tokio::test]
async fn redirects_never_follow_or_forward_credentials() {
    let (origin, request, task) = server(
        "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/stolen\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    let api = ApiClient::new(&origin, true, Some("session=secret".into())).unwrap();
    assert_eq!(api.me().await, Err(Error::Refused { status: 302 }));
    request.recv().unwrap();
    assert_eq!(task.join().unwrap(), 1);
}
#[tokio::test]
async fn truncated_successful_write_is_unknown() {
    let (origin, request, task) =
        server("HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n{}");
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.manual_journal("demo", JournalMutation::Post, BODY.to_vec())
            .await,
        Err(Error::OutcomeUnknown)
    );
    request.recv().unwrap();
    assert_eq!(task.join().unwrap(), 1);
}
#[test]
fn fail_closed_configuration_and_secret_redaction() {
    for origin in [
        "http://example.com",
        "http://localhost:8080",
        "https://u:password@example.com",
        "https://example.com/path",
        "https://example.com?token=secret",
        "https://example.com#secret",
        "file:///tmp/x",
    ] {
        let error = ApiClient::new(origin, true, None).err().unwrap();
        assert!(!format!("{error:?} {error}").contains("password"));
        assert!(!format!("{error:?} {error}").contains("secret"));
    }
    assert!(ApiClient::new("http://127.0.0.1:8080", false, None).is_err());
    assert!(ApiClient::new("http://[::1]:8080", true, None).is_ok());
    assert!(ApiClient::new("https://example.com", false, Some("secret\r\nx: y".into())).is_err());
}
#[tokio::test]
async fn mutation_requires_key_confirmation_and_safe_company_before_network() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    assert!(matches!(
        api.manual_journal("demo", JournalMutation::Post, b"{}".to_vec())
            .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.manual_journal("../elsewhere", JournalMutation::Post, BODY.to_vec())
            .await,
        Err(Error::InvalidInput(_))
    ));
}

#[tokio::test]
async fn domain_mutations_preserve_flat_native_bytes_and_methods() {
    use rentemester_client::{Confirmation, DomainMutation as D};
    for(operation,method,path,body)in[
 (D::CreateCustomer,"POST","customers",br#"{ "name":"Synthetic client", "email":null }"#.as_slice()),
 (D::UpdateCustomer(7),"PATCH","customers/7",br#"{ "email":null }"#.as_slice()),
 (D::CreateVendor,"POST","vendors",br#"{"name":"Synthetic vendor","countryCode":"DK","identifierKind":"dk_cvr","vatOrCvr":"DK12345678"}"#.as_slice()),
 (D::UpdateVendor(8),"PATCH","vendors/8",br#"{"phone":"1234"}"#.as_slice()),
 (D::IssueInvoice,"POST","invoices/issue",br#"{ "issueDate":"2026-10-19","lines":[{"description":"Synthetic exact decimal","quantity":3,"unitPriceExVat":0.335}] }"#.as_slice()),
 (D::PostInvoice,"POST","invoices/post",br#"{"invoiceDocumentId":9,"revenueAccountNo":"S","confirm":true}"#.as_slice())]{
 let(origin,request,task)=server(OK);let api=ApiClient::new(&origin,true,Some("session=secret".into())).unwrap();assert_eq!(api.domain_mutation("demo",operation,body.to_vec(),Confirmation{confirm:true,invoice_preview_reviewed:true}).await.unwrap()["ok"],true);let wire=request.recv().unwrap();assert!(wire.starts_with(format!("{method} /api/companies/demo/{path} HTTP/1.1").as_bytes()));assert!(wire.ends_with(body));assert!(!String::from_utf8_lossy(&wire).contains("idempotency"));assert_eq!(task.join().unwrap(),1);
 }
}
#[tokio::test]
async fn native_preview_is_binary_nonmutating_and_does_not_claim_unknown_write() {
    let (origin, request, task) = server(
        "HTTP/1.1 200 OK\r\nContent-Type: application/pdf\r\nContent-Length: 9\r\nConnection: close\r\n\r\n%PDF-1.7\n",
    );
    let api = ApiClient::new(&origin, true, None).unwrap();
    let body=br#"{"issueDate":"2026-10-19","lines":[{"description":"Synthetic","quantity":1,"unitPriceExVat":10}]}"#;
    let result = api.invoice_preview("demo", body.to_vec()).await.unwrap();
    assert_eq!(result["preview"]["contentType"], "application/pdf");
    assert_eq!(result["preview"]["bytes"][0], 37);
    assert!(request.recv().unwrap().ends_with(body));
    task.join().unwrap();
    let (origin, request, task) = server(
        "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.invoice_preview("demo", body.to_vec()).await,
        Err(Error::Refused { status: 503 })
    );
    request.recv().unwrap();
    assert_eq!(task.join().unwrap(), 1);
}
#[tokio::test]
async fn domain_confirmation_and_unknown_fields_fail_before_network() {
    use rentemester_client::{Confirmation, DomainMutation as D};
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    let confirmed = Confirmation {
        confirm: true,
        invoice_preview_reviewed: false,
    };
    for(operation,body,confirmation)in[(D::CreateCustomer,br#"{"name":"Synthetic","idempotencyKey":"invented"}"#.as_slice(),confirmed),(D::CreateCustomer,br#"{"name":"Synthetic"}"#.as_slice(),Confirmation{confirm:false,invoice_preview_reviewed:false}),(D::IssueInvoice,br#"{"issueDate":"2026-10-19","lines":[{"description":"Synthetic","quantity":1,"unitPriceExVat":10}]}"#.as_slice(),confirmed),(D::PostInvoice,br#"{"invoiceDocumentId":9,"revenueAccountNo":"S","confirm":false}"#.as_slice(),confirmed)]{assert!(matches!(api.domain_mutation("demo",operation,body.to_vec(),confirmation).await,Err(Error::InvalidInput(_))));}
}
#[tokio::test]
async fn domain_uncertainty_is_never_retried() {
    use rentemester_client::{Confirmation, DomainMutation as D};
    let (origin, request, task) = server(
        "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.domain_mutation(
            "demo",
            D::CreateCustomer,
            br#"{"name":"Synthetic"}"#.to_vec(),
            Confirmation {
                confirm: true,
                invoice_preview_reviewed: false
            }
        )
        .await,
        Err(Error::OutcomeUnknown)
    );
    request.recv().unwrap();
    assert_eq!(task.join().unwrap(), 1);
}
#[tokio::test]
async fn domain_reads_target_existing_discovery_routes() {
    for resource in ["accounts", "contacts", "documents", "invoices"] {
        let (origin, request, task) = server(OK);
        let api = ApiClient::new(&origin, true, None).unwrap();
        match resource {
            "accounts" => api.accounts("demo").await.unwrap(),
            "contacts" => api.contacts("demo").await.unwrap(),
            "documents" => api.documents("demo").await.unwrap(),
            _ => api.invoices("demo", Some("2026")).await.unwrap(),
        };
        let query = if resource == "invoices" {
            "?year=2026"
        } else {
            ""
        };
        assert!(
            request.recv().unwrap().starts_with(
                format!("GET /api/companies/demo/{resource}{query} HTTP/1.1").as_bytes()
            )
        );
        task.join().unwrap();
    }
}
#[tokio::test]
async fn bank_payable_writes_keep_exact_native_contracts() {
    use rentemester_client::BankMutation as B;
    for(operation,path,body)in[
 (B::CreateAccount,"bank-accounts",br#"{ "name":"Synthetic client bank", "slug":"synthetic-clients", "currency":"DKK", "ledgerAccountNo":"1800" }"#.as_slice()),
 (B::ImportStatement,"bank/import",br#"{ "csvContent":"date;text;amount;currency\n2026-10-19;Synthetic payment;-125;DKK\n", "account":"synthetic-clients", "confirm":true }"#.as_slice()),
 (B::RegisterPayable,"payables",br#"{ "documentId":7,"billDate":"2026-10-19","dueDate":"2026-10-19","expenseAccountNo":"E","vatTreatment":"standard","confirm":true }"#.as_slice()),
 (B::PayPayable(9),"payables/9/pay",br#"{ "bankTransactionId":8,"amount":125,"paymentAccountNo":"1800","confirm":true }"#.as_slice()),
 (B::MatchExpense,"documents/book-expense",br#"{ "documentId":7,"bankTransactionId":8,"expenseAccountNo":"E","vatTreatment":"exempt","confirm":true }"#.as_slice())]{let(origin,request,task)=server(OK);let api=ApiClient::new(&origin,true,Some("session=synthetic".into())).unwrap();assert_eq!(api.bank_mutation("demo",operation,body.to_vec(),true).await.unwrap()["ok"],true);let wire=request.recv().unwrap();assert!(wire.starts_with(format!("POST /api/companies/demo/{path} HTTP/1.1").as_bytes()));assert!(wire.ends_with(body));assert_eq!(task.join().unwrap(),1);}
}
#[tokio::test]
async fn bank_reads_use_actual_filter_and_original_options_routes() {
    for path in [
        "bank?year=2026",
        "bank-accounts",
        "payables?status=all&asOf=2026-10-19",
        "documents/7/booking-options",
    ] {
        let (origin, request, task) = server(OK);
        let api = ApiClient::new(&origin, true, None).unwrap();
        if path.starts_with("bank?") {
            api.bank("demo", Some("2026")).await.unwrap();
        } else if path == "bank-accounts" {
            api.bank_accounts("demo").await.unwrap();
        } else if path.starts_with("payables") {
            api.payables("demo", Some("all"), Some("2026-10-19"))
                .await
                .unwrap();
        } else {
            api.expense_booking_options("demo", 7).await.unwrap();
        }
        assert!(
            request
                .recv()
                .unwrap()
                .starts_with(format!("GET /api/companies/demo/{path} HTTP/1.1").as_bytes())
        );
        task.join().unwrap();
    }
}
#[tokio::test]
async fn bank_and_bill_confirmation_shape_and_target_fail_before_network() {
    use rentemester_client::BankMutation as B;
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for(operation,bytes,confirm)in[(B::CreateAccount,br#"{"name":"Synthetic"}"#.as_slice(),false),(B::ImportStatement,br#"{"csvContent":"x","confirm":true,"profile":"named-unsupported"}"#.as_slice(),true),(B::PayPayable(0),br#"{"bankTransactionId":1,"confirm":true}"#.as_slice(),true),(B::RegisterPayable,br#"{"documentId":1,"billDate":"2026-10-19","dueDate":"2026-10-19","expenseAccountNo":"E","confirm":false}"#.as_slice(),true),(B::MatchExpense,br#"{"documentId":1,"bankTransactionId":2,"expenseAccountNo":"E","vatTreatment":"invented","confirm":true}"#.as_slice(),true)]{assert!(matches!(api.bank_mutation("demo",operation,bytes.to_vec(),confirm).await,Err(Error::InvalidInput(_))));}
    assert!(matches!(
        api.payables("demo", Some("invented"), None).await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.expense_booking_options("demo", 0).await,
        Err(Error::InvalidInput(_))
    ));
}
#[tokio::test]
async fn uncertain_payable_payment_is_attempted_once() {
    use rentemester_client::BankMutation as B;
    let (origin, request, task) = server(
        "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.bank_mutation(
            "demo",
            B::PayPayable(3),
            br#"{"bankTransactionId":7,"confirm":true}"#.to_vec(),
            true
        )
        .await,
        Err(Error::OutcomeUnknown)
    );
    request.recv().unwrap();
    assert_eq!(task.join().unwrap(), 1);
}

#[tokio::test]
async fn period_lifecycle_preserves_exact_review_proof_and_single_attempt() {
    use rentemester_client::PeriodMutation as P;
    for(op,path,bytes) in [
 (P::Review,"close-review",br#"{ "periodStart":"2025-01-01","periodEnd":"2025-03-31","confirm":true }"#.as_slice()),
 (P::Close,"close",br#"{"periodStart":"2025-01-01","periodEnd":"2025-03-31","reviewId":7,"packetHash":"original-proof","confirm":true}"#.as_slice()),
 (P::Reopen,"reopen",br#"{"periodStart":"2025-01-01","periodEnd":"2025-03-31","reason":"Additional original evidence","confirm":true}"#.as_slice()),
 (P::Report,"report",br#"{"periodStart":"2025-01-01","periodEnd":"2025-03-31","reference":"Synthetic receipt","packetHash":"original-close-proof","confirm":true}"#.as_slice())]{
 let(origin,request,task)=server(OK);let api=ApiClient::new(&origin,true,Some("session=synthetic".into())).unwrap();
 api.period_mutation("demo",op,bytes.to_vec(),true).await.unwrap();let wire=request.recv().unwrap();assert!(wire.starts_with(format!("POST /api/companies/demo/periods/{path} HTTP/1.1").as_bytes()));assert!(wire.ends_with(bytes));assert_eq!(task.join().unwrap(),1);
 }
}
#[tokio::test]
async fn period_reads_and_invalid_commit_contracts() {
    use rentemester_client::PeriodMutation as P;
    for path in [
        "vat?year=2025",
        "periods",
        "periods/close-readiness?from=2025-01-01&to=2025-03-31",
    ] {
        let (origin, request, task) = server(OK);
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        match path {
            "periods" => {
                api.periods("demo").await.unwrap();
            }
            "vat?year=2025" => {
                api.vat("demo", Some("2025")).await.unwrap();
            }
            _ => {
                api.period_close_readiness("demo", "2025-01-01", "2025-03-31")
                    .await
                    .unwrap();
            }
        }
        assert!(
            request
                .recv()
                .unwrap()
                .starts_with(format!("GET /api/companies/demo/{path} HTTP/1.1").as_bytes())
        );
        assert_eq!(task.join().unwrap(), 1);
    }
    let api = ApiClient::new("http://127.0.0.1:9", true, Some("s=x".into())).unwrap();
    for (op, body, confirm) in [
        (
            P::Review,
            r#"{"periodStart":"2025-01-01","periodEnd":"2025-03-31","confirm":true}"#,
            false,
        ),
        (
            P::Close,
            r#"{"periodStart":"2025-01-01","periodEnd":"2025-03-31","confirm":true}"#,
            true,
        ),
        (
            P::Close,
            r#"{"periodStart":"2025-01-01","periodEnd":"2025-03-31","reviewId":1,"packetHash":"x","force":true,"confirm":true}"#,
            true,
        ),
        (
            P::Report,
            r#"{"periodStart":"2025-01-01","periodEnd":"2025-03-31","reference":"x","packetHash":"x","confirm":true,"idempotencyKey":"ignored"}"#,
            true,
        ),
    ] {
        assert!(matches!(
            api.period_mutation("demo", op, body.as_bytes().to_vec(), confirm)
                .await,
            Err(Error::InvalidInput(_))
        ));
    }
}
#[tokio::test]
async fn uncertain_period_close_never_retries() {
    use rentemester_client::PeriodMutation;
    let (origin, request, task) = server(
        "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    );
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(api.period_mutation("demo",PeriodMutation::Close,br#"{"periodStart":"2025-01-01","periodEnd":"2025-03-31","reviewId":9,"packetHash":"reviewed-proof","confirm":true}"#.to_vec(),true).await,Err(Error::OutcomeUnknown));
    request.recv().unwrap();
    assert_eq!(task.join().unwrap(), 1);
}
