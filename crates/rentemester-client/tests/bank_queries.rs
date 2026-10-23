mod common;
use common::TcpListener;
use rentemester_client::{ApiClient, BankQuery, Error};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    thread,
};
fn server(status: &'static str, value: Value) -> (ApiClient, thread::JoinHandle<String>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = vec![];
        while !request.ends_with(b"\r\n\r\n") {
            let mut b = [0; 4096];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            request.extend(&b[..n]);
        }
        let body = value.to_string();
        write!(
            s,
            "HTTP/1.1 {status}\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        String::from_utf8(request).unwrap()
    });
    (
        ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap(),
        t,
    )
}
#[tokio::test]
async fn canonical_original_units_nulls_filters_and_page_modes_are_forwarded_without_math() {
    let row = json!({"id":7,"transactionDate":"2020-01-02","bookingDate":null,"text":"Synthetic fx","amount":-12.34,"currency":"EUR","reference":null,"importBatchId":3,"bankAccountId":2,"ledgerStatus":"unposted","reconciliationStatus":"unmatched","journalEntryId":null,"journalEntryNo":null});
    let response = json!({"ok":true,"data":{"rows":[row],"count":1},"errors":[]});
    let (api, t) = server("200 OK", response.clone());
    let query = BankQuery {
        status: Some("unmatched".into()),
        from: Some("2020-01-01".into()),
        to: Some("2027-12-31".into()),
        text_match: Some("A & B".into()),
        amount: Some("-12.345".into()),
        account: Some(" 2 ".into()),
        ..Default::default()
    };
    assert_eq!(
        api.bank_transactions("synthetic", &query).await.unwrap(),
        response
    );
    let request = t.join().unwrap();
    assert!(request.starts_with("GET /api/companies/synthetic/bank/transactions?status=unmatched&from=2020-01-01&to=2027-12-31&textMatch=A+%26+B&amount=-12.345&account=+2+ HTTP/1.1"));
    assert!(!request.contains("limit="));
    assert!(!request.contains("year="));
    assert!(request.contains("cookie: session=synthetic"));
    let response = json!({"ok":true,"data":{"rows":[],"count":0,"total":1,"limit":500,"offset":900,"hasMore":false},"errors":[]});
    let (api, t) = server("200 OK", response.clone());
    let query = BankQuery {
        limit: Some(500),
        offset: Some(900),
        ..Default::default()
    };
    assert_eq!(
        api.bank_transactions("synthetic", &query).await.unwrap(),
        response
    );
    assert!(t.join().unwrap().contains("?limit=500&offset=900 HTTP/1.1"));
}
#[tokio::test]
async fn report_accounts_and_business_refusals_preserve_exact_server_envelopes() {
    let report = json!({"ok":true,"data":{"periodStart":"2020-01-01","periodEnd":"2027-12-31","matchedCount":1,"unmatchedCount":0,"matchedAmountTotal":-17.35,"unmatchedAmountTotal":0,"matched":[],"unmatched":[]},"errors":[],"appliedRules":["DK-BOOKKEEPING-RECONCILIATION-001"]});
    let (api, t) = server("200 OK", report.clone());
    let query = BankQuery {
        from: Some("2020-01-01".into()),
        to: Some("2027-12-31".into()),
        ..Default::default()
    };
    assert_eq!(
        api.bank_reconciliation("synthetic", &query).await.unwrap(),
        report
    );
    assert!(
        t.join()
            .unwrap()
            .contains("/bank/reconciliation?from=2020-01-01&to=2027-12-31 ")
    );
    let accounts = json!({"ok":true,"data":{"count":1,"accounts":[{"id":2,"slug":"fx","currency":"EUR","active":false,"ledgerAccountNo":null}]},"errors":[]});
    let (api, t) = server("200 OK", accounts.clone());
    assert_eq!(
        api.bank_account_list("synthetic", Some(false))
            .await
            .unwrap(),
        accounts
    );
    assert!(
        t.join()
            .unwrap()
            .contains("/bank/accounts?includeInactive=false ")
    );
    for query in [
        BankQuery {
            account: Some("unknown".into()),
            ..Default::default()
        },
        BankQuery {
            from: Some("2026-02-30".into()),
            status: Some("invalid".into()),
            ..Default::default()
        },
    ] {
        let refusal = json!({"ok":false,"code":"NOT_FOUND","errors":["bank account 'unknown' does not exist"]});
        let (api, t) = server("200 OK", refusal.clone());
        assert_eq!(
            api.bank_transactions("synthetic", &query).await.unwrap(),
            refusal
        );
        t.join().unwrap();
    }
}
#[tokio::test]
async fn structural_errors_auth_and_invalid_success_do_not_become_empty_data() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for query in [
        BankQuery {
            limit: Some(0),
            ..Default::default()
        },
        BankQuery {
            offset: Some(9007199254740992),
            ..Default::default()
        },
        BankQuery {
            amount: Some("NaN".into()),
            ..Default::default()
        },
    ] {
        assert!(matches!(
            api.bank_transactions("synthetic", &query).await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.bank_reconciliation("synthetic", &BankQuery::default())
            .await,
        Err(Error::InvalidInput(_))
    ));
    let (api, t) = server("403 Forbidden", json!({"secret":"never expose"}));
    assert_eq!(
        api.bank_account_list("synthetic", None).await.unwrap_err(),
        Error::Refused { status: 403 }
    );
    t.join().unwrap();
    for value in [
        json!({"ok":true,"data":{},"errors":[]}),
        json!({"ok":false,"data":{"rows":[]},"errors":["refused"]}),
    ] {
        let (api, t) = server("200 OK", value);
        assert_eq!(
            api.bank_transactions("synthetic", &BankQuery::default())
                .await
                .unwrap_err(),
            Error::Protocol
        );
        t.join().unwrap();
    }
}
