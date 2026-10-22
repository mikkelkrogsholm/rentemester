//! Original recurring histories remain immutable in complete signed snapshots.
use super::*;
use rentemester_storage::{
    idempotency::PrincipalKind,
    recurring_invoices::{self as recurring, original},
    sales,
};
use serde_json::json;
const TODAY: &str = "2026-01-01";
const AT: &str = "2026-01-01T00:00:00.000Z";
fn ledger(archive: &artifact::VerifiedArtifact) -> Connection {
    let bytes = archive
        .entries()
        .iter()
        .find(|e| matches!(e.kind(), artifact::EntryKind::CompanyLedger { .. }))
        .unwrap()
        .bytes();
    let mut db = Connection::open_in_memory().unwrap();
    db.deserialize_read_exact(MAIN_DB, bytes, bytes.len(), false)
        .unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA trusted_schema=OFF")
        .unwrap();
    db
}
fn add_case(db: &mut Connection, channel: &str) -> Vec<artifact::Snapshot> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET address='Synthetic Seller Street',cvr='87654321',vat_period_type='quarter' WHERE id=1").unwrap();
    let input:original::Create=serde_json::from_value(json!({"name":format!("Synthetic archive {channel}"),"interval":"monthly","deliveryChannel":channel,"firstIssueDate":TODAY,"invoice":{"invoiceType":"full","currency":"DKK","seller":{"name":"Synthetic backup coverage","address":"Synthetic Seller Street","vatOrCvr":"87654321"},"buyer":{"name":"Synthetic Archive Buyer","address":"Synthetic Buyer Street"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100,"lineTotalExVat":100}],"totals":{"netAmount":100,"vatRate":0.25,"vatAmount":25,"grossAmount":125}},"confirm":true})).unwrap();
    let id=original::create(&tx,&input,"historical-owner",PrincipalKind::User).unwrap()["templateId"].as_i64().unwrap();
    let recurring::Prepared::Issue(pending) = original::prepare(
        &tx,
        &original::Generate {
            template_id: id,
            as_of_date: TODAY.into(),
            confirm: true,
        },
        TODAY,
        AT,
    )
    .unwrap() else {
        panic!("new generation expected")
    };
    let json = serde_json::to_vec_pretty(&pending.payload).unwrap();
    let hash = rentemester_runtime::backup_keys::digest(&json);
    let pdf = rentemester_engine::invoice_pdf::build(&pending.payload);
    let pdf_hash = rentemester_runtime::backup_keys::digest(&pdf);
    let json_path = format!("documents/{hash}/synthetic.json");
    let pdf_path = format!("documents/{pdf_hash}/synthetic.pdf");
    let issued = sales::register(
        &tx,
        &pending.payload,
        sales::Artifact {
            filename: "synthetic.json",
            path: &json_path,
            hash: &hash,
            mime: "application/json",
        },
        sales::Artifact {
            filename: "synthetic.pdf",
            path: &pdf_path,
            hash: &pdf_hash,
            mime: "application/pdf",
        },
        "historical-owner",
    )
    .unwrap();
    let document = issued["documentId"].as_i64().unwrap();
    recurring::finish(&tx, &pending, &issued, "historical-owner").unwrap();
    let generation: i64 = tx
        .query_row(
            "SELECT id FROM recurring_invoice_generations WHERE document_id=?1",
            [document],
            |r| r.get(0),
        )
        .unwrap();
    for kind in [
        "attempted",
        if channel == "email" {
            "acknowledged"
        } else {
            "uncertain"
        },
    ] {
        let message = "rentemester-recurring/1: Synthetic email dry-run; no actual delivery";
        let event:i64=tx.query_row("INSERT INTO recurring_invoice_delivery_events(generation_id,channel,event_type,message) VALUES(?1,?2,?3,?4) RETURNING id",rusqlite::params![generation,channel,kind,message],|r|r.get(0)).unwrap();
        let proof=json!({"version":1,"generationId":generation,"eventId":event,"documentId":document,"documentSha256":hash,"channel":channel,"eventType":kind,"providerId":null,"eventMessage":message,"actor":"user:historical-owner","mode":if channel=="email"{"email_dry_run"}else{"native_public"}}).to_string();
        tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('original_recurring_delivery_event','recurring_invoice_delivery_event',?1,?2,'user:historical-owner')",rusqlite::params![event.to_string(),proof]).unwrap();
    }
    tx.commit().unwrap();
    vec![
        artifact::Snapshot {
            kind: artifact::EntryKind::DocumentOriginal {
                company: "synthetic".into(),
            },
            path: format!("synthetic/{json_path}"),
            bytes: json,
        },
        artifact::Snapshot {
            kind: artifact::EntryKind::DocumentOriginal {
                company: "synthetic".into(),
            },
            path: format!("synthetic/{pdf_path}"),
            bytes: pdf,
        },
    ]
}
fn signed(
    old: &artifact::VerifiedArtifact,
    db: &Connection,
    extra: Vec<artifact::Snapshot>,
) -> artifact::VerifiedArtifact {
    let mut entries = old
        .entries()
        .iter()
        .map(|e| artifact::Snapshot {
            kind: e.kind().clone(),
            path: e.path().into(),
            bytes: if matches!(e.kind(), artifact::EntryKind::CompanyLedger { .. }) {
                db.serialize(MAIN_DB).unwrap().to_vec()
            } else {
                e.bytes().to_vec()
            },
        })
        .collect::<Vec<_>>();
    entries.extend(extra);
    // Published RFC8032 test vector; never operator signing material.
    let hex = concat!(
        "3051020101300506032b657004220420",
        "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
        "812100",
        "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
    );
    let der = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    let key = artifact::SigningKey::from_pkcs8(&der).unwrap();
    let trusted = artifact::TrustedPublicKey::from_bytes(&key.public_key_bytes()).unwrap();
    let bytes = artifact::create(old.manifest().context.clone(), entries, &key).unwrap();
    artifact::verify(&bytes, &trusted).unwrap()
}
fn histories(db: &Connection) -> Vec<Vec<String>> {
    let mut out = vec![];
    for table in [
        "recurring_invoice_templates",
        "recurring_invoice_generations",
        "recurring_invoice_generation_claims",
        "recurring_invoice_delivery_events",
    ] {
        let mut q = db
            .prepare(&format!("SELECT * FROM {table} ORDER BY id"))
            .unwrap();
        let count = q.column_count();
        let rows = q
            .query_map([], |r| {
                Ok((0..count)
                    .map(|i| format!("{:?}", r.get_ref(i).unwrap()))
                    .collect::<Vec<_>>())
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        out.extend(rows);
    }
    out
}
#[test]
fn signed_original_recurring_dryrun_and_uncertain_rows_survive_restore_rebackup_and_reopen() {
    let old = tests::signed(false, false, false, false, false, None);
    let mut source = ledger(&old);
    let mut originals = add_case(&mut source, "email");
    originals.extend(add_case(&mut source, "digisense"));
    rentemester_storage::recurring_workspace::validate_history(&source).unwrap();
    let before = histories(&source);
    let archive = signed(&old, &source, originals);
    drop(source);
    validate(&archive).unwrap();
    let mut restored = ledger(&archive);
    store::restore_boundary(
        &mut restored,
        &rentemester_runtime::backup_keys::digest(archive.manifest_bytes()),
    )
    .unwrap();
    assert_eq!(histories(&restored), before);
    assert_eq!(
        restored
            .query_row("SELECT count(*) FROM peppol_submissions", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        restored
            .query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let second = signed(&archive, &restored, vec![]);
    validate(&second).unwrap();
    drop(restored);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("restored.sqlite");
    let mut reopened = ledger(&second);
    store::restore_boundary(
        &mut reopened,
        &rentemester_runtime::backup_keys::digest(second.manifest_bytes()),
    )
    .unwrap();
    std::fs::write(&path, reopened.serialize(MAIN_DB).unwrap().to_vec()).unwrap();
    drop(reopened);
    let disk = Connection::open(&path).unwrap();
    assert_eq!(histories(&disk), before);
    store::company(&disk).unwrap();
    let mut control = Connection::open_in_memory().unwrap();
    restore_business(
        &mut control,
        &second,
        &rentemester_runtime::backup_keys::digest(second.manifest_bytes()),
    )
    .unwrap();
    for table in [
        "user",
        "session",
        "twoFactor",
        "rm_workspace_user_access_events",
        "rm_company_membership_events",
    ] {
        assert_eq!(
            control
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
#[test]
fn even_valid_signature_cannot_adopt_missing_original_recurring_provenance() {
    let old = tests::signed(false, false, false, false, false, None);
    let mut db = ledger(&old);
    let originals = add_case(&mut db, "email");
    let triggers = db
        .prepare("SELECT name,sql FROM sqlite_schema WHERE type='trigger' AND tbl_name='audit_log'")
        .unwrap()
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    for (name, _) in &triggers {
        db.execute_batch(&format!("DROP TRIGGER {name}")).unwrap();
    }
    db.execute(
        "DELETE FROM audit_log WHERE event_type='original_recurring_delivery_event'",
        [],
    )
    .unwrap();
    for (_, sql) in triggers {
        db.execute_batch(&sql).unwrap();
    }
    let tampered = signed(&old, &db, originals);
    assert!(validate(&tampered).is_err());
}
