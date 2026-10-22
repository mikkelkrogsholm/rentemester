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
    let csv = "date;text;amount;currency;reference\n2026-01-07;Synthetic backdated principal;500;DKK;principal-backdated\n2026-01-20;Synthetic principal remainder;500;DKK;principal-final\n2026-01-21;Synthetic net interest;2.05;DKK;interest-net\n2026-01-21;Synthetic full interest;2.74;DKK;interest-full\n2026-01-21;Synthetic ambiguous interest;2.50;DKK;interest-ambiguous\n2026-01-09;Synthetic later effective refund;-500;DKK;later-refund\n";
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
fn calculation(id: i64) -> Calculate {
    serde_json::from_value(
        json!({"invoiceDocumentId":id,"asOfDate":"2026-01-12","referenceRatePercent":2}),
    )
    .unwrap()
}
fn registration(id: i64) -> Register {
    serde_json::from_value(json!({"invoiceDocumentId":id,"asOfDate":"2026-01-12","referenceRatePercent":2,"confirm":true})).unwrap()
}
fn posting(id: i64) -> Post {
    serde_json::from_value(json!({"invoiceDocumentId":id,"receivableAccountNo":"R","interestIncomeAccountNo":"I","confirm":true})).unwrap()
}
fn adjustment(id: i64) -> PostCorrection {
    serde_json::from_value(json!({"invoiceDocumentId":id,"reason":"Synthetic backdated principal correction","confirm":true})).unwrap()
}
fn latest() -> Date {
    Date::parse("2026-01-21").unwrap()
}
fn principal(tx: &Transaction<'_>, id: i64, reference: &str) {
    crate::invoice_settlement::settle(
        tx,
        &serde_json::from_value(
            json!({"invoiceDocumentId":id,"bankTransactionReference":reference,"confirm":true}),
        )
        .unwrap(),
        "synthetic",
        latest(),
    )
    .unwrap();
}
fn receipt(tx: &Transaction<'_>, id: i64, reference: &str) -> crate::reminder_claims::Settle {
    let bank: i64 = tx
        .query_row(
            "SELECT id FROM bank_transactions WHERE reference=?1",
            [reference],
            |r| r.get(0),
        )
        .unwrap();
    serde_json::from_value(json!({"invoiceDocumentId":id,"bankTransactionId":bank,"confirm":true}))
        .unwrap()
}
fn claimed(tx: &Transaction<'_>, id: i64) {
    let count: i64 = tx
        .query_row("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        calculate(tx, &calculation(id)).unwrap()["accruedInterestAmount"],
        json!(2.74)
    );
    register(tx, &registration(id), "synthetic").unwrap();
    assert_eq!(
        tx.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        count
    );
    assert!(register(tx, &registration(id), "synthetic").is_err());
    let before = crate::reminder_claims::read(tx, id, "2026-01-21").unwrap();
    assert_eq!(before["pendingUnpostedInterest"], true);
    post(tx, &posting(id), "synthetic", latest()).unwrap();
    assert!(post(tx, &posting(id), "synthetic", latest()).is_err());
}
#[test]
fn actual_interest_registration_post_backdated_principal_correction_receipt_audit_reopen() {
    let (mut db, id) = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    claimed(&tx, id);
    principal(&tx, id, "principal-backdated");
    let proposal = correction(
        &tx,
        &Correction {
            invoice_document_id: id,
        },
    )
    .unwrap();
    assert_eq!(proposal["overClaimedAmount"], json!(0.69));
    assert_eq!(proposal["canPost"], true);
    post_correction(&tx, &adjustment(id), "synthetic", latest()).unwrap();
    assert!(
        !correction(
            &tx,
            &Correction {
                invoice_document_id: id
            }
        )
        .unwrap()["canPost"]
            .as_bool()
            .unwrap()
    );
    principal(&tx, id, "principal-final");
    let paid = receipt(&tx, id, "interest-net");
    crate::reminder_claims::settle(&tx, &paid, "synthetic", latest()).unwrap();
    assert!(crate::reminder_claims::settle(&tx, &paid, "synthetic", latest()).is_err());
    let report = crate::reminder_claims::read(&tx, id, "2026-01-21").unwrap();
    assert_eq!(report["principalOpenBalance"], json!(0.0));
    assert_eq!(report["ledgerBackedClaimBalance"], json!(0.0));
    assert_eq!(report["totalInterestDkk"], json!(2.74));
    assert_eq!(report["totalInterestCorrectionsDkk"], json!(0.69));
    assert!(
        crate::reminder_claims::audit_evidence(&tx)
            .unwrap()
            .is_empty()
    );
    let audit = crate::audit_invoice::verify(&tx).unwrap();
    assert_eq!(audit["ok"], true, "{audit}");
    let close_assurance = crate::periods::audit::verify(&tx).unwrap();
    assert!(close_assurance.is_empty(), "{close_assurance:?}");
    tx.commit().unwrap();
    let tx = db.transaction().unwrap();
    assert_eq!(
        crate::reminder_claims::read(&tx, id, "2026-01-21").unwrap(),
        report
    );
}
#[test]
fn later_effective_refund_preserves_authenticated_historical_correction() {
    let (mut db, id) = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    claimed(&tx, id);
    principal(&tx, id, "principal-backdated");
    post_correction(&tx, &adjustment(id), "synthetic", latest()).unwrap();
    // A later-issued full credit creates an actual refundable balance. The
    // refund is then applied with its archived earlier bank date: its effective
    // principal increase changes lawful interest without rewriting the old plan.
    let prepared=crate::credit_note::prepare(&tx,&serde_json::from_value(json!({"invoiceDocumentId":id,"issueDate":"2026-01-20","reason":"Synthetic service credit","grossAmount":1000,"confirm":true})).unwrap(),"2026-01-21T12:00:00Z").unwrap();
    let hash = crate::invoice_email::sha(&serde_json::to_vec_pretty(&prepared.payload).unwrap());
    crate::credit_note::register_and_post(
        &tx,
        prepared,
        crate::sales::Artifact {
            filename: "credit.json",
            path: "credit.json",
            hash: &hash,
            mime: "application/json",
        },
        "synthetic",
        latest(),
    )
    .unwrap();
    crate::invoice_settlement::refund(&tx,&serde_json::from_value(json!({"invoiceDocumentId":id,"bankTransactionReference":"later-refund","confirm":true})).unwrap(),"synthetic",latest()).unwrap();
    let result = correction(
        &tx,
        &Correction {
            invoice_document_id: id,
        },
    )
    .unwrap();
    assert_eq!(result["alreadyCorrected"], json!(0.69));
    assert_eq!(result["lawfulInterest"], json!(2.47));
    assert_eq!(result["overClaimedAmount"], json!(0.0));
    assert_eq!(result["canPost"], false);
    let audit = crate::audit_invoice::verify(&tx).unwrap();
    assert_eq!(audit["ok"], true, "{audit}");
}
#[test]
fn paid_or_shared_account_ambiguous_interest_cannot_be_automatically_corrected() {
    for ambiguous in [false, true] {
        let (mut db, id) = fixture();
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        claimed(&tx, id);
        if ambiguous {
            let reminder=crate::reminder_claims::register(&tx,&serde_json::from_value(json!({"invoiceDocumentId":id,"reminderDate":"2026-01-13","feeAmount":50,"confirm":true})).unwrap(),"synthetic").unwrap();
            crate::reminder_claims::post(&tx,&serde_json::from_value(json!({"invoiceDocumentId":id,"reminderId":reminder["reminderId"],"receivableAccountNo":"R","reminderIncomeAccountNo":"I","confirm":true})).unwrap(),"synthetic",latest()).unwrap();
        }
        principal(&tx, id, "principal-backdated");
        principal(&tx, id, "principal-final");
        let receipt = receipt(
            &tx,
            id,
            if ambiguous {
                "interest-ambiguous"
            } else {
                "interest-full"
            },
        );
        crate::reminder_claims::settle(&tx, &receipt, "synthetic", latest()).unwrap();
        let p = correction(
            &tx,
            &Correction {
                invoice_document_id: id,
            },
        )
        .unwrap();
        assert_eq!(p["hasProposal"], true);
        assert_eq!(p["canPost"], false);
        assert_eq!(p["requiresRefund"], true);
        assert_eq!(p["proposedLines"], json!([]));
        assert!(post_correction(&tx, &adjustment(id), "synthetic", latest()).is_err());
    }
}
#[test]
fn rate_wire_rejects_fractional_basis_points() {
    assert_eq!(rate(&serde_json::from_str("1.75").unwrap()).unwrap(), 175);
    assert!(rate(&serde_json::from_str("1.751").unwrap()).is_err());
}

