use super::*;
use rusqlite::TransactionBehavior;
fn fixture() -> (Connection, Connection) {
    let mut c = Connection::open_in_memory().unwrap();
    let mut l = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic opening").unwrap();
    l.execute_batch("UPDATE companies SET fiscal_year_start_month=7,fiscal_year_label_strategy='span' WHERE id=1;INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('A','Synthetic asset','asset','debit'),('L','Synthetic liability','liability','credit'),('E','Synthetic equity','equity','credit'),('X','Synthetic expense','expense','debit');").unwrap();
    (c, l)
}
fn input() -> Input {
    serde_json::from_value(json!({"cutOverDate":"2024-07-01","lines":[{"accountNo":"A","debitAmount":"100.00"},{"accountNo":"L","creditAmount":"30.00"},{"accountNo":"E","creditAmount":"70.00"}],"note":"Synthetic approved adoption"})).unwrap()
}
fn latest() -> Date {
    Date::parse("2026-10-20").unwrap()
}
fn command(tx: &Transaction<'_>, i: Input) -> Apply {
    let p = prepare(tx, &i, &[], latest()).unwrap();
    Apply {
        input: i,
        plan_hash: p.review()["planHash"].as_str().unwrap().into(),
        idempotency_key: "synthetic-opening".into(),
        confirm: true,
    }
}
fn post(tx: &Transaction<'_>, i: &Apply) -> Result<Value, Error> {
    apply(
        tx,
        i,
        &[],
        latest(),
        Actor {
            subject: "user:synthetic",
            program: PROGRAM,
        },
        1792497600000,
    )
}
fn journal(tx: &Transaction<'_>, date: &str) -> Result<Value, Error> {
    let request=serde_json::from_value(json!({"transactionDate":date,"text":"Synthetic later adjustment","currency":"DKK","lines":[{"accountNo":"E","debitAmount":"10.00"},{"accountNo":"A","creditAmount":"10.00"}]})).unwrap();
    journal_post::post(
        tx,
        &request,
        &crate::journal_preview::manual_journal_rule_version().unwrap(),
        latest(),
        Actor {
            subject: "user:synthetic",
            program: "synthetic-later",
        },
    )
}
#[test]
fn atomic_opening_later_post_cutover_reports_and_historical_backup_reopen() {
    let (_c, mut l) = fixture();
    let tx = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let before = tx.total_changes();
    let cmd = command(&tx, input());
    assert_eq!(before, tx.total_changes());
    assert_eq!(
        prepare(&tx, &cmd.input, &[], latest()).unwrap().review()["status"],
        "ready"
    );
    let accepted = post(&tx, &cmd).unwrap();
    assert_eq!(
        accepted["openingBalance"]["journalEntryNo"],
        "2024-25-00001"
    );
    assert_eq!(
        read(&tx).unwrap()["openingBalance"],
        accepted["openingBalance"]
    );
    assert!(post(&tx, &cmd).is_err());
    assert!(require_report_range(&tx, "2024-06-30", "2024-12-31").is_err());
    assert!(require_report_as_of(&tx, "2024-06-30").is_err());
    assert_eq!(
        require_report_range(&tx, "2024-07-01", "2024-12-31").unwrap()["cutOverDate"],
        "2024-07-01"
    );
    assert!(journal(&tx, "2024-06-30").is_err());
    assert_eq!(journal(&tx, "2024-07-02").unwrap()["ok"], true);
    let report = crate::trial_balance_owned(
        &tx,
        rentemester_engine::trial_balance::Period::new("2024-07-01", "2024-12-31").unwrap(),
    )
    .unwrap();
    let sheet = rentemester_engine::financial_statements::balance_sheet(&report);
    assert_eq!(sheet.assets.total.decimal(), "90.00");
    assert_eq!(sheet.equity.total.decimal(), "60.00");
    assert_eq!(sheet.liabilities.total.decimal(), "30.00");
    assert!(sheet.balanced);
    let reverse = crate::journal_reversal::Input {
        entry_id: rentemester_engine::identifiers::JournalEntryId::from_sqlite(
            accepted["openingBalance"]["journalEntryId"]
                .as_i64()
                .unwrap(),
        ),
        transaction_date: Date::parse("2024-07-03").unwrap(),
        reason: "Synthetic reason".into(),
    };
    let refused = crate::journal_reversal::reverse(
        &tx,
        &reverse,
        &crate::journal_preview::manual_journal_rule_version().unwrap(),
        latest(),
        Actor {
            subject: "user:synthetic",
            program: "synthetic-reverse",
        },
    )
    .unwrap();
    assert_eq!(refused["ok"], false);
    assert!(refused["errors"].to_string().contains("opening"));
    tx.commit().unwrap();
    // Historical proof is independent of current editable configuration and classifications.
    l.execute_batch("UPDATE companies SET currency='EUR' WHERE id=1;UPDATE accounts SET name='Current renamed',type='expense' WHERE account_no='A';").unwrap();
    let record = read(&l).unwrap();
    assert_eq!(record["openingBalance"]["sourceBasis"]["currency"], "DKK");
    crate::portable_backup::company(&l).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("snapshot.sqlite");
    l.execute("VACUUM INTO ?", [path.to_str().unwrap()])
        .unwrap();
    drop(l);
    let reopened = Connection::open(path).unwrap();
    assert_eq!(read(&reopened).unwrap(), record);
    crate::portable_backup::company(&reopened).unwrap();
}
#[test]
fn stale_closed_unbalanced_prior_history_document_rule_and_confirm_are_effect_free() {
    let (_c, mut l) = fixture();
    let tx = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let mut i = input();
    i.lines[0].debit_amount = Money::parse_canonical("99.00").unwrap();
    let plan = prepare(&tx, &i, &[], latest()).unwrap();
    assert_eq!(plan.review()["status"], "blocked");
    assert!(plan.review()["blockers"].to_string().contains("balance"));
    let mut pnl = input();
    pnl.lines[0].account_no = rentemester_engine::identifiers::AccountNo::parse("X").unwrap();
    let p = prepare(&tx, &pnl, &[], latest()).unwrap();
    assert_eq!(p.review()["status"], "blocked");
    assert!(p.review()["blockers"].to_string().contains("documentId"));
    let mut cmd = command(&tx, input());
    cmd.confirm = false;
    assert!(post(&tx, &cmd).is_err());
    cmd.confirm = true;
    tx.execute(
        "UPDATE accounts SET name='Changed after review' WHERE account_no='A'",
        [],
    )
    .unwrap();
    assert!(post(&tx, &cmd).is_err());
    assert_eq!(
        tx.query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
            .unwrap(),
        0
    );
    tx.rollback().unwrap();
    let tx = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    journal(&tx, "2024-06-30").unwrap();
    let plan = prepare(&tx, &input(), &[], latest()).unwrap();
    assert_eq!(plan.review()["status"], "blocked");
    assert!(plan.review()["blockers"].to_string().contains("predates"));
    tx.rollback().unwrap();
    let tx = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("INSERT INTO accounting_periods(kind,period_start,period_end,status)VALUES('custom','2024-07-01','2024-07-31','closed');").unwrap();
    let plan = prepare(&tx, &input(), &[], latest()).unwrap();
    assert_eq!(plan.review()["status"], "blocked");
    assert!(plan.review()["blockers"].to_string().contains("closed"));
}
#[test]
fn marker_write_failure_rolls_back_journal_sequence_and_audits() {
    let (_c, mut l) = fixture();
    l.execute_batch("CREATE TEMP TRIGGER synthetic_opening_abort BEFORE INSERT ON opening_balances BEGIN SELECT RAISE(ABORT,'synthetic failure');END;").unwrap();
    let tx = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let cmd = command(&tx, input());
    assert!(post(&tx, &cmd).is_err());
    tx.rollback().unwrap();
    for table in [
        "journal_entries",
        "journal_lines",
        "opening_balances",
        "audit_log",
        "sequences",
    ] {
        assert_eq!(
            l.query_row::<i64, _, _>(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap(),
            0,
            "{table}"
        );
    }
}
#[test]
fn authoritative_orphans_and_marker_corruption_refuse_not_textual_resemblance() {
    let (_c, mut l) = fixture();
    let tx = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let mut request:rentemester_engine::manual_journal::ManualJournal=serde_json::from_value(json!({"transactionDate":"2024-07-01","text":"Primobalance pr. 2024-07-01 resemblance only","currency":"DKK","lines":[{"accountNo":"A","debitAmount":"1.00"},{"accountNo":"E","creditAmount":"1.00"}]})).unwrap();
    journal_post::post(
        &tx,
        &request,
        &crate::journal_preview::manual_journal_rule_version().unwrap(),
        latest(),
        Actor {
            subject: "user:synthetic",
            program: "ordinary-user-post",
        },
    )
    .unwrap();
    assert!(validate_history(&tx).unwrap().is_none());
    let cmd = command(&tx, input());
    post(&tx, &cmd).unwrap();
    tx.commit().unwrap();
    let ddl: String = l
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE name='opening_balances_no_update'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    l.execute_batch("DROP TRIGGER opening_balances_no_update;UPDATE opening_balances SET cut_over_date='2024-07-02';").unwrap();
    l.execute_batch(&ddl).unwrap();
    assert!(read(&l).is_err());
    assert!(
        crate::annual_report::read(
            &l.transaction().unwrap(),
            "synthetic",
            "2025-07-01",
            "2026-06-30"
        )
        .is_err()
    );
    assert!(crate::portable_backup::company(&l).is_err());
    // Separate authoritative program orphan must never be adopted from a raw row.
    let (_c, mut other) = fixture();
    let tx = other
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    request.text = "Synthetic orphan".into();
    journal_post::post(
        &tx,
        &request,
        &crate::journal_preview::manual_journal_rule_version().unwrap(),
        latest(),
        Actor {
            subject: "user:synthetic",
            program: PROGRAM,
        },
    )
    .unwrap();
    assert!(validate_history(&tx).is_err());
}
#[test]
fn explicit_archive_citation_requires_verified_original_and_remains_historical() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let (_c, mut l) = fixture();
    let bytes=serde_json::to_vec(&json!({"format":"rentemester-dinero-archive-source/1","currency":"DKK","files":[{"path":"2023/Posteringer.csv","contentBase64":STANDARD.encode("Konto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Moms;Beløb;Saldo\nA;Historical;2023-07-01;1;Source;Synthetic;;999;999\n")},{"path":"2024/Posteringer.csv","contentBase64":STANDARD.encode("Konto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Moms;Beløb;Saldo\nA;Excluded;2024-07-01;0;;Excluded;;0;0\n")}]})).unwrap();
    let tx = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let (source, parsed) = archive_import::decode_source(&bytes).unwrap();
    let prepared = archive_import::prepare(&tx, &source, &parsed).unwrap();
    let imported = archive_import::apply(
        &tx,
        &prepared,
        &archive_import::Apply {
            plan_hash: prepared.review()["planHash"].as_str().unwrap().into(),
            idempotency_key: "synthetic-archive".into(),
            confirm: true,
        },
        "synthetic",
        1792497600000,
    )
    .unwrap();
    tx.commit().unwrap();
    // A real attested archive is not an implicit live opening basis.
    assert!(
        crate::annual_report::read(
            &l.transaction().unwrap(),
            "synthetic",
            "2025-07-01",
            "2026-06-30"
        )
        .is_err()
    );
    let original = archive_import::originals(&l).unwrap().remove(0);
    let verified = vec![archive_import::verify_original(&l, &original, &bytes).unwrap()];
    let mut i = input();
    i.source_archive_import_ids = vec![imported["archiveImport"]["importId"].as_i64().unwrap()];
    let tx = l
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    assert!(prepare(&tx, &i, &[], latest()).is_err());
    let p = prepare(&tx, &i, &verified, latest()).unwrap();
    assert_eq!(p.review()["totals"]["debit"], "100.00");
    assert_eq!(
        p.review()["sourceBasis"]["sourceArchives"][0]["sourceHash"],
        source.sha256
    );
    let cmd = Apply {
        input: i,
        plan_hash: p.review()["planHash"].as_str().unwrap().into(),
        idempotency_key: "synthetic-cited-opening".into(),
        confirm: true,
    };
    let accepted = apply(
        &tx,
        &cmd,
        &verified,
        latest(),
        Actor {
            subject: "user:synthetic",
            program: PROGRAM,
        },
        1792497600000,
    )
    .unwrap();
    assert_eq!(
        accepted["openingBalance"]["sourceBasis"]["sourceArchives"][0]["declaredCurrency"],
        "DKK"
    );
    tx.commit().unwrap();
    l.execute_batch("UPDATE companies SET currency='EUR' WHERE id=1;")
        .unwrap();
    let history = validate_history(&l).unwrap().unwrap();
    assert_eq!(
        history.source_archive_import_ids(),
        vec![original.import_id]
    );
    assert_eq!(history.record().source_basis.currency, "DKK");
    crate::portable_backup::company(&l).unwrap();
}

