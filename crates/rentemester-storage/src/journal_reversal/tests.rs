use super::*;
use rusqlite::{Connection, TransactionBehavior};
#[test]
fn real_append_only_reversal_and_single_use() {
    let mut db = Connection::open_in_memory().unwrap();
    crate::company_baseline::rehearse(&mut db).unwrap();
    db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('2000','Synthetic bank','asset','debit'),('5000','Synthetic equity','equity','credit')").unwrap();
    let request:ManualJournal=serde_json::from_value(json!({"transactionDate":"2026-01-01","text":"Synthetic capital","currency":"DKK","lines":[{"accountNo":"2000","debitAmount":"10.00","text":"Original"},{"accountNo":"5000","creditAmount":"10.00"}]})).unwrap();
    let version = crate::journal_preview::manual_journal_rule_version().unwrap();
    let latest = Date::parse("2026-02-15").unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    journal_post::post(
        &tx,
        &request,
        &version,
        latest,
        Actor {
            subject: "user:synthetic",
            program: "rentemester-rust",
        },
    )
    .unwrap();
    tx.commit().unwrap();
    let input = Input {
        entry_id: JournalEntryId::from_sqlite(1),
        transaction_date: Date::parse("2026-01-02").unwrap(),
        reason: "  correction  ".to_owned(),
    };
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let result = reverse(
        &tx,
        &input,
        &version,
        latest,
        Actor {
            subject: "user:synthetic",
            program: "rentemester-rust",
        },
    )
    .unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(result["originalEntryId"], 1);
    assert_eq!(result["entryNo"], "2026-00002");
    tx.commit().unwrap();
    assert!(crate::journal_hash::verify(&db).unwrap().ok);
    assert_eq!(
        db.query_row("SELECT status FROM journal_entries WHERE id=1", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        "posted"
    );
    assert_eq!(
        db.query_row(
            "SELECT debit_amount FROM journal_lines WHERE journal_entry_id=2 ORDER BY id LIMIT 1",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert_eq!(
        reverse(
            &tx,
            &input,
            &version,
            latest,
            Actor {
                subject: "user:synthetic",
                program: "rentemester-rust"
            }
        )
        .unwrap()["ok"],
        false
    );
    tx.rollback().unwrap();
}

#[test]
#[ignore = "canonical synthetic reversal crash/fault process helper"]
fn child() {
    let config: Value = serde_json::from_str(&std::env::var("RM_POST_REQUEST").unwrap()).unwrap();
    let mut db = Connection::open(std::env::var("RM_POST_COMPANY").unwrap()).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL")
        .unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let request: Input = serde_json::from_value(config["input"]["request"].clone()).unwrap();
    let result = reverse(
        &tx,
        &request,
        config["input"]["expectedRuleVersion"].as_str().unwrap(),
        Date::parse("2026-02-15").unwrap(),
        Actor {
            subject: "user:u",
            program: "rentemester-rust",
        },
    );
    let output = match result {
        Ok(value) if value["ok"] == true => {
            tx.commit().unwrap();
            json!({"outcome":"committed","result":value})
        }
        Ok(value) => {
            tx.rollback().unwrap();
            json!({"outcome":"not_committed","business":value})
        }
        Err(error) => {
            tx.rollback().unwrap();
            json!({"outcome":"not_committed","code":error.code})
        }
    };
    std::fs::write(
        std::env::var("RM_POST_OUTPUT").unwrap(),
        serde_json::to_vec(&output).unwrap(),
    )
    .unwrap();
}

#[test]
fn request_cannot_supply_original_evidence_or_policy() {
    let value = json!({"entryId":"1","transactionDate":"2026-01-01","reason":"Synthetic"});
    for field in [
        "historicalImport",
        "allowUncodedVatControl",
        "createdBy",
        "createdByProgram",
        "lines",
        "documentId",
        "status",
        "repairAuthorization",
    ] {
        let mut attempt = value.clone();
        attempt[field] = json!(true);
        assert!(serde_json::from_value::<Input>(attempt).is_err(), "{field}");
    }
    let mut db = Connection::open_in_memory().unwrap();
    crate::company_baseline::rehearse(&mut db).unwrap();
    let version = crate::journal_preview::manual_journal_rule_version().unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    for id in ["0", "-1", "9007199254740992"] {
        let mut attempt = value.clone();
        attempt["entryId"] = json!(id);
        let attempt: Input = serde_json::from_value(attempt).unwrap();
        assert_eq!(
            reverse(
                &tx,
                &attempt,
                &version,
                Date::parse("2026-01-01").unwrap(),
                Actor {
                    subject: "user:synthetic",
                    program: "rentemester-rust"
                }
            )
            .unwrap_err()
            .code,
            "invalid_identifier"
        );
    }
    tx.rollback().unwrap();
}
