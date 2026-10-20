use super::*;
use std::path::Path;
fn pair(root: &Path) -> (Connection, Connection) {
    (
        Connection::open(root.join("control.sqlite")).unwrap(),
        Connection::open(root.join("company.sqlite")).unwrap(),
    )
}
fn count(db: &Connection, table: &str) -> i64 {
    db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
#[test]
fn complete_empty_native_pair_persists_without_credentials_accounts_or_access() {
    let root = tempfile::tempdir().unwrap();
    let (mut control, mut company) = pair(root.path());
    let report = initialize_empty_pair(&mut control, &mut company, "Synthetic North").unwrap();
    assert_eq!((report.control_version, report.company_version), (31, 36));
    assert_eq!(report.ledger_uuid.len(), 32);
    assert_eq!(count(&control, "workspace_schema_migrations"), 31);
    assert_eq!(count(&company, "schema_migrations"), 36);
    for table in [
        "user",
        "session",
        "account",
        "rm_company_membership_events",
        "rm_workspace_user_access_events",
        "rm_workspace_bootstrap_events",
    ] {
        assert_eq!(count(&control, table), 0, "{table}");
    }
    for table in ["accounts", "journal_entries", "journal_lines"] {
        assert_eq!(count(&company, table), 0, "{table}");
    }
    assert_eq!(
        company
            .query_row("SELECT name FROM companies WHERE id=1", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "Synthetic North"
    );
    assert!(crate::company_baseline::recognizes(&company, 36).unwrap());
    control.close().unwrap();
    company.close().unwrap();
    let (control, company) = pair(root.path());
    assert_eq!(fingerprint(&control).unwrap(), report.control_schema_sha256);
    assert_eq!(fingerprint(&company).unwrap(), report.company_schema_sha256);
    assert_eq!(
        company
            .query_row(
                "SELECT ledger_uuid FROM ledger_identity WHERE id=1",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        report.ledger_uuid
    );
    control.close().unwrap();
    company.close().unwrap();
    let other = tempfile::tempdir().unwrap();
    let (mut c, mut l) = pair(other.path());
    let second = initialize_empty_pair(&mut c, &mut l, "Synthetic South").unwrap();
    assert_ne!(report.ledger_uuid, second.ledger_uuid);
}
#[test]
fn either_nonempty_database_refuses_before_mutating_the_other() {
    for side in [0, 1] {
        let root = tempfile::tempdir().unwrap();
        let (mut c, mut l) = pair(root.path());
        let selected = if side == 0 { &c } else { &l };
        selected
            .execute_batch("CREATE TABLE sentinel(value); INSERT INTO sentinel VALUES(7)")
            .unwrap();
        let before = (c.total_changes(), l.total_changes());
        assert!(initialize_empty_pair(&mut c, &mut l, "Synthetic").is_err());
        assert_eq!((c.total_changes(), l.total_changes()), before);
        assert_eq!(count(if side == 0 { &l } else { &c }, "sqlite_schema"), 0);
        assert_eq!(count(if side == 0 { &c } else { &l }, "sentinel"), 1);
    }
}
#[test]
fn repeat_initialization_cannot_replace_native_identity_or_any_data() {
    let root = tempfile::tempdir().unwrap();
    let (mut c, mut l) = pair(root.path());
    let first = initialize_empty_pair(&mut c, &mut l, "Synthetic").unwrap();
    let before = (c.total_changes(), l.total_changes());
    assert!(initialize_empty_pair(&mut c, &mut l, "Different").is_err());
    assert_eq!((c.total_changes(), l.total_changes()), before);
    assert_eq!(
        l.query_row("SELECT ledger_uuid FROM ledger_identity", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        first.ledger_uuid
    );
}
#[test]
fn temporary_attached_metadata_readonly_or_active_transaction_refuses() {
    for sql in [
        "CREATE TEMP TABLE sentinel(value)",
        "ATTACH ':memory:' AS other",
        "PRAGMA user_version=1",
        "PRAGMA application_id=1",
        "PRAGMA query_only=ON",
        "BEGIN IMMEDIATE",
    ] {
        let root = tempfile::tempdir().unwrap();
        let (mut c, mut l) = pair(root.path());
        l.execute_batch(sql).unwrap();
        let before = (c.total_changes(), l.total_changes());
        assert!(
            initialize_empty_pair(&mut c, &mut l, "Synthetic").is_err(),
            "{sql}"
        );
        assert_eq!((c.total_changes(), l.total_changes()), before);
        assert_eq!(count(&c, "sqlite_schema"), 0);
    }
}
#[test]
fn company_write_contention_leaves_control_complete_and_company_empty() {
    let root = tempfile::tempdir().unwrap();
    let (mut c, mut l) = pair(root.path());
    let blocker = Connection::open(root.path().join("company.sqlite")).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
    l.busy_timeout(std::time::Duration::ZERO).unwrap();
    assert!(initialize_empty_pair(&mut c, &mut l, "Synthetic").is_err());
    assert_eq!(count(&c, "workspace_schema_migrations"), 31);
    assert_eq!(count(&l, "sqlite_schema"), 0);
    blocker.execute_batch("ROLLBACK").unwrap();
    blocker.close().unwrap();
}

#[test]
fn explicit_chart_seed_is_atomic_and_never_replaces_an_existing_chart() {
    let root = tempfile::tempdir().unwrap();
    let (mut control, mut company) = pair(root.path());
    initialize_empty_pair(&mut control, &mut company, "Synthetic Chart").unwrap();
    let chart = |other: &str| {
        serde_json::from_value::<Vec<InitialAccount>>(serde_json::json!([
            {"accountNo":"A","name":"Synthetic Asset","type":"asset","normalBalance":"debit"},
            {"accountNo":other,"name":"Synthetic Equity","type":"equity","normalBalance":"credit"}
        ]))
        .unwrap()
    };
    assert!(initialize_accounts(&mut company, &chart("A")).is_err());
    assert_eq!(count(&company, "accounts"), 0);
    initialize_accounts(&mut company, &chart("B")).unwrap();
    assert_eq!(count(&company, "accounts"), 2);
    assert!(initialize_accounts(&mut company, &chart("C")).is_err());
    assert_eq!(count(&company, "accounts"), 2);
    assert_eq!(
        company
            .query_row(
                "SELECT account_no FROM accounts WHERE name='Synthetic Equity'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "B"
    );
    assert_eq!(count(&company, "journal_entries"), 0);
}
