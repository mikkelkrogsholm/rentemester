use super::*;
use rusqlite::{TransactionBehavior, types::ValueRef};
const NOW: i64 = 1_792_497_600_000;
fn db() -> (Connection, Connection) {
    let mut control = Connection::open_in_memory().unwrap();
    let mut company = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic archive").unwrap();
    (control, company)
}
fn source() -> Vec<u8> {
    serde_json::to_vec(&json!({"format":policy::PACKAGE,"currency":"DKK","files":[{"path":"2024/Posteringer.csv","contentBase64":STANDARD.encode("Konto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Moms;Beløb;Saldo\n6200;Synthetic;2024-06-01;1;Historical;Synthetic first;;0,005000;1,234567\n6200;Synthetic;2024-06-02;2;Historical;Synthetic second;;0,005000;\n6201;Synthetic credit;2024-06-03;3;Historical;Synthetic signed;;-0,005000;-1,234567\n")},{"path":"2024/SaldoBalance.csv","contentBase64":STANDARD.encode("Konto;Kontonavn;Beløb\n6200;Synthetic;1,234567\n6201;Synthetic credit;-1,234567\n")},{"path":"2025/Posteringer.csv","contentBase64":STANDARD.encode("Konto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Moms;Beløb;Saldo\n6200;Excluded;2025-01-01;0;;Primobeholdning;;0;0\n")}]})).unwrap()
}
fn install(tx: &Transaction<'_>, bytes: &[u8]) -> Value {
    let (source, parsed) = decode_source(bytes).unwrap();
    let p = prepare(tx, &source, &parsed).unwrap();
    let apply = Apply {
        plan_hash: p.review()["planHash"].as_str().unwrap().into(),
        idempotency_key: "synthetic-key".into(),
        confirm: true,
    };
    apply_domain(tx, &p, &apply)
}
fn apply_domain(tx: &Transaction<'_>, p: &Prepared, i: &Apply) -> Value {
    apply(tx, p, i, "synthetic-owner", NOW).unwrap()
}
fn proofs(db: &Connection, bytes: &[u8]) -> Vec<VerifiedArchive> {
    originals(db)
        .unwrap()
        .iter()
        .map(|r| verify_original(db, r, bytes).unwrap())
        .collect()
}
#[test]
fn original_backed_install_read_cfo_replay_and_history_survive_profile_change_and_reopen() {
    let (_control, mut db) = db();
    let bytes = source();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let (source, parsed) = decode_source(&bytes).unwrap();
    let p = prepare(&tx, &source, &parsed).unwrap();
    assert_eq!(p.review()["years"][0]["status"], "new");
    let before = tx
        .query_row::<i64, _, _>("SELECT COUNT(*) FROM journal_entries", [], |r| r.get(0))
        .unwrap();
    let result = install(&tx, &bytes);
    let proof = proofs(&tx, &bytes);
    let detail = read(&tx, &proof, Some(2024)).unwrap();
    assert_eq!(detail["years"][0]["postings"][0]["amount"], "0.01");
    assert_eq!(
        detail["years"][0]["postings"][0]["sourceAmount"],
        "0.005000"
    );
    assert_eq!(
        detail["years"][0]["balances"][0]["sourceAmount"],
        "1.234567"
    );
    assert_eq!(detail["years"][0]["sourceHash"], source.sha256);
    assert!(read(&tx, &[], None).is_err());
    let rows = cfo_rows(&tx, &proof, "synthetic", "2024-01-01", "2024-12-31").unwrap();
    let mut total = rentemester_engine::money::Money::default();
    for r in rows.iter().filter(|r| r.account_no == "6200") {
        total.add(&r.amount);
    }
    assert_eq!(total.decimal(), "0.02");
    assert_eq!(rows[0].source_hash, source.sha256);
    let changes = tx.total_changes();
    assert_eq!(install(&tx, &bytes), result);
    assert_eq!(changes, tx.total_changes());
    tx.execute(
        "UPDATE companies SET currency='EUR',fiscal_year_start_month=7",
        [],
    )
    .unwrap();
    validate_history(&tx).unwrap();
    let proof = proofs(&tx, &bytes);
    assert_eq!(
        read(&tx, &proof, Some(2024)).unwrap()["years"][0]["declaredCurrency"],
        "DKK"
    );
    assert_eq!(install(&tx, &bytes), result);
    assert_eq!(
        before,
        tx.query_row::<i64, _, _>("SELECT COUNT(*) FROM journal_entries", [], |r| r.get(0))
            .unwrap()
    );
    tx.commit().unwrap();
    let copy = crate::portable_backup::company(&db).unwrap();
    assert!(
        copy.originals
            .iter()
            .any(|(path, hash)| path == &source.path && hash == &source.sha256)
    );
    let target = tempfile::tempdir().unwrap();
    let path = target.path().join("restored.sqlite");
    db.execute("VACUUM INTO ?", [path.to_str().unwrap()])
        .unwrap();
    let restored = Connection::open(path).unwrap();
    validate_history(&restored).unwrap();
    let restored_proofs = proofs(&restored, &bytes);
    assert_eq!(restored_proofs.len(), 1);
}

