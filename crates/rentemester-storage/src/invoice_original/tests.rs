use super::*;
use crate::sales::Artifact;
use rusqlite::{Connection, TransactionBehavior};
fn fixture() -> Connection {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Portfolio").unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET address='Synthetic Street',cvr='87654321',vat_period_type='quarter',fiscal_year_start_month=7,fiscal_year_label_strategy='span' WHERE id=1;INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('R','Synthetic debtor','asset','debit'),('S','Synthetic sales','income','credit'),('V','Synthetic VAT','vat','credit'),('B','Synthetic bank','asset','debit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('debtors','R',1,'synthetic'),('output_vat','V',1,'synthetic');INSERT INTO bank_accounts(slug,name,currency,ledger_account_no)VALUES('synthetic','Synthetic bank','DKK','B');").unwrap();
    tx.commit().unwrap();
    db
}
fn issue_currency(tx: &Transaction<'_>, date: &str, amount: i32, currency: &str) -> i64 {
    let input=serde_json::from_value(json!({"issueDate":date,"currency":currency,"fxRateToDkk":if currency=="DKK"{1.0}else{7.5},"buyer":{"name":"Synthetic Buyer","address":"Synthetic Street 2","vatOrCvr":"DK12345678"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":amount}]})).unwrap();
    let mut payload = crate::sales::prepare(tx, &input, "2026-10-22T12:00:00Z", true).unwrap();
    let unposted = date.starts_with("2025");
    if unposted {
        payload.as_object_mut().unwrap().remove("dueDate");
    }
    let hash =
        crate::invoice_email::sha(serde_json::to_string_pretty(&payload).unwrap().as_bytes());
    let pdf = crate::invoice_email::sha(format!("synthetic-pdf-{hash}").as_bytes());
    let id = crate::sales::register(
        tx,
        &payload,
        Artifact {
            filename: "synthetic.json",
            path: &format!("invoices/{hash}.json"),
            hash: &hash,
            mime: "application/json",
        },
        Artifact {
            filename: "synthetic.pdf",
            path: &format!("invoices/{pdf}.pdf"),
            hash: &pdf,
            mime: "application/pdf",
        },
        "synthetic",
    )
    .unwrap()["documentId"]
        .as_i64()
        .unwrap();
    if !unposted {
        crate::sales::post(
            tx,
            &crate::sales::Post {
                invoice_document_id: id,
                revenue_account_no: "S".into(),
                confirm: true,
            },
            "synthetic",
            Date::parse("2026-10-22").unwrap(),
        )
        .unwrap();
    }
    id
}
fn credit_amount(tx: &Transaction<'_>, id: i64, date: &str, amount: i32) {
    let request = serde_json::from_value(json!({"invoiceDocumentId":id,"issueDate":date,"reason":"Synthetic dated correction","grossAmount":amount,"confirm":true})).unwrap();
    let credit = crate::credit_note::prepare(tx, &request, "2026-10-22T12:00:00Z").unwrap();
    let hash = crate::invoice_email::sha(&serde_json::to_vec_pretty(&credit.payload).unwrap());
    crate::credit_note::register_and_post(
        tx,
        credit,
        Artifact {
            filename: "synthetic-credit.json",
            path: &format!("invoices/{hash}.json"),
            hash: &hash,
            mime: "application/json",
        },
        "synthetic",
        Date::parse("2026-10-22").unwrap(),
    )
    .unwrap();
}
#[test]
fn original_invoice_filters_current_history_unposted_credit_refund_and_fx_units() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('G','Synthetic FX gain','income','credit'),('L','Synthetic FX loss','expense','debit');INSERT INTO bank_accounts(slug,name,ledger_account_no,currency)VALUES('eur','Synthetic EUR bank','B','EUR');").unwrap();
    crate::native_fx_roles::confirm(&tx, "fx_gain", "G", "synthetic").unwrap();
    crate::native_fx_roles::confirm(&tx, "fx_loss", "L", "synthetic").unwrap();
    let old = issue_currency(&tx, "2025-12-01", 80, "DKK");
    let id = issue_currency(&tx, "2026-07-02", 100, "EUR");
    let initial = read(&tx, Kind::List, &Query::default(), "2024-01-01").unwrap();
    assert_eq!(initial["data"]["count"], 2);
    assert_eq!(initial["data"]["rows"][0]["documentId"], old);
    assert_eq!(initial["data"]["rows"][0]["effectiveDueDate"], "2025-12-31");
    assert_eq!(
        tx.query_row(
            "SELECT count(*) FROM journal_entries WHERE document_id=?1",
            [old],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    let no = initial["data"]["rows"][1]["invoiceNumber"]
        .as_str()
        .unwrap()
        .to_owned();
    let q = Query {
        invoice_number: Some(format!(" {no} ")),
        customer: Some(" SYNTHETIC ".into()),
        customer_cvr: Some("dk12345678".into()),
        from: Some("2026-07-02".into()),
        to: Some("2026-07-02".into()),
        min_amount: Some(125.into()),
        max_amount: Some(125.into()),
        ..Query::default()
    };
    assert_eq!(
        read(&tx, Kind::List, &q, "2024-01-01").unwrap()["data"]["count"],
        1
    );
    let q = Query {
        query: Some(" buyer ".into()),
        amount: Some(125.into()),
        min_amount: Some(500.into()),
        max_amount: Some(1.into()),
        ..Query::default()
    };
    assert_eq!(
        read(&tx, Kind::Find, &q, "2024-01-01").unwrap()["data"]["count"],
        1
    );
    assert_eq!(
        read(
            &tx,
            Kind::Find,
            &Query {
                query: Some("%".into()),
                ..Query::default()
            },
            "2027-01-01"
        )
        .unwrap()["data"]["count"],
        0
    );
    let q = Query {
        document_id: Some(old),
        invoice_number: Some(no.clone()),
        ..Query::default()
    };
    assert_eq!(
        read(&tx, Kind::Status, &q, "2026-01-01").unwrap()["data"]["invoiceDocumentId"],
        old
    );
    let q = Query {
        invoice_number: Some(no.chars().take(no.chars().count() - 1).collect()),
        ..Query::default()
    };
    assert_eq!(
        read(&tx, Kind::List, &q, "2027-01-01").unwrap()["data"]["count"],
        0
    );
    let overdue = read(
        &tx,
        Kind::Overdue,
        &Query {
            min_days: Some(1),
            ..Query::default()
        },
        "2027-01-01",
    )
    .unwrap();
    assert_eq!(overdue["data"]["rows"][0]["documentId"], old);
    assert_eq!(overdue["data"]["rows"][0]["status"], "open");
    let csv = "date;text;amount;currency;reference;amount_dkk;fx_rate_to_dkk\n2026-08-02;Synthetic full payment;125;EUR;paid;950;7.6\n2026-08-04;Synthetic refund;-25;EUR;refund;-190;7.6\n";
    crate::bank::import(
        &tx,
        csv,
        &rentemester_engine::bank::parse(csv).unwrap(),
        Some("eur"),
        "synthetic",
        "2026-10-22T12:00:00Z",
        "synthetic",
    )
    .unwrap();
    let q = Query {
        document_id: Some(id),
        as_of: Some("2024-01-01".into()),
        ..Query::default()
    };
    crate::invoice_settlement::settle(
        &tx,
        &serde_json::from_value(
            json!({"invoiceDocumentId":id,"bankTransactionReference":"paid","confirm":true}),
        )
        .unwrap(),
        "synthetic",
        Date::parse("2026-10-22").unwrap(),
    )
    .unwrap();
    assert_eq!(
        read(&tx, Kind::Status, &q, "2027-01-01").unwrap()["data"]["status"],
        "paid"
    );
    credit_amount(&tx, id, "2026-08-03", 25);
    assert_eq!(
        read(&tx, Kind::Status, &q, "2027-01-01").unwrap()["data"]["status"],
        "overpaid"
    );
    crate::invoice_settlement::refund(
        &tx,
        &serde_json::from_value(
            json!({"invoiceDocumentId":id,"bankTransactionReference":"refund","confirm":true}),
        )
        .unwrap(),
        "synthetic",
        Date::parse("2026-10-22").unwrap(),
    )
    .unwrap();
    let view = read(&tx, Kind::Status, &q, "2027-01-01").unwrap();
    assert_eq!(view["data"]["status"], "refunded");
    assert_eq!(view["data"]["payments"][0]["amount"], 125.0);
    assert_eq!(view["data"]["creditNotes"][0]["amount"], 25.0);
    assert_eq!(view["data"]["refunds"][0]["amount"], 25.0);
    assert_eq!(view["data"]["currency"], "EUR");
    assert!(view["data"]["claimOpenBalance"].is_null());
    assert_eq!(view["data"]["registeredClaimBalanceDkk"], 0.0);
    assert_eq!(view["data"]["asOfDate"], "2024-01-01");
    let credited = issue_currency(&tx, "2026-09-01", 100, "DKK");
    credit_amount(&tx, credited, "2026-09-02", 125);
    assert_eq!(
        read(
            &tx,
            Kind::Status,
            &Query {
                document_id: Some(credited),
                ..Query::default()
            },
            "2024-01-01"
        )
        .unwrap()["data"]["status"],
        "credited"
    );
    let changes = tx.total_changes();
    read(&tx, Kind::List, &Query::default(), "2027-01-01").unwrap();
    assert_eq!(tx.total_changes(), changes);
}
fn latest() -> Date {
    Date::parse("2026-01-21").unwrap()
}
fn loss_fixture(currency: &str) -> (Connection, i64) {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic loss seller").unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET address='Synthetic Street 1',cvr='12345678',vat_period_type='quarter';INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('R','Synthetic receivable','asset','debit'),('S','Synthetic sales','income','credit'),('E','Synthetic established loss','expense','debit'),('I','Synthetic claims','income','credit'),('V','Synthetic output VAT','vat','credit'),('V2','Later VAT account','vat','credit'),('A','Synthetic bank','asset','debit'),('G','Synthetic FX gain','income','credit'),('L','Synthetic FX loss','expense','debit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('debtors','R',1,'synthetic'),('output_vat','V',1,'synthetic');INSERT INTO bank_accounts(slug,name,ledger_account_no,currency)VALUES('synthetic-dkk','Synthetic DKK bank','A','DKK'),('synthetic-eur','Synthetic EUR bank','A','EUR');").unwrap();
    crate::native_fx_roles::confirm(&tx, "fx_gain", "G", "synthetic").unwrap();
    crate::native_fx_roles::confirm(&tx, "fx_loss", "L", "synthetic").unwrap();
    let customer=crate::sales::create_customer(&tx,&serde_json::from_value(json!({"name":"Synthetic buyer","address":"Synthetic Street 2","vatOrCvr":"DK87654321","paymentTermsDays":1})).unwrap(),"synthetic").unwrap();
    let input=serde_json::from_value(json!({"issueDate":"2026-01-01","dueDate":"2026-01-02","currency":currency,"fxRateToDkk":if currency=="DKK"{1.0}else{7.4321},"customerId":customer["id"],"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":if currency=="DKK"{1000}else{100}}]})).unwrap();
    let payload = crate::sales::prepare(&tx, &input, "2026-01-01T12:00:00Z", true).unwrap();
    let hash = crate::invoice_email::sha(&serde_json::to_vec_pretty(&payload).unwrap());
    let pdf = crate::invoice_email::sha(b"Synthetic loss PDF");
    let result = crate::sales::register(
        &tx,
        &payload,
        crate::sales::Artifact {
            filename: "synthetic.json",
            path: "synthetic.json",
            hash: &hash,
            mime: "application/json",
        },
        crate::sales::Artifact {
            filename: "synthetic.pdf",
            path: "synthetic.pdf",
            hash: &pdf,
            mime: "application/pdf",
        },
        "synthetic",
    )
    .unwrap();
    let id = result["documentId"].as_i64().unwrap();
    crate::sales::post(
        &tx,
        &crate::sales::Post {
            invoice_document_id: id,
            revenue_account_no: "S".into(),
            confirm: true,
        },
        "synthetic",
        latest(),
    )
    .unwrap();
    for (slug, csv) in [
        (
            "synthetic-dkk",
            "date;text;amount;currency;reference\n2026-01-15;Synthetic remaining principal;750;DKK;principal-dkk\n2026-01-20;Synthetic claim receipt;50;DKK;claim-dkk\n",
        ),
        (
            "synthetic-eur",
            "date;text;amount;currency;reference;amount_dkk;fx_rate_to_dkk\n2026-01-05;Synthetic earlier payment;50.05;EUR;principal-before;371.98;7.4321\n2026-01-15;Synthetic later payment;50.05;EUR;principal-after;375.38;7.5\n2026-01-16;Synthetic final payment;54.95;EUR;principal-final;408.39;7.4321\n",
        ),
    ] {
        let rows = rentemester_engine::bank::parse(csv).unwrap();
        crate::bank::import(
            &tx,
            csv,
            &rows,
            Some(slug),
            "synthetic",
            "2026-01-21T12:00:00Z",
            "synthetic",
        )
        .unwrap();
    }
    tx.commit().unwrap();
    (db, id)
}
fn claims(tx: &Transaction<'_>, id: i64) {
    crate::invoice_compensation::register(tx,&serde_json::from_value(json!({"invoiceDocumentId":id,"asOfDate":"2026-01-03","compensationAmountDkk":50,"confirm":true})).unwrap(),"synthetic").unwrap();
    crate::invoice_compensation::post(tx,&serde_json::from_value(json!({"invoiceDocumentId":id,"receivableAccountNo":"R","compensationIncomeAccountNo":"I","confirm":true})).unwrap(),"synthetic",latest()).unwrap();
}

#[test]
fn original_current_loss_and_separate_claims_have_original_and_dkk_units() {
    for currency in ["DKK", "EUR"] {
        let (mut db, id) = loss_fixture(currency);
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        claims(&tx, id);
        let writeoff:crate::invoice_bad_debt::WriteOff=serde_json::from_value(json!({"invoiceDocumentId":id,"writeOffDate":"2026-01-10","expenseAccountNo":"E","note":"Synthetic established loss","confirm":true})).unwrap();
        crate::invoice_bad_debt::write_off(&tx, &writeoff, "synthetic", latest()).unwrap();
        let query = Query {
            document_id: Some(id),
            ..Query::default()
        };
        let view = read(&tx, Kind::Status, &query, "2025-01-01").unwrap();
        let data = &view["data"];
        assert_eq!(data["status"], "written_off");
        assert_eq!(data["openBalance"], 0.0);
        assert_eq!(
            data["totalBadDebtWrittenOff"],
            if currency == "DKK" { 1250.0 } else { 125.0 }
        );
        assert_eq!(data["totalCompensationClaims"], 50.0);
        assert_eq!(data["registeredClaimBalanceDkk"], 50.0);
        assert_eq!(data["claimCurrency"], "DKK");
        assert_eq!(data["principalCurrency"], currency);
        if currency == "DKK" {
            assert_eq!(data["claimOpenBalance"], 50.0);
        } else {
            assert!(data["claimOpenBalance"].is_null());
        }
        assert_eq!(data["badDebtWriteOffs"][0]["writeOffDate"], "2026-01-10");
        assert_eq!(data["compensationClaims"][0]["amountDkk"], 50.0);
    }
}

#[test]
fn original_peppol_summary_exposes_only_safe_source_fields() {
    let native = json!({"id":7,"companyKey":"private","idempotencyKey":"private","status":"acknowledged","submissionReference":"synthetic","transmissionId":"provider-id","history":[{"eventType":"delivered","at":"2026-01-02T00:00:00Z"}]});
    assert_eq!(
        peppol_summary(&native, None),
        json!({"status":"acknowledged","submissionReference":"synthetic","transmissionId":"provider-id","acknowledgedAt":"2026-01-02T00:00:00Z"})
    );
    let reserved = json!({"status":"uncertain","submissionReference":"synthetic","transmissionId":null,"history":[{"eventType":"delivery_reserved","at":"2026-01-02T00:00:00Z"}]});
    assert_eq!(peppol_summary(&reserved, None)["status"], "in_progress");
    assert_eq!(
        peppol_summary(
            &json!({"status":"acknowledged","submissionReference":"legacy","transmissionId":"legacy"}),
            Some("2025-01-01T00:00:00Z")
        )["acknowledgedAt"],
        "2025-01-01T00:00:00Z"
    );
}
