//! Exact company suffix33–36 on disposable copies. Never a writable original API.
use crate::{Migration, Snapshot, invalid, require_columns};
use rentemester_engine::Error;
use rusqlite::{Connection, TransactionBehavior, params, types::ValueRef};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::Path};

const ARTIFACTS: [&str; 4] = [
    include_str!("../../../src/core/migrations/0033-bank-reconciliation-corrections.json"),
    include_str!("../../../src/core/migrations/0034-direct-bank-purchase-payable-corrections.json"),
    include_str!(
        "../../../src/core/migrations/0035-bank-reconciliation-account-role-fallback.json"
    ),
    include_str!("../../../src/core/migrations/0036-imported-receivables.json"),
];
#[derive(Deserialize)]
struct Artifact {
    id: Option<i64>,
    name: Option<String>,
    sql: String,
}
struct Step {
    identity: Migration,
    sql: String,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SchemaObject {
    pub(super) kind: String,
    pub(super) name: String,
    pub(super) table: String,
    pub(super) sql: String,
}
#[derive(Debug, Serialize)]
pub struct AppliedMigration {
    id: i64,
    name: String,
    checksum: String,
}
#[derive(Debug, Serialize)]
pub struct RehearsalReport {
    from_version: usize,
    to_version: usize,
    applied: Vec<AppliedMigration>,
    affected_schema: Vec<SchemaObject>,
    business_data_digest_before: String,
    business_data_digest_after: String,
    private_copy_discarded: bool,
    writer_authority: bool,
}
fn unsupported(message: impl Into<String>) -> Error {
    Error::new("unsupported_schema", message)
}
fn failed(error: impl std::fmt::Display) -> Error {
    Error::new("migration_failed", error.to_string())
}
fn catalogue() -> Result<Vec<Migration>, Error> {
    serde_json::from_str(include_str!("legacy-schema.json")).map_err(invalid)
}
fn steps(raw: &[&str], expected: &[Migration]) -> Result<Vec<Step>, Error> {
    if expected.len() != 36 || raw.len() != 4 {
        return Err(unsupported("migration suffix/catalogue revision differs"));
    }
    raw.iter()
        .zip(&expected[32..])
        .map(|(raw, identity)| {
            let a: Artifact = serde_json::from_str(raw).map_err(invalid)?;
            if format!("{:x}", Sha256::digest(raw.as_bytes())) != identity.checksum
                || a.id.is_some_and(|id| id != identity.id)
                || a.name.as_ref().is_some_and(|name| name != &identity.name)
                || a.sql.is_empty()
            {
                return Err(unsupported("embedded migration artifact identity differs"));
            }
            Ok(Step {
                identity: Migration {
                    id: identity.id,
                    name: identity.name.clone(),
                    checksum: identity.checksum.clone(),
                },
                sql: a.sql,
            })
        })
        .collect()
}
fn history(db: &Connection, expected: &[Migration]) -> Result<Vec<Vec<String>>, Error> {
    let rows = strict_history(db, "schema_migrations", expected)?;
    if !(32..=36).contains(&rows.len()) {
        return Err(unsupported(
            "only strict company prefixes32–36 qualify for this rehearsal",
        ));
    }
    Ok(rows)
}
pub(super) fn strict_history(
    db: &Connection,
    table: &str,
    expected: &[Migration],
) -> Result<Vec<Vec<String>>, Error> {
    let present: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE name=?1 AND type='table')",
            [table],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    if !present {
        return Err(unsupported("strict migration history table required"));
    }
    require_columns(
        db,
        table,
        &[
            ("id", "INTEGER", 0, 1),
            ("name", "TEXT", 1, 0),
            ("checksum", "TEXT", 1, 0),
            ("applied_at", "TEXT", 1, 0),
            ("applied_by_version", "TEXT", 1, 0),
            ("applied_by_commit", "TEXT", 0, 0),
        ],
    )?;
    let mut query = db.prepare(&format!("SELECT id,name,checksum,applied_at,applied_by_version,applied_by_commit FROM {} ORDER BY id", quote(table))).map_err(|_| unsupported("strict history required"))?;
    let mut rows = query.query([]).map_err(invalid)?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().map_err(invalid)? {
        let id: i64 = row.get(0).map_err(invalid)?;
        let name: String = row.get(1).map_err(invalid)?;
        let checksum: String = row.get(2).map_err(invalid)?;
        let Some(e) = expected.get(result.len()) else {
            return Err(unsupported("newer history"));
        };
        if id != e.id || name != e.name || checksum != e.checksum {
            return Err(unsupported("history is not an exact checksummed prefix"));
        }
        let at: String = row.get(3).map_err(invalid)?;
        let version: String = row.get(4).map_err(invalid)?;
        let commit: Option<String> = row.get(5).map_err(invalid)?;
        if at.trim().is_empty() || version.trim().is_empty() {
            return Err(invalid("empty migration provenance"));
        }
        // Null and empty commit remain distinct in the comparison.
        result.push(vec![
            id.to_string(),
            name,
            checksum,
            at,
            version,
            serde_json::to_string(&commit).map_err(invalid)?,
        ]);
    }
    Ok(result)
}
pub(super) fn integrity(db: &Connection) -> Result<(), Error> {
    let mut query = db.prepare("PRAGMA quick_check").map_err(invalid)?;
    let checks = query
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    if checks != ["ok"] {
        return Err(invalid("SQLite integrity check failed"));
    }
    let mut query = db.prepare("PRAGMA foreign_key_check").map_err(invalid)?;
    if query
        .query([])
        .map_err(invalid)?
        .next()
        .map_err(invalid)?
        .is_some()
    {
        return Err(invalid("foreign-key violations"));
    }
    Ok(())
}
pub(super) fn schema(db: &Connection) -> Result<Vec<SchemaObject>, Error> {
    db.prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema WHERE substr(name,1,7) <> 'sqlite_' ORDER BY name").map_err(invalid)?
        .query_map([], |r| Ok(SchemaObject { kind:r.get(0)?, name:r.get(1)?, table:r.get(2)?, sql:r.get(3)? })).map_err(invalid)?
        .collect::<Result<Vec<_>, _>>().map_err(invalid)
}
fn reference(steps: &[Step], version: usize) -> Result<Vec<SchemaObject>, Error> {
    let db = Connection::open_in_memory().map_err(invalid)?;
    for step in steps.iter().take(version - 32) {
        db.execute_batch(&step.sql).map_err(failed)?;
    }
    schema(&db)
}
fn checkpoint(db: &Connection, steps: &[Step], version: usize) -> Result<Vec<SchemaObject>, Error> {
    let all = reference(steps, 36)?;
    let names: BTreeSet<_> = all.iter().map(|o| &o.name).collect();
    let tables: BTreeSet<_> = all
        .iter()
        .filter(|o| o.kind == "table")
        .map(|o| &o.name)
        .collect();
    let actual: Vec<_> = schema(db)?
        .into_iter()
        .filter(|o| {
            (names.contains(&o.name) || tables.contains(&o.table))
                && !(version == 32 && o.name == "bank_journal_reconciliations" && o.kind == "view")
        })
        .collect();
    if actual != reference(steps, version)? {
        return Err(unsupported(
            "affected migration schema checkpoint differs; no repair attempted",
        ));
    }
    Ok(actual)
}
pub(super) fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
fn business_tables(db: &Connection) -> Result<Vec<String>, Error> {
    Ok(schema(db)?
        .into_iter()
        .filter(|o| o.kind == "table" && o.name != "schema_migrations")
        .map(|o| o.name)
        .collect())
}
pub(super) fn digest(db: &Connection, tables: &[String]) -> Result<String, Error> {
    let mut total = Sha256::new();
    for table in tables {
        total.update((table.len() as u64).to_le_bytes());
        total.update(table.as_bytes());
        let mut query = db
            .prepare(&format!("SELECT * FROM {}", quote(table)))
            .map_err(invalid)?;
        let columns = query.column_count();
        let mut rows = query.query([]).map_err(invalid)?;
        let mut hashes = Vec::new();
        while let Some(row) = rows.next().map_err(invalid)? {
            let mut hash = Sha256::new();
            hash.update((columns as u64).to_le_bytes());
            for col in 0..columns {
                match row.get_ref(col).map_err(invalid)? {
                    ValueRef::Null => hash.update([0]),
                    ValueRef::Integer(n) => {
                        hash.update([1]);
                        hash.update(n.to_le_bytes());
                    }
                    ValueRef::Real(n) => {
                        hash.update([2]);
                        hash.update(n.to_bits().to_le_bytes());
                    }
                    ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
                        hash.update([
                            if matches!(row.get_ref(col).map_err(invalid)?, ValueRef::Text(_)) {
                                3
                            } else {
                                4
                            },
                        ]);
                        hash.update((bytes.len() as u64).to_le_bytes());
                        hash.update(bytes);
                    }
                }
            }
            hashes.push(hash.finalize());
        }
        hashes.sort();
        total.update((hashes.len() as u64).to_le_bytes());
        for hash in hashes {
            total.update(hash);
        }
    }
    Ok(format!("{:x}", total.finalize()))
}
fn apply_step(db: &mut Connection, step: &Step) -> Result<(), Error> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(failed)?;
    tx.execute_batch(&step.sql).map_err(failed)?;
    #[cfg(test)]
    test_checkpoint("sql");
    tx.execute("INSERT INTO schema_migrations(id,name,checksum,applied_by_version,applied_by_commit) VALUES (?1,?2,?3,?4,NULL)", params![step.identity.id,step.identity.name,step.identity.checksum,concat!("rentemester-rust/", env!("CARGO_PKG_VERSION"))]).map_err(failed)?;
    #[cfg(test)]
    test_checkpoint("history");
    tx.commit().map_err(failed)?;
    #[cfg(test)]
    test_checkpoint("committed");
    Ok(())
}
fn rehearse(
    db: &mut Connection,
    steps: &[Step],
    expected: &[Migration],
) -> Result<RehearsalReport, Error> {
    db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA trusted_schema=OFF; PRAGMA synchronous=FULL;")
        .map_err(invalid)?;
    let old = history(db, expected)?;
    integrity(db)?;
    checkpoint(db, steps, old.len())?;
    let tables = business_tables(db)?;
    let before = digest(db, &tables)?;
    let mut applied = Vec::new();
    for step in steps.iter().skip(old.len() - 32) {
        apply_step(db, step)?;
        applied.push(AppliedMigration {
            id: step.identity.id,
            name: step.identity.name.clone(),
            checksum: step.identity.checksum.clone(),
        });
    }
    integrity(db)?;
    let affected_schema = checkpoint(db, steps, 36)?;
    let new = history(db, expected)?;
    let after = digest(db, &tables)?;
    if new.len() != 36 || new[..old.len()] != old || before != after {
        return Err(invalid(
            "migration changed preexisting provenance or business data",
        ));
    }
    Ok(RehearsalReport {
        from_version: old.len(),
        to_version: 36,
        applied,
        affected_schema,
        business_data_digest_before: before,
        business_data_digest_after: after,
        private_copy_discarded: true,
        writer_authority: false,
    })
}
/// Migrate only a private copy, return evidence and discard every private file.
pub fn rehearse_company_migration(company: &Path) -> Result<RehearsalReport, Error> {
    let expected = catalogue()?;
    let steps = steps(&ARTIFACTS, &expected)?;
    Snapshot::capture_unvalidated(company)?.with_writable_copy(|db| rehearse(db, &steps, &expected))
}