#[test]
fn annual_full_year_coverage_refuses_partial_current_and_hides_partial_prior() {
    let (_control, mut db) = fixture();
    db.execute("UPDATE companies SET cvr='87654321' WHERE id=1", [])
        .unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let mut opening = input();
    opening.cut_over_date = Date::parse("2023-10-01").unwrap();
    let request = command(&tx, opening);
    post(&tx, &request).unwrap();
    journal(&tx, "2024-09-01").unwrap();
    tx.commit().unwrap();
    // Only the complete later fiscal years are locked; the opening itself is real,
    // source-bound and generated by the ordinary typed opening operation above.
    db.execute_batch("INSERT INTO accounting_periods(period_start,period_end,kind,status,closed_at,closed_by) VALUES('2024-07-01','2025-06-30','fiscal_year','closed','2025-07-01','user:synthetic'),('2025-07-01','2026-06-30','fiscal_year','closed','2026-07-01','user:synthetic');").unwrap();
    let before = db.total_changes();
    let tx = db.transaction().unwrap();
    assert!(crate::annual_report::read(&tx, "synthetic", "2023-07-01", "2024-06-30").is_err());
    let first = crate::annual_report::read(&tx, "synthetic", "2024-07-01", "2025-06-30").unwrap();
    assert_eq!(first["report"]["ok"], true);
    assert_eq!(first["report"]["balanceSheet"]["totalAssets"], json!(90.0));
    let comparison = &first["report"]["comparison"];
    assert_eq!(comparison["available"], false);
    assert_eq!(
        comparison["reason"],
        "prior_range_precedes_explicit_cutover"
    );
    assert_eq!(comparison["fiscalYearStart"], "2023-07-01");
    for key in [
        "totalIncome",
        "totalExpense",
        "aretsResultat",
        "totalAssets",
        "totalLiabilitiesAndEquity",
        "equity",
    ] {
        assert!(comparison[key].is_null(), "{key}");
    }
    let next = crate::annual_report::read(&tx, "synthetic", "2025-07-01", "2026-06-30").unwrap();
    assert_eq!(next["report"]["ok"], true);
    assert_eq!(next["report"]["comparison"]["available"], true);
    assert_eq!(
        next["report"]["comparison"]["totalAssets"],
        first["report"]["balanceSheet"]["totalAssets"]
    );
    assert_eq!(
        next["report"]["comparison"]["equity"],
        first["report"]["balanceSheet"]["equity"]["total"]
    );
    assert_eq!(before, tx.total_changes());
}
