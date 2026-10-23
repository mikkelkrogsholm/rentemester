use super::*;
use rentemester_engine::document::Metadata;
use rusqlite::{Connection, TransactionBehavior};
fn setup() -> Connection {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic VAT purchases").unwrap();
    db.execute_batch("UPDATE companies SET vat_period_type='quarter' WHERE id=1;
        INSERT INTO accounts(account_no,name,type,normal_balance,default_vat_code) VALUES
        ('A','Synthetic cash','asset','debit',NULL),('X','Synthetic expense','expense','debit','REPRESENTATION_SPECIAL'),
        ('V','Synthetic input VAT','vat','debit',NULL),('R','Synthetic reverse VAT','vat','credit',NULL);
        INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by) VALUES
        ('bank','A','confirmed',1,'synthetic'),('input_vat','V','confirmed',1,'synthetic'),('reverse_charge_vat','R','confirmed',1,'synthetic');").unwrap();
    db
}
fn original(tx: &Transaction<'_>, eu: bool, invoice: &str, currency: &str) -> i64 {
    let meta: Metadata = serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale",
        "issueDate":"2026-10-18","deliveryDescription":"Synthetic service","invoiceNo":invoice,
        "amountIncVat":if eu {100}else{125},"vatAmount":if eu {0}else{25},"currency":currency,
        "sender":{"name":"Synthetic supplier","address":"Synthetic street 1","vatOrCvr":if eu {"DE123456789"}else{"DK12345678"}},
        "recipient":{"name":"Synthetic buyer","address":"Synthetic street 2","vatOrCvr":"DK87654321"}})).unwrap();
    let count: i64 = tx
        .query_row("SELECT count(*) FROM documents", [], |r| r.get(0))
        .unwrap();
    let hash = format!("{:064x}", count + 1);
    crate::documents::register(
        tx,
        crate::documents::Original {
            filename: "synthetic.txt",
            hash: &hash,
            path: "synthetic-original",
            mime: "text/plain",
        },
        &meta,
        "synthetic",
        "2026-10-18",
    )
    .unwrap();
    tx.query_row("SELECT max(id) FROM documents", [], |r| r.get(0))
        .unwrap()
}
fn input(id: i64) -> Input {
    serde_json::from_value(json!({"transactionDate":"2026-10-18","text":"Synthetic purchase","documentId":id,"netAmount":100})).unwrap()
}
fn post_rep(tx: &Transaction<'_>, input: &Input) -> Result<Value, Error> {
    post(
        tx,
        input,
        Operation::Representation,
        "synthetic",
        Date::parse("2026-12-01").unwrap(),
        PrincipalKind::User,
        "2026-10-18T12:00:00Z",
    )
}
fn entries(tx: &Transaction<'_>) -> i64 {
    tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
        .unwrap()
}
#[test]
fn strict_input_and_operation_specific_selector() {
    let base = json!({"transactionDate":"bad","text":"","netAmount":-1});
    assert!(serde_json::from_value::<Input>(base.clone()).is_ok()); // domain semantics
    for key in [
        "actor",
        "createdBy",
        "createdByProgram",
        "vatTreatment",
        "bankTransactionId",
    ] {
        let mut v = base.clone();
        v[key] = json!("untrusted");
        assert!(serde_json::from_value::<Input>(v).is_err());
    }
    let mut db = setup();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let mut i = input(1);
    i.invoice_no = Some("".into());
    assert!(resolve_document(&tx, &i, Operation::Representation).is_err());
    i.invoice_no = None;
    i.document_id = Some(9_007_199_254_740_992);
    assert!(resolve_document(&tx, &i, Operation::EuService).is_err());
}
#[test]
fn invoice_exact_latest_override_and_missing_lookup() {
    let mut db = setup();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let first = original(&tx, false, "SYN", "DKK");
    let last = original(&tx, true, "SYN", "DKK");
    let mut i = input(first);
    i.invoice_no = Some(" SYN ".into());
    assert_eq!(
        resolve_document(&tx, &i, Operation::EuService).unwrap(),
        last
    );
    i.invoice_no = Some("syn".into());
    assert_eq!(
        resolve_document(&tx, &i, Operation::EuService)
            .unwrap_err()
            .message,
        "Could not resolve document for invoiceNo syn"
    );
    i.invoice_no = Some("  ".into());
    assert_eq!(
        resolve_document(&tx, &i, Operation::EuService).unwrap(),
        first
    );
    assert_eq!(entries(&tx), 0);
}
#[test]
fn representation_unique_configuration_exact_allocation_receipt_duplicate_and_rollback() {
    let mut db = setup();
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let id = original(&tx, false, "REP", "DKK");
        let receipt = post_rep(&tx, &input(id)).unwrap();
        assert_eq!(receipt["ok"], true, "{receipt}");
        assert_eq!(receipt.as_object().unwrap().len(), 6);
        let amounts: Vec<(String,f64,f64)> = tx.prepare("SELECT a.account_no,l.debit_amount,l.credit_amount FROM journal_lines l JOIN accounts a ON a.id=l.account_id ORDER BY l.id").unwrap().query_map([],|r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap().collect::<Result<_,_>>().unwrap();
        assert_eq!(
            amounts,
            vec![
                ("X".into(), 100.0, 0.0),
                ("X".into(), 18.75, 0.0),
                ("V".into(), 6.25, 0.0),
                ("A".into(), 0.0, 125.0)
            ]
        );
        assert_eq!(
            receipt["appliedRules"].as_array().unwrap().last().unwrap(),
            "DK-VAT-REPRESENTATION-001"
        );
        let actor: String = tx
            .query_row("SELECT created_by FROM journal_entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(actor, "user:synthetic");
        assert!(
            post_rep(&tx, &input(id))
                .unwrap_err()
                .message
                .contains("already posted")
        );
        assert_eq!(entries(&tx), 1);
        tx.rollback().unwrap();
    }
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
            .unwrap(),
        0
    );
}
#[test]
fn missing_ambiguous_inactive_mapping_never_guesses_chart_default() {
    let mut db = setup();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let id = original(&tx, false, "REP", "DKK");
    let i = input(id);
    tx.execute(
        "UPDATE accounts SET default_vat_code=NULL WHERE account_no='X'",
        [],
    )
    .unwrap();
    assert!(
        post_rep(&tx, &i)
            .unwrap_err()
            .message
            .contains("explicit expenseAccountNo")
    );
    tx.execute_batch("UPDATE accounts SET default_vat_code='REPRESENTATION_SPECIAL' WHERE account_no='X'; INSERT INTO accounts(account_no,name,type,normal_balance,default_vat_code) VALUES('Y','Synthetic other expense','expense','debit','REPRESENTATION_SPECIAL');").unwrap();
    assert!(post_rep(&tx, &i).is_err());
    tx.execute("UPDATE accounts SET active=0 WHERE account_no='Y'", [])
        .unwrap();
    assert_eq!(
        expense_account(&tx, &i, Operation::Representation).unwrap(),
        "X"
    );
    assert_eq!(entries(&tx), 0);
}
#[test]
fn invalid_net_date_text_supplier_and_uncached_eu_have_no_effects() {
    let mut db = setup();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let dk = original(&tx, false, "DK", "DKK");
    for value in [json!(0), json!(-1), json!(99), json!(100.001)] {
        let mut i = input(dk);
        i.net_amount = serde_json::from_value(value).unwrap();
        assert!(post_rep(&tx, &i).is_err());
    }
    let mut i = input(dk);
    i.text = "  ".into();
    assert!(post_rep(&tx, &i).is_err());
    i = input(dk);
    i.transaction_date = "2026-02-30".into();
    assert!(post_rep(&tx, &i).is_err());
    i = input(dk);
    i.expense_account_no = Some("X".into());
    assert!(
        prepare(&tx, &i, Operation::EuService, "2026-10-18T12:00:00Z")
            .err()
            .unwrap()
            .message
            .contains("non-Danish EU")
    );
    let eu = original(&tx, true, "EU", "DKK");
    i.document_id = Some(eu);
    assert!(prepare(&tx, &i, Operation::EuService, "2026-10-18T12:00:00Z").is_err());
    assert_eq!(entries(&tx), 0);
}
#[test]
fn foreign_no_bank_explicit_valuation_matches_original_net_and_gross() {
    let mut db = setup();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let id = original(&tx, false, "FX", "EUR");
    let mut i = input(id);
    i.currency = Some("EUR".into());
    i.net_amount = Number::from(746);
    i.amount_foreign = Some(Number::from(125));
    i.amount_dkk = Some(serde_json::from_value(json!(932.5)).unwrap());
    i.fx_rate_to_dkk = Some(serde_json::from_value(json!(7.46)).unwrap());
    let request = prepare(&tx, &i, Operation::Representation, "2026-10-18T12:00:00Z").unwrap();
    assert_eq!(request.amount_dkk.unwrap(), Money::parse("932.50").unwrap());
    i.amount_dkk = Some(Number::from(933));
    assert!(post_rep(&tx, &i).is_err());
    i.amount_dkk = None;
    assert!(post_rep(&tx, &i).is_err());
    assert_eq!(entries(&tx), 0);
}
#[test]
fn cached_eu_without_bank_and_representation_with_imported_payment() {
    let mut db = setup();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let eu = original(&tx, true, "EU-CACHED", "DKK");
    let now = "2026-10-18T12:00:00Z";
    let record = rentemester_engine::vies::record(&json!({"countryCode":"DE","vatNumber":"123456789","normalized":"DE123456789","valid":true,"name":null,"address":null,"rawResponse":"{}"}),now).unwrap();
    crate::vies::store(&tx, &record, "user:synthetic").unwrap();
    let mut i = input(eu);
    i.expense_account_no = Some("X".into());
    let receipt = post(
        &tx,
        &i,
        Operation::EuService,
        "synthetic",
        Date::parse("2026-12-01").unwrap(),
        PrincipalKind::ServiceAccount,
        now,
    )
    .unwrap();
    assert_eq!(receipt["ok"], true, "{receipt}");
    let amounts: Vec<(String,f64,f64)> = tx.prepare("SELECT a.account_no,l.debit_amount,l.credit_amount FROM journal_lines l JOIN accounts a ON a.id=l.account_id ORDER BY l.id").unwrap().query_map([],|r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap().collect::<Result<_,_>>().unwrap();
    assert_eq!(
        amounts,
        vec![
            ("X".into(), 100.0, 0.0),
            ("V".into(), 25.0, 0.0),
            ("A".into(), 0.0, 100.0),
            ("R".into(), 0.0, 25.0)
        ]
    );
    let rep = original(&tx, false, "REP-BANK", "DKK");
    let bank_input = serde_json::from_value(
        json!({"name":"Synthetic bank","slug":"synthetic","ledgerAccountNo":"A"}),
    )
    .unwrap();
    crate::bank::create_account(&tx, &bank_input, "user:synthetic").unwrap();
    let csv = "date;text;amount\n2026-10-18;Synthetic representation;-125\n";
    let rows = rentemester_engine::bank::parse(csv).unwrap();
    crate::bank::import(
        &tx,
        csv,
        &rows,
        Some("synthetic"),
        "synthetic",
        now,
        "synthetic-original",
    )
    .unwrap();
    let mut rep_input = input(rep);
    rep_input.source_bank_transaction_id = Some(1);
    assert_eq!(post_rep(&tx, &rep_input).unwrap()["ok"], true);
    assert!(post_rep(&tx, &rep_input).is_err());
    let matched: i64 = tx
        .query_row(
            "SELECT count(*) FROM bank_journal_reconciliations WHERE bank_transaction_id=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(matched, 1);
}
