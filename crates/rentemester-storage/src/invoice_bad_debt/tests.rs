use super::*;
use rusqlite::{Connection, TransactionBehavior};
fn latest() -> Date {
    Date::parse("2026-01-21").unwrap()
}
fn fixture(currency: &str) -> (Connection, i64) {
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
fn request(id: i64, amount: Option<f64>, date: &str) -> WriteOff {
    serde_json::from_value(json!({"invoiceDocumentId":id,"writeOffDate":date,"grossAmount":amount,"expenseAccountNo":"E","note":"Synthetic documented established loss","confirm":true})).unwrap()
}
fn principal(tx: &Transaction<'_>, id: i64, reference: &str) -> Result<Value, Error> {
    crate::invoice_settlement::settle(
        tx,
        &serde_json::from_value(
            json!({"invoiceDocumentId":id,"bankTransactionReference":reference,"confirm":true}),
        )
        .unwrap(),
        "synthetic",
        latest(),
    )
}
fn claims(tx: &Transaction<'_>, id: i64) {
    crate::invoice_compensation::register(tx,&serde_json::from_value(json!({"invoiceDocumentId":id,"asOfDate":"2026-01-03","compensationAmountDkk":50,"confirm":true})).unwrap(),"synthetic").unwrap();
    crate::invoice_compensation::post(tx,&serde_json::from_value(json!({"invoiceDocumentId":id,"receivableAccountNo":"R","compensationIncomeAccountNo":"I","confirm":true})).unwrap(),"synthetic",latest()).unwrap();
}
fn claim_receipt(tx: &Transaction<'_>, id: i64) {
    let bank: i64 = tx
        .query_row(
            "SELECT id FROM bank_transactions WHERE reference='claim-dkk'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    crate::reminder_claims::settle(
        tx,
        &serde_json::from_value(
            json!({"invoiceDocumentId":id,"bankTransactionId":bank,"confirm":true}),
        )
        .unwrap(),
        "synthetic",
        latest(),
    )
    .unwrap();
}
fn audit(tx: &Transaction<'_>) {
    let a = crate::audit_invoice::verify(tx).unwrap();
    assert_eq!(a["ok"], true, "{a}");
    assert!(
        crate::reminder_claims::audit_evidence(tx)
            .unwrap()
            .is_empty()
    );
    let assurance = crate::periods::audit::verify(tx).unwrap();
    assert!(assurance.is_empty(), "{assurance:?}");
}
#[test]
fn canonical_claim_status_supplies_only_available_loss_expense_accounts() {
    let (mut db, id) = fixture("DKK");
    let tx = db.transaction().unwrap();
    let available = |tx: &Transaction<'_>| {
        crate::reminder_claims::read(tx, id, "2026-01-21").unwrap()["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| {
                a["accountNo"] == "E" && a["type"] == "expense" && a["normalBalance"] == "debit"
            })
    };
    assert!(available(&tx));
    tx.execute(
        "UPDATE accounts SET allow_direct_posting=0 WHERE account_no='E'",
        [],
    )
    .unwrap();
    assert!(!available(&tx));
    tx.execute(
        "UPDATE accounts SET allow_direct_posting=1,active=0 WHERE account_no='E'",
        [],
    )
    .unwrap();
    assert!(!available(&tx));
}
#[test]
fn dkk_writeoff_then_remaining_principal_claim_receipt_vat_audit_and_reopen() {
    let (mut db, id) = fixture("DKK");
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    claims(&tx, id);
    let r = write_off(
        &tx,
        &request(id, Some(500.0), "2026-01-10"),
        "synthetic",
        latest(),
    )
    .unwrap();
    assert_eq!(r["grossAmountDkk"], json!(500.0));
    assert_eq!(r["netAmount"], json!(400.0));
    assert_eq!(r["vatAmount"], json!(100.0));
    assert_eq!(r["remainingPrincipal"], json!(750.0));
    assert_eq!(r["remainingClaimDkk"], json!(50.0));
    principal(&tx, id, "principal-dkk").unwrap();
    claim_receipt(&tx, id);
    audit(&tx);
    let vat = crate::vat_report::range(&tx, "2026-01-01", "2026-01-31").unwrap();
    assert_eq!(vat["payable"], json!(150.0));
    assert_eq!(vat["errors"], json!([]));
    let report = crate::reminder_claims::read(&tx, id, "2026-01-21").unwrap();
    assert_eq!(report["principalOpenBalance"], json!(0.0));
    assert_eq!(report["registeredClaimBalanceDkk"], json!(0.0));
    assert_eq!(report["badDebtWriteOffs"].as_array().unwrap().len(), 1);
    let list = crate::sales::list(&tx, "synthetic", "2026-01-21", Some("2026")).unwrap();
    assert_eq!(list["invoices"][0]["status"], "written_off");
    assert_eq!(list["invoices"][0]["totalBadDebtWrittenOff"], json!(500.0));
    tx.commit().unwrap();
    let tx = db.transaction().unwrap();
    assert_eq!(
        crate::reminder_claims::read(&tx, id, "2026-01-21").unwrap(),
        report
    );
}
#[test]
fn full_source_dkk_and_cumulative_small_partial_vat_rounding() {
    for partial in [false, true] {
        let (mut db, id) = fixture("DKK");
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        if !partial {
            crate::invoice_compensation::register(&tx,&serde_json::from_value(json!({"invoiceDocumentId":id,"asOfDate":"2026-01-03","compensationAmountDkk":50,"confirm":true})).unwrap(),"synthetic").unwrap();
        }
        if partial {
            let a = write_off(
                &tx,
                &request(id, Some(0.03), "2026-01-08"),
                "synthetic",
                latest(),
            )
            .unwrap();
            let b = write_off(
                &tx,
                &request(id, Some(0.03), "2026-01-09"),
                "synthetic",
                latest(),
            )
            .unwrap();
            assert_eq!(a["vatAmount"], json!(0.01));
            assert_eq!(b["vatAmount"], json!(0.0));
        }
        let last = write_off(&tx, &request(id, None, "2026-01-10"), "synthetic", latest()).unwrap();
        assert_eq!(last["remainingPrincipal"], json!(0.0));
        assert_eq!(
            last["vatAmount"],
            if partial { json!(249.99) } else { json!(250.0) }
        );
        assert_eq!(
            history(&tx, id).unwrap().gross_dkk,
            Money::parse("1250").unwrap()
        );
        if !partial {
            assert_eq!(last["remainingClaimDkk"], json!(50.0));
            crate::invoice_compensation::post(&tx,&serde_json::from_value(json!({"invoiceDocumentId":id,"receivableAccountNo":"R","compensationIncomeAccountNo":"I","confirm":true})).unwrap(),"synthetic",latest()).unwrap();
            claim_receipt(&tx, id);
        }
        assert!(principal(&tx, id, "principal-dkk").is_err());
        let vat = crate::vat_report::range(&tx, "2026-01-01", "2026-01-31").unwrap();
        assert_eq!(vat["payable"], json!(0.0));
        audit(&tx);
    }
}
#[test]
fn foreign_source_vector_uses_remaining_carrying_and_separate_frozen_output_vat() {
    let (mut db, id) = fixture("EUR");
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    principal(&tx, id, "principal-before").unwrap();
    claims(&tx, id);
    let r = write_off(&tx, &request(id, None, "2026-01-10"), "synthetic", latest()).unwrap();
    assert_eq!(r["principalCurrency"], "EUR");
    assert_eq!(r["grossAmount"], json!(74.95));
    assert_eq!(r["vatAmount"], json!(14.99));
    assert_eq!(r["grossAmountDkk"], json!(557.03));
    assert_eq!(r["vatAmountDkk"], json!(111.41));
    assert_eq!(r["netAmountDkk"], json!(445.62));
    assert_eq!(r["remainingPrincipalDkk"], json!(0.0));
    let rate: String = tx
        .query_row(
            "SELECT CAST(fx_rate_to_dkk AS TEXT) FROM journal_entries WHERE id=?1",
            [r["journalEntryId"].as_i64()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(FxRate::parse(&rate).unwrap().decimal(), "7.432021");
    claim_receipt(&tx, id);
    audit(&tx);
    let vat = crate::vat_report::range(&tx, "2026-01-01", "2026-01-31").unwrap();
    assert_eq!(vat["payable"], json!(74.39));
    assert_eq!(vat["errors"], json!([]));
}
#[test]
fn foreign_receipts_after_partial_writeoff_clear_exact_carrying_and_realized_fx() {
    let (mut db, id) = fixture("EUR");
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    write_off(
        &tx,
        &request(id, Some(20.0), "2026-01-10"),
        "synthetic",
        latest(),
    )
    .unwrap();
    let first = principal(&tx, id, "principal-after").unwrap();
    assert_eq!(first["principalAmountDkk"], json!(371.98));
    assert_eq!(first["realizedFxAmountDkk"], json!(3.40));
    principal(&tx, id, "principal-final").unwrap();
    let report = crate::reminder_claims::read(&tx, id, "2026-01-21").unwrap();
    assert_eq!(report["principalOpenBalance"], json!(0.0));
    assert_eq!(report["claimOpenBalance"], Value::Null);
    audit(&tx);
}
#[test]
fn wrong_accounts_dates_fractional_amount_over_principal_closed_period_and_history_refuse() {
    let (mut db, id) = fixture("DKK");
    for mut request in [
        request(id, Some(1250.01), "2026-01-10"),
        request(id, Some(1.001), "2026-01-10"),
        request(id, Some(100.0), "2025-12-31"),
        request(id, Some(100.0), "2026-01-10"),
    ] {
        if request
            .gross_amount
            .as_ref()
            .is_some_and(|a| a.to_string() == "100.0")
            && request.write_off_date == "2026-01-10"
        {
            request.expense_account_no = "S".into();
        }
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(write_off(&tx, &request, "synthetic", latest()).is_err());
        tx.rollback().unwrap();
    }
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        tx.execute("INSERT INTO accounting_periods(period_start,period_end,kind,status)VALUES('2026-01-01','2026-01-31','custom','closed')",[]).unwrap();
        assert!(
            write_off(
                &tx,
                &request(id, Some(100.0), "2026-01-10"),
                "synthetic",
                latest()
            )
            .is_err()
        );
        tx.rollback().unwrap();
    }
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let written = write_off(&tx, &request(id, None, "2026-01-10"), "synthetic", latest()).unwrap();
    assert!(
        write_off(
            &tx,
            &request(id, Some(1.0), "2026-01-11"),
            "synthetic",
            latest()
        )
        .is_err()
    );
    let registration: String = tx
        .query_row(
            "SELECT registration_datetime FROM journal_entries WHERE id=?1",
            [written["journalEntryId"].as_i64()],
            |r| r.get(0),
        )
        .unwrap();
    tx.execute("UPDATE account_role_mappings SET status='superseded' WHERE role='output_vat' AND status='confirmed'",[]).unwrap();
    tx.execute("INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by,confirmation_source,confirmed_at)VALUES('output_vat','V2','confirmed',2,'user:synthetic','explicit',datetime(?1,'+1 second'))",[registration]).unwrap();
    tx.execute(
        "UPDATE accounts SET active=0 WHERE account_no IN ('V','E')",
        [],
    )
    .unwrap();
    assert_eq!(
        history(&tx, id).unwrap().gross,
        Money::parse("1250").unwrap()
    );
    let triggers=tx.prepare("SELECT name FROM sqlite_master WHERE type='trigger' AND tbl_name='invoice_bad_debt_writeoffs'").unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
    for name in triggers {
        tx.execute_batch(&format!("DROP TRIGGER \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
    }
    tx.execute(
        "UPDATE invoice_bad_debt_writeoffs SET vat_amount=249,net_amount=1001",
        [],
    )
    .unwrap();
    assert!(history(&tx, id).is_err());
}

#[test]
fn original_loss_adapter_uses_number_historical_assertions_and_exact_service_audit() {
    use crate::{idempotency::PrincipalKind, invoice_bad_debt_original as original};
    let (mut db, id) = fixture("DKK");
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let no: String = tx
        .query_row("SELECT invoice_no FROM documents WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .unwrap();
    let body = json!({"payload":{"invoiceNumber":no,"writeOffDate":"2026-01-10","grossAmount":250,"expenseAccountNo":"E","receivableAccountNo":"R","vatAccountNo":"V"},"confirm":true});
    assert_eq!(original::invoice_id(&tx, &body).unwrap(), id);
    for (k, v) in [("receivableAccountNo", "A"), ("vatAccountNo", "V2")] {
        let mut wrong = body.clone();
        wrong["payload"][k] = json!(v);
        assert_eq!(
            original::apply(
                &tx,
                &wrong,
                id,
                "svc:exact",
                PrincipalKind::ServiceAccount,
                latest()
            )
            .unwrap_err()
            .code,
            "invoice_account_mismatch"
        );
    }
    let result = original::apply(
        &tx,
        &body,
        id,
        "svc:exact",
        PrincipalKind::ServiceAccount,
        latest(),
    )
    .unwrap();
    assert_eq!(result["grossAmount"].as_f64(), Some(250.0));
    assert_eq!(result["netAmount"].as_f64(), Some(200.0));
    assert_eq!(result["vatAmount"].as_f64(), Some(50.0));
    assert_eq!(result["openBalance"].as_f64(), Some(1000.0));
    assert_eq!(result["claimOpenBalance"].as_f64(), Some(1000.0));
    let wid = result["writeOffId"].as_i64().unwrap();
    assert!(result["entryId"].as_i64().is_some());
    crate::audit_invoice::bad_debt_evidence(&tx, wid).unwrap();
    let audit:(String,String)=tx.query_row("SELECT actor,message FROM audit_log WHERE event_type='invoice_bad_debt_writeoff' AND entity_id=?1",[wid.to_string()],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    assert_eq!(audit.0, "service_account:svc:exact");
    assert_eq!(
        crate::reminder_claims::principal::service_subject(&audit.0, &audit.1).unwrap(),
        "svc:exact"
    );
    tx.execute("UPDATE account_role_mappings SET status='superseded' WHERE role='output_vat' AND status='confirmed'",[]).unwrap();
    tx.execute("INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by,confirmation_source,confirmed_at)VALUES('output_vat','V2','confirmed',2,'user:synthetic','explicit',datetime('now','+1 second'))",[]).unwrap();
    assert_eq!(historical_vat_account(&tx, id).unwrap(), "V");
    assert_eq!(
        original::apply(
            &tx,
            &body,
            id,
            "svc:exact",
            PrincipalKind::ServiceAccount,
            latest()
        )
        .unwrap_err()
        .code,
        "invoice_account_mismatch"
    );
    let count: i64 = tx
        .query_row(
            "SELECT count(*) FROM invoice_bad_debt_writeoffs WHERE invoice_document_id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('invoice_bad_debt_writeoff','invoice_bad_debt_writeoff',?1,'forged','service_account:svc:exact')",[wid.to_string()]).unwrap();
    assert!(crate::audit_invoice::bad_debt_evidence(&tx, wid).is_err());
}

#[test]
fn source_created_false_expense_is_refused_by_existing_bad_debt_guard_without_effects() {
    use crate::accounts_original::{Operation, execute};
    let (mut db, id) = fixture("DKK");
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let created = execute(&tx, Operation::Add, &json!({"input":{"accountNo":"CONTROL","name":"Synthetic controlled expense","type":"expense","allowDirectPosting":false},"confirm":true}), "service_account:synthetic").unwrap();
    assert_eq!(created["ok"], true);
    let stored: (String, bool) = tx
        .query_row(
            "SELECT normal_balance,allow_direct_posting FROM accounts WHERE account_no='CONTROL'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(stored, ("debit".into(), false));
    let inventory = || -> Vec<i64> {
        [
            "journal_entries",
            "journal_lines",
            "invoice_bad_debt_writeoffs",
            "audit_log",
        ]
        .iter()
        .map(|table| {
            tx.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap()
        })
        .collect()
    };
    let before = inventory();
    let mut controlled = request(id, Some(100.0), "2026-01-10");
    controlled.expense_account_no = "CONTROL".into();
    let error = write_off(&tx, &controlled, "synthetic", latest()).unwrap_err();
    assert!(
        error.message.contains("active") && error.message.contains("compatible"),
        "{error}"
    );
    assert_eq!(inventory(), before);
    // The very same valid invoice/date/amount succeeds with the regular expense:
    // refusal above is the targeted direct flag guard, not an invalid fixture.
    controlled.expense_account_no = "E".into();
    let result = write_off(&tx, &controlled, "synthetic", latest()).unwrap();
    assert_eq!(result["grossAmountDkk"], json!(100.0));
    assert_eq!(inventory()[2], before[2] + 1);
    audit(&tx);
}
