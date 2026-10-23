//! File-backed writer-order acceptance; no business mutation is retried.
use super::*;
use crate::journal_post::{self, Actor};
use rusqlite::{Connection, TransactionBehavior, types::ValueRef};
use std::time::Duration;

const START: &str = "2025-07-01";
const END: &str = "2026-06-30";
const TODAY: &str = "2026-10-18";

fn fixture() -> (tempfile::TempDir, Connection, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic-company.sqlite");
    let mut a = Connection::open(&path).unwrap();
    let mut control = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut control, &mut a, "Synthetic race700").unwrap();
    // Same ready-period fixture as periods::tests, with a configured shifted year.
    a.execute_batch("UPDATE companies SET cvr='87654321',vat_period_type='quarter',fiscal_year_start_month=7,fiscal_year_label_strategy='span' WHERE id=1;INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('B','Synthetic bank','asset','debit'),('R','Synthetic receivables','asset','debit'),('C','Synthetic creditors','liability','credit'),('E1','Synthetic equity one','equity','credit'),('E2','Synthetic equity two','equity','credit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by) VALUES('bank','B',1,'synthetic'),('debtors','R',1,'synthetic'),('creditors','C',1,'synthetic');").unwrap();
    let b = Connection::open(&path).unwrap();
    for db in [&a, &b] {
        db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL")
            .unwrap();
        db.busy_timeout(Duration::ZERO).unwrap();
    }
    (dir, a, b)
}
fn writer(db: &mut Connection) -> Transaction<'_> {
    db.transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap()
}
fn contends(db: &mut Connection) {
    // Called while the independently opened holder has an admitted writer.
    // Only this known, mutation-free BEGIN may be attempted again after commit.
    let error = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap_err();
    assert!(
        matches!(error, rusqlite::Error::SqliteFailure(e, _) if e.code == rusqlite::ErrorCode::DatabaseBusy)
    );
    assert!(db.is_autocommit());
}
fn packet(tx: &Transaction<'_>) -> Value {
    compute(tx, START, END, Ok(vec![])).unwrap()
}
fn approve(tx: &Transaction<'_>) -> (Close, Value) {
    let p = packet(tx);
    assert_eq!(p["blockers"], 0, "{p}");
    let r = review(tx, &p, "synthetic").unwrap();
    let input = serde_json::from_value(json!({"periodStart":START,"periodEnd":END,"kind":"fiscal_year","packetHash":p["hash"],"reviewId":r["id"],"confirm":true})).unwrap();
    (input, p)
}
fn close_ready(tx: &Transaction<'_>) {
    let (input, p) = approve(tx);
    assert_eq!(
        close(tx, &input, &p, "synthetic", TODAY).unwrap()["status"],
        "closed"
    );
}
fn reopen_ready(tx: &Transaction<'_>) {
    let input = serde_json::from_value(json!({"periodStart":START,"periodEnd":END,"kind":"fiscal_year","reason":"Synthetic additional evidence","confirm":true})).unwrap();
    assert_eq!(
        reopen(tx, &input, &packet(tx), "synthetic").unwrap()["effectiveStatus"],
        "open"
    );
}
fn actor() -> Actor<'static> {
    Actor {
        subject: "user:synthetic",
        program: "rentemester-rust",
    }
}
fn post(tx: &Transaction<'_>, date: &str) -> Value {
    let request = serde_json::from_value(json!({"transactionDate":date,"text":"Synthetic ordinary transfer","currency":"DKK","lines":[{"accountNo":"E1","debitAmount":"10.00"},{"accountNo":"E2","creditAmount":"10.00"}]})).unwrap();
    journal_post::post(
        tx,
        &request,
        &crate::manual_journal_rule_version().unwrap(),
        Date::parse(TODAY).unwrap(),
        actor(),
    )
    .unwrap()
}
// Exact typed row bytes, including timestamps, hashes, sequence values and audit actors.
fn rows(db: &Connection, table: &str, condition: &str) -> Vec<Vec<Vec<u8>>> {
    let mut q = db
        .prepare(&format!("SELECT * FROM {table} {condition} ORDER BY rowid"))
        .unwrap();
    let n = q.column_count();
    q.query_map([], |r| {
        (0..n)
            .map(|i| {
                Ok(match r.get_ref(i)? {
                    ValueRef::Null => vec![0],
                    ValueRef::Integer(v) => [vec![1], v.to_le_bytes().to_vec()].concat(),
                    ValueRef::Real(v) => [vec![2], v.to_bits().to_le_bytes().to_vec()].concat(),
                    ValueRef::Text(v) => [vec![3], v.to_vec()].concat(),
                    ValueRef::Blob(v) => [vec![4], v.to_vec()].concat(),
                })
            })
            .collect()
    })
    .unwrap()
    .collect::<Result<_, _>>()
    .unwrap()
}
fn evidence(db: &Connection) -> Vec<Vec<Vec<Vec<u8>>>> {
    [
        "accounting_periods",
        "period_close_decisions",
        "period_close_reviews",
        "period_close_readiness_packets",
        "journal_entries",
        "journal_lines",
        "sequences",
        "audit_log",
    ]
    .iter()
    .map(|t| rows(db, t, ""))
    .collect()
}
fn locked(tx: &Transaction<'_>) {
    let before = evidence(tx);
    let changes = tx.total_changes();
    let result = post(tx, "2025-08-02");
    assert_eq!(result["ok"], false, "{result}");
    assert_eq!(
        result["errors"],
        json!([
            "transactionDate 2025-08-02 falls in closed period fiscal_year 2025-07-01..2026-06-30"
        ])
    );
    assert_eq!(evidence(tx), before);
    assert_eq!(tx.total_changes(), changes);
}
fn assert_book(db: &Connection, journals: i64, closes: i64, reopens: i64) {
    for (sql, expected) in [
        ("SELECT count(*) FROM journal_entries", journals),
        ("SELECT count(*) FROM journal_lines", journals * 2),
        (
            "SELECT coalesce(sum(value),0) FROM sequences WHERE kind='journal_entry'",
            journals,
        ),
        (
            "SELECT count(*) FROM audit_log WHERE event_type='journal_post'",
            journals,
        ),
        (
            "SELECT count(*) FROM audit_log WHERE event_type='period_close'",
            closes,
        ),
        (
            "SELECT count(*) FROM audit_log WHERE event_type='period_reopen'",
            reopens,
        ),
        (
            "SELECT count(*) FROM period_close_decisions",
            closes + reopens,
        ),
    ] {
        assert_eq!(
            db.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap(),
            expected,
            "{sql}"
        );
    }
    assert!(crate::journal_hash::verify(db).unwrap().ok);
}

