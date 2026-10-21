use rentemester_client::{ApiClient, Error};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};
fn server(value: Value, status: u16) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = vec![];
        while !request.ends_with(b"\r\n\r\n") {
            let mut b = [0; 4096];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            request.extend_from_slice(&b[..n]);
        }
        let body = value.to_string();
        write!(s,"HTTP/1.1 {status} Test\r\nContent-Type:application/json\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{body}",body.len()).unwrap();
        String::from_utf8(request).unwrap()
    });
    (origin, task)
}
#[tokio::test]
async fn tax_preserves_exact_inner_values_review_and_annual_blockers() {
    for ok in [true, false] {
        let value = json!({"ok":ok,"errors":if ok{json!([])}else{json!(["year is open"])},"appliedRules":["DK-TAX-RETURN-CORP-001"],"companyForm":"unknown","taxableIncome":-25.05,"corporateTax":null,"needsReview":[{"kind":"tax_loss_carry_forward"}],"acontoTaxDeadlines":[{"installment":1,"dueDate":"2020-03-20"}]});
        let (origin, t) = server(json!({"ok":true,"annualExport":value}), 200);
        let api = ApiClient::new(&origin, true, Some("session=synthetic".into())).unwrap();
        assert_eq!(
            api.annual_export("synthetic", "2020-01-01", "2020-12-31", "tax")
                .await
                .unwrap(),
            value
        );
        let request = t.join().unwrap();
        assert!(request.starts_with("GET /api/companies/synthetic/annual-exports?fiscalYearStart=2020-01-01&fiscalYearEnd=2020-12-31&kind=tax HTTP/1.1"));
        assert!(request.to_lowercase().contains("cookie: session=synthetic"));
    }
}
#[tokio::test]
async fn xhtml_hash_and_same_snapshot_annual_report_are_verified_before_return() {
    let xhtml = "<?xml version=\"1.0\"?><html>Ærlig &amp; sikker</html>";
    let valid = json!({"ok":true,"errors":[],"xhtml":xhtml,"sha256":format!("{:x}",Sha256::digest(xhtml.as_bytes())),"annualReport":{"ok":true,"company":{"name":"Synthetic"}},"taxonomy":{"name":"bounded","version":"0.2.0"},"appliedRules":["DK-ANNUAL-REPORT-IXBRL-002"]});
    for tamper in [false, true] {
        let mut value = valid.clone();
        if tamper {
            value["xhtml"] = json!("tampered");
        }
        let (origin, t) = server(json!({"ok":true,"annualExport":value}), 200);
        let api = ApiClient::new(&origin, true, None).unwrap();
        let result = api
            .annual_export("synthetic", "2020-01-01", "2020-12-31", "ixbrl")
            .await;
        if tamper {
            assert_eq!(result, Err(Error::Protocol));
        } else {
            assert_eq!(result.unwrap(), valid);
        }
        assert!(t.join().unwrap().contains("kind=ixbrl HTTP/1.1"));
    }
}
#[tokio::test]
async fn invalid_kind_date_and_source_failures_are_explicit_not_zero_reports() {
    let api = ApiClient::new("http://127.0.0.1:9", true, None).unwrap();
    for (start, end, kind) in [
        ("2020-02-30", "2020-12-31", "tax"),
        ("2021-01-01", "2020-12-31", "tax"),
        ("2020-01-01", "2020-12-31", "submit"),
    ] {
        assert!(matches!(
            api.annual_export("synthetic", start, end, kind).await,
            Err(Error::InvalidInput(_))
        ));
    }
    let (origin, t) = server(json!({"ok":false,"code":"access_denied"}), 403);
    let api = ApiClient::new(&origin, true, None).unwrap();
    assert_eq!(
        api.annual_export("synthetic", "2020-01-01", "2020-12-31", "tax")
            .await,
        Err(Error::Refused { status: 403 })
    );
    t.join().unwrap();
}
