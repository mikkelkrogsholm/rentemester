use super::*;
#[test]
fn mileage_posting_schema_rollback_and_partial_refusal() {
    let mut control = rusqlite::Connection::open_in_memory().unwrap();
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic Mileage Posting")
        .unwrap();
    let before = crate::migration::schema(&db).unwrap();
    {
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        schema::install(&tx).unwrap();
        crate::journal_preview::require_current(&tx).unwrap();
        tx.rollback().unwrap();
    }
    assert_eq!(crate::migration::schema(&db).unwrap(), before);
    db.execute_batch("CREATE TABLE native_mileage_posting_foreign(id INTEGER)")
        .unwrap();
    assert!(crate::journal_preview::require_current(&db).is_err());
}
fn setup(db: &mut rusqlite::Connection) {
    let mut control = rusqlite::Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, db, "Synthetic Mileage Posting").unwrap();
    db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('ME','Mileage expense','expense','debit'),('ML','Mileage liability','liability','credit'); INSERT INTO documents(id,source,original_filename,stored_path,mime_type,sha256_hash) VALUES(1,'manual','claim.txt','claim.txt','text/plain','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa');").unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let input:crate::mileage::Register=serde_json::from_value(json!({"tripDate":"2026-01-01","purpose":"Synthetic visit","fromLocation":"Origin","toLocation":"Destination","kilometers":12.35,"vehicle":"Synthetic car","driver":"Synthetic driver","ratePerKm":2,"rateBasis":"Explicit synthetic basis","confirm":true})).unwrap();
    crate::mileage::register(&tx, &input, "owner").unwrap();
    tx.commit().unwrap();
}
fn config() -> Configure {
    serde_json::from_value(json!({"version":"synthetic-v1","validFrom":"2026-01-01","validTo":"2026-12-31","vehicle":"Synthetic car","ratePerKm":"2.00","basis":"Explicit synthetic basis","source":"Synthetic policy only","expenseAccountNo":"ME","liabilityAccountNo":"ML","confirm":true})).unwrap()
}
fn request() -> Preview {
    Preview {
        mileage_entry_id: 1,
        rate_version: "synthetic-v1".into(),
        document_id: 1,
    }
}
fn latest() -> Date {
    Date::parse("2026-10-23").unwrap()
}
const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
fn count(tx: &Transaction<'_>, table: &str) -> i64 {
    tx.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
#[test]
fn mileage_posting_canonical_journal_exact_replay_key_principal_restart() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("company.sqlite");
    let mut db = rusqlite::Connection::open(&file).unwrap();
    setup(&mut db);
    let receipt;
    let input;
    {
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        assert_eq!(rate_versions(&tx).unwrap(), json!({"rateVersions":[]}));
        assert_eq!(postings(&tx, latest()).unwrap(), json!({"postings":[]}));
        let cfg = configure(&tx, &config(), "owner", PrincipalKind::User).unwrap();
        assert_eq!(cfg["replayed"], false);
        assert_eq!(
            configure(&tx, &config(), "other", PrincipalKind::User).unwrap()["replayed"],
            true
        );
        let mut wrong = config();
        wrong.rate_per_km = "3.00".into();
        assert!(configure(&tx, &wrong, "owner", PrincipalKind::User).is_err());
        let before = count(&tx, "audit_log");
        let p = preview(&tx, &request(), HASH, latest()).unwrap();
        assert_eq!(p["amount"], 24.7);
        assert_eq!(p["entryNo"], "MIL-2026-000001");
        assert_eq!(count(&tx, "audit_log"), before);
        assert_eq!(count(&tx, "journal_entries"), 0);
        input = Post {
            mileage_entry_id: 1,
            rate_version: "synthetic-v1".into(),
            document_id: 1,
            expected_preview_hash: p["previewHash"].as_str().unwrap().into(),
            operation_key: "synthetic-operation".into(),
            confirm: true,
        };
        let mut stale: Post =
            serde_json::from_value(serde_json::to_value(&input).unwrap()).unwrap();
        stale.expected_preview_hash = "0".repeat(64);
        assert!(post(&tx, &stale, "owner", PrincipalKind::User, HASH, latest()).is_err());
        assert_eq!(count(&tx, "journal_entries"), 0);
        assert!(preview(&tx, &request(), &"b".repeat(64), latest()).is_err());
        receipt = post(&tx, &input, "owner", PrincipalKind::User, HASH, latest()).unwrap();
        assert_eq!(receipt["replayed"], false);
        assert_eq!(count(&tx, "journal_entries"), 1);
        assert_eq!(count(&tx, "journal_lines"), 2);
        assert_eq!(count(&tx, "audit_log"), before + 1);
        let lines:Vec<(String,f64,f64,Option<String>)>=tx.prepare("SELECT a.account_no,l.debit_amount,l.credit_amount,l.vat_code FROM journal_lines l JOIN accounts a ON a.id=l.account_id ORDER BY l.id").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap().collect::<Result<_,_>>().unwrap();
        assert_eq!(
            lines,
            vec![
                ("ME".into(), 24.7, 0.0, None),
                ("ML".into(), 0.0, 24.7, None)
            ]
        );
        assert!(post(&tx, &input, "other", PrincipalKind::User, HASH, latest()).is_err());
        assert!(
            post(
                &tx,
                &input,
                "owner",
                PrincipalKind::ServiceAccount,
                HASH,
                latest()
            )
            .is_err()
        );
        let mut other: Post =
            serde_json::from_value(serde_json::to_value(&input).unwrap()).unwrap();
        other.operation_key = "other-key".into();
        assert!(post(&tx, &other, "owner", PrincipalKind::User, HASH, latest()).is_err());
        let replay = post(&tx, &input, "owner", PrincipalKind::User, HASH, latest()).unwrap();
        let mut expected = receipt.clone();
        expected["replayed"] = json!(true);
        assert_eq!(replay, expected);
        assert_eq!(
            postings(&tx, latest()).unwrap()["postings"],
            json!([receipt.clone()])
        );
        tx.commit().unwrap();
    }
    drop(db);
    let mut db = rusqlite::Connection::open(&file).unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    assert_eq!(
        postings(&tx, latest()).unwrap()["postings"],
        json!([receipt])
    );
    assert_eq!(
        post(&tx, &input, "owner", PrincipalKind::User, HASH, latest()).unwrap()["replayed"],
        true
    );
    assert_eq!(count(&tx, "journal_entries"), 1);
}
#[test]
fn mileage_posting_rollback_closed_missing_mismatch_no_partial_effects() {
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    setup(&mut db);
    let before = crate::migration::schema(&db).unwrap();
    {
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        configure(&tx, &config(), "owner", PrincipalKind::User).unwrap();
        let p = preview(&tx, &request(), HASH, latest()).unwrap();
        let input = Post {
            mileage_entry_id: 1,
            rate_version: "synthetic-v1".into(),
            document_id: 1,
            expected_preview_hash: p["previewHash"].as_str().unwrap().into(),
            operation_key: "rollback".into(),
            confirm: true,
        };
        post(&tx, &input, "owner", PrincipalKind::User, HASH, latest()).unwrap();
        tx.rollback().unwrap();
    }
    assert_eq!(crate::migration::schema(&db).unwrap(), before);
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    assert_eq!(count(&tx, "journal_entries"), 0);
    assert_eq!(count(&tx, "mileage_entries"), 1);
    let mut c = config();
    c.vehicle = "Wrong vehicle".into();
    configure(&tx, &c, "owner", PrincipalKind::User).unwrap();
    assert!(preview(&tx, &request(), HASH, latest()).is_err());
    tx.rollback().unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    configure(&tx, &config(), "owner", PrincipalKind::User).unwrap();
    let mut missing = request();
    missing.document_id = 999;
    assert!(preview(&tx, &missing, HASH, latest()).is_err());
    tx.execute_batch("INSERT INTO accounting_periods(period_start,period_end,kind,status) VALUES('2026-01-01','2026-12-31','fiscal_year','closed')").unwrap();
    assert!(preview(&tx, &request(), HASH, latest()).is_err());
    assert_eq!(count(&tx, "journal_entries"), 0);
}
