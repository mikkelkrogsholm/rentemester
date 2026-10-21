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
fn issue(tx: &Transaction<'_>, date: &str, amount: i32) -> i64 {
    issue_currency(tx, date, amount, "DKK")
}
fn issue_currency(tx: &Transaction<'_>, date: &str, amount: i32, currency: &str) -> i64 {
    let input=serde_json::from_value(json!({"issueDate":date,"currency":currency,"fxRateToDkk":if currency=="DKK"{1.0}else{7.5},"buyer":{"name":"Synthetic Buyer","address":"Synthetic Street 2","vatOrCvr":"DK12345678"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":amount}]})).unwrap();
    let payload = crate::sales::prepare(tx, &input, "2026-10-22T12:00:00Z", true).unwrap();
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
    id
}
#[test]
fn shifted_year_true_cutoff_ignores_later_sale_and_credit() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let id = issue(&tx, "2026-07-02", 100);
    issue(&tx, "2026-10-21", 200);
    let request=serde_json::from_value(json!({"invoiceDocumentId":id,"issueDate":"2026-10-21","reason":"Synthetic full correction","confirm":true})).unwrap();
    let credit = crate::credit_note::prepare(&tx, &request, "2026-10-21T12:00:00Z").unwrap();
    let raw = serde_json::to_vec_pretty(&credit.payload).unwrap();
    let hash = crate::invoice_email::sha(&raw);
    crate::credit_note::register_and_post(
        &tx,
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
    let before = company(&tx, "synthetic", "2026-10-20").unwrap();
    assert_eq!(before["fiscalPeriodStart"], "2026-07-01");
    assert_eq!(before["fiscalPeriodEnd"], "2027-06-30");
    assert_eq!(before["periodEnd"], "2026-10-20");
    assert_eq!(before["resultat"], 100.0);
    assert_eq!(before["omsaetning"], 100.0);
    assert_eq!(before["openInvoiceTotal"], 125.0);
    assert_eq!(before["vat"]["payable"], 0.0);
    assert_eq!(before["vat"]["periodStart"], "2026-10-01");
    let after = company(&tx, "synthetic", "2026-10-21").unwrap();
    assert_eq!(after["resultat"], 200.0);
    assert_eq!(after["openInvoiceTotal"], 250.0);
    assert_eq!(after["vat"]["payable"], 25.0);
}
#[test]
fn absent_statement_is_unknown_but_documented_zero_is_known() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let csv = "date;text;amount;currency;reference;balance_after\n2026-10-19;Synthetic inflow;100;DKK;in;100\n2026-10-20;Synthetic outflow;-100;DKK;out;0\n";
    let rows = rentemester_engine::bank::parse(csv).unwrap();
    crate::bank::import(
        &tx,
        csv,
        &rows,
        Some("synthetic"),
        "synthetic",
        "2026-10-20T12:00:00Z",
        "synthetic-source",
    )
    .unwrap();
    let missing = company(&tx, "synthetic", "2026-10-18").unwrap();
    assert!(missing["actualBankBalance"].is_null());
    assert_eq!(missing["bankStatementStatus"], "none");
    assert_eq!(missing["unlinkedBankCount"], 0);
    let known = company(&tx, "synthetic", "2026-10-19").unwrap();
    assert_eq!(known["actualBankBalance"], 100.0);
    assert_eq!(known["unlinkedBankCount"], 1);
    let zero = company(&tx, "synthetic", "2026-10-20").unwrap();
    assert_eq!(zero["actualBankBalance"], 0.0);
    assert_eq!(zero["bankStatementStatus"], "known");
    assert_eq!(zero["unlinkedBankCount"], 2);
}
#[test]
fn tasks_are_current_status_of_dated_sources_not_invented_historical_health() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("INSERT INTO exceptions(type,severity,message,created_at)VALUES('SYNTHETIC','medium','Synthetic task','2026-10-19 12:00:00'),('SYNTHETIC','medium','Synthetic future task','2026-10-21 12:00:00');").unwrap();
    let before: i64 = tx
        .query_row("SELECT total_changes()", [], |r| r.get(0))
        .unwrap();
    let card = company(&tx, "synthetic", "2026-10-20").unwrap();
    assert_eq!(card["openTaskCount"], 1);
    assert_eq!(card["availability"]["tasks"]["status"], "current_state");
    assert_eq!(
        card["availability"]["tasks"]["reason"],
        "current_open_state_for_sources_through_cutoff"
    );
    let after: i64 = tx
        .query_row("SELECT total_changes()", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(card["audit"]["scope"], "current_journal_hash_chain");
}
#[test]
fn invalid_date_and_non_dkk_functional_currency_refuse() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert!(company(&tx, "synthetic", "2026-02-30").is_err());
    tx.execute("UPDATE companies SET currency='EUR' WHERE id=1", [])
        .unwrap();
    assert!(
        company(&tx, "synthetic", "2026-10-20")
            .unwrap_err()
            .message
            .contains("DKK")
    );
}

#[test]
fn real_import_preserves_separate_historical_schedule_and_dated_native_invoice_subset() {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic imported portfolio")
        .unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let bytes = include_bytes!("../dinero_import/fixtures/source126-deflated.zip");
    let latest = Date::parse("2026-10-22").unwrap();
    let source = crate::dinero_import::decode_source(bytes).unwrap();
    let prepared = crate::dinero_import::prepare(&tx, &source, latest).unwrap();
    let review = prepared.review();
    assert_eq!(review["status"], "ready", "{review}");
    crate::dinero_import::apply(
        &tx,
        &prepared,
        &crate::dinero_import::Apply {
            plan_hash: review["planHash"].as_str().unwrap().into(),
            idempotency_key: "synthetic-portfolio-import128".into(),
            confirm: true,
        },
        latest,
        "synthetic",
        1_792_497_600_000,
    )
    .unwrap();
    let boundary = |card: &Value| {
        assert_eq!(card["availability"]["invoices"]["status"], "partial");
        assert_eq!(
            card["availability"]["invoices"]["reason"],
            "native_invoices_only_imported_receivables_separate"
        );
        assert!(
            card["limitations"].as_array().unwrap().iter().any(|v| v
                .as_str()
                .is_some_and(|s| s.contains("importerede tilgodehavender")
                    && s.contains("separate oversigt")))
        );
    };
    let changes = tx.total_changes();
    let imported_only = company(&tx, "synthetic", "2026-10-20").unwrap();
    boundary(&imported_only);
    assert_eq!(imported_only["openInvoiceCount"], 0);
    assert_eq!(imported_only["openInvoiceTotal"], 0.0);
    assert_eq!(imported_only["overdueInvoiceCount"], 0);
    let schedule = crate::imported_receivables::read(&tx, "2026-10-20").unwrap();
    assert_eq!(schedule["totalOpen"], 24500.0);
    assert_eq!(tx.total_changes(), changes);
    // Explicit native setup follows import; source mapping/provenance remains untouched.
    tx.execute_batch("UPDATE companies SET address='Synthetic Street',vat_period_type='quarter' WHERE id=1;INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('R','Synthetic native debtor','asset','debit'),('S','Synthetic native sales','income','credit'),('V','Synthetic native VAT','vat','credit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('debtors','R',1,'synthetic'),('output_vat','V',1,'synthetic');").unwrap();
    issue(&tx, "2026-07-02", 100);
    let counts = || {
        tx.query_row(
            "SELECT (SELECT count(*) FROM documents),(SELECT count(*) FROM journal_entries)",
            [],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )
        .unwrap()
    };
    let before_counts = counts();
    let changes = tx.total_changes();
    let before_issue = company(&tx, "synthetic", "2026-07-01").unwrap();
    boundary(&before_issue);
    assert_eq!(before_issue["openInvoiceCount"], 0);
    assert_eq!(before_issue["openInvoiceTotal"], 0.0);
    let issued = company(&tx, "synthetic", "2026-07-02").unwrap();
    boundary(&issued);
    assert_eq!(issued["openInvoiceCount"], 1);
    assert_eq!(issued["openInvoiceTotal"], 125.0);
    assert_eq!(issued["overdueInvoiceCount"], 0);
    let later = company(&tx, "synthetic", "2026-10-20").unwrap();
    boundary(&later);
    assert_eq!(later["openInvoiceCount"], 1);
    assert_eq!(later["openInvoiceTotal"], 125.0);
    assert_eq!(later["overdueInvoiceCount"], 1);
    assert_eq!(
        crate::imported_receivables::read(&tx, "2026-10-20").unwrap(),
        schedule
    );
    assert_eq!(counts(), before_counts);
    assert_eq!(tx.total_changes(), changes);
    crate::dinero_import::validate_history(&tx).unwrap();
    assert!(crate::journal_hash::verify(&tx).unwrap().ok);
}
#[test]
fn foreign_native_invoice_has_explicit_original_and_dkk_units() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    issue_currency(&tx, "2026-07-02", 100, "EUR");
    let before = company(&tx, "synthetic", "2026-07-01").unwrap();
    assert_eq!(before["openInvoiceTotal"], 0.0);
    let changes = tx.total_changes();
    let card = company(&tx, "synthetic", "2026-10-20").unwrap();
    assert_eq!(card["openInvoiceTotal"], 937.5);
    assert_eq!(card["openInvoiceCount"], 1);
    assert_eq!(card["availability"]["invoices"]["status"], "complete");
    let dated = dated_invoices(&tx, "2026-10-20").unwrap();
    assert_eq!(dated["rows"][0]["currency"], "EUR");
    assert_eq!(dated["rows"][0]["openBalance"], 125.0);
    assert_eq!(dated["rows"][0]["openBalanceDkk"], 937.5);
    assert_eq!(tx.total_changes(), changes);
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
fn loss_amount(tx: &Transaction<'_>, id: i64, date: &str, amount: i32) {
    crate::invoice_bad_debt::write_off(tx, &serde_json::from_value(json!({"invoiceDocumentId":id,"writeOffDate":date,"grossAmount":amount,"expenseAccountNo":"E","note":"Synthetic established loss","confirm":true})).unwrap(), "synthetic", Date::parse("2026-10-22").unwrap()).unwrap();
}
#[test]
fn dated_fx_losses_credits_and_claims_keep_exact_separate_units() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('E','Synthetic loss','expense','debit'),('I','Synthetic claim income','income','credit'),('G','Synthetic FX gain','income','credit'),('L','Synthetic FX loss','expense','debit');INSERT INTO bank_accounts(slug,name,ledger_account_no,currency)VALUES('eur','Synthetic EUR bank','B','EUR');").unwrap();
    crate::native_fx_roles::confirm(&tx, "fx_gain", "G", "synthetic").unwrap();
    crate::native_fx_roles::confirm(&tx, "fx_loss", "L", "synthetic").unwrap();
    let eur = issue_currency(&tx, "2026-07-02", 100, "EUR");
    let dkk = issue(&tx, "2026-07-03", 100);
    let csv = "date;text;amount;currency;reference;amount_dkk;fx_rate_to_dkk\n2026-08-02;Synthetic partial;25;EUR;first;190;7.6\n2026-08-05;Synthetic remaining;50;EUR;last;380;7.6\n";
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
    let pay = |reference: &str| {
        crate::invoice_settlement::settle(&tx, &serde_json::from_value(json!({"invoiceDocumentId":eur,"bankTransactionReference":reference,"confirm":true})).unwrap(), "synthetic", Date::parse("2026-10-22").unwrap()).unwrap();
    };
    pay("first");
    credit_amount(&tx, eur, "2026-08-03", 25);
    loss_amount(&tx, eur, "2026-08-04", 25);
    pay("last");
    // Future unrelated loss must not suppress earlier invoices or their totals.
    loss_amount(&tx, dkk, "2026-08-20", 25);
    crate::invoice_compensation::register(&tx, &serde_json::from_value(json!({"invoiceDocumentId":dkk,"asOfDate":"2026-08-10","compensationAmountDkk":50,"confirm":true})).unwrap(), "synthetic").unwrap();
    crate::invoice_compensation::post(&tx, &serde_json::from_value(json!({"invoiceDocumentId":dkk,"transactionDate":"2026-08-10","receivableAccountNo":"R","compensationIncomeAccountNo":"I","confirm":true})).unwrap(), "synthetic", Date::parse("2026-10-22").unwrap()).unwrap();
    let csv = "date;text;amount;currency;reference\n2026-08-22;Synthetic claim;50;DKK;claim\n";
    crate::bank::import(
        &tx,
        csv,
        &rentemester_engine::bank::parse(csv).unwrap(),
        Some("synthetic"),
        "synthetic",
        "2026-10-22T12:00:00Z",
        "synthetic",
    )
    .unwrap();
    // Claim receipt requires principal cleared. Its separate journal still must
    // not enter the historical principal carrying on the shared debtor account.
    let bank: i64 = tx
        .query_row(
            "SELECT id FROM bank_transactions WHERE reference='claim'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    // The supported claim writer permits payment only after principal settlement;
    // settle that remaining principal on a later date and verify earlier reads.
    let csv = "date;text;amount;currency;reference\n2026-08-21;Synthetic DKK principal;100;DKK;dkk-principal\n";
    crate::bank::import(
        &tx,
        csv,
        &rentemester_engine::bank::parse(csv).unwrap(),
        Some("synthetic"),
        "synthetic",
        "2026-10-22T12:00:00Z",
        "synthetic",
    )
    .unwrap();
    crate::invoice_settlement::settle(&tx, &serde_json::from_value(json!({"invoiceDocumentId":dkk,"bankTransactionReference":"dkk-principal","confirm":true})).unwrap(), "synthetic", Date::parse("2026-10-22").unwrap()).unwrap();
    crate::reminder_claims::settle(
        &tx,
        &serde_json::from_value(
            json!({"invoiceDocumentId":dkk,"bankTransactionId":bank,"confirm":true}),
        )
        .unwrap(),
        "synthetic",
        Date::parse("2026-10-22").unwrap(),
    )
    .unwrap();
    let changes = tx.total_changes();
    for (date, original, carrying, total, count) in [
        ("2026-08-01", 125.0, 937.5, 1062.5, 2),
        ("2026-08-02", 100.0, 750.0, 875.0, 2),
        ("2026-08-03", 75.0, 562.5, 687.5, 2),
        ("2026-08-04", 50.0, 375.0, 500.0, 2),
        ("2026-08-05", 0.0, 0.0, 125.0, 1),
        ("2026-08-07", 0.0, 0.0, 125.0, 1),
        ("2026-08-11", 0.0, 0.0, 125.0, 1),
        ("2026-08-20", 0.0, 0.0, 100.0, 1),
        ("2026-08-21", 0.0, 0.0, 0.0, 0),
        ("2026-08-22", 0.0, 0.0, 0.0, 0),
    ] {
        let principal = crate::invoice_fx::dated_principal(&tx, eur, date).unwrap();
        assert_eq!(wire(&principal.open).unwrap(), json!(original), "{date}");
        assert_eq!(
            wire(&principal.carrying_dkk).unwrap(),
            json!(carrying),
            "{date}"
        );
        assert_eq!(principal.currency, "EUR");
        let snapshot = dated_invoices(&tx, date).unwrap();
        assert_eq!(snapshot["openInvoiceTotal"], json!(total), "{date}");
        assert_eq!(snapshot["openInvoiceCount"], json!(count), "{date}");
        if original > 0.0 {
            assert_eq!(snapshot["rows"][0]["documentId"], eur);
            assert_eq!(snapshot["rows"][0]["currency"], "EUR");
            assert_eq!(snapshot["rows"][0]["openBalanceDkk"], json!(carrying));
        }
        for row in snapshot["rows"].as_array().unwrap() {
            if row["currency"] == "DKK" {
                assert_eq!(row["openBalance"], row["openBalanceDkk"]);
            }
        }
    }
    assert_eq!(tx.total_changes(), changes);
    assert_eq!(crate::audit_invoice::verify(&tx).unwrap()["ok"], true);
    // Deliberate retained-link corruption must refuse, including an earlier read.
    let triggers = tx
        .prepare(
            "SELECT name FROM sqlite_schema WHERE type='trigger' AND tbl_name='invoice_payments'",
        )
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for trigger in triggers {
        tx.execute_batch(&format!(
            "DROP TRIGGER \"{}\"",
            trigger.replace('"', "\"\"")
        ))
        .unwrap();
    }
    tx.execute(
        "UPDATE invoice_payments SET currency='USD' WHERE invoice_document_id=?1",
        [eur],
    )
    .unwrap();
    assert!(dated_invoices(&tx, "2026-08-01").is_err());
}

#[test]
fn dated_fx_refund_reuses_frozen_principal_without_cash_rate_revaluation() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('G','Synthetic FX gain','income','credit'),('L','Synthetic FX loss','expense','debit');INSERT INTO bank_accounts(slug,name,ledger_account_no,currency)VALUES('eur','Synthetic EUR bank','B','EUR');").unwrap();
    crate::native_fx_roles::confirm(&tx, "fx_gain", "G", "synthetic").unwrap();
    crate::native_fx_roles::confirm(&tx, "fx_loss", "L", "synthetic").unwrap();
    let id = issue_currency(&tx, "2026-07-02", 100, "EUR");
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
    credit_amount(&tx, id, "2026-08-03", 25);
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
    let changes = tx.total_changes();
    for (date, original, carrying) in [
        ("2026-08-01", 125.0, 937.5),
        ("2026-08-02", 0.0, 0.0),
        ("2026-08-03", -25.0, -187.5),
        ("2026-08-04", 0.0, 0.0),
    ] {
        let dated = crate::invoice_fx::dated_principal(&tx, id, date).unwrap();
        assert_eq!(dated.currency, "EUR");
        assert_eq!(wire(&dated.open).unwrap(), json!(original), "{date}");
        assert_eq!(
            wire(&dated.carrying_dkk).unwrap(),
            json!(carrying),
            "{date}"
        );
        assert_eq!(
            dated_invoices(&tx, date).unwrap()["openInvoiceCount"],
            if original > 0.0 { 1 } else { 0 }
        );
    }
    assert_eq!(tx.total_changes(), changes);
    assert_eq!(crate::audit_invoice::verify(&tx).unwrap()["ok"], true);
}
