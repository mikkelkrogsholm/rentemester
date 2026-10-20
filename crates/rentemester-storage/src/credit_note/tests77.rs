//! Synthetic EUR lifecycle: original units, frozen DKK and actual bank value stay distinct.
use super::*;
fn fixture() -> rusqlite::Connection {
    let mut control = rusqlite::Connection::open_in_memory().unwrap();
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic EU Seller").unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET address='Synthetic Street',cvr='87654321',vat_period_type='quarter' WHERE id=1; INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('R','Synthetic receivables','asset','debit'),('S','Synthetic services','income','credit'),('B','Synthetic EUR bank','asset','debit'),('G','Synthetic FX gain','income','credit'),('L','Synthetic FX loss','expense','debit'); INSERT INTO account_role_mappings(role,account_no,version,confirmed_by) VALUES('debtors','R',1,'synthetic'),('fx_gain','G',1,'synthetic'),('fx_loss','L',1,'synthetic'); INSERT INTO bank_accounts(slug,name,currency,ledger_account_no) VALUES('synthetic-eur','Synthetic EUR bank','EUR','B');").unwrap();
    crate::vies::store(&tx,&json!({"countryCode":"DE","vatNumber":"123456789","normalized":"DE123456789","valid":true,"name":"Synthetic EU Buyer","address":"Synthetic EU Street","validatedAt":"2026-10-18T10:00:00Z","expiresAt":"2027-01-16T10:00:00Z","rawResponse":{"isValid":true}}),"user:synthetic").unwrap();
    let input=serde_json::from_value(json!({"issueDate":"2026-10-18","currency":"EUR","fxRateToDkk":7.46,"vatTreatment":"foreign_reverse_charge","reverseChargeBasis":"EU_B2B_SERVICES","reverseChargeNote":"Reverse charge, EU B2B services","buyer":{"name":"Synthetic EU Buyer","address":"Synthetic EU Street","vatOrCvr":"DE123456789"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100,"taxClassification":"reverse_charge"}]})).unwrap();
    let payload = crate::sales::prepare(&tx, &input, "2026-10-18T12:00:00Z", true).unwrap();
    crate::sales::register(
        &tx,
        &payload,
        Artifact {
            filename: "invoice.json",
            path: "invoice.json",
            hash: &"a".repeat(64),
            mime: "application/json",
        },
        Artifact {
            filename: "invoice.pdf",
            path: "invoice.pdf",
            hash: &"b".repeat(64),
            mime: "application/pdf",
        },
        "synthetic",
    )
    .unwrap();
    crate::sales::post(
        &tx,
        &crate::sales::Post {
            invoice_document_id: 1,
            revenue_account_no: "S".into(),
            confirm: true,
        },
        "synthetic",
        Date::parse("2026-10-20").unwrap(),
    )
    .unwrap();
    let csv = "date;text;amount;currency;reference;amount_dkk;fx_rate_to_dkk\n2026-10-18;Synthetic part;40;EUR;part;300;7.5\n2026-10-18;Synthetic final;60;EUR;final;444;7.4\n2026-10-19;Synthetic refund;-25;EUR;refund;-187.5;7.5\n2026-10-19;Synthetic excess;101;EUR;excess;753.46;7.46\n";
    crate::bank::import(
        &tx,
        csv,
        &rentemester_engine::bank::parse(csv).unwrap(),
        Some("synthetic-eur"),
        "synthetic",
        "2026-10-20T12:00:00Z",
        "synthetic-eur.csv",
    )
    .unwrap();
    tx.commit().unwrap();
    db
}
fn receipt(tx: &Transaction<'_>, reference: &str) -> Result<Value, Error> {
    crate::invoice_settlement::settle(
        tx,
        &serde_json::from_value(
            json!({"invoiceDocumentId":1,"bankTransactionReference":reference,"confirm":true}),
        )
        .unwrap(),
        "synthetic",
        Date::parse("2026-10-20").unwrap(),
    )
}
fn credit(tx: &Transaction<'_>, amount: Value) -> Result<Value, Error> {
    let prepared=prepare(tx,&serde_json::from_value(json!({"invoiceDocumentId":1,"issueDate":"2026-10-19","reason":"Synthetic service correction","grossAmount":amount,"confirm":true})).unwrap(),"2026-10-20T12:00:00Z")?;
    let hash =
        crate::invoice_email::sha(serde_json::to_string(&prepared.payload).unwrap().as_bytes());
    register_and_post(
        tx,
        prepared,
        Artifact {
            filename: "credit.json",
            path: "credit.json",
            hash: &hash,
            mime: "application/json",
        },
        "synthetic",
        Date::parse("2026-10-20").unwrap(),
    )
}
#[test]
fn eur_partial_final_receipt_credit_refund_book_original_basis_and_realized_fx() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let original: String = tx
        .query_row("SELECT payload_json FROM documents WHERE id=1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(rentemester_engine::bank::parse("date;text;amount;currency;reference\n2026-10-19;Synthetic missing valuation;1;EUR;missing\n").is_err());
    assert!(receipt(&tx, "excess").is_err());
    let first = receipt(&tx, "part").unwrap();
    assert_eq!(first["principalAmountDkk"], json!(298.4));
    assert_eq!(first["realizedFxAmountDkk"], json!(1.6));
    assert!(receipt(&tx, "part").is_err());
    let final_payment = receipt(&tx, "final").unwrap();
    assert_eq!(final_payment["principalAmountDkk"], json!(447.6));
    assert_eq!(final_payment["realizedFxAmountDkk"], json!(-3.6));
    credit(&tx, json!(25)).unwrap();
    assert_eq!(credited(&tx, 1).unwrap().decimal(), "25.00");
    assert_eq!(
        crate::invoice_fx::credited_dkk(&tx, 1).unwrap().decimal(),
        "186.50"
    );
    let request = serde_json::from_value(
        json!({"invoiceDocumentId":1,"bankTransactionReference":"refund","confirm":true}),
    )
    .unwrap();
    let refund = crate::invoice_settlement::refund(
        &tx,
        &request,
        "synthetic",
        Date::parse("2026-10-20").unwrap(),
    )
    .unwrap();
    assert_eq!(refund["remainingCreditBalance"], json!(0));
    assert_eq!(refund["principalAmountDkk"], json!(186.5));
    assert_eq!(refund["realizedFxAmountDkk"], json!(-1));
    assert!(
        crate::invoice_settlement::refund(
            &tx,
            &request,
            "synthetic",
            Date::parse("2026-10-20").unwrap()
        )
        .is_err()
    );
    assert_eq!(
        tx.query_row::<String, _, _>("SELECT payload_json FROM documents WHERE id=1", [], |r| r
            .get(0))
            .unwrap(),
        original
    );
    assert_eq!(crate::invoice_settlement::principal_carrying(&tx,1,tx.query_row("SELECT receivable_account_id FROM issued_invoice_postings WHERE invoice_document_id=1",[],|r|r.get(0)).unwrap()).unwrap(),Money::default());
}
#[test]
fn eur_partial_and_final_credit_conserve_original_cents_and_dkk() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    credit(&tx, json!(33.33)).unwrap();
    credit(&tx, json!(33.33)).unwrap();
    credit(&tx, Value::Null).unwrap();
    assert_eq!(credited(&tx, 1).unwrap().decimal(), "100.00");
    assert_eq!(
        crate::invoice_fx::credited_dkk(&tx, 1).unwrap().decimal(),
        "746.00"
    );
    assert!(credit(&tx, Value::Null).is_err());
}
#[test]
fn foreign_nonzero_difference_requires_confirmed_fx_role_before_any_effect() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute(
        "UPDATE account_role_mappings SET status='superseded' WHERE role='fx_gain'",
        [],
    )
    .unwrap();
    let before: i64 = tx
        .query_row("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
        .unwrap();
    assert!(receipt(&tx, "part").is_err());
    assert_eq!(
        tx.query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
            .unwrap(),
        before
    );
    assert_eq!(
        tx.query_row::<i64, _, _>("SELECT count(*) FROM invoice_payments", [], |r| r.get(0))
            .unwrap(),
        0
    );
}

#[test]
fn reversed_foreign_receipt_blocks_following_principal_and_refund() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let paid = receipt(&tx, "part").unwrap();
    let input = crate::journal_reversal::Input {
        entry_id: rentemester_engine::identifiers::JournalEntryId::from_sqlite(
            paid["entryId"].as_i64().unwrap(),
        ),
        transaction_date: Date::parse("2026-10-19").unwrap(),
        reason: "Synthetic reversal".into(),
    };
    let result = crate::journal_reversal::reverse(
        &tx,
        &input,
        &crate::journal_preview::manual_journal_rule_version().unwrap(),
        Date::parse("2026-10-20").unwrap(),
        crate::journal_post::Actor {
            subject: "user:synthetic",
            program: "rentemester-rust",
        },
    )
    .unwrap();
    assert_eq!(result["ok"], true);
    assert!(receipt(&tx, "final").is_err());
}
