//! Corrected source contract: foreign amounts must never be presented as DKK interest.
use super::*;
use rusqlite::{Connection, TransactionBehavior};
#[test]
fn foreign_interest_actions_refuse_explicitly_without_accounting_or_claim_effects() {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic foreign interest seller")
        .unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET address='Synthetic Street',cvr='12345678',vat_period_type='quarter';INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('R','Synthetic debtor','asset','debit'),('S','Synthetic sales','income','credit'),('I','Synthetic interest','income','credit'),('V','Synthetic VAT','vat','credit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('debtors','R',1,'synthetic'),('output_vat','V',1,'synthetic');").unwrap();
    let payload=crate::sales::prepare(&tx,&serde_json::from_value(json!({"issueDate":"2026-01-01","dueDate":"2026-01-02","currency":"EUR","fxRateToDkk":7.45,"vatRatePercent":25,"seller":{"name":"Synthetic foreign interest seller","address":"Synthetic Street","vatOrCvr":"DK12345678"},"buyer":{"name":"Synthetic buyer","address":"Synthetic Buyer Street","vatOrCvr":"DK87654321"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":80}]})).unwrap(),"2026-01-01T12:00:00Z",true).unwrap();
    let hash = crate::invoice_email::sha(&serde_json::to_vec_pretty(&payload).unwrap());
    let pdf_hash = crate::invoice_email::sha(b"synthetic PDF");
    let issued = crate::sales::register(
        &tx,
        &payload,
        crate::sales::Artifact {
            filename: "invoice.json",
            path: "invoice.json",
            hash: &hash,
            mime: "application/json",
        },
        crate::sales::Artifact {
            filename: "invoice.pdf",
            path: "invoice.pdf",
            hash: &pdf_hash,
            mime: "application/pdf",
        },
        "synthetic",
    )
    .unwrap();
    let id = issued["documentId"].as_i64().unwrap();
    let latest = Date::parse("2026-01-21").unwrap();
    crate::sales::post(
        &tx,
        &crate::sales::Post {
            invoice_document_id: id,
            revenue_account_no: "S".into(),
            confirm: true,
        },
        "synthetic",
        latest,
    )
    .unwrap();
    let counts = || {
        [
            "journal_entries",
            "audit_log",
            "invoice_interest_claims",
            "invoice_interest_postings",
            "invoice_interest_corrections",
        ]
        .map(|table| {
            tx.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
        })
    };
    let before = counts();
    let calculate: Calculate =
        serde_json::from_value(json!({"invoiceDocumentId":id,"asOfDate":"2026-01-20"})).unwrap();
    let register: Register = serde_json::from_value(
        json!({"invoiceDocumentId":id,"asOfDate":"2026-01-20","confirm":true}),
    )
    .unwrap();
    let post:Post=serde_json::from_value(json!({"invoiceDocumentId":id,"receivableAccountNo":"R","interestIncomeAccountNo":"I","confirm":true})).unwrap();
    let correction = Correction {
        invoice_document_id: id,
    };
    let post_correction: PostCorrection = serde_json::from_value(
        json!({"invoiceDocumentId":id,"reason":"Synthetic reviewed correction","confirm":true}),
    )
    .unwrap();
    for outcome in [
        super::calculate(&tx, &calculate),
        super::register(&tx, &register, "synthetic"),
        super::post(&tx, &post, "synthetic", latest),
        super::correction(&tx, &correction),
        super::post_correction(&tx, &post_correction, "synthetic", latest),
    ] {
        assert_eq!(outcome.unwrap_err().code, "unsupported_foreign_interest");
        assert_eq!(counts(), before);
    }
    assert_eq!(
        crate::reminder_claims::principal_open(&tx, id).unwrap().1,
        "EUR"
    );
    let audit = crate::audit_invoice::verify(&tx).unwrap();
    assert_eq!(audit["ok"], true, "{audit}");
}
