//! Signed, source-free disposal history and durable receipt restore.
use super::*;
use rentemester_engine::{fiscal_year::Date, manual_journal::ManualJournal};
use rentemester_storage::{asset_disposal as disposal, assets, idempotency::PrincipalKind};
use serde_json::json;
const TODAY: &str = "2026-10-23";
const NOW: i64 = 1_792_713_600_000;
const ORIGINAL: &[u8] = b"Synthetic capitalized asset original";
fn day() -> Date {
    Date::parse(TODAY).unwrap()
}
fn seed(db: &mut Connection) -> (i64, i64) {
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('A','Synthetic equipment','asset','debit'),('D','Synthetic depreciation','expense','debit'),('AD','Synthetic accumulated','asset','credit'),('L','Synthetic loss','expense','debit'),('C','Synthetic payable','liability','credit');").unwrap();
    let meta=serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","issueDate":"2026-07-01","invoiceNo":"SYN-DISPOSAL-1","deliveryDescription":"Synthetic equipment","amountIncVat":1000,"vatAmount":0,"sender":{"name":"Synthetic supplier","address":"Synthetic2","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic company","address":"Synthetic1","vatOrCvr":"DK87654321"}})).unwrap();
    let doc = rentemester_storage::documents::register(
        &tx,
        rentemester_storage::documents::Original {
            filename: "synthetic.txt",
            path: &format!(
                "documents/originals/{}.txt",
                rentemester_runtime::backup_keys::digest(ORIGINAL)
            ),
            hash: &rentemester_runtime::backup_keys::digest(ORIGINAL),
            mime: "text/plain",
        },
        &meta,
        "synthetic",
        TODAY,
    )
    .unwrap()["document"]["id"]
        .as_i64()
        .unwrap();
    let acquisition:ManualJournal=serde_json::from_value(json!({"transactionDate":"2026-07-01","text":"Synthetic capitalized purchase","currency":"DKK","documentId":doc.to_string(),"lines":[{"accountNo":"A","debitAmount":"1000.00"},{"accountNo":"C","creditAmount":"1000.00"}]})).unwrap();
    let receipt = rentemester_storage::journal_post::post(
        &tx,
        &acquisition,
        &rentemester_storage::journal_preview::manual_journal_rule_version().unwrap(),
        day(),
        rentemester_storage::journal_post::Actor {
            subject: "user:synthetic",
            program: "synthetic",
        },
    )
    .unwrap();
    assert_eq!(receipt["ok"], true, "{receipt}");
    let input=serde_json::from_value(json!({"name":"Synthetic equipment","category":"equipment","acquisitionDate":"2026-07-01","cost":1000,"usefulLifeMonths":3,"purchaseDocumentId":doc,"assetAccountNo":"A","depreciationExpenseAccountNo":"D","accumulatedDepreciationAccountNo":"AD","confirm":true})).unwrap();
    let id = assets::register(&tx, &input, "synthetic", day()).unwrap()["assetId"]
        .as_i64()
        .unwrap();
    tx.commit().unwrap();
    (id, doc)
}
fn sign(old: &artifact::VerifiedArtifact, db: &Connection) -> artifact::VerifiedArtifact {
    let mut entries: Vec<_> = old
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
        .collect();
    entries.push(artifact::Snapshot {
        kind: artifact::EntryKind::DocumentOriginal {
            company: "synthetic".into(),
        },
        path: format!(
            "synthetic/documents/originals/{}.txt",
            rentemester_runtime::backup_keys::digest(ORIGINAL)
        ),
        bytes: ORIGINAL.to_vec(),
    });
    tests::verified(old.manifest().context.clone(), entries)
}
fn ledger(a: &artifact::VerifiedArtifact) -> Connection {
    let bytes = a
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
#[test]
fn asset_disposal_signed_source_free_restore_retains_history_receipt_and_original() {
    let old = tests::signed(false, false, false, false, false, None);
    let mut source = ledger(&old);
    let (id, doc) = seed(&mut source);
    let tx = source
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let input = assets::Depreciate {
        transaction_date: Some("2026-10-20".into()),
        period_index: Some(1),
        confirm: true,
    };
    assets::depreciate(&tx, id, &input, "historical-owner", TODAY, day()).unwrap();
    let p:disposal::Preview=serde_json::from_value(json!({"transactionDate":TODAY,"documentId":doc,"reason":"Synthetic retirement","lossAccountNo":"L"})).unwrap();
    let plan = disposal::preview(&tx, id, &p, day()).unwrap();
    let apply:disposal::Dispose=serde_json::from_value(json!({"transactionDate":TODAY,"documentId":doc,"reason":"Synthetic retirement","lossAccountNo":"L","expectedPlanHash":plan["disposalPreview"]["planHash"],"idempotencyKey":"signed-disposal-key","confirm":true})).unwrap();
    let accepted = disposal::dispose(
        &tx,
        id,
        &apply,
        "historical-owner",
        PrincipalKind::User,
        NOW,
        day(),
    )
    .unwrap();
    tx.commit().unwrap();
    let archive = sign(&old, &source);
    drop(source);
    validate(&archive).unwrap();
    let digest = rentemester_runtime::backup_keys::digest(archive.manifest_bytes());
    let mut restored = ledger(&archive);
    store::restore_boundary(&mut restored, &digest).unwrap();
    store::company(&restored).unwrap();
    let tx = restored
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert_eq!(
        disposal::record(&tx, id).unwrap().unwrap(),
        accepted["disposal"]
    );
    assert_eq!(
        assets::list(&tx, "synthetic").unwrap()["totals"]["netBookValue"],
        0.0
    );
    assert!(
        disposal::receipt(
            &tx,
            id,
            "signed-disposal-key",
            "fresh-owner",
            PrincipalKind::User
        )
        .is_err()
    );
    assert_eq!(
        rentemester_storage::receipt_reconciliation::inventory(&tx).unwrap()["rows"][0]["operation"],
        "asset_dispose"
    );
    tx.rollback().unwrap();
    let again = sign(&old, &restored);
    drop(restored);
    validate(&again).unwrap();
    let reopened = ledger(&again);
    assert_eq!(
        disposal::record(&reopened, id).unwrap().unwrap()["journalEntryHash"],
        accepted["disposal"]["journalEntryHash"]
    );
}
