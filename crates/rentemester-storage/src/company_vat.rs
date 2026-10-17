//! Exact canonical VAT-kind normalization without replacing a referenced parent.
use crate::{
    invalid,
    migration::{integrity, project_digest, projection, schema},
};
use rentemester_engine::{Error, calendar::valid_iso_date};
use rusqlite::{Connection, OptionalExtension};
fn unsupported(s: &str) -> Error {
    Error::new("unsupported_schema", s)
}
fn failed(e: impl std::fmt::Display) -> Error {
    Error::new("migration_failed", e.to_string())
}
#[derive(Clone)]
pub(super) struct Period {
    pub(super) id: i64,
    start: String,
    end: String,
    pub(super) kind: String,
}
pub(super) fn periods(db: &Connection) -> Result<Vec<Period>, Error> {
    db.prepare("SELECT id,period_start,period_end,kind FROM accounting_periods ORDER BY id")
        .map_err(invalid)?
        .query_map([], |r| {
            Ok(Period {
                id: r.get(0)?,
                start: r.get(1)?,
                end: r.get(2)?,
                kind: r.get(3)?,
            })
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)
}
fn canonical(p: &Period, span: u32) -> bool {
    if !valid_iso_date(&p.start) || !valid_iso_date(&p.end) || &p.start[8..] != "01" {
        return false;
    }
    let year = p.start[..4].parse::<u32>().unwrap();
    let month = p.start[5..7].parse::<u32>().unwrap();
    if !(month - 1).is_multiple_of(span) {
        return false;
    }
    let last = month + span - 1;
    let mut day = 31;
    while !valid_iso_date(&format!("{year:04}-{last:02}-{day:02}")) {
        day -= 1;
    }
    p.end == format!("{year:04}-{last:02}-{day:02}")
}
pub(super) fn validate_vat(db: &Connection, rows: &[Period]) -> Result<(), Error> {
    let vat: Vec<_> = rows
        .iter()
        .filter(|p| matches!(p.kind.as_str(), "vat_period" | "vat_quarter"))
        .collect();
    if vat.iter().any(|p| {
        p.start
            .get(..4)
            .and_then(|s| s.parse::<u32>().ok())
            .is_some_and(|y| y < 100)
    }) {
        return Err(unsupported(
            "historical VAT years below100 require explicit resolution",
        ));
    }
    let cadence: Option<Option<String>> = db
        .query_row(
            "SELECT vat_period_type FROM companies WHERE id=1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(invalid)?;
    let cadence = cadence.unwrap_or(Some("quarter".into()));
    let span = match cadence.as_deref() {
        Some("month") => Some(1),
        Some("quarter") => Some(3),
        Some("half-year") => Some(6),
        _ => None,
    };
    if !span
        .map(|s| vat.iter().all(|p| canonical(p, s)))
        .unwrap_or_else(|| {
            [1, 3, 6]
                .iter()
                .any(|s| vat.iter().all(|p| canonical(p, *s)))
        })
    {
        return Err(invalid(
            "VAT periods do not share the required historical cadence",
        ));
    }
    let duplicate: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM accounting_periods legacy JOIN accounting_periods canonical ON canonical.kind='vat_period' AND canonical.period_start=legacy.period_start AND canonical.period_end=legacy.period_end WHERE legacy.kind='vat_quarter')", [], |r|r.get(0)).map_err(invalid)?;
    if duplicate {
        return Err(invalid(
            "duplicate legacy/canonical VAT periods require resolution",
        ));
    }
    let overlap: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM accounting_periods a JOIN accounting_periods b ON a.id<b.id WHERE a.kind IN ('vat_period','vat_quarter') AND b.kind IN ('vat_period','vat_quarter') AND a.status='reported' AND b.status='reported' AND NOT(a.period_end<b.period_start OR a.period_start>b.period_end))", [], |r|r.get(0)).map_err(invalid)?;
    if overlap {
        return Err(invalid(
            "overlapping reported VAT periods require resolution",
        ));
    }
    Ok(())
}
pub(super) fn normalize_in_transaction(db: &Connection) -> Result<usize, Error> {
    if db.is_autocommit() {
        return Err(invalid("VAT normalization requires an owned transaction"));
    }
    let old = periods(db)?;
    let legacy = old.iter().filter(|p| p.kind == "vat_quarter").count();
    if legacy == 0 {
        return Ok(0);
    }
    validate_vat(db, &old)?;
    let original_schema = schema(db)?;
    let tables = original_schema
        .iter()
        .filter(|o| o.kind == "table")
        .map(|o| o.name.clone())
        .collect::<Vec<_>>();
    let stable = projection(db, &tables)?
        .into_iter()
        .map(|(t, cols)| {
            let selected = cols
                .into_iter()
                .filter(|c| !(t == "accounting_periods" && c == "kind"))
                .collect();
            (t, selected)
        })
        .collect::<Vec<_>>();
    let before = project_digest(db, &stable)?;
    let guard:Option<String>=db.query_row("SELECT sql FROM sqlite_schema WHERE type='trigger' AND name='accounting_periods_guard_update' AND tbl_name='accounting_periods'",[],|r|r.get(0)).optional().map_err(invalid)?;
    db.execute_batch("DROP TRIGGER IF EXISTS accounting_periods_guard_update")
        .map_err(failed)?;
    #[cfg(test)]
    checkpoint("guard");
    let changed = db
        .execute(
            "UPDATE accounting_periods SET kind='vat_period' WHERE kind='vat_quarter'",
            [],
        )
        .map_err(failed)?;
    #[cfg(test)]
    checkpoint("updated");
    if let Some(guard) = guard {
        db.execute_batch(&guard).map_err(failed)?;
    }
    #[cfg(test)]
    checkpoint("restored");
    let new = periods(db)?;
    if changed != legacy
        || old.len() != new.len()
        || old.iter().zip(&new).any(|(a, b)| {
            a.id != b.id
                || b.kind
                    != if a.kind == "vat_quarter" {
                        "vat_period"
                    } else {
                        &a.kind
                    }
        })
        || schema(db)? != original_schema
        || project_digest(db, &stable)? != before
    {
        return Err(invalid(
            "VAT normalization changed unrelated cells, kinds, history or guards",
        ));
    }
    integrity(db)?;
    Ok(legacy)
}
#[cfg(test)]
fn checkpoint(phase: &str) {
    if std::env::var("RM_VAT_PHASE").as_deref() == Ok(phase) {
        std::fs::write(std::env::var("RM_VAT_MARKER").unwrap(), phase).unwrap();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        process::{Child, Command, Stdio},
        time::{Duration, Instant},
    };
    fn fixture(db: &mut Connection) {
        crate::company_baseline::rehearse(db).unwrap();
        db.execute_batch("INSERT INTO companies(id,name,vat_period_type) VALUES(1,'Synthetic','quarter');INSERT INTO accounting_periods(id,period_start,period_end,kind,status,reference,created_at) VALUES(9007199254740993,'2027-01-01','2027-03-31','vat_quarter','closed','Synthetic','2020-01-01'),(7,'2026-01-01','2026-12-31','fiscal_year','open','Preserve fiscal','2020-01-01');INSERT INTO period_close_readiness_packets(id,packet_hash,period_start,period_end,cutoff,packet_json,created_at) VALUES(1,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','2027-01-01','2027-03-31','2027-04-01','{}','2020-01-01');INSERT INTO period_close_decisions(id,period_id,packet_hash,decision,actor,created_at) VALUES(1,9007199254740993,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','closed','agent:synthetic','2020-01-01');INSERT INTO period_close_open_items(id,decision_id,period_id,packet_hash,code,severity,count,amount,evidence_json,reason,actor,created_at) VALUES(1,1,9007199254740993,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','synthetic','warning',1,10,'{}','synthetic','agent:synthetic','2020-01-01')").unwrap();
    }
    fn all_columns(db: &Connection) -> Vec<(String, Vec<String>)> {
        projection(
            db,
            &schema(db)
                .unwrap()
                .into_iter()
                .filter(|o| o.kind == "table")
                .map(|o| o.name)
                .collect::<Vec<_>>(),
        )
        .unwrap()
    }
    #[test]
    fn current_fk_children_exact_schema_history_and_nonlegacy_cells_survive() {
        let mut db = Connection::open_in_memory().unwrap();
        fixture(&mut db);
        let old = schema(&db).unwrap();
        let report = crate::company_baseline::rehearse(&mut db).unwrap();
        let v = serde_json::to_value(report).unwrap();
        assert_eq!(v["normalization"]["vat_rows_relabeled"], 1);
        assert_eq!(v["schema_scope"], "full-normalized-company-prefix");
        assert_eq!(schema(&db).unwrap(), old);
        assert_eq!(
            db.query_row(
                "SELECT kind FROM accounting_periods WHERE id=9007199254740993",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "vat_period"
        );
        assert_eq!(
            db.query_row("SELECT kind FROM accounting_periods WHERE id=7", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            "fiscal_year"
        );
        integrity(&db).unwrap();
        let cols = all_columns(&db);
        let before = project_digest(&db, &cols).unwrap();
        let again =
            serde_json::to_value(crate::company_baseline::rehearse(&mut db).unwrap()).unwrap();
        assert_eq!(again["normalization"]["vat_rows_relabeled"], 0);
        assert_eq!(project_digest(&db, &cols).unwrap(), before);
        assert!(
            db.execute_batch(
                "UPDATE accounting_periods SET kind='vat_quarter' WHERE id=9007199254740993"
            )
            .is_err()
        );
        assert!(
            db.execute_batch("DELETE FROM period_close_decisions")
                .is_err()
        );
    }
    #[test]
    fn other_trigger_and_later_data_failure_roll_back_kind_and_named_guard() {
        for late in [false, true] {
            let mut db = Connection::open_in_memory().unwrap();
            fixture(&mut db);
            if late {
                db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('2000','Synthetic','asset','debit');INSERT INTO bank_transactions(id,transaction_date,text,amount) VALUES(1,'2026-01-01','Synthetic',1);INSERT INTO journal_entries(id,entry_no,transaction_date,text,source_bank_transaction_id,entry_hash,rule_version) VALUES(1,'SYN-1','2026-01-01','Synthetic',1,'synthetic','synthetic');INSERT INTO documents(id,source,sha256_hash,invoice_date) VALUES(1,'synthetic','synthetic','2026-01-01')").unwrap();
            } else {
                db.execute_batch("CREATE TRIGGER independent_guard BEFORE UPDATE ON accounting_periods BEGIN SELECT RAISE(ABORT,'independent VAT refusal');END").unwrap();
            }
            let cols = all_columns(&db);
            let before = project_digest(&db, &cols).unwrap();
            let old = schema(&db).unwrap();
            assert_eq!(
                crate::company_normalization::normalize(&mut db)
                    .unwrap_err()
                    .code,
                "migration_failed"
            );
            assert_eq!(project_digest(&db, &cols).unwrap(), before);
            assert_eq!(schema(&db).unwrap(), old);
            integrity(&db).unwrap();
        }
    }
    #[test]
    #[ignore = "actual VAT crash/parity child invoked by parent/independent harness"]
    fn crash_child() {
        let mut db = Connection::open(std::env::var("RM_VAT_DB").unwrap()).unwrap();
        let report = crate::company_baseline::rehearse(&mut db).unwrap();
        if let Ok(out) = std::env::var("RM_VAT_OUTPUT") {
            std::fs::write(out,serde_json::to_vec(&serde_json::json!({"report":report,"tables":crate::test_support::typed_tables(&db)})).unwrap()).unwrap();
        }
    }
    struct Reaped(Child);
    impl Drop for Reaped {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    #[test]
    fn actual_process_death_restores_exact_guard_and_current_fk_data() {
        for phase in ["guard", "updated", "restored"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("db");
            let marker = dir.path().join("phase");
            let mut db = Connection::open(&path).unwrap();
            db.execute_batch("PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL")
                .unwrap();
            fixture(&mut db);
            let old = schema(&db).unwrap();
            let cols = all_columns(&db);
            let before = project_digest(&db, &cols).unwrap();
            drop(db);
            let mut child = Reaped(
                Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "company_vat::tests::crash_child",
                        "--ignored",
                        "--nocapture",
                    ])
                    .env("RM_VAT_DB", &path)
                    .env("RM_VAT_PHASE", phase)
                    .env("RM_VAT_MARKER", &marker)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
            let now = Instant::now();
            while !marker.exists() {
                assert!(child.0.try_wait().unwrap().is_none());
                assert!(now.elapsed() < Duration::from_secs(10));
                std::thread::sleep(Duration::from_millis(10));
            }
            child.0.kill().unwrap();
            assert!(!child.0.wait().unwrap().success());
            let mut db = Connection::open(&path).unwrap();
            assert_eq!(schema(&db).unwrap(), old);
            assert_eq!(project_digest(&db, &cols).unwrap(), before);
            integrity(&db).unwrap();
            crate::company_baseline::rehearse(&mut db).unwrap();
        }
    }
}
