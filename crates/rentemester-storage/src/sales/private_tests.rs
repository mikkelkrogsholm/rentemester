use super::*;
const NOW: &str = "2026-10-20T10:00:00Z";
fn fixture() -> rusqlite::Connection {
    let mut control = rusqlite::Connection::open_in_memory().unwrap();
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Private Seller").unwrap();
    db.execute_batch("UPDATE companies SET address='Synthetic Street 78',cvr='87654321',vat_period_type='quarter' WHERE id=1; INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('R78','Synthetic receivable','asset','debit'),('S78','Synthetic sale','income','credit'),('V78','Synthetic output VAT','vat','credit'); INSERT INTO account_role_mappings(role,account_no,version,confirmed_by) VALUES('debtors','R78',1,'synthetic'),('output_vat','V78',1,'synthetic');").unwrap();
    db
}
fn reduced(gross: f64) -> Value {
    json!({"issueDate":"2026-10-20","invoiceType":"simplified","lines":[{"description":"Synthetic inclusive sale"}],"totals":{"grossAmount":gross,"vatRate":0.25,"vatComputationBasis":"VAT_20_OF_GROSS"}})
}
fn prepare_value(tx: &Transaction<'_>, value: Value) -> Result<Value, Error> {
    prepare(tx, &serde_json::from_value(value).unwrap(), NOW, true)
}
fn register_value(tx: &Transaction<'_>, payload: &Value) -> Value {
    let raw = serde_json::to_string_pretty(payload).unwrap();
    let hash = crate::invoice_email::sha(raw.as_bytes());
    let pdf_hash = crate::invoice_email::sha(&rentemester_engine::invoice_pdf::build(payload));
    register(
        tx,
        payload,
        Artifact {
            filename: "synthetic.json",
            path: "synthetic.json",
            hash: &hash,
            mime: "application/json",
        },
        Artifact {
            filename: "synthetic.pdf",
            path: "synthetic.pdf",
            hash: &pdf_hash,
            mime: "application/pdf",
        },
        "synthetic",
    )
    .unwrap()
}
#[test]
fn full_private_buyer_needs_name_address_but_no_cvr_and_keeps_seller_gate() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let source = json!({"issueDate":"2026-10-20","buyer":{"name":"Synthetic Private Buyer","address":"Synthetic Buyer Street"},"lines":[{"description":"Synthetic sale","quantity":1,"unitPriceExVat":10}]});
    for buyer in [
        Value::Null,
        json!({"name":"Synthetic Private Buyer"}),
        json!({"name":"Synthetic Buyer","address":"Synthetic street","vatOrCvr":"invalid"}),
    ] {
        let mut bad = source.clone();
        bad["buyer"] = buyer;
        assert!(prepare_value(&tx, bad).is_err());
    }
    let value = prepare_value(&tx, source.clone()).unwrap();
    assert_eq!(value["invoiceNumber"], "2026-0001");
    assert_eq!(
        value["buyer"],
        json!({"name":"Synthetic Private Buyer","address":"Synthetic Buyer Street","publicRecipient":false})
    );
    assert_eq!(value["invoiceType"], "full");
    tx.execute("UPDATE companies SET cvr=NULL WHERE id=1", [])
        .unwrap();
    assert!(prepare_value(&tx, source).is_err());
}
#[test]
fn simplified_refusals_preserve_sequence_and_optional_buyer_has_no_fabricated_identity() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let source = reduced(100.07);
    let mut cases = vec![];
    let mut bad = source.clone();
    bad["totals"]["grossAmount"] = json!(3000.01);
    cases.push(bad);
    let mut bad = source.clone();
    bad["buyer"] = json!({"name":"Synthetic public","address":"Synthetic street","eanNumber":"5790000000000","publicRecipient":false});
    cases.push(bad);
    let mut bad = source.clone();
    bad["vatTreatment"] = json!("domestic_reverse_charge");
    cases.push(bad);
    let mut bad = source.clone();
    bad["lines"][0]["quantity"] = json!(1);
    cases.push(bad);
    let mut bad = source.clone();
    bad["totals"]["vatAmount"] = json!(20.02);
    cases.push(bad);
    for bad in cases {
        assert!(prepare_value(&tx, bad).is_err());
    }
    let payload = prepare_value(&tx, source).unwrap();
    assert_eq!(payload["invoiceNumber"], "2026-0001");
    assert!(payload.get("buyer").is_none());
    assert!(payload["lines"][0].get("quantity").is_none());
    let registered = register_value(&tx, &payload);
    assert_eq!(registered["invoiceType"], "simplified");
    assert_eq!(registered["netAmount"].as_f64(), Some(80.06));
    assert_eq!(registered["vatAmount"].as_f64(), Some(20.01));
    assert_eq!(registered["grossAmount"].as_f64(), Some(100.07));
    assert_eq!(
        prepare_value(&tx, reduced(3000.0)).unwrap()["invoiceNumber"],
        "2026-0002"
    );
}
#[test]
fn gross_credits_preserve_original_net_vat_cents_at_partial_and_final_boundary() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    for (gross, part, expected_net, expected_vat) in [
        (100.07, 40.03, "80.06", "20.01"),
        (0.03, 0.01, "0.02", "0.01"),
    ] {
        let payload = prepare_value(&tx, reduced(gross)).unwrap();
        let reg = register_value(&tx, &payload);
        let id = reg["documentId"].as_i64().unwrap();
        post(
            &tx,
            &Post {
                invoice_document_id: id,
                revenue_account_no: "S78".into(),
                confirm: true,
            },
            "synthetic",
            Date::parse("2026-10-20").unwrap(),
        )
        .unwrap();
        let report = crate::vat_report::range(&tx, "2026-10-01", "2026-12-31").unwrap();
        assert!(report["errors"].as_array().unwrap().is_empty(), "{report}");
        assert_eq!(
            report["outputVat"].as_f64(),
            Some(expected_vat.parse().unwrap())
        );
        let before: String = tx
            .query_row(
                "SELECT payload_json FROM documents WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        for amount in [Some(part), None] {
            let mut request = json!({"invoiceDocumentId":id,"issueDate":"2026-10-20","reason":"Synthetic correction","confirm":true});
            if let Some(amount) = amount {
                request["grossAmount"] = json!(amount);
            }
            let request = serde_json::from_value(request).unwrap();
            let credit = crate::credit_note::prepare(&tx, &request, NOW).unwrap();
            let hash = crate::invoice_email::sha(
                serde_json::to_string(&credit.payload).unwrap().as_bytes(),
            );
            crate::credit_note::register_and_post(
                &tx,
                credit,
                Artifact {
                    filename: "credit.json",
                    path: "credit.json",
                    hash: &hash,
                    mime: "application/json",
                },
                "synthetic",
                Date::parse("2026-10-20").unwrap(),
            )
            .unwrap();
        }
        let report = crate::vat_report::range(&tx, "2026-10-01", "2026-12-31").unwrap();
        assert!(report["errors"].as_array().unwrap().is_empty(), "{report}");
        assert_eq!(report["outputVat"].as_f64(), Some(0.0));
        assert_eq!(
            crate::credit_note::credited(&tx, id).unwrap(),
            Money::parse(&gross.to_string()).unwrap()
        );
        let totals:(f64,f64)=tx.query_row("SELECT coalesce(sum(CASE WHEN a.account_no='S78' THEN jl.debit_amount ELSE 0 END),0),coalesce(sum(CASE WHEN a.account_no='V78' THEN jl.debit_amount ELSE 0 END),0) FROM credit_note_postings p JOIN journal_lines jl ON jl.journal_entry_id=p.journal_entry_id JOIN accounts a ON a.id=jl.account_id WHERE p.original_invoice_document_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(
            Money::parse(&totals.0.to_string()).unwrap().decimal(),
            expected_net
        );
        assert_eq!(
            Money::parse(&totals.1.to_string()).unwrap().decimal(),
            expected_vat
        );
        let after: String = tx
            .query_row(
                "SELECT payload_json FROM documents WHERE id=?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(before, after);
    }
}
#[test]
fn private_identity_erasure_fences_direct_and_saved_customer_inputs_before_numbering() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("INSERT INTO customers(id,name,address)VALUES(78,'Synthetic Erased Buyer','Synthetic private address');INSERT INTO gdpr_erasures(subject_key,source,source_row_id,redacted_fields,rule_id,reason)VALUES('Synthetic Erased Buyer','customers',78,'[\"name\"]','synthetic','legacy erasure fixture');").unwrap();
    let direct = json!({"issueDate":"2026-10-20","buyer":{"name":"Synthetic Erased Buyer","address":"Synthetic private address"},"lines":[{"description":"Synthetic private sale","quantity":1,"unitPriceExVat":10}]});
    assert!(prepare_value(&tx, direct).is_err());
    let mut contact = reduced(100.07);
    contact["customerId"] = json!(78);
    assert!(prepare_value(&tx, contact).is_err());
    assert_eq!(
        prepare_value(&tx, reduced(100.07)).unwrap()["invoiceNumber"],
        "2026-0001"
    );
}

