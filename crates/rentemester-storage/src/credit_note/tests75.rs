use super::tests::{fixture, fixture_with};
use super::*;
fn issue_amount(
    tx: &Transaction<'_>,
    amount: Value,
    date: &str,
    mut change: impl FnMut(&mut Prepared),
) -> Result<Value, Error> {
    let input:Input=serde_json::from_value(json!({"invoiceDocumentId":1,"issueDate":date,"reason":"Synthetic correction","grossAmount":amount,"confirm":true})).unwrap();
    let mut prepared = prepare(tx, &input, "2026-10-20T12:00:00Z")?;
    change(&mut prepared);
    register_and_post(
        tx,
        prepared,
        Artifact {
            filename: "synthetic-credit.json",
            path: "synthetic-credit.json",
            hash: &"c".repeat(64),
            mime: "application/json",
        },
        "synthetic",
        Date::parse("2026-10-31")?,
    )
}
#[test]
fn mixed_repeated_bases_partial_and_final_credit_keep_each_counter_and_original() {
    let mut db = fixture_with(
        json!({"lines":[
            {"description":"Taxable A","quantity":1,"unitPriceExVat":0.03,"taxClassification":"taxable","vatRate":0.25},
            {"description":"Taxable B","quantity":1,"unitPriceExVat":0.03,"taxClassification":"taxable","vatRate":0.25},
            {"description":"Exempt","quantity":1,"unitPriceExVat":0.03,"taxClassification":"exempt"}
        ]}),
        false,
    );
    let tx = db.transaction().unwrap();
    let original: String = tx
        .query_row("SELECT payload_json FROM documents WHERE id=1", [], |r| {
            r.get(0)
        })
        .unwrap();
    issue_amount(&tx, json!(0.04), "2026-10-18", |_| {}).unwrap();
    issue_amount(&tx, Value::Null, "2026-10-18", |_| {}).unwrap();
    let mut q = tx
        .prepare("SELECT payload_json FROM documents WHERE document_type='credit_note' ORDER BY id")
        .unwrap();
    let notes: Vec<Value> = q
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(|r| serde_json::from_str(&r.unwrap()).unwrap())
        .collect();
    let first = notes[0]["creditAllocations"].as_array().unwrap();
    assert_eq!(first.len(), 4); // two taxable counters remain separate, exempt, VAT
    assert_ne!(first[0]["journalLineId"], first[1]["journalLineId"]);
    assert_eq!(first[0]["accountNo"], first[1]["accountNo"]);
    for (a, b) in first
        .iter()
        .zip(notes[1]["creditAllocations"].as_array().unwrap())
    {
        assert_eq!(a["journalLineId"], b["journalLineId"]);
        let mut total = Money::parse_canonical(a["allocatedAmount"].as_str().unwrap()).unwrap();
        total.add(&Money::parse_canonical(b["allocatedAmount"].as_str().unwrap()).unwrap());
        assert_eq!(total.decimal(), a["originalAmount"].as_str().unwrap());
    }
    assert_eq!(credited(&tx, 1).unwrap().decimal(), "0.11");
    assert!(issue_amount(&tx, json!(0.01), "2026-10-18", |_| {}).is_err());
    assert_eq!(
        tx.query_row::<String, _, _>("SELECT payload_json FROM documents WHERE id=1", [], |r| r
            .get(0))
            .unwrap(),
        original
    );
    let vat = crate::vat_report::read(&tx, "synthetic", "2026-10-18", None).unwrap();
    assert_eq!(vat["outputVat"], 0.0);
    assert_eq!(vat["vatReportErrors"], json!([]));
}
#[test]
fn exempt_and_domestic_reverse_credit_do_not_require_vat_counter() {
    for input in [
        json!({"lines":[{"description":"Exempt","quantity":1,"unitPriceExVat":100,"taxClassification":"exempt"}]}),
        json!({"vatTreatment":"domestic_reverse_charge","reverseChargeBasis":"DK_MOMSLOVEN_§46_STK_1_NR_6","reverseChargeNote":"Omvendt betalingspligt","lines":[{"description":"Reverse","quantity":1,"unitPriceExVat":100}]}),
    ] {
        let mut db = fixture_with(input, false);
        let tx = db.transaction().unwrap();
        issue_amount(&tx, json!(20), "2026-10-18", |_| {}).unwrap();
        issue_amount(&tx, Value::Null, "2026-10-18", |_| {}).unwrap();
        let vat = crate::vat_report::read(&tx, "synthetic", "2026-10-18", None).unwrap();
        assert_eq!(vat["outputVat"], 0.0);
        assert_eq!(vat["vatReportErrors"], json!([]));
        assert_eq!(credited(&tx, 1).unwrap().decimal(), "100.00");
    }
}
#[test]
fn prior_allocation_must_match_original_and_actual_posted_counters() {
    let mut db = fixture();
    let tx = db.transaction().unwrap();
    issue_amount(&tx, json!(250), "2026-10-18", |note| {
        note.payload["creditAllocations"][0]["allocatedAmount"] = json!("199.99")
    })
    .unwrap();
    assert!(
        issue_amount(&tx, Value::Null, "2026-10-18", |_| {})
            .unwrap_err()
            .message
            .contains("posted credit differs")
    );
}
#[test]
fn legacy_aggregate_invoice_remains_creditable_without_new_evidence_codec() {
    let mut db = fixture_with(
        json!({"lines":[{"description":"Legacy","quantity":1,"unitPriceExVat":100}]}),
        true,
    );
    let tx = db.transaction().unwrap();
    issue_amount(&tx, json!(25), "2026-10-18", |_| {}).unwrap();
    issue_amount(&tx, Value::Null, "2026-10-18", |_| {}).unwrap();
    let payload: String = tx
        .query_row(
            "SELECT payload_json FROM documents WHERE document_type='credit_note' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        serde_json::from_str::<Value>(&payload)
            .unwrap()
            .get("creditAllocations")
            .is_none()
    );
}
#[test]
fn dated_warning_excludes_future_credit_and_accounts_for_refunds() {
    let mut db = fixture();
    let tx = db.transaction().unwrap();
    let csv = "date;text;amount;currency;reference\n2026-10-19;Synthetic receipt;1250;DKK;received\n2026-10-21;Synthetic refund;-500;DKK;refund\n";
    let parsed = rentemester_engine::bank::parse(csv).unwrap();
    crate::bank::import(
        &tx,
        csv,
        &parsed,
        Some("synthetic"),
        "synthetic",
        "2026-10-21T12:00:00Z",
        "synthetic-original",
    )
    .unwrap();
    crate::invoice_settlement::settle(
        &tx,
        &serde_json::from_value(
            json!({"invoiceDocumentId":1,"bankTransactionReference":"received","confirm":true}),
        )
        .unwrap(),
        "synthetic",
        Date::parse("2026-10-31").unwrap(),
    )
    .unwrap();
    issue_amount(&tx, json!(500), "2026-10-20", |_| {}).unwrap();
    crate::invoice_settlement::refund(
        &tx,
        &serde_json::from_value(
            json!({"invoiceDocumentId":1,"bankTransactionReference":"refund","confirm":true}),
        )
        .unwrap(),
        "synthetic",
        Date::parse("2026-10-31").unwrap(),
    )
    .unwrap();
    let warning = |end: &str| {
        let packet = crate::periods::compute(&tx, "2026-10-01", end, Ok(vec![])).unwrap();
        packet["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["code"] == "RECEIVABLE_OUTSTANDING")
            .unwrap()
            .clone()
    };
    assert_eq!(warning("2026-10-18")["evidence"][0]["open_amount"], 1250.0);
    for date in ["2026-10-19", "2026-10-20", "2026-10-21"] {
        assert_eq!(warning(date)["count"], 0);
    }
}
#[test]
fn dated_warning_counts_full_credit_only_from_its_posting_date() {
    let mut db = fixture();
    let tx = db.transaction().unwrap();
    issue_amount(&tx, Value::Null, "2026-10-20", |_| {}).unwrap();
    for (date, count) in [("2026-10-18", 1), ("2026-10-20", 0)] {
        let packet = crate::periods::compute(&tx, "2026-10-01", date, Ok(vec![])).unwrap();
        let warning = packet["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["code"] == "RECEIVABLE_OUTSTANDING")
            .unwrap();
        assert_eq!(warning["count"], count);
    }
}
