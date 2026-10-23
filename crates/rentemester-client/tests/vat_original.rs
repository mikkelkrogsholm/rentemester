use rentemester_client::{ApiClient, Error, validate_original_vat_result};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};
fn server(status: u16, value: Value) -> (ApiClient, thread::JoinHandle<(String, Value)>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let api = ApiClient::new(
        &format!("http://{}", listener.local_addr().unwrap()),
        true,
        None,
    )
    .unwrap();
    let t = thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(4)))
            .unwrap();
        let mut bytes = vec![];
        let end = loop {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let header = String::from_utf8_lossy(&bytes[..end]).to_string();
        let len: usize = header
            .lines()
            .find_map(|l| {
                l.to_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse().unwrap())
            })
            .unwrap_or(0);
        while bytes.len() < end + len {
            let mut b = [0; 1024];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
        }
        let body = if len == 0 {
            Value::Null
        } else {
            serde_json::from_slice(&bytes[end..end + len]).unwrap()
        };
        let text = value.to_string();
        write!(
            s,
            "HTTP/1.1 {status} Mock\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        )
        .unwrap();
        (header, body)
    });
    (api, t)
}

fn report() -> Value {
    let mut v = json!({"ok":true,"errors":[],"warnings":[],"appliedRules":["DK-VAT-REPORT-001"],"periodStart":"2026-01-01","periodEnd":"2026-03-31","vatPeriodType":"quarterly","filingDeadline":"2026-06-01","outputVat":300,"inputVat":50,"netVatPayable":250,"ossConsumerSalesEntryCount":1,"journalEntryCount":2,"reversedJournalEntryCount":1,"reversalJournalEntryCount":1,"totalJournalEntryCount":4,"linesConsidered":6,"reversedLinesConsidered":2,"reversalLinesConsidered":2,"totalLinesConsidered":10,"rubrikker":{"salgsmoms":250,"momsAfVarekobUdland":25,"momsAfYdelseskobUdland":25,"kobsmoms":50,"momstilsvar":250,"rubrikA":200,"rubrikB":200,"rubrikC":90}});
    for (k, n) in [
        ("purchaseBase25", 200),
        ("salesBase25", 1000),
        ("reverseChargeSalesBase", 250),
        ("foreignReverseChargeSalesBase", 200),
        ("domesticReverseChargeSalesBase", 50),
        ("reverseChargePurchaseBase", 100),
        ("euGoodsAcquisitionPurchaseBase", 100),
        ("euGoodsAcquisitionOutputVat", 25),
        ("nonEuServiceReverseChargePurchaseBase", 0),
        ("reverseChargePurchaseOutputVat", 50),
        ("representationPurchaseBase", 40),
        ("badDebtReliefBase25", 0),
        ("exemptSalesBase", 40),
        ("ossConsumerSalesBase", 120),
    ] {
        v[k] = json!(n);
    }
    v
}

#[tokio::test]
async fn four_nonempty_routes_and_preserved_domain_refusals() {
    let r = report();
    let eu = json!({"ok":true,"errors":[],"warnings":[],"appliedRules":["DK-VAT-EU-SALES-LIST-001"],"periodStart":"2026-01-01","periodEnd":"2026-03-31","customers":[{"vatNumber":"DE123456789","countryCode":"DE","customerName":"Synthetic customer","invoiceCount":1,"totalValue":200,"invoiceNumbers":["S-1"],"creditNoteCount":0,"creditNoteNumbers":[]}],"totalValue":200,"invoiceCount":1,"creditNoteCount":0,"sources":[]});
    let oss = json!({"ok":true,"errors":[],"warnings":["preview"],"appliedRules":["DK-VAT-OSS-001"],"periodStart":"2026-01-01","periodEnd":"2026-03-31","ossConsumerSalesBase":120,"entryCount":1,"submission":false,"vatReport":r});
    let filing = json!({"ok":true,"errors":[],"warnings":[],"appliedRules":["DK-VAT-FILING-001"],"periodStart":"2026-01-01","periodEnd":"2026-03-31","vatPeriodType":"quarterly","periodStatus":"closed","periodReference":"Q1","filingDeadline":"2026-06-01","rubrikker":r["rubrikker"],"vatReport":r});
    for (op, v) in [
        ("report", r),
        ("eu-sales-list", eu),
        ("oss-report", oss),
        ("filing", filing),
    ] {
        assert!(validate_original_vat_result(op, &v, true), "{op}");
        let (api, t) = server(200, v.clone());
        let got = match op {
            "report" => {
                api.original_vat_report("synthetic", "2026-01-01", "2026-03-31")
                    .await
            }
            "eu-sales-list" => {
                api.original_vat_eu_sales_list("synthetic", "2026-01-01", "2026-03-31")
                    .await
            }
            "oss-report" => {
                api.original_vat_oss_report("synthetic", "2026-01-01", "2026-03-31")
                    .await
            }
            _ => {
                api.original_vat_filing("synthetic", "2026-01-01", "2026-03-31")
                    .await
            }
        }
        .unwrap();
        assert_eq!(got, v);
        let (h, b) = t.join().unwrap();
        assert!(h.starts_with(&format!(
            "GET /api/companies/synthetic/original/vat/{op}?from=2026-01-01&to=2026-03-31 "
        )));
        assert!(b.is_null());
        let mut bad = v.clone();
        if op == "report" {
            bad["totalLinesConsidered"] = json!(11);
        } else if op == "eu-sales-list" {
            bad["totalValue"] = json!(201);
        } else {
            bad["vatReport"]["netVatPayable"] = json!(251);
        }
        assert!(!validate_original_vat_result(op, &bad, true));
    }
    for errors in [
        json!(["selskabet er ikke momsregistreret"]),
        json!(["invalid date", "ordered dates required"]),
    ] {
        let v = json!({"ok":false,"errors":errors});
        let (api, t) = server(422, v.clone());
        assert_eq!(
            api.original_vat_report("synthetic", "malformed", "inverted")
                .await
                .unwrap(),
            v
        );
        assert!(t.join().unwrap().0.contains("from=malformed&to=inverted"));
    }
    let (api, t) = server(200, json!({"ok":true,"errors":[]}));
    assert_eq!(
        api.original_vat_report("synthetic", "2026-01-01", "2026-03-31")
            .await,
        Err(Error::Protocol)
    );
    t.join().unwrap();
}
#[test]
fn arithmetic_rubric_and_date_corruption_fail_closed() {
    for (k, v) in [
        ("outputVat", json!(301)),
        ("netVatPayable", json!(251)),
        ("totalJournalEntryCount", json!(9)),
        ("periodStart", json!("2026-02-30")),
        ("warnings", json!(null)),
        ("inputVat", json!("50")),
    ] {
        let mut r = report();
        r[k] = v;
        assert!(!validate_original_vat_result("report", &r, true), "{k}");
    }
    for k in [
        "salgsmoms",
        "kobsmoms",
        "momstilsvar",
        "rubrikA",
        "rubrikB",
        "rubrikC",
    ] {
        let mut r = report();
        r["rubrikker"][k] = json!(999);
        assert!(!validate_original_vat_result("report", &r, true));
    }
}
