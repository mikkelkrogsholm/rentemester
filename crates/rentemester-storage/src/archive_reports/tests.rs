use super::*;
use crate::archive_import::{Apply, apply, decode_source, originals, prepare, verify_original};
use base64::{Engine as _, engine::general_purpose::STANDARD};
fn source() -> Vec<u8> {
    serde_json::to_vec(&json!({"format":"rentemester-dinero-archive-source/1","currency":"DKK","files":[{"path":"2024/Posteringer.csv","contentBase64":STANDARD.encode("Konto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Moms;Beløb;Saldo\nA;Original asset;2023-07-01;42;Source;Original;;0,005;1,234567\nX;Original expense;2024-02-02;;Source;Original;;-0,004999;\n")},{"path":"2024/SaldoBalance.csv","contentBase64":STANDARD.encode("Konto;Kontonavn;Beløb\nA;Original asset;100000000000000\nE;Original equity;-100000000000000\n")},{"path":"2025/Posteringer.csv","contentBase64":STANDARD.encode("Konto;Kontonavn;Dato;Bilag;Bilagstype;Tekst;Moms;Beløb;Saldo\nA;Excluded;2025-01-01;0;;Excluded;;0;0\n")}]})).unwrap()
}
fn install(db: &mut rusqlite::Connection, bytes: &[u8]) {
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let (s, p) = decode_source(bytes).unwrap();
    let prepared = prepare(&tx, &s, &p).unwrap();
    let input = Apply {
        plan_hash: prepared.review()["planHash"].as_str().unwrap().into(),
        idempotency_key: "synthetic-archive-reports".into(),
        confirm: true,
    };
    apply(&tx, &prepared, &input, "synthetic-owner", 1792497600000).unwrap();
    tx.commit().unwrap();
}
fn proof(db: &rusqlite::Connection, bytes: &[u8]) -> Vec<VerifiedArchive> {
    originals(db)
        .unwrap()
        .iter()
        .map(|r| verify_original(db, r, bytes).unwrap())
        .collect()
}
#[test]
fn verified_originals_current_chart_no_effects_all_five_reports_and_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("company.sqlite");
    let mut control = rusqlite::Connection::open_in_memory().unwrap();
    let mut db = rusqlite::Connection::open(&path).unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic archive report").unwrap();
    let bytes = source();
    install(&mut db, &bytes);
    let verified = proof(&db, &bytes);
    let before = db.total_changes();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    for r in [
        Resource::Journal,
        Resource::TrialBalance,
        Resource::IncomeStatement,
        Resource::Balance,
        Resource::Overview,
    ] {
        let value = read(&tx, &verified, "synthetic", 2024, None, r).unwrap();
        assert_eq!(value["source"], "archive");
        assert_eq!(
            value["evidence"]["sourceHash"],
            decode_source(&bytes).unwrap().0.sha256
        );
        assert!(value["periodStart"].is_null());
    }
    assert_eq!(
        read(&tx, &verified, "synthetic", 2024, None, Resource::Balance).unwrap()["completeness"]["status"],
        "incomplete"
    );
    assert!(read(&tx, &[], "synthetic", 2024, None, Resource::Journal).is_err());
    assert!(read(&tx, &verified, "synthetic", 2025, None, Resource::Journal).is_err());
    assert_eq!(before, tx.total_changes());
    tx.rollback().unwrap();
    // Current classification changes neither historical names nor source currency/bytes.
    db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('A','Current renamed asset','asset','debit'),('E','Current equity','equity','credit'),('X','Current expense','expense','debit');UPDATE companies SET currency='EUR',fiscal_year_start_month=7,fiscal_year_label_strategy='span' WHERE id=1;").unwrap();
    let after_setup = db.total_changes();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let expected = read(&tx, &verified, "synthetic", 2024, None, Resource::Balance).unwrap();
    assert_eq!(expected["completeness"]["status"], "ready");
    assert_eq!(expected["totalAssets"], "100000000000000.00");
    assert_eq!(expected["balanced"], true);
    assert_eq!(expected["assets"]["lines"][0]["name"], "Original asset");
    assert_eq!(expected["company"]["currency"], "EUR");
    assert_eq!(expected["declaredCurrency"], "DKK");
    assert_eq!(after_setup, tx.total_changes());
    tx.rollback().unwrap();
    drop(db);
    let mut reopened = rusqlite::Connection::open(path).unwrap();
    let verified = proof(&reopened, &bytes);
    let tx = reopened
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    assert_eq!(
        read(&tx, &verified, "synthetic", 2024, None, Resource::Balance).unwrap(),
        expected
    );
}
#[test]
fn retained_proof_never_authorizes_changed_archive_rows_or_substituted_original() {
    let mut control = rusqlite::Connection::open_in_memory().unwrap();
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic archive report").unwrap();
    let bytes = source();
    install(&mut db, &bytes);
    let verified = proof(&db, &bytes);
    let original = originals(&db).unwrap().remove(0);
    let mut wrong = bytes.clone();
    wrong.push(b' ');
    assert!(verify_original(&db, &original, &wrong).is_err());
    let trigger: String = db
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE name='import_archive_postings_no_update'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    db.execute_batch("DROP TRIGGER import_archive_postings_no_update;UPDATE import_archive_postings SET amount=0 WHERE line_no=0;").unwrap();
    db.execute_batch(&trigger).unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    for r in [
        Resource::Journal,
        Resource::TrialBalance,
        Resource::IncomeStatement,
        Resource::Balance,
        Resource::Overview,
    ] {
        assert!(read(&tx, &verified, "synthetic", 2024, None, r).is_err());
    }
}