#[test]
fn exact_numeric_roundtrip_refuses_lost_micros_and_cell_corruption() {
    for raw in [
        "0.005000",
        "-0.005000",
        "1.234567",
        "-1.234567",
        "100000000000.000000",
        "9007199254740991.000000",
    ] {
        let value = ArchiveDecimal::parse_canonical(raw).unwrap();
        let cell = numeric::value(&value).unwrap();
        let cell_ref = match cell {
            rusqlite::types::Value::Integer(n) => ValueRef::Integer(n),
            rusqlite::types::Value::Real(n) => ValueRef::Real(n),
            _ => panic!("numeric adapter emitted nonnumeric"),
        };
        numeric::verify(cell_ref, &value).unwrap();
    }
    let lost = ArchiveDecimal::parse_canonical("9007199254740990.000001").unwrap();
    assert!(numeric::value(&lost).is_err());
    let x = ArchiveDecimal::parse_canonical("1.234567").unwrap();
    assert!(numeric::verify(ValueRef::Real(1.234568), &x).is_err());
    assert!(numeric::verify(ValueRef::Text(b"1.234567"), &x).is_err());
}
#[test]
fn changed_year_source_missing_original_and_legacy_inventory_fail_closed() {
    let (_c, mut db) = db();
    let bytes = source();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    install(&tx, &bytes);
    let r = originals(&tx).unwrap().remove(0);
    assert!(verify_original(&tx, &r, b"changed").is_err());
    let mut package: Value = serde_json::from_slice(&bytes).unwrap();
    package["files"][0]["contentBase64"] = json!(
        STANDARD.encode(
            String::from_utf8(
                STANDARD
                    .decode(package["files"][0]["contentBase64"].as_str().unwrap())
                    .unwrap()
            )
            .unwrap()
            .replace("0,005000", "0,006000")
        )
    );
    let changed = serde_json::to_vec(&package).unwrap();
    let (s, p) = decode_source(&changed).unwrap();
    assert!(prepare(&tx, &s, &p).is_err());
    tx.execute("INSERT INTO import_archive_years(source_system,fiscal_year,posting_count,balance_count)VALUES('legacy',2023,0,0)",[]).unwrap();
    assert!(validate_history(&tx).is_err());
}
#[test]
fn stale_plan_rolls_back_prefix_and_unexpected_storage_failure_keeps_no_inventory() {
    let (_c, mut db) = db();
    let bytes = source();
    let (s, p) = decode_source(&bytes).unwrap();
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let prepared = prepare(&tx, &s, &p).unwrap();
        tx.execute("UPDATE companies SET fiscal_year_start_month=7", [])
            .unwrap();
        let i = Apply {
            plan_hash: prepared.review()["planHash"].as_str().unwrap().into(),
            idempotency_key: "stale".into(),
            confirm: true,
        };
        assert!(apply(&tx, &prepared, &i, "synthetic", NOW).is_err());
        tx.rollback().unwrap();
    }
    db.execute_batch("CREATE TEMP TRIGGER synthetic_archive_failure BEFORE INSERT ON import_archive_balances BEGIN SELECT RAISE(ABORT,'synthetic'); END").unwrap();
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let prepared = prepare(&tx, &s, &p).unwrap();
        let i = Apply {
            plan_hash: prepared.review()["planHash"].as_str().unwrap().into(),
            idempotency_key: "failure".into(),
            confirm: true,
        };
        assert!(apply(&tx, &prepared, &i, "synthetic", NOW).is_err());
        tx.rollback().unwrap();
    }
    assert_eq!(
        db.query_row::<i64, _, _>("SELECT COUNT(*) FROM import_archive_years", [], |r| r
            .get(0))
            .unwrap(),
        0
    );
    validate_history(&db).unwrap();
}
#[test]
fn complete_history_rejects_same_cent_numeric_corruption_and_unknown_attestation_version() {
    let (_c, mut db) = db();
    let bytes = source();
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        install(&tx, &bytes);
        tx.commit().unwrap();
    }
    let trigger: String = db
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE name='import_archive_postings_no_update'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    db.execute_batch("DROP TRIGGER import_archive_postings_no_update;UPDATE import_archive_postings SET running_balance=1.234568 WHERE id=1").unwrap();
    db.execute_batch(&trigger).unwrap();
    assert!(validate_history(&db).is_err());
    assert!(crate::portable_backup::company(&db).is_err());
    let (_c, mut clean) = self::db();
    {
        let tx = clean
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        install(&tx, &bytes);
        tx.commit().unwrap();
    }
    let mut event: Value = serde_json::from_str(
        &clean
            .query_row::<String, _, _>(
                "SELECT message FROM audit_log WHERE event_type=?",
                [EVENT],
                |r| r.get(0),
            )
            .unwrap(),
    )
    .unwrap();
    event["version"] = json!(2);
    let trigger: String = clean
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE name='audit_log_no_update'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    clean
        .execute_batch("DROP TRIGGER audit_log_no_update")
        .unwrap();
    clean
        .execute(
            "UPDATE audit_log SET message=? WHERE event_type=?",
            params![serde_json::to_string(&event).unwrap(), EVENT],
        )
        .unwrap();
    clean.execute_batch(&trigger).unwrap();
    assert!(validate_history(&clean).is_err());
}
#[test]
fn public_trial_balance_refuses_archive_false_zero_without_blocking_verified_cfo_or_raw_live_reader()
 {
    let (mut control, mut db) = db();
    let bytes = source();
    {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        install(&tx, &bytes);
        tx.commit().unwrap();
    }
    let target = tempfile::tempdir().unwrap();
    std::fs::create_dir(target.path().join("data")).unwrap();
    let path = target.path().join("data/ledger.sqlite");
    db.execute("VACUUM INTO ?", [path.to_str().unwrap()])
        .unwrap();
    let period =
        rentemester_engine::trial_balance::Period::new("2024-01-01", "2024-12-31").unwrap();
    assert_eq!(
        crate::trial_balance(target.path(), period.clone())
            .unwrap_err()
            .code,
        "archive_report_unsupported"
    );
    let ct = control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert_eq!(
        require_live_report_only(&tx).unwrap_err().code,
        "archive_report_unsupported"
    );
    assert!(crate::trial_balance_owned(&tx, period).unwrap().ok);
    let verified = proofs(&tx, &bytes);
    let query = rentemester_engine::cfo_analytics::Query {
        scope: "company".into(),
        from: "2024-01-01".into(),
        to: "2024-12-31".into(),
        company_slugs: vec!["synthetic".into()],
        group_profile_id: None,
        account: Some("6200".into()),
        party: None,
        dimension: None,
        currency: None,
        cursor: None,
        aggregate: "none".into(),
        limit: 200,
    };
    let rows = crate::cfo_analytics::collect_with_archives(
        &tx,
        &ct,
        "synthetic",
        &query,
        &BTreeSet::from(["synthetic".into()]),
        &verified,
    )
    .unwrap();
    let result = rentemester_engine::cfo_analytics::render(
        &query,
        &rentemester_engine::cfo_analytics::Selection {
            companies: vec!["synthetic".into()],
            partial: false,
        },
        &[rows],
    )
    .unwrap();
    assert_eq!(result["reconciliation"]["amountByCurrency"]["DKK"], "0.02");
}