#[cfg(test)]
fn test_checkpoint(phase: &str) {
    if std::env::var("RM_MIGRATION_TEST_PHASE").as_deref() == Ok(phase) {
        std::fs::write(std::env::var("RM_MIGRATION_TEST_MARKER").unwrap(), phase).unwrap();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        process::{Child, Command},
        time::{Duration, Instant},
    };
    fn fixture(db: &mut Connection, version: usize) {
        db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;
            CREATE TABLE schema_migrations(id INTEGER PRIMARY KEY,name TEXT NOT NULL UNIQUE,checksum TEXT NOT NULL,applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,applied_by_version TEXT NOT NULL,applied_by_commit TEXT);
            CREATE TABLE accounts(id INTEGER PRIMARY KEY,account_no TEXT NOT NULL UNIQUE);
            CREATE TABLE dinero_import_attempts(id INTEGER PRIMARY KEY);
            CREATE TABLE synthetic_cells(a,b,c,d,e);
            INSERT INTO synthetic_cells VALUES (NULL,1,1.25,'unicode-ø',X'00FF'),(NULL,1,1.25,'unicode-ø',X'00FF');
            INSERT INTO accounts VALUES (1,'1000'); INSERT INTO dinero_import_attempts VALUES (1);").unwrap();
        for m in catalogue().unwrap().iter().take(version) {
            db.execute("INSERT INTO schema_migrations(id,name,checksum,applied_by_version) VALUES (?1,?2,?3,'synthetic-prior')",params![m.id,m.name,m.checksum]).unwrap();
        }
        for s in steps(&ARTIFACTS, &catalogue().unwrap())
            .unwrap()
            .iter()
            .take(version - 32)
        {
            db.execute_batch(&s.sql).unwrap();
        }
    }
    #[test]
    fn exact_artifact_bytes_and_optional_identity_are_bound() {
        let expected = catalogue().unwrap();
        assert_eq!(steps(&ARTIFACTS, &expected).unwrap().len(), 4);
        let mut changed = ARTIFACTS.to_vec();
        let changed_bytes = format!("{} ", ARTIFACTS[0]);
        changed[0] = &changed_bytes;
        assert_eq!(
            steps(&changed, &expected).err().unwrap().code,
            "unsupported_schema"
        );
        let mut forged = catalogue().unwrap();
        forged[32].name = "wrong".into();
        assert_eq!(
            steps(&ARTIFACTS, &forged).err().unwrap().code,
            "unsupported_schema"
        );
    }
    #[test]
    fn digest_preserves_type_multiplicity_and_bytes_without_row_order() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE cells(x);INSERT INTO cells VALUES (1),(1),('1'),(X'31'),(NULL);",
        )
        .unwrap();
        let tables = vec!["cells".to_string()];
        let before = digest(&db, &tables).unwrap();
        db.execute_batch("CREATE TABLE other AS SELECT * FROM cells ORDER BY rowid DESC;DELETE FROM cells;INSERT INTO cells SELECT * FROM other;").unwrap();
        assert_eq!(digest(&db, &tables).unwrap(), before);
        db.execute_batch("DELETE FROM cells WHERE rowid=(SELECT MIN(rowid) FROM cells)")
            .unwrap();
        assert_ne!(digest(&db, &tables).unwrap(), before);
    }
    #[test]
    fn suffix_rehearsal_and_receivable_constraints() {
        let mut db = Connection::open_in_memory().unwrap();
        fixture(&mut db, 35);
        let expected = catalogue().unwrap();
        let steps = steps(&ARTIFACTS, &expected).unwrap();
        let report = rehearse(&mut db, &steps, &expected).unwrap();
        assert_eq!(report.applied.len(), 1);
        assert_eq!(
            report.business_data_digest_before,
            report.business_data_digest_after
        );
        db.execute("INSERT INTO imported_receivable_headers(id,dinero_import_attempt_id,external_invoice_id,source_document_hash,invoice_date,gross_amount,control_account_no,source_recognition_ref,schedule_hash) VALUES (1,1,'synthetic',?1,'2026-01-01',123.45,'1000','ref',?1)",["a".repeat(64)]).unwrap();
        db.execute("INSERT INTO imported_receivable_events(id,receivable_id,external_event_id,event_kind,effective_date,amount,source_event_ref,source_document_hash,schedule_hash) VALUES (1,1,'payment','payment','2026-01-02',12.34,'event',?1,?1)",["b".repeat(64)]).unwrap();
        for sql in [
            "UPDATE imported_receivable_headers SET customer_name='changed' WHERE id=1",
            "DELETE FROM imported_receivable_headers WHERE id=1",
            "UPDATE imported_receivable_events SET amount=9 WHERE id=1",
            "DELETE FROM imported_receivable_events WHERE id=1",
            "INSERT INTO imported_receivable_events SELECT 2,999,'bad','payment','2026-01-02',12,'ref',source_document_hash,schedule_hash,created_at FROM imported_receivable_events WHERE id=1",
            "INSERT INTO imported_receivable_events SELECT 2,1,'bad','payment','2026-01-02',0,'ref',source_document_hash,schedule_hash,created_at FROM imported_receivable_events WHERE id=1",
            "INSERT INTO imported_receivable_events SELECT 2,1,'payment','payment','2026-01-02',12,'ref',source_document_hash,schedule_hash,created_at FROM imported_receivable_events WHERE id=1",
        ] {
            assert!(db.execute(sql, []).is_err(), "{sql}");
        }
        let again = rehearse(&mut db, &steps, &expected).unwrap();
        assert!(again.applied.is_empty());
    }
    #[test]
    fn sql_and_history_failure_roll_back_one_step_and_preserve_prior_commits() {
        let expected = catalogue().unwrap();
        let steps = steps(&ARTIFACTS, &expected).unwrap();
        for failure in ["sql", "history"] {
            let mut db = Connection::open_in_memory().unwrap();
            fixture(&mut db, 32);
            for step in &steps[..3] {
                apply_step(&mut db, step).unwrap();
            }
            let before = history(&db, &expected).unwrap();
            if failure == "sql" {
                db.execute_batch("CREATE VIEW imported_receivable_events AS SELECT 1;")
                    .unwrap();
            } else {
                db.execute_batch("CREATE TRIGGER refuse_history BEFORE INSERT ON schema_migrations WHEN NEW.id=36 BEGIN SELECT RAISE(ABORT,'synthetic insertion failure'); END;").unwrap();
            }
            assert_eq!(
                apply_step(&mut db, &steps[3]).unwrap_err().code,
                "migration_failed"
            );
            assert_eq!(history(&db, &expected).unwrap(), before);
            assert!(
                !schema(&db)
                    .unwrap()
                    .iter()
                    .any(|o| o.name == "imported_receivable_headers")
            );
            assert!(
                schema(&db)
                    .unwrap()
                    .iter()
                    .any(|o| o.name == "direct_bank_purchase_payable_corrections")
            );
        }
    }
    #[test]
    #[ignore = "subprocess helper selected by actual_process_death test"]
    fn crash_child() {
        let Ok(path) = std::env::var("RM_MIGRATION_TEST_DB") else {
            return;
        };
        let mut db = Connection::open(path).unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;")
            .unwrap();
        let steps = steps(&ARTIFACTS, &catalogue().unwrap()).unwrap();
        apply_step(&mut db, &steps[3]).unwrap();
    }
    struct Reaped(Child);
    impl Drop for Reaped {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    #[test]
    fn actual_process_death_never_leaves_half_applied_step() {
        for phase in ["sql", "history", "committed"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("private.sqlite");
            let marker = dir.path().join("phase");
            let mut db = Connection::open(&path).unwrap();
            fixture(&mut db, 35);
            drop(db);
            let mut child = Reaped(
                Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "migration::tests::crash_child",
                        "--ignored",
                        "--nocapture",
                    ])
                    .env("RM_MIGRATION_TEST_DB", &path)
                    .env("RM_MIGRATION_TEST_MARKER", &marker)
                    .env("RM_MIGRATION_TEST_PHASE", phase)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .unwrap(),
            );
            let start = Instant::now();
            while !marker.exists() {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "child stopped before {phase}"
                );
                assert!(
                    start.elapsed() < Duration::from_secs(10),
                    "child checkpoint timed out: {phase}"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(std::fs::read_to_string(&marker).unwrap(), phase);
            child.0.kill().unwrap();
            assert!(!child.0.wait().unwrap().success());
            let mut db = Connection::open(&path).unwrap();
            let expected = catalogue().unwrap();
            let steps = steps(&ARTIFACTS, &expected).unwrap();
            let rows = history(&db, &expected).unwrap();
            let version = if phase == "committed" { 36 } else { 35 };
            assert_eq!(rows.len(), version);
            checkpoint(&db, &steps, version).unwrap();
            integrity(&db).unwrap();
            if version == 35 {
                apply_step(&mut db, &steps[3]).unwrap();
            }
            assert_eq!(history(&db, &expected).unwrap().len(), 36);
            checkpoint(&db, &steps, 36).unwrap();
        }
    }
}
