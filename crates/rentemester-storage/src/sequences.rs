//! Exact sequence values inside a caller-owned transaction; never write authority.
use rentemester_engine::Error;
use rusqlite::{Connection, MAIN_DB, OptionalExtension, Transaction, TransactionState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Reservation {
    pub ok: bool,
    pub current_value: i64,
    pub expected_value: i64,
}
fn storage(_: rusqlite::Error) -> Error {
    Error::new(
        "storage_failure",
        "sequence storage operation failed; abort transaction",
    )
}
fn nonnegative(value: i64) -> Result<i64, Error> {
    if value < 0 {
        Err(Error::new(
            "invalid_input",
            "sequence input must be nonnegative",
        ))
    } else {
        Ok(value)
    }
}
/// Pure scope composition. Caller supplies already-normalized trusted company CVR and fiscal label.
pub fn company_scope(cvr: Option<&str>, company_id: i64, fiscal_scope: &str) -> String {
    format!(
        "{}:{fiscal_scope}",
        cvr.map(str::to_owned)
            .unwrap_or_else(|| format!("company-{company_id}"))
    )
}
/// Advisory observation only; it does not reserve the returned value.
pub fn current(db: &Connection, kind: &str, scope: &str, floor: i64) -> Result<i64, Error> {
    nonnegative(floor)?;
    let value: Option<Option<i64>> = db
        .query_row(
            "SELECT value FROM main.sequences WHERE kind=?1 AND scope=?2",
            [kind, scope],
            |r| {
                Ok(match r.get_ref(0)? {
                    rusqlite::types::ValueRef::Integer(n) => Some(n),
                    _ => None,
                })
            },
        )
        .optional()
        .map_err(storage)?;
    let stored = match value {
        None => 0,
        Some(Some(n)) if n >= 0 => n,
        _ => {
            return Err(Error::new(
                "invalid_ledger",
                "stored sequence must be a nonnegative INTEGER",
            ));
        }
    };
    Ok(stored.max(floor))
}
fn expected(
    tx: &Transaction<'_>,
    kind: &str,
    scope: &str,
    floor: i64,
) -> Result<(i64, i64), Error> {
    if tx.transaction_state(Some(MAIN_DB)).map_err(storage)? != TransactionState::Write {
        return Err(Error::new(
            "transaction_required",
            "sequence allocation requires main write transaction",
        ));
    }
    let value = current(tx, kind, scope, floor)?;
    let next = value.checked_add(1).ok_or_else(|| {
        Error::new(
            "sequence_exhausted",
            "sequence exhausted; abort transaction",
        )
    })?;
    Ok((value, next))
}
fn write(tx: &Transaction<'_>, kind: &str, scope: &str, value: i64) -> Result<(), Error> {
    let count=tx.execute("INSERT INTO main.sequences(kind,scope,value) VALUES(?1,?2,?3) ON CONFLICT(kind,scope) DO UPDATE SET value=excluded.value",rusqlite::params![kind,scope,value]).map_err(storage)?;
    if count != 1 || current(tx, kind, scope, 0)? != value {
        return Err(Error::new(
            "storage_failure",
            "sequence write was not retained; abort transaction",
        ));
    }
    Ok(())
}
/// Allocate under the caller's write lock. Every error requires caller rollback.
pub fn next(tx: &Transaction<'_>, kind: &str, scope: &str, floor: i64) -> Result<i64, Error> {
    let (_, value) = expected(tx, kind, scope, floor)?;
    write(tx, kind, scope, value)?;
    Ok(value)
}
/// Reserve exactly current+1. Mismatch is a nonmutating successful result.
pub fn reserve(
    tx: &Transaction<'_>,
    kind: &str,
    scope: &str,
    requested: i64,
    floor: i64,
) -> Result<Reservation, Error> {
    nonnegative(requested)?;
    let (current_value, expected_value) = expected(tx, kind, scope, floor)?;
    let ok = requested == expected_value;
    if ok {
        write(tx, kind, scope, requested)?;
    }
    Ok(Reservation {
        ok,
        current_value,
        expected_value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::TransactionBehavior;
    use std::{
        process::{Child, Command, Stdio},
        time::{Duration, Instant},
    };
    fn fixture() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        crate::company_baseline::rehearse(&mut db).unwrap();
        db
    }
    fn immediate(db: &mut Connection) -> Transaction<'_> {
        db.transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap()
    }
    #[test]
    fn exact_values_scope_guards_and_owner_rollback() {
        let mut db = fixture();
        assert_eq!(company_scope(None, 7, "2026"), "company-7:2026");
        assert_eq!(company_scope(Some(""), 7, "2026"), ":2026");
        assert_eq!(company_scope(Some("ø\0"), 7, "a:b"), "ø\0:a:b");
        assert_eq!(current(&db, "journal", "a", 30).unwrap(), 30);
        {
            let tx = immediate(&mut db);
            assert_eq!(next(&tx, "journal", "a", 30).unwrap(), 31);
            assert_eq!(
                reserve(&tx, "journal", "a", 33, 0).unwrap(),
                Reservation {
                    ok: false,
                    current_value: 31,
                    expected_value: 32
                }
            );
            assert_eq!(current(&tx, "journal", "a", 0).unwrap(), 31);
            assert!(reserve(&tx, "journal", "a", 32, 0).unwrap().ok);
            assert_eq!(next(&tx, "journal", "b", 0).unwrap(), 1);
            assert_eq!(next(&tx, "invoice", "a", 0).unwrap(), 1);
            tx.commit().unwrap();
        }
        {
            let tx = immediate(&mut db);
            assert_eq!(next(&tx, "journal", "a", 0).unwrap(), 33);
            tx.execute(
                "INSERT INTO documents(id,source,sha256_hash) VALUES(100,'synthetic','rollback')",
                [],
            )
            .unwrap();
            tx.rollback().unwrap();
        }
        assert_eq!(current(&db, "journal", "a", 0).unwrap(), 32);
        assert_eq!(
            db.query_row("SELECT count(*) FROM documents WHERE id=100", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        for sql in [
            "UPDATE sequences SET value=0",
            "UPDATE sequences SET scope='changed'",
            "DELETE FROM sequences",
        ] {
            assert!(db.execute_batch(sql).is_err());
        }
        {
            let tx = immediate(&mut db);
            assert_eq!(
                next(&tx, "large", "", 9_007_199_254_740_992).unwrap(),
                9_007_199_254_740_993
            );
            assert_eq!(next(&tx, "maximum", "", i64::MAX - 1).unwrap(), i64::MAX);
            assert_eq!(
                next(&tx, "maximum", "", 0).unwrap_err().code,
                "sequence_exhausted"
            );
            tx.rollback().unwrap();
        }
        assert_eq!(current(&db, "large", "", 0).unwrap(), 0);
    }
    #[test]
    fn invalid_storage_deferred_and_failed_sql_require_owner_abort() {
        let mut db = fixture();
        {
            let tx = db.transaction().unwrap();
            assert_eq!(
                next(&tx, "k", "s", 0).unwrap_err().code,
                "transaction_required"
            );
            assert_eq!(
                reserve(&tx, "k", "s", 1, 0).unwrap_err().code,
                "transaction_required"
            );
        }
        assert_eq!(
            current(&db, "k", "s", -1).unwrap_err().code,
            "invalid_input"
        );
        db.execute("INSERT INTO sequences VALUES('real','s',1.5)", [])
            .unwrap();
        assert_eq!(
            current(&db, "real", "s", 0).unwrap_err().code,
            "invalid_ledger"
        );
        for sql in [
            "INSERT INTO sequences VALUES('bad','blob',x'80')",
            "INSERT INTO sequences VALUES('bad','text',CAST(x'80' AS TEXT))",
        ] {
            db.execute_batch(sql).unwrap();
        }
        for scope in ["blob", "text"] {
            assert_eq!(
                current(&db, "bad", scope, 0).unwrap_err().code,
                "invalid_ledger"
            );
        }

        {
            let tx = immediate(&mut db);
            assert_eq!(
                reserve(&tx, "k", "s", -1, 0).unwrap_err().code,
                "invalid_input"
            );
            assert_eq!(next(&tx, "k", "s", -1).unwrap_err().code, "invalid_input");
        }
        db.execute_batch("CREATE TRIGGER fail_sequence AFTER INSERT ON sequences WHEN NEW.kind='failure' BEGIN SELECT RAISE(FAIL,'private evidence'); END").unwrap();
        {
            let tx = immediate(&mut db);
            next(&tx, "prior", "s", 0).unwrap();
            let e = next(&tx, "failure", "s", 0).unwrap_err();
            assert_eq!(e.code, "storage_failure");
            assert!(!e.message.contains("private evidence"));
            // RAISE(FAIL) can retain statement writes: abort the entire owner transaction.
        }
        assert_eq!(current(&db, "failure", "s", 0).unwrap(), 0);
        assert_eq!(current(&db, "prior", "s", 0).unwrap(), 0);
    }
    #[test]
    fn wal_reader_snapshot_and_deferred_write_state_are_explicit() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("ledger.sqlite");
        let mut writer = Connection::open(&path).unwrap();
        crate::company_baseline::rehearse(&mut writer).unwrap();
        writer
            .execute_batch("PRAGMA journal_mode=WAL; INSERT INTO sequences VALUES('k','s',5)")
            .unwrap();
        let mut reader = Connection::open(&path).unwrap();
        let read = reader.transaction().unwrap();
        assert_eq!(current(&read, "k", "s", 0).unwrap(), 5);
        assert_eq!(
            next(&read, "k", "s", 0).unwrap_err().code,
            "transaction_required"
        );
        let write = immediate(&mut writer);
        assert_eq!(next(&write, "k", "s", 0).unwrap(), 6);
        assert_eq!(current(&read, "k", "s", 0).unwrap(), 5);
        write.commit().unwrap();
        assert_eq!(current(&read, "k", "s", 0).unwrap(), 5);
        read.rollback().unwrap();
        assert_eq!(current(&reader, "k", "s", 0).unwrap(), 6);
        let deferred = writer.transaction().unwrap();
        deferred.execute("INSERT INTO documents(id,source,sha256_hash) VALUES(100,'synthetic','write-state')",[]).unwrap();
        assert_eq!(next(&deferred, "k", "s", 0).unwrap(), 7);
        deferred.rollback().unwrap();
        assert_eq!(current(&writer, "k", "s", 0).unwrap(), 6);
        writer.execute_batch("CREATE TEMP TABLE sequences(kind TEXT,scope TEXT,value INTEGER);INSERT INTO temp.sequences VALUES('k','s',900)").unwrap();
        let write = immediate(&mut writer);
        assert_eq!(next(&write, "k", "s", 0).unwrap(), 7);
        write.commit().unwrap();
        assert_eq!(
            writer
                .query_row("SELECT value FROM temp.sequences", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            900
        );
    }
    fn child(path: &std::path::Path, mode: &str, marker: &std::path::Path) -> Child {
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sequences::tests::process_child",
                "--ignored",
                "--nocapture",
            ])
            .env("RM_SEQUENCE_DB", path)
            .env("RM_SEQUENCE_MODE", mode)
            .env("RM_SEQUENCE_MARKER", marker)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }
    #[test]
    fn concurrent_processes_commit_unique_numbers_and_one_reservation() {
        for mode in ["next", "reserve"] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("ledger.sqlite");
            let mut db = Connection::open(&path).unwrap();
            crate::company_baseline::rehearse(&mut db).unwrap();
            db.execute("INSERT INTO sequences VALUES('race','scope',5)", [])
                .unwrap();
            drop(db);
            let marker = temp.path().join("start");
            let children = (0..8)
                .map(|_| child(&path, mode, &marker))
                .collect::<Vec<_>>();
            std::fs::write(&marker, "").unwrap();
            let mut values = Vec::new();
            for c in children {
                let o = c.wait_with_output().unwrap();
                assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
                let text = String::from_utf8(o.stdout).unwrap();
                let line = text
                    .lines()
                    .find_map(|l| l.strip_prefix("VALUES:"))
                    .unwrap();
                values.extend(serde_json::from_str::<Vec<i64>>(line).unwrap());
            }
            values.sort();
            if mode == "next" {
                assert_eq!(values, (6..166).collect::<Vec<_>>());
            } else {
                assert_eq!(values, vec![6]);
            }
            let db = Connection::open(&path).unwrap();
            assert_eq!(
                current(&db, "race", "scope", 0).unwrap(),
                *values.last().unwrap()
            );
        }
    }
    #[test]
    fn killed_process_rolls_back_sequence_and_companion_row() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("ledger.sqlite");
        let mut db = Connection::open(&path).unwrap();
        crate::company_baseline::rehearse(&mut db).unwrap();
        db.execute("INSERT INTO sequences VALUES('race','scope',5)", [])
            .unwrap();
        drop(db);
        let marker = temp.path().join("allocated");
        let mut c = child(&path, "crash", &marker);
        let deadline = Instant::now() + Duration::from_secs(15);
        while !marker.exists() {
            if c.try_wait().unwrap().is_some() || Instant::now() > deadline {
                let _ = c.kill();
                let _ = c.wait();
                panic!("child failed to reach allocated checkpoint");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        c.kill().unwrap();
        assert!(!c.wait().unwrap().success());
        let mut db = Connection::open(&path).unwrap();
        assert_eq!(current(&db, "race", "scope", 0).unwrap(), 5);
        assert_eq!(
            db.query_row("SELECT count(*) FROM documents WHERE id=100", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        let tx = immediate(&mut db);
        assert_eq!(next(&tx, "race", "scope", 0).unwrap(), 6);
        tx.commit().unwrap();
    }
    #[test]
    #[ignore = "synthetic process/parity helper selected explicitly"]
    fn process_child() {
        let mut db = Connection::open(std::env::var("RM_SEQUENCE_DB").unwrap()).unwrap();
        db.busy_timeout(Duration::from_secs(10)).unwrap();
        let mode = std::env::var("RM_SEQUENCE_MODE").unwrap();
        if mode == "parity" {
            let operations: Vec<serde_json::Value> =
                serde_json::from_str(&std::env::var("RM_SEQUENCE_OPS").unwrap()).unwrap();
            let mut results = Vec::new();
            for o in operations {
                let op = o["op"].as_str().unwrap();
                if op == "scope" {
                    results.push(serde_json::json!(company_scope(
                        o["cvr"].as_str(),
                        o["id"].as_i64().unwrap(),
                        o["scope"].as_str().unwrap()
                    )));
                    continue;
                }
                let kind = o["kind"].as_str().unwrap();
                let scope = o["scope"].as_str().unwrap();
                let floor = o["floor"].as_i64().unwrap();
                if op == "current" {
                    results.push(serde_json::json!(current(&db, kind, scope, floor).unwrap()));
                    continue;
                }
                let tx = immediate(&mut db);
                results.push(if op == "next" {
                    serde_json::json!(next(&tx, kind, scope, floor).unwrap())
                } else {
                    serde_json::to_value(
                        reserve(&tx, kind, scope, o["requested"].as_i64().unwrap(), floor).unwrap(),
                    )
                    .unwrap()
                });
                if o["rollback"].as_bool().unwrap_or(false) {
                    tx.rollback().unwrap();
                } else {
                    tx.commit().unwrap();
                }
            }
            let tables = crate::test_support::typed_tables(&db);
            std::fs::write(
                std::env::var("RM_SEQUENCE_OUTPUT").unwrap(),
                serde_json::to_vec(
                    &serde_json::json!({"results":results,"sequences":tables["sequences"]}),
                )
                .unwrap(),
            )
            .unwrap();
            return;
        }
        let marker = std::path::PathBuf::from(std::env::var("RM_SEQUENCE_MARKER").unwrap());
        let deadline = Instant::now() + Duration::from_secs(15);
        if mode != "crash" {
            while !marker.exists() {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        let mut values = Vec::new();
        for _ in 0..if mode == "next" { 20 } else { 1 } {
            let tx = immediate(&mut db);
            if mode == "reserve" {
                if reserve(&tx, "race", "scope", 6, 0).unwrap().ok {
                    values.push(6);
                }
            } else {
                values.push(next(&tx, "race", "scope", 0).unwrap());
            }
            if mode == "crash" {
                tx.execute(
                    "INSERT INTO documents(id,source,sha256_hash) VALUES(100,'synthetic','crash')",
                    [],
                )
                .unwrap();
                std::fs::write(&marker, "").unwrap();
                loop {
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
            tx.commit().unwrap();
        }
        println!("VALUES:{}", serde_json::to_string(&values).unwrap());
    }
}
