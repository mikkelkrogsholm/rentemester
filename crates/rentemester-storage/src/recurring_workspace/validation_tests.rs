use super::*;
use crate::{
    idempotency::PrincipalKind,
    recurring_invoices::{self as recurring, original},
    sales,
};
use rusqlite::TransactionBehavior;
const DATE: &str = "2026-01-01";
const NOW: &str = "2026-01-01T00:00:00.000Z";
fn fixture(channel: &str) -> (Connection, i64, i64) {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Recurring Archive")
        .unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET address='Synthetic Seller Street',cvr='87654321',vat_period_type='quarter' WHERE id=1").unwrap();
    let input:original::Create=serde_json::from_value(json!({"name":"Synthetic recurring archive","interval":"monthly","deliveryChannel":channel,"firstIssueDate":DATE,"invoice":{"invoiceType":"full","currency":"DKK","seller":{"name":"Synthetic Recurring Archive","address":"Synthetic Seller Street","vatOrCvr":"87654321"},"buyer":{"name":"Synthetic Archive Buyer","address":"Synthetic Buyer Street"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100,"lineTotalExVat":100}],"totals":{"netAmount":100,"vatRate":0.25,"vatAmount":25,"grossAmount":125}},"confirm":true})).unwrap();
    let id=original::create(&tx,&input,"synthetic-owner",PrincipalKind::User).unwrap()["templateId"].as_i64().unwrap();
    let recurring::Prepared::Issue(pending) = original::prepare(
        &tx,
        &original::Generate {
            template_id: id,
            as_of_date: DATE.into(),
            confirm: true,
        },
        DATE,
        NOW,
    )
    .unwrap() else {
        panic!("new generation required")
    };
    let raw = serde_json::to_vec_pretty(&pending.payload).unwrap();
    let hash = sha(&raw);
    let pdf = rentemester_engine::invoice_pdf::build(&pending.payload);
    let pdf_hash = sha(&pdf);
    let issued = sales::register(
        &tx,
        &pending.payload,
        sales::Artifact {
            filename: "synthetic.json",
            path: &format!("documents/{hash}/synthetic.json"),
            hash: &hash,
            mime: "application/json",
        },
        sales::Artifact {
            filename: "synthetic.pdf",
            path: &format!("documents/{pdf_hash}/synthetic.pdf"),
            hash: &pdf_hash,
            mime: "application/pdf",
        },
        "synthetic-owner",
    )
    .unwrap();
    let document = issued["documentId"].as_i64().unwrap();
    recurring::finish(&tx, &pending, &issued, "synthetic-owner").unwrap();
    let generation = tx
        .query_row(
            "SELECT id FROM recurring_invoice_generations WHERE document_id=?1",
            [document],
            |r| r.get(0),
        )
        .unwrap();
    tx.commit().unwrap();
    (db, generation, document)
}
fn event(db: &Connection, generation: i64, kind: &str, typed: bool) -> i64 {
    let (document,channel,hash):(i64,String,String)=db.query_row("SELECT g.document_id,t.delivery_channel,d.sha256_hash FROM recurring_invoice_generations g JOIN recurring_invoice_templates t ON t.id=g.template_id JOIN documents d ON d.id=g.document_id WHERE g.id=?1",[generation],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    let message = if typed {
        format!("{MESSAGE_PREFIX}Synthetic email dry-run; no delivery")
    } else {
        "Historical source event".into()
    };
    let id=db.query_row("INSERT INTO recurring_invoice_delivery_events(generation_id,channel,event_type,message) VALUES(?1,?2,?3,?4) RETURNING id",params![generation,channel,kind,message],|r|r.get::<_,i64>(0)).unwrap();
    if typed {
        let proof=json!({"version":1,"generationId":generation,"eventId":id,"documentId":document,"documentSha256":hash,"channel":channel,"eventType":kind,"providerId":null,"eventMessage":message,"actor":"user:synthetic-owner","mode":if channel=="email"{"email_dry_run"}else{"native_public"}}).to_string();
        db.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES(?1,'recurring_invoice_delivery_event',?2,?3,'user:synthetic-owner')",params![AUDIT,id.to_string(),proof]).unwrap();
    }
    id
}
#[test]
fn typed_dryrun_and_unresolved_public_histories_remain_valid_evidence() {
    for channel in ["email", "digisense"] {
        let (db, g, _) = fixture(channel);
        event(&db, g, "attempted", true);
        event(
            &db,
            g,
            if channel == "email" {
                "acknowledged"
            } else {
                "uncertain"
            },
            true,
        );
        validate_history(&db).unwrap();
        crate::portable_backup::company(&db).unwrap();
        // Signed image roundtrip/reopen is exercised by the existing host backup seam.
    }
    let (db, g, _) = fixture("email");
    event(&db, g, "attempted", false);
    event(&db, g, "uncertain", false);
    validate_history(&db).unwrap();
}
#[test]
fn missing_mismatched_or_orphan_provenance_never_downgrades_to_legacy() {
    for corrupt in [
        "DELETE FROM audit_log WHERE event_type='original_recurring_delivery_event'",
        "UPDATE audit_log SET message='{}' WHERE event_type='original_recurring_delivery_event'",
        "INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('original_recurring_delivery_event','wrong','999','{}','user:synthetic')",
    ] {
        let (db, g, _) = fixture("email");
        event(&db, g, "attempted", true);
        validate_history(&db).unwrap();
        // Recreate only audit trigger declarations around a hostile signed-image mutation.
        let triggers: Vec<(String, String)> = db
            .prepare(
                "SELECT name,sql FROM sqlite_schema WHERE type='trigger' AND tbl_name='audit_log'",
            )
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        for (name, _) in &triggers {
            db.execute_batch(&format!("DROP TRIGGER {name}")).unwrap();
        }
        db.execute_batch(corrupt).unwrap();
        for (_, sql) in triggers {
            db.execute_batch(&sql).unwrap();
        }
        assert!(validate_history(&db).is_err(), "{corrupt}");
        assert!(crate::portable_backup::company(&db).is_err());
    }
}
#[test]
fn dryrun_is_not_public_ack_and_legacy_failure_is_not_new_attempt_authority() {
    let (db, g, _) = fixture("digisense");
    event(&db, g, "attempted", true);
    event(&db, g, "acknowledged", true);
    assert!(validate_history(&db).is_err());
    let (db, g, _) = fixture("email");
    event(&db, g, "preflight_failed", false);
    event(&db, g, "attempted", true);
    assert!(validate_history(&db).is_err());
}