#[test]
fn period_race700_post_commits_before_close_stales_review_without_effects() {
    let (_dir, mut a, mut b) = fixture();
    let tx = writer(&mut a);
    let (input, reviewed_packet) = approve(&tx);
    tx.commit().unwrap();
    let holder = writer(&mut a);
    contends(&mut b);
    assert_eq!(post(&holder, "2025-08-01")["ok"], true);
    holder.commit().unwrap();
    let before = evidence(&b);
    let tx = writer(&mut b); // BEGIN-only retry, after acknowledged holder commit.
    let current = packet(&tx);
    assert_ne!(current["hash"], reviewed_packet["hash"]);
    let changes = tx.total_changes();
    let error = close(&tx, &input, &current, "synthetic", TODAY).unwrap_err();
    assert_eq!(error.code, "invoice_refused");
    assert_eq!(error.message, "PERIOD_CLOSE_PACKET_STALE_OR_MISSING");
    assert_eq!(tx.total_changes(), changes);
    assert_eq!(evidence(&tx), before);
    tx.rollback().unwrap();
    assert_eq!(evidence(&a), before);
    assert_book(&a, 1, 0, 0);
}

#[test]
fn period_race700_close_commits_before_post_refuses_current_writer() {
    let (_dir, mut a, mut b) = fixture();
    let holder = writer(&mut a);
    contends(&mut b);
    close_ready(&holder);
    holder.commit().unwrap();
    let before = evidence(&a);
    let tx = writer(&mut b);
    locked(&tx);
    tx.rollback().unwrap();
    assert_eq!(evidence(&b), before);
    assert_book(&a, 0, 1, 0);
}

#[test]
fn period_race700_post_before_reopen_refuses_then_deliberate_post_succeeds() {
    let (_dir, mut a, mut b) = fixture();
    let tx = writer(&mut a);
    close_ready(&tx);
    tx.commit().unwrap();
    let decision = rows(&a, "period_close_decisions", "WHERE decision='closed'");
    let before = evidence(&a);
    let holder = writer(&mut a);
    contends(&mut b);
    locked(&holder);
    holder.rollback().unwrap(); // Known refusal, no mutation to retry.
    assert_eq!(evidence(&b), before);
    let tx = writer(&mut b);
    reopen_ready(&tx);
    tx.commit().unwrap();
    let tx = writer(&mut a);
    assert_eq!(post(&tx, "2025-08-02")["ok"], true);
    tx.commit().unwrap();
    assert_eq!(
        rows(&b, "period_close_decisions", "WHERE decision='closed'"),
        decision
    );
    assert_book(&b, 1, 1, 1);
}

