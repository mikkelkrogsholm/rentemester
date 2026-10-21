use super::*;
use crate::journal_post::{self, Actor};
use rentemester_engine::{identifiers::JournalEntryId, manual_journal::ManualJournal};
use rusqlite::{Connection, TransactionBehavior};

fn fixture() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    crate::company_baseline::rehearse(&mut db).unwrap();
    db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('2000','Synthetic bank','asset','debit'),('5000','Synthetic equity','equity','credit');").unwrap();
    db
}
fn actor() -> Actor<'static> {
    Actor {
        subject: "user:synthetic",
        program: "rentemester-rust",
    }
}
#[test]
fn real_post_reversal_keeps_stored_status_null_headers_and_exact_human_amount() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let request: ManualJournal = serde_json::from_value(json!({"transactionDate":"2025-12-31","text":"Synthetic capital","currency":"DKK","lines":[{"accountNo":"2000","debitAmount":"10.01"},{"accountNo":"5000","creditAmount":"10.01"}]})).unwrap();
    let version = crate::manual_journal_rule_version().unwrap();
    let latest = Date::parse("2026-02-15").unwrap();
    assert_eq!(
        journal_post::post(&tx, &request, &version, latest, actor()).unwrap()["ok"],
        true
    );
    assert_eq!(
        crate::journal_reversal::reverse(
            &tx,
            &crate::journal_reversal::Input {
                entry_id: JournalEntryId::from_sqlite(1),
                transaction_date: Date::parse("2026-01-01").unwrap(),
                reason: "Synthetic correction".into()
            },
            &version,
            latest,
            actor()
        )
        .unwrap()["ok"],
        true
    );
    let changes = tx.total_changes();
    let page = read(&tx, &Query::default()).unwrap();
    assert_eq!(page["total"], 2);
    assert_eq!(page["entries"][0]["id"], 2);
    assert_eq!(page["entries"][0]["status"], "reversed");
    assert_eq!(page["entries"][0]["reversalOfEntryId"], 1);
    assert_eq!(page["entries"][1]["status"], "posted");
    assert_eq!(page["entries"][1]["transactionDate"], "2025-12-31");
    for entry in page["entries"].as_array().unwrap() {
        assert_eq!(entry.as_object().unwrap().len(), 12);
        for field in [
            "amountForeign",
            "amountDkk",
            "fxRateToDkk",
            "documentId",
            "sourceBankTransactionId",
        ] {
            assert!(entry[field].is_null(), "{field}");
        }
    }
    let all = read(
        &tx,
        &Query {
            all: true,
            ..Query::default()
        },
    )
    .unwrap();
    assert_eq!(all["limit"], 2);
    for entry in all["entries"].as_array().unwrap() {
        assert_eq!(entry["bookedDebitTotal"], 10.01);
    }
    let only = read(
        &tx,
        &Query {
            status: Some("reversed".into()),
            from: Some("2026-01-01".into()),
            to: Some("2026-01-01".into()),
            ..Query::default()
        },
    )
    .unwrap();
    assert_eq!(only["total"], 1);
    assert_eq!(only["entries"][0], page["entries"][0]);
    assert_eq!(tx.total_changes(), changes);
    tx.rollback().unwrap();
    let ddl: String = db
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE name='journal_entries_no_update'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    // Recreate the canonical guard after deliberate retained hash corruption.
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    journal_post::post(&tx, &request, &version, latest, actor()).unwrap();
    tx.commit().unwrap();
    db.execute_batch("DROP TRIGGER journal_entries_no_update; UPDATE journal_entries SET entry_hash='corrupt' WHERE id=1;").unwrap();
    db.execute_batch(&ddl).unwrap();
    let tx = db.transaction().unwrap();
    assert_eq!(
        read(&tx, &Query::default()).unwrap_err().code,
        "journal_history_unavailable"
    );
}

#[test]
fn dates_status_and_page_contract_are_precise_and_reversed_ranges_are_empty() {
    for query in [
        Query {
            from: Some("2026-02-30".into()),
            ..Query::default()
        },
        Query {
            status: Some("active".into()),
            ..Query::default()
        },
        Query {
            limit: Some(0),
            ..Query::default()
        },
        Query {
            limit: Some(5001),
            ..Query::default()
        },
        Query {
            offset: Some(SAFE + 1),
            ..Query::default()
        },
        Query {
            all: true,
            limit: Some(1),
            ..Query::default()
        },
        Query {
            all: true,
            offset: Some(0),
            ..Query::default()
        },
    ] {
        assert!(query.validate().is_err());
    }
    let mut db = fixture();
    let tx = db.transaction().unwrap();
    for query in [
        Query::default(),
        Query {
            all: true,
            ..Query::default()
        },
        Query {
            from: Some("2026-02-02".into()),
            to: Some("2026-01-01".into()),
            ..Query::default()
        },
        Query {
            offset: Some(SAFE),
            ..Query::default()
        },
    ] {
        let page = read(&tx, &query).unwrap();
        assert_eq!(page["count"], 0);
        assert_eq!(page["total"], 0);
        assert_eq!(page["hasMore"], false);
        assert!(page.get("nextOffset").is_none());
        if query.all {
            assert_eq!(page["limit"], 0);
        }
    }
}

