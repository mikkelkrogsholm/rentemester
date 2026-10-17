//! Explicit known unversioned physical baseline rehearsal on owned disposable copies.
use crate::company_vat::{periods, validate_vat};
use crate::{
    Migration, Snapshot, company_baseline,
    company_normalization::{self, Effects},
    invalid,
    migration::{
        RehearsalReport, SchemaObject, integrity, project_digest, projection, quote, schema,
    },
};
use rentemester_engine::Error;
use rusqlite::{Connection, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
#[derive(Deserialize)]
struct Rule {
    table: String,
    source_sql: String,
    baseline_delta: Vec<SchemaObject>,
    current_delta: Vec<SchemaObject>,
}
#[derive(Deserialize)]
struct Contract {
    baseline_artifact_sha256: String,
    schema_images_sha256: String,
    rules: Vec<Rule>,
}
#[derive(Debug, Serialize)]
pub struct PhysicalRehearsalReport {
    source_layouts: Vec<String>,
    baseline_history_stamped: bool,
    vat_rows_relabeled: usize,
    unchanged_cell_digest_before: String,
    unchanged_cell_digest_after: String,
    data_normalization: Effects,
    migration: RehearsalReport,
    private_copy_discarded: bool,
    writer_authority: bool,
}
fn unsupported(s: &str) -> Error {
    Error::new("unsupported_schema", s)
}
fn failed(e: impl std::fmt::Display) -> Error {
    Error::new("migration_failed", e.to_string())
}
fn contract(raw: &str) -> Result<Contract, Error> {
    if format!("{:x}", Sha256::digest(raw.as_bytes()))
        != include_str!("legacy-company-physical.sha256").trim()
    {
        return Err(unsupported("physical schema catalogue checksum differs"));
    }
    let c: Contract = serde_json::from_str(raw).map_err(invalid)?;
    let identities: Vec<Migration> =
        serde_json::from_str(include_str!("legacy-schema.json")).map_err(invalid)?;
    if c.baseline_artifact_sha256 != identities[0].checksum
        || c.schema_images_sha256
            != format!(
                "{:x}",
                Sha256::digest(include_bytes!("legacy-company-schemas.json"))
            )
        || c.rules.len() != 4
    {
        return Err(unsupported("physical catalogue identity differs"));
    }
    Ok(c)
}
fn overlay(objects: &mut [SchemaObject], delta: &[SchemaObject]) -> Result<(), Error> {
    for changed in delta {
        let target = objects
            .iter_mut()
            .find(|o| o.name == changed.name)
            .ok_or_else(|| unsupported("physical overlay object is unknown"))?;
        *target = changed.clone();
    }
    Ok(())
}
fn rebuild(db: &Connection, rule: &Rule, normalized: &[SchemaObject]) -> Result<(), Error> {
    let table = normalized
        .iter()
        .find(|o| o.name == rule.table && o.kind == "table")
        .ok_or_else(|| unsupported("normalized table definition missing"))?;
    let (_, body) = table
        .sql
        .split_once('(')
        .ok_or_else(|| unsupported("normalized table body missing"))?;
    let scratch = format!("{}_rust_baseline_rebuild", rule.table);
    db.execute_batch(&format!("CREATE TABLE {} ({body}", quote(&scratch)))
        .map_err(failed)?;
    let cols = projection(db, std::slice::from_ref(&rule.table))?
        .remove(0)
        .1;
    let selected = cols
        .iter()
        .map(|c| {
            if rule.table == "accounting_periods" && c == "kind" {
                "CASE WHEN kind='vat_quarter' THEN 'vat_period' ELSE kind END".into()
            } else {
                quote(c)
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    db.execute_batch(&format!(
        "INSERT INTO {} ({}) SELECT {selected} FROM {}",
        quote(&scratch),
        cols.iter().map(|c| quote(c)).collect::<Vec<_>>().join(","),
        quote(&rule.table)
    ))
    .map_err(failed)?;
    #[cfg(test)]
    checkpoint(&rule.table, "copied");
    // All source objects have already matched the complete fixed catalogue.
    for o in schema(db)?
        .into_iter()
        .filter(|o| o.kind == "trigger" && o.table == rule.table)
    {
        db.execute_batch(&format!("DROP TRIGGER {}", quote(&o.name)))
            .map_err(failed)?;
    }
    db.execute_batch(&format!("DROP TABLE {}", quote(&rule.table)))
        .map_err(failed)?;
    #[cfg(test)]
    checkpoint(&rule.table, "dropped");
    db.execute_batch(&format!(
        "ALTER TABLE {} RENAME TO {}",
        quote(&scratch),
        quote(&rule.table)
    ))
    .map_err(failed)?;
    #[cfg(test)]
    checkpoint(&rule.table, "renamed");
    for o in normalized
        .iter()
        .filter(|o| o.table == rule.table && matches!(o.kind.as_str(), "index" | "trigger"))
    {
        db.execute_batch(&o.sql).map_err(failed)?;
    }
    Ok(())
}
fn rehearse(db: &mut Connection) -> Result<PhysicalRehearsalReport, Error> {
    let c = contract(include_str!("legacy-company-physical.json"))?;
    let mut images = company_baseline::load_images()?;
    db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA trusted_schema=OFF;PRAGMA synchronous=FULL")
        .map_err(invalid)?;
    let actual = schema(db)?;
    let mut admitted = images.baseline.clone();
    let mut selected = Vec::new();
    for rule in &c.rules {
        let a = actual
            .iter()
            .find(|o| o.kind == "table" && o.name == rule.table)
            .ok_or_else(|| unsupported("known baseline table missing"))?;
        if a.sql == rule.source_sql {
            let o = admitted.iter_mut().find(|o| o.name == rule.table).unwrap();
            o.sql = rule.source_sql.clone();
            selected.push(rule);
        }
    }
    admitted.sort_by(|a, b| a.name.cmp(&b.name));
    if actual != admitted {
        return Err(unsupported(
            "source is not a complete known physical baseline",
        ));
    }
    let history: i64 = db
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
        .map_err(invalid)?;
    if history != 0 {
        return Err(unsupported(
            "physical baseline rehearsal requires empty unversioned history",
        ));
    }
    integrity(db)?;
    let old_periods = periods(db)?;
    let legacy = old_periods
        .iter()
        .filter(|p| p.kind == "vat_quarter")
        .count();
    if selected.iter().any(|r| r.table == "accounting_periods") {
        validate_vat(db, &old_periods)?;
    }
    for r in &selected {
        overlay(&mut images.baseline, &r.baseline_delta)?;
        overlay(&mut images.current, &r.current_delta)?;
    }
    let cols = projection(
        db,
        &actual
            .iter()
            .filter(|o| o.kind == "table" && o.name != "schema_migrations")
            .map(|o| o.name.clone())
            .collect::<Vec<_>>(),
    )?;
    let stable: Vec<_> = cols
        .iter()
        .map(|(t, c)| {
            (
                t.clone(),
                c.iter()
                    .filter(|c| !(t == "accounting_periods" && c.as_str() == "kind"))
                    .cloned()
                    .collect(),
            )
        })
        .collect();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(failed)?;
    let before = project_digest(&tx, &stable)?;
    for r in &selected {
        rebuild(&tx, r, &images.baseline)?;
    }
    if legacy > 0 && !selected.iter().any(|r| r.table == "accounting_periods") {
        crate::company_vat::normalize_in_transaction(&tx)?;
    }
    let mut expected = images.baseline.clone();
    expected.sort_by(|a, b| a.name.cmp(&b.name));
    if schema(&tx)? != expected {
        return Err(unsupported("normalized physical schema differs"));
    }
    let after = project_digest(&tx, &stable)?;
    if before != after {
        return Err(invalid("physical rebuild changed original typed cells"));
    }
    let new_periods = periods(&tx)?;
    if old_periods.len() != new_periods.len()
        || old_periods.iter().zip(&new_periods).any(|(a, b)| {
            a.id != b.id
                || b.kind
                    != if a.kind == "vat_quarter" {
                        "vat_period"
                    } else {
                        &a.kind
                    }
        })
    {
        return Err(invalid("physical VAT relabel effects differ"));
    }
    let data_normalization = company_normalization::normalize_in_transaction(&tx)?;
    let identities: Vec<Migration> =
        serde_json::from_str(include_str!("legacy-schema.json")).map_err(invalid)?;
    company_baseline::stamp(&tx, &identities[0])?;
    integrity(&tx)?;
    #[cfg(test)]
    checkpoint("baseline", "history");
    tx.commit().map_err(failed)?;
    let source_layouts = selected.iter().map(|r| r.table.clone()).collect();
    let migration = company_baseline::rehearse_with_images(db, images)?;
    Ok(PhysicalRehearsalReport {
        source_layouts,
        baseline_history_stamped: true,
        vat_rows_relabeled: legacy,
        unchanged_cell_digest_before: before,
        unchanged_cell_digest_after: after,
        data_normalization,
        migration,
        private_copy_discarded: true,
        writer_authority: false,
    })
}
pub fn rehearse_company_baseline(company: &Path) -> Result<PhysicalRehearsalReport, Error> {
    Snapshot::capture_unvalidated(company)?.with_writable_copy(rehearse)
}
#[cfg(test)]
fn checkpoint(table: &str, phase: &str) {
    if std::env::var("RM_PHYSICAL_TABLE").as_deref() == Ok(table)
        && std::env::var("RM_PHYSICAL_PHASE").as_deref() == Ok(phase)
    {
        std::fs::write(std::env::var("RM_PHYSICAL_MARKER").unwrap(), phase).unwrap();
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
    fn fixture(db: &Connection, mask: usize) {
        let images = company_baseline::load_images().unwrap();
        let c = contract(include_str!("legacy-company-physical.json")).unwrap();
        for o in images.baseline {
            let sql = c
                .rules
                .iter()
                .enumerate()
                .find(|(i, r)| mask & (1 << i) != 0 && r.table == o.name)
                .map(|(_, r)| r.source_sql.as_str())
                .unwrap_or(&o.sql);
            db.execute_batch(sql).unwrap();
        }
        db.execute_batch("INSERT INTO companies(id,name,vat_period_type) VALUES(1,'Synthetic','quarter');INSERT INTO customers(id,name,payment_terms_days,created_at) VALUES(9007199254740993,'Synthetic customer',30,'2020-01-01 00:00:00');INSERT INTO gdpr_erasures(id,subject_key,source,source_row_id,redacted_fields,rule_id,reason,erased_at) VALUES(9007199254740993,'synthetic'||char(0)||'subject','customers',9007199254740993,'[]','synthetic','synthetic','2020-01-01 00:00:00');INSERT INTO accounting_periods(id,period_start,period_end,kind,status,closed_at,closed_by,reference,created_at) VALUES(9007199254740993,'2026-01-01','2026-03-31','vat_quarter','closed','2026-04-01','agent:synthetic','Synthetic reference','2020-01-01 00:00:00');INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('2000','Synthetic bank','asset','debit')").unwrap();
    }
    #[test]
    fn all_known_layout_combinations_preserve_data_and_restore_constraints() {
        for mask in 0..16 {
            let mut db = Connection::open_in_memory().unwrap();
            fixture(&db, mask);
            let report = rehearse(&mut db).unwrap();
            assert_eq!(report.vat_rows_relabeled, 1);
            assert_eq!(
                report.unchanged_cell_digest_before,
                report.unchanged_cell_digest_after
            );
            assert_eq!(report.migration.to_version, 36);
            assert_eq!(
                db.query_row(
                    "SELECT payment_terms_days FROM customers WHERE id=9007199254740993",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
                30
            );
            db.execute_batch("INSERT INTO customers(id,name,payment_terms_days) VALUES(2,'Synthetic new',NULL);UPDATE companies SET vat_period_type=NULL WHERE id=1;INSERT INTO gdpr_erasures(subject_key,source,source_row_id,redacted_fields,rule_id,reason) VALUES('other subject','customers',9007199254740993,'[]','synthetic','synthetic'),('audit subject','audit_log',1,'[]','synthetic','synthetic')").unwrap();
            db.execute_batch("INSERT INTO journal_entries(id,entry_no,transaction_date,text,entry_hash,rule_version) VALUES(1,'SYN-1','2027-01-01','Synthetic','synthetic','synthetic')").unwrap();
            assert!(
                db.execute_batch("UPDATE companies SET fiscal_year_start_month=2 WHERE id=1")
                    .is_err()
            );
            assert!(db.execute_batch("INSERT INTO gdpr_erasures(subject_key,source,source_row_id,redacted_fields,rule_id,reason) SELECT subject_key,source,source_row_id,redacted_fields,rule_id,reason FROM gdpr_erasures WHERE id=9007199254740993").is_err());
            assert!(db.execute_batch("UPDATE accounting_periods SET period_start='2026-02-01' WHERE id=9007199254740993").is_err());
            assert!(
                db.execute_batch("DELETE FROM accounting_periods WHERE id=9007199254740993")
                    .is_err()
            );
            for sql in [
                "UPDATE gdpr_erasures SET reason='changed' WHERE id=9007199254740993",
                "DELETE FROM gdpr_erasures WHERE id=9007199254740993",
                "UPDATE customers SET payment_terms_days=0 WHERE id=2",
                "UPDATE companies SET fiscal_year_start_month=99 WHERE id=1",
                "UPDATE companies SET vat_period_type='yearly' WHERE id=1",
            ] {
                assert!(db.execute_batch(sql).is_err(), "{mask}:{sql}");
            }
        }
    }
    #[test]
    fn catalogue_binding_unknown_objects_and_vat_conflicts_refuse() {
        assert!(
            contract(&format!(
                "{} ",
                include_str!("legacy-company-physical.json")
            ))
            .is_err()
        );
        for sql in [
            "CREATE TABLE extra(a)",
            "ALTER TABLE customers ADD COLUMN unexplained TEXT",
            "INSERT INTO accounting_periods(period_start,period_end,kind,status) VALUES('2026-01-01','2026-03-31','vat_period','closed')",
            "UPDATE companies SET vat_period_type='month'",
        ] {
            let mut db = Connection::open_in_memory().unwrap();
            fixture(&db, 0);
            db.execute_batch(sql).unwrap();
            let before = schema(&db).unwrap();
            assert!(rehearse(&mut db).is_err());
            assert_eq!(schema(&db).unwrap(), before);
        }
    }
    #[test]
    #[ignore = "actual process-death helper selected by parent"]
    fn crash_child() {
        let mut db = Connection::open(std::env::var("RM_PHYSICAL_DB").unwrap()).unwrap();
        let report = rehearse(&mut db).unwrap();
        if let Ok(output) = std::env::var("RM_PHYSICAL_OUTPUT") {
            std::fs::write(output,serde_json::to_vec(&serde_json::json!({"report":report,"tables":crate::test_support::typed_tables(&db)})).unwrap()).unwrap();
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
    fn actual_process_death_rolls_back_whole_unversioned_baseline() {
        for (table, phase) in [
            ("accounting_periods", "copied"),
            ("gdpr_erasures", "dropped"),
            ("customers", "renamed"),
            ("companies", "dropped"),
            ("baseline", "history"),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("db");
            let marker = dir.path().join("phase");
            let db = Connection::open(&path).unwrap();
            db.execute_batch("PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL")
                .unwrap();
            fixture(&db, 15);
            let old = schema(&db).unwrap();
            let cols = projection(
                &db,
                &old.iter()
                    .filter(|o| o.kind == "table")
                    .map(|o| o.name.clone())
                    .collect::<Vec<_>>(),
            )
            .unwrap();
            let before = project_digest(&db, &cols).unwrap();
            drop(db);
            let mut child = Reaped(
                Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "company_physical::tests::crash_child",
                        "--ignored",
                        "--nocapture",
                    ])
                    .env("RM_PHYSICAL_DB", &path)
                    .env("RM_PHYSICAL_TABLE", table)
                    .env("RM_PHYSICAL_PHASE", phase)
                    .env("RM_PHYSICAL_MARKER", &marker)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
            let now = Instant::now();
            while !marker.exists() {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "child terminated before {table}/{phase}"
                );
                assert!(now.elapsed() < Duration::from_secs(10));
                std::thread::sleep(Duration::from_millis(10));
            }
            child.0.kill().unwrap();
            assert!(!child.0.wait().unwrap().success());
            let mut db = Connection::open(&path).unwrap();
            assert_eq!(schema(&db).unwrap(), old);
            assert_eq!(project_digest(&db, &cols).unwrap(), before);
            rehearse(&mut db).unwrap();
        }
    }
}