#[test]
fn period_race700_reopen_commits_before_post_uses_effective_state_preserves_close() {
    let (_dir, mut a, mut b) = fixture();
    let tx = writer(&mut a);
    close_ready(&tx);
    tx.commit().unwrap();
    let decision = rows(&a, "period_close_decisions", "WHERE decision='closed'");
    let period = rows(&a, "accounting_periods", "");
    let holder = writer(&mut a);
    contends(&mut b);
    reopen_ready(&holder);
    holder.commit().unwrap();
    let tx = writer(&mut b);
    assert_eq!(
        list(&tx, "synthetic").unwrap()["periods"][0]["effectiveStatus"],
        "open"
    );
    assert_eq!(post(&tx, "2025-08-02")["ok"], true);
    tx.commit().unwrap();
    assert_eq!(
        rows(&a, "period_close_decisions", "WHERE decision='closed'"),
        decision
    );
    assert_eq!(rows(&a, "accounting_periods", ""), period);
    assert_book(&a, 1, 1, 1);
}

fn report(tx: &Transaction<'_>, from: &str, to: &str) -> Value {
    crate::dated_reports::read(
        tx,
        &crate::dated_reports::Input {
            kind: crate::dated_reports::Kind::TrialBalance,
            from: Some(from.into()),
            to: Some(to.into()),
            as_of: None,
        },
    )
    .unwrap()
}
#[test]
fn period_race700_ordinary_reversal_next_shifted_year_preserves_closed_history() {
    let (_dir, mut a, mut b) = fixture();
    let tx = writer(&mut a);
    let original = post(&tx, "2025-08-01");
    assert_eq!(original["ok"], true, "{original}");
    assert_eq!(original["entryNo"], "2025-26-00001");
    close_ready(&tx);
    let old_report = report(&tx, START, END);
    assert_eq!(old_report["totalDebit"], 10);
    tx.commit().unwrap();
    let old_entry = rows(&a, "journal_entries", "WHERE id=1");
    let old_lines = rows(&a, "journal_lines", "WHERE journal_entry_id=1");
    let old_period = rows(&a, "accounting_periods", "");
    let old_decision = rows(&a, "period_close_decisions", "");
    let old_audit = rows(&a, "audit_log", "");
    let tx = writer(&mut b);
    let result = crate::journal_reversal::reverse(
        &tx,
        &crate::journal_reversal::Input {
            entry_id: rentemester_engine::identifiers::JournalEntryId::from_sqlite(
                original["entryId"].as_i64().unwrap(),
            ),
            transaction_date: Date::parse("2026-07-01").unwrap(),
            reason: "Synthetic ordinary correction".into(),
        },
        &crate::manual_journal_rule_version().unwrap(),
        Date::parse(TODAY).unwrap(),
        actor(),
    )
    .unwrap();
    assert_eq!(result["ok"], true, "{result}");
    assert_eq!(result["entryNo"], "2026-27-00001");
    assert_eq!(result["originalEntryId"], original["entryId"]);
    assert_eq!(report(&tx, START, END), old_report);
    let next = report(&tx, "2026-07-01", "2027-06-30");
    assert_eq!(next["journalEntryCount"], 1);
    assert_eq!(next["totalDebit"], 10);
    assert_eq!(next["totalCredit"], 10);
    assert_eq!(next["balanced"], true);
    for account in next["accounts"].as_array().unwrap() {
        let original_account = old_report["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["accountNo"] == account["accountNo"])
            .unwrap();
        assert_eq!(account["debit"], original_account["credit"]);
        assert_eq!(account["credit"], original_account["debit"]);
    }
    let combined = report(&tx, START, "2027-06-30");
    assert_eq!(combined["journalEntryCount"], 2);
    assert!(
        combined["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["balance"] == 0)
    );
    tx.commit().unwrap();
    assert_eq!(rows(&a, "journal_entries", "WHERE id=1"), old_entry);
    assert_eq!(
        rows(&a, "journal_lines", "WHERE journal_entry_id=1"),
        old_lines
    );
    assert_eq!(rows(&a, "accounting_periods", ""), old_period);
    assert_eq!(rows(&a, "period_close_decisions", ""), old_decision);
    assert_eq!(
        &rows(&a, "audit_log", "")[..old_audit.len()],
        old_audit.as_slice()
    );
    for (sql, expected) in [
        ("SELECT count(*) FROM journal_entries", 2),
        ("SELECT count(*) FROM journal_lines", 4),
        (
            "SELECT count(*) FROM sequences WHERE kind='journal_entry' AND value=1",
            2,
        ),
        (
            "SELECT count(*) FROM audit_log WHERE event_type='journal_reverse'",
            1,
        ),
        (
            "SELECT count(*) FROM journal_entries WHERE reversal_of_entry_id=1 AND transaction_date='2026-07-01'",
            1,
        ),
    ] {
        assert_eq!(
            a.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap(),
            expected,
            "{sql}"
        );
    }
    assert!(crate::journal_hash::verify(&a).unwrap().ok);
}
