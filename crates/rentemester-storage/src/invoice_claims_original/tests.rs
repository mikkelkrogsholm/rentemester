use super::*;
use rusqlite::{Connection, TransactionBehavior};
fn fixture() -> (Connection, i64) {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic interest seller")
        .unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET address='Synthetic Street 1',cvr='12345678',vat_period_type='quarter';INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('R','Synthetic debtor','asset','debit'),('R2','Other debtor','asset','debit'),('S','Synthetic sales','income','credit'),('I','Synthetic interest income','income','credit'),('V','Synthetic VAT','vat','credit'),('A','Synthetic bank','asset','debit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('debtors','R',1,'synthetic'),('output_vat','V',1,'synthetic');INSERT INTO bank_accounts(slug,name,ledger_account_no)VALUES('interest-bank','Synthetic bank','A');").unwrap();
    let customer=crate::sales::create_customer(&tx,&serde_json::from_value(json!({"name":"Synthetic buyer","address":"Synthetic Street 2","vatOrCvr":"DK87654321","paymentTermsDays":1})).unwrap(),"synthetic").unwrap();
    let payload=crate::sales::prepare(&tx,&serde_json::from_value(json!({"issueDate":"2026-01-01","dueDate":"2026-01-02","customerId":customer["id"],"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":800}]})).unwrap(),"2026-01-01T12:00:00Z",true).unwrap();
    let hash = crate::invoice_email::sha(&serde_json::to_vec_pretty(&payload).unwrap());
    let pdf = crate::invoice_email::sha(b"synthetic PDF");
    let registered = crate::sales::register(
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
    let id = registered["documentId"].as_i64().unwrap();
    crate::sales::post(
        &tx,
        &crate::sales::Post {
            invoice_document_id: id,
            revenue_account_no: "S".into(),
            confirm: true,
        },
        "synthetic",
        Date::parse("2026-01-21").unwrap(),
    )
    .unwrap();
    let csv = "date;text;amount;currency;reference\n2026-01-07;Synthetic backdated principal;500;DKK;principal-backdated\n2026-01-20;Synthetic principal remainder;500;DKK;principal-final\n2026-01-21;Synthetic net interest;412.05;DKK;interest-net\n2026-01-21;Synthetic full interest;2.74;DKK;interest-full\n2026-01-21;Synthetic ambiguous interest;2.50;DKK;interest-ambiguous\n2026-01-09;Synthetic later effective refund;-500;DKK;later-refund\n";
    let rows = rentemester_engine::bank::parse(csv).unwrap();
    crate::bank::import(
        &tx,
        csv,
        &rows,
        Some("interest-bank"),
        "synthetic",
        "2026-01-21T12:00:00Z",
        "synthetic",
    )
    .unwrap();
    tx.commit().unwrap();
    (db, id)
}