#[test]
fn simplified_vat_report_refuses_corrupted_credit_allocation_without_guessing() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let payload = prepare_value(&tx, reduced(100.07)).unwrap();
    let id = register_value(&tx, &payload)["documentId"]
        .as_i64()
        .unwrap();
    post(
        &tx,
        &Post {
            invoice_document_id: id,
            revenue_account_no: "S78".into(),
            confirm: true,
        },
        "synthetic",
        Date::parse("2026-10-20").unwrap(),
    )
    .unwrap();
    let request=serde_json::from_value(json!({"invoiceDocumentId":id,"issueDate":"2026-10-20","reason":"Synthetic corruption fixture","grossAmount":40.03,"confirm":true})).unwrap();
    let mut credit = crate::credit_note::prepare(&tx, &request, NOW).unwrap();
    credit.payload["creditAllocations"][0]["originalAmount"] = json!("80.07");
    let hash =
        crate::invoice_email::sha(serde_json::to_string(&credit.payload).unwrap().as_bytes());
    crate::credit_note::register_and_post(
        &tx,
        credit,
        Artifact {
            filename: "corrupted.json",
            path: "corrupted.json",
            hash: &hash,
            mime: "application/json",
        },
        "synthetic",
        Date::parse("2026-10-20").unwrap(),
    )
    .unwrap();
    assert!(crate::vat_report::range(&tx, "2026-10-01", "2026-12-31").is_err());
}