#[test]
fn one_snapshot_all_mode_never_silently_truncates_at_default_page() {
    // Synthetic canonical headers use the real hash encoder. The writer itself
    // is exercised separately above; avoid 501 repeated posting admissions.
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let mut previous = "GENESIS".to_owned();
    for id in 1..=501 {
        let entry = crate::journal_hash::Entry {
            id,
            entry_no: format!("SYN-{id}"),
            transaction_date: if id % 2 == 0 {
                "2025-12-31"
            } else {
                "2026-01-01"
            }
            .into(),
            text: format!("Synthetic {id}"),
            source_bank_transaction_id: None,
            document_id: None,
            currency: "DKK".into(),
            amount_foreign: None,
            amount_dkk: None,
            fx_rate_to_dkk: None,
            rule_version: "synthetic".into(),
            created_by: "user:synthetic".into(),
            created_by_program: "synthetic".into(),
            status: "posted".into(),
            reversal_of_entry_id: None,
            lines: vec![
                crate::journal_hash::Line {
                    ordinal: 0,
                    account_no: "2000".into(),
                    debit_amount: crate::journal_hash::json_number(1.0).unwrap(),
                    credit_amount: crate::journal_hash::json_number(0.0).unwrap(),
                    vat_code: None,
                    text: None,
                },
                crate::journal_hash::Line {
                    ordinal: 1,
                    account_no: "5000".into(),
                    debit_amount: crate::journal_hash::json_number(0.0).unwrap(),
                    credit_amount: crate::journal_hash::json_number(1.0).unwrap(),
                    vat_code: None,
                    text: None,
                },
            ],
        };
        let hash = crate::journal_hash::canonical_hash(&entry, &previous).unwrap();
        tx.execute("INSERT INTO journal_entries(id,entry_no,transaction_date,text,rule_version,created_by,created_by_program,previous_hash,entry_hash) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![id,entry.entry_no,entry.transaction_date,entry.text,entry.rule_version,entry.created_by,entry.created_by_program,previous,hash]).unwrap();
        tx.execute_batch(&format!("INSERT INTO journal_lines(journal_entry_id,account_id,debit_amount,credit_amount)VALUES({id},1,1,0),({id},2,0,1)")).unwrap();
        previous = hash;
    }
    let changes = tx.total_changes();
    let first = read(&tx, &Query::default()).unwrap();
    assert_eq!(first["count"], 500);
    assert_eq!(first["total"], 501);
    assert_eq!(first["nextOffset"], 500);
    assert_eq!(first["entries"][0]["id"], 501);
    let last = read(
        &tx,
        &Query {
            offset: Some(500),
            ..Query::default()
        },
    )
    .unwrap();
    assert_eq!(last["count"], 1);
    assert_eq!(last["entries"][0]["id"], 1);
    assert_eq!(last["hasMore"], false);
    assert!(last.get("nextOffset").is_none());
    let all = read(
        &tx,
        &Query {
            all: true,
            ..Query::default()
        },
    )
    .unwrap();
    assert_eq!(all["count"], 501);
    assert_eq!(all["limit"], 501);
    assert_eq!(all["total"], 501);
    assert_eq!(all["offset"], 0);
    assert_eq!(all["hasMore"], false);
    let reversed = read(
        &tx,
        &Query {
            from: Some("2026-01-02".into()),
            to: Some("2025-12-30".into()),
            ..Query::default()
        },
    )
    .unwrap();
    assert_eq!(reversed["total"], 0);
    let dated = read(
        &tx,
        &Query {
            from: Some("2025-12-31".into()),
            to: Some("2025-12-31".into()),
            limit: Some(1),
            offset: Some(1),
            ..Query::default()
        },
    )
    .unwrap();
    assert_eq!(dated["total"], 250);
    assert_eq!(dated["entries"][0]["id"], 498);
    assert_eq!(tx.total_changes(), changes);
}
