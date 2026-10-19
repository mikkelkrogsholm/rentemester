//! Read-only journal hash-chain scope. No backup/compliance claim without trusted evidence.
use rentemester_engine::Error;
use rusqlite::Transaction;
use serde_json::{Value, json};
fn unavailable(_: rusqlite::Error) -> Error {
    Error::new(
        "integrity_storage_unavailable",
        "company integrity metadata unavailable",
    )
}
pub fn read(tx: &Transaction<'_>, slug: &str) -> Result<Value, Error> {
    let company=tx.query_row("SELECT name,cvr,country,currency FROM companies WHERE id=1",[],|r|Ok(json!({"name":r.get::<_,String>(0)?,"cvr":r.get::<_,Option<String>>(1)?,"country":r.get::<_,String>(2)?,"currency":r.get::<_,String>(3)?}))).map_err(unavailable)?;
    let scope = "journal_hash_chain";
    let note = "Kontrollen sammenligner posteringernes gemte hashværdier med deres indhold og kæde. Den omfatter ikke fuld revisionsintegritet, bilagsfiler, bankafstemning eller backup.";
    let audit = match crate::journal_hash::verify(tx) {
        Ok(report) => {
            json!({"available":true,"ok":report.ok,"entries":report.entries_checked,"errors":report.errors,"scope":scope,"scopeNote":note})
        }
        Err(_) => {
            json!({"available":false,"ok":null,"entries":null,"errors":[],"scope":scope,"scopeNote":note,"unavailableReason":"Hashkontrollen kunne ikke gennemføres for de aktuelle data. Det er ikke i sig selv tegn på en brudt kæde."})
        }
    };
    Ok(json!({"slug":slug,"company":company,"auditChain":audit,
        "backup":{"available":false,"ok":null,"latestBackupAt":null,"latestBackupId":null,"backupDue":null,"hasActivitySinceBackup":null,"daysSinceLatestBackup":null,"backupsFound":null,"requiredBy":null,"checkedAt":null,"unavailableReason":"Denne Rust-visning kan endnu ikke læse og verificere backup-manifester og placeringsbeviser i det beskyttede workspace. Backup, forfald og compliance er derfor ukendte."},
        "destinations":[],"destinationsAvailable":false,"destinationsUnavailableReason":"Konfigurerede backup-destinationer er ikke kontrolleret af denne backend.",
        "legalCitation":{"sourceId":"DK-BOGFORINGSLOVEN-2022-700","note":"Denne kontrol er ikke en vurdering af, om virksomheden opfylder kravene til sikker opbevaring og backup."}}))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> rusqlite::Connection {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        let mut control = rusqlite::Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic").unwrap();
        db
    }
    #[test]
    fn empty_chain_is_measured_while_unread_backup_evidence_stays_unknown() {
        let mut db = fixture();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let result = read(&tx, "synthetic").unwrap();
        assert_eq!(result["auditChain"]["ok"], true);
        assert_eq!(result["auditChain"]["entries"], 0);
        assert_eq!(result["auditChain"]["scope"], "journal_hash_chain");
        for field in [
            "ok",
            "backupsFound",
            "backupDue",
            "hasActivitySinceBackup",
            "checkedAt",
        ] {
            assert!(result["backup"][field].is_null());
        }
        assert_eq!(result["destinationsAvailable"], false);
    }
    #[test]
    fn unsupported_hash_storage_is_unavailable_not_verified_corruption() {
        let mut db = fixture();
        db.execute_batch("DROP TABLE journal_lines").unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let result = read(&tx, "synthetic").unwrap();
        assert_eq!(result["auditChain"]["available"], false);
        assert!(result["auditChain"]["ok"].is_null());
        assert_eq!(result["auditChain"]["errors"], json!([]));
    }
    #[test]
    fn actual_post_hash_corruption_is_reported_using_existing_verifier() {
        let mut db = fixture();
        db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('1000','Synthetic bank','asset','debit'),('2000','Synthetic equity','equity','credit')").unwrap();
        let request=serde_json::from_value(json!({"transactionDate":"2026-01-01","text":"Synthetic posting","currency":"DKK","lines":[{"accountNo":"1000","debitAmount":"10.00"},{"accountNo":"2000","creditAmount":"10.00"}]})).unwrap();
        let version = crate::journal_preview::manual_journal_rule_version().unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        crate::journal_post::post(
            &tx,
            &request,
            &version,
            rentemester_engine::fiscal_year::Date::parse("2026-02-01").unwrap(),
            crate::journal_post::Actor {
                subject: "user:synthetic",
                program: "synthetic",
            },
        )
        .unwrap();
        tx.commit().unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        assert_eq!(read(&tx, "synthetic").unwrap()["auditChain"]["ok"], true);
        tx.rollback().unwrap();
        db.execute_batch("DROP TRIGGER journal_entries_no_update;UPDATE journal_entries SET entry_hash='corrupt' WHERE id=1").unwrap();
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let result = read(&tx, "synthetic").unwrap();
        assert_eq!(result["auditChain"]["available"], true);
        assert_eq!(result["auditChain"]["ok"], false);
        assert_eq!(result["auditChain"]["entries"], 1);
        assert!(
            result["auditChain"]["errors"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v.as_str().unwrap().contains("entry_hash mismatch"))
        );
    }
}
