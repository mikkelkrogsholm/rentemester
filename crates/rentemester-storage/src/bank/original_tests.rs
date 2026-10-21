use super::*;
use rusqlite::{Connection, TransactionBehavior};
const CSV: &str = "transaction_date,text,amount,balance_after\n2026-04-01,Synthetic first,10,100\n2026-04-02,Synthetic gap,5,999\n";
fn fixture() -> Connection {
    let mut control = Connection::open_in_memory().unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic original bank import")
        .unwrap();
    db
}
fn imported(
    tx: &Transaction<'_>,
    csv: &str,
    account: Option<&str>,
    original: bool,
) -> Result<Value, Error> {
    let rows = rentemester_engine::bank::parse(csv)?;
    if original {
        import_original_bytes_with_principal(
            tx,
            csv.as_bytes(),
            &rows,
            account,
            "synthetic",
            "2026-04-03T00:00:00Z",
            "bank/processed/synthetic.txt",
            crate::idempotency::PrincipalKind::User,
        )
    } else {
        import_bytes(
            tx,
            csv.as_bytes(),
            &rows,
            account,
            "synthetic",
            "2026-04-03T00:00:00Z",
            "bank/processed/synthetic.txt",
        )
    }
}
#[test]
fn original_import_receipt_counts_unmatched_only_omits_nulls_and_keeps_native_receipt() {
    let mut legacy = fixture();
    let mut native = fixture();
    let tx = legacy
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let original = imported(&tx, CSV, None, true).unwrap();
    assert_eq!(original["imported"], 2);
    assert_eq!(original["exceptionsCreated"], 2);
    assert_eq!(original["balanceWarnings"].as_array().unwrap().len(), 1);
    assert_eq!(original["firstBalance"], 100.0);
    assert_eq!(original["lastBalance"], 999.0);
    for field in ["bankAccountId", "bankAccountSlug", "profile"] {
        assert!(original.get(field).is_none());
    }
    assert_eq!(
        tx.query_row::<i64, _, _>(
            "SELECT count(*) FROM exceptions WHERE type='UNMATCHED_BANK_TRANSACTION'",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(
        tx.query_row::<i64, _, _>(
            "SELECT count(*) FROM exceptions WHERE type='BANK_BALANCE_GAP'",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        tx.query_row::<i64, _, _>(
            "SELECT count(*) FROM bank_transactions WHERE bank_account_id IS NULL",
            [],
            |r| r.get(0)
        )
        .unwrap(),
        2
    );
    let repeat = imported(&tx, CSV, Some("  "), true).unwrap();
    assert_eq!(repeat["imported"], 0);
    assert_eq!(repeat["skippedDuplicates"], 2);
    assert_eq!(repeat["exceptionsCreated"], 0);
    assert_eq!(repeat["skippedDuplicateRows"].as_array().unwrap().len(), 2);
    let nt = native
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let mut native = imported(&nt, CSV, None, false).unwrap();
    assert_eq!(native["exceptionsCreated"], 3);
    for field in ["bankAccountId", "bankAccountSlug", "profile"] {
        assert!(native[field].is_null());
        native.as_object_mut().unwrap().remove(field);
    }
    native["exceptionsCreated"] = json!(2);
    assert_eq!(native, original);
    assert_eq!(
        tx.query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
            .unwrap(),
        0
    );
}
#[test]
fn original_import_selector_is_trimmed_exact_id_or_slug_and_source_conflicts_stay_typed() {
    let mut db = fixture();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("INSERT INTO bank_accounts(id,slug,name,currency)VALUES(1,'synthetic','Human account name','DKK'),(2,'inactive','Inactive','DKK');UPDATE bank_accounts SET active=0 WHERE id=2;").unwrap();
    let first = imported(&tx, CSV, Some(" synthetic "), true).unwrap();
    assert_eq!(first["bankAccountId"], 1);
    assert_eq!(first["bankAccountSlug"], "synthetic");
    assert_eq!(
        imported(&tx, CSV, Some(" 1 "), true).unwrap()["skippedDuplicates"],
        2
    );
    for selector in ["Human account name", "missing", "-1"] {
        assert_eq!(
            validate_original_import_account(&tx, Some(selector))
                .unwrap_err()
                .code,
            "bank_import_account_not_found"
        );
    }
    assert_eq!(
        validate_original_import_account(&tx, Some(" inactive "))
            .unwrap_err()
            .code,
        "bank_import_account_inactive"
    );
    let changed = CSV.replace(",100\n", ",101\n");
    assert_eq!(
        imported(&tx, &changed, Some("synthetic"), true)
            .unwrap_err()
            .code,
        "bank_refused"
    );
    assert_eq!(
        tx.query_row::<i64, _, _>("SELECT count(*) FROM bank_transactions", [], |r| r.get(0))
            .unwrap(),
        2
    );
}