fn call(tx: &Transaction<'_>, id: i64, kind: Kind, mut body: Value) -> Value {
    if kind != Kind::SettleClaimBank {
        body["documentId"] = json!(id);
    }
    if !kind.readonly() {
        body["confirm"] = json!(true);
    }
    apply(
        tx,
        kind,
        &body,
        id,
        "synthetic",
        PrincipalKind::User,
        Date::parse("2026-01-21").unwrap(),
    )
    .unwrap()
}
#[test]
fn all_eleven_original_bindings_keep_separate_claims_causal_correction_and_receipt() {
    let (mut db, id) = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("PRAGMA query_only=ON").unwrap();
    let before = tx.total_changes();
    let assessment = call(&tx, id, Kind::Compensation, json!({"asOf":"2026-01-12"}));
    assert_eq!(assessment["data"]["compensationAmountDkk"], 310.0);
    let interest = call(
        &tx,
        id,
        Kind::Interest,
        json!({"asOf":"2026-01-12","referenceRate":2}),
    );
    assert_eq!(interest["data"]["accruedInterestAmount"], 2.74);
    let initial_correction = call(&tx, id, Kind::InterestCorrection, json!({}));
    assert_eq!(initial_correction["data"]["hasProposal"], false);
    assert_eq!(before, tx.total_changes());
    tx.execute_batch("PRAGMA query_only=OFF").unwrap();
    let journals: i64 = tx
        .query_row("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
        .unwrap();
    let reminder = call(&tx, id, Kind::Remind, json!({"date":"2026-01-12"}));
    assert_eq!(reminder["data"]["feeAmount"], 100.0);
    let compensation = call(
        &tx,
        id,
        Kind::ClaimCompensation,
        json!({"asOf":"2026-01-12"}),
    );
    assert_eq!(compensation["data"]["eligible"], true);
    assert_eq!(compensation["data"]["reason"], "registered");
    let claimed = call(
        &tx,
        id,
        Kind::ClaimInterest,
        json!({"asOf":"2026-01-12","referenceRate":2}),
    );
    assert_eq!(claimed["data"]["accruedInterestAmount"], 2.74);
    assert_eq!(claimed["data"]["claimDate"], "2026-01-12");
    assert_eq!(
        tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        journals
    );
    let rem = call(
        &tx,
        id,
        Kind::PostReminder,
        json!({"receivableAccountNo":"R","reminderIncomeAccountNo":"I"}),
    );
    assert!(rem["data"]["entryId"].is_i64());
    call(
        &tx,
        id,
        Kind::PostCompensation,
        json!({"receivableAccountNo":"R","compensationIncomeAccountNo":"I"}),
    );
    let posted = call(
        &tx,
        id,
        Kind::PostInterest,
        json!({"receivableAccountNo":"R","interestIncomeAccountNo":"I"}),
    );
    assert_eq!(posted["data"]["accruedInterestAmount"], 2.74);
    crate::invoice_settlement::settle(&tx,&decode(json!({"invoiceDocumentId":id,"bankTransactionReference":"principal-backdated","confirm":true})).unwrap(),"synthetic",Date::parse("2026-01-21").unwrap()).unwrap();
    let before = tx.total_changes();
    let proposal = call(&tx, id, Kind::InterestCorrection, json!({}));
    assert_eq!(proposal["data"]["canPost"], true);
    assert_eq!(before, tx.total_changes());
    let correction = call(
        &tx,
        id,
        Kind::PostInterestCorrection,
        json!({"reason":"Synthetic backdated payment"}),
    );
    assert_eq!(correction["data"]["correctedAmount"], 0.69);
    let date: String = tx
        .query_row(
            "SELECT correction_date FROM invoice_interest_corrections",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(date, "2026-01-12");
    crate::invoice_settlement::settle(&tx,&decode(json!({"invoiceDocumentId":id,"bankTransactionReference":"principal-final","confirm":true})).unwrap(),"synthetic",Date::parse("2026-01-21").unwrap()).unwrap();
    let paid = call(
        &tx,
        id,
        Kind::SettleClaimBank,
        json!({"payload":{"invoiceDocumentId":id,"bankTransactionReference":"interest-net","bankAccountNo":"A"}}),
    );
    assert_eq!(paid["data"]["remainingClaimOpenBalance"], 0.0);
    assert_eq!(crate::audit_invoice::verify(&tx).unwrap()["ok"], true);
}
#[test]
fn strict_fields_and_original_earliest_reminder_refuses_already_posted() {
    for (kind, body) in [
        (
            Kind::Interest,
            json!({"documentId":1,"asOf":"2026-01-12","confirm":true}),
        ),
        (
            Kind::Remind,
            json!({"documentId":1,"date":"2026-01-12","fee":null,"confirm":true}),
        ),
        (
            Kind::PostInterestCorrection,
            json!({"documentId":1,"reason":" ","confirm":true}),
        ),
        (
            Kind::SettleClaimBank,
            json!({"payload":{"invoiceDocumentId":1,"bankTransactionId":1,"actor":"other"},"confirm":true}),
        ),
    ] {
        assert!(validate(kind, &body).is_err());
    }
    let (mut db, id) = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    call(&tx, id, Kind::Remind, json!({"date":"2026-01-03"}));
    call(
        &tx,
        id,
        Kind::PostReminder,
        json!({"receivableAccountNo":"R","reminderIncomeAccountNo":"I"}),
    );
    call(&tx, id, Kind::Remind, json!({"date":"2026-01-13"}));
    let before = tx.total_changes();
    let body = json!({"documentId":id,"receivableAccountNo":"R","reminderIncomeAccountNo":"I","confirm":true});
    assert!(
        apply(
            &tx,
            Kind::PostReminder,
            &body,
            id,
            "synthetic",
            PrincipalKind::User,
            Date::parse("2026-01-21").unwrap()
        )
        .is_err()
    );
    assert_eq!(before, tx.total_changes());
    let number: String = tx
        .query_row("SELECT invoice_no FROM documents WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        invoice_id(&tx, Kind::Interest, &json!({"invoiceNumber":number})).unwrap(),
        id
    );
    assert_eq!(
        invoice_id(
            &tx,
            Kind::Interest,
            &json!({"documentId":id,"invoiceNumber":"wrong"})
        )
        .unwrap(),
        id
    );
}