#[test]
fn service_principal_claim_interest_post_correction_and_receipt_keep_full_audit() {
    let (mut db, id) = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    register_with_principal(
        &tx,
        &registration(id),
        "synthetic-service",
        PrincipalKind::ServiceAccount,
    )
    .unwrap();
    post_with_principal(
        &tx,
        &posting(id),
        "synthetic-service",
        latest(),
        PrincipalKind::ServiceAccount,
    )
    .unwrap();
    principal(&tx, id, "principal-backdated");
    let proposed = correction(
        &tx,
        &Correction {
            invoice_document_id: id,
        },
    )
    .unwrap();
    assert_eq!(proposed["overClaimedAmount"], json!(0.69));
    post_correction_with_principal(
        &tx,
        &adjustment(id),
        "synthetic-service",
        latest(),
        PrincipalKind::ServiceAccount,
    )
    .unwrap();
    assert!(
        post_correction_with_principal(
            &tx,
            &adjustment(id),
            "synthetic-service",
            latest(),
            PrincipalKind::ServiceAccount
        )
        .is_err()
    );
    principal(&tx, id, "principal-final");
    crate::reminder_claims::settle_with_principal(
        &tx,
        &receipt(&tx, id, "interest-net"),
        "synthetic-service",
        latest(),
        PrincipalKind::ServiceAccount,
    )
    .unwrap();
    assert_eq!(
        crate::reminder_claims::read(&tx, id, "2026-01-21").unwrap()["ledgerBackedClaimBalance"],
        json!(0.0)
    );
    assert_eq!(tx.query_row::<i64,_,_>("SELECT count(*) FROM audit_log WHERE actor='service_account:synthetic-service' AND event_type IN ('invoice_interest_register','invoice_interest_post','invoice_interest_correction','invoice_claim_payment_apply')",[],|r|r.get(0)).unwrap(),4);
    assert_eq!(tx.query_row::<i64,_,_>("SELECT count(*) FROM journal_entries WHERE created_by='user:synthetic-service' AND created_by_program='service-principal'",[],|r|r.get(0)).unwrap(),3);
    assert!(
        crate::reminder_claims::audit_evidence(&tx)
            .unwrap()
            .is_empty()
    );
    let audit = crate::audit_invoice::verify(&tx).unwrap();
    assert_eq!(audit["ok"], true, "{audit}");
}
