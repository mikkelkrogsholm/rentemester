//! Native normalized company bootstrap and canonical early-prefix upgrades.
use crate::migration::{
    AppliedMigration, RehearsalReport, SchemaObject, integrity, project_digest, projection, quote,
    schema, strict_history,
};
use crate::{Migration, invalid};
use rentemester_engine::Error;
use rusqlite::{Connection, TransactionBehavior, params};
use serde::Deserialize;
use sha2::{Digest, Sha256};
const RAW: [&str; 35] = [
    include_str!("../../../src/core/migrations/0002-peppol-submission-events.json"),
    include_str!("../../../src/core/migrations/0003-recurring-automation.json"),
    include_str!("../../../src/core/migrations/0004-dinero-import-provenance.json"),
    include_str!("../../../src/core/migrations/0005-migration-open-items.json"),
    include_str!("../../../src/core/migrations/0006-bank-journal-reconciliation-links.json"),
    include_str!("../../../src/core/migrations/0007-document-scan-evidence.json"),
    include_str!("../../../src/core/migrations/0008-issued-invoice-pdf-immutability.json"),
    include_str!("../../../src/core/migrations/0009-accounting-draft-workflow.json"),
    include_str!("../../../src/core/migrations/0010-internal-voucher-evidence.json"),
    include_str!("../../../src/core/migrations/0011-purchase-vat-preflight.json"),
    include_str!("../../../src/core/migrations/0012-posting-rules.json"),
    include_str!("../../../src/core/migrations/0013-bookkeeping-batches.json"),
    include_str!("../../../src/core/migrations/0014-invoice-extraction-evidence.json"),
    include_str!("../../../src/core/migrations/0015-bookkeeping-batch-retries.json"),
    include_str!("../../../src/core/migrations/0016-invoice-extraction-actors.json"),
    include_str!("../../../src/core/migrations/0017-document-pdf-parses.json"),
    include_str!("../../../src/core/migrations/0018-document-metadata-enrichments.json"),
    include_str!("../../../src/core/migrations/0019-document-company-contexts.json"),
    include_str!("../../../src/core/migrations/0020-mutation-idempotency-receipts.json"),
    include_str!("../../../src/core/migrations/0021-bookkeeping-batch-revisions.json"),
    include_str!("../../../src/core/migrations/0022-period-close-readiness.json"),
    include_str!("../../../src/core/migrations/0023-local-idempotency-tombstones.json"),
    include_str!("../../../src/core/migrations/0024-bookkeeping-batch-principals.json"),
    include_str!("../../../src/core/migrations/0025-period-close-reviews.json"),
    include_str!("../../../src/core/migrations/0026-document-party-links.json"),
    include_str!("../../../src/core/migrations/0027-supplier-commitments.json"),
    include_str!("../../../src/core/migrations/0028-accounting-dimensions.json"),
    include_str!("../../../src/core/migrations/0029-document-party-resolution.json"),
    include_str!("../../../src/core/migrations/0030-accounting-dimension-lifecycle.json"),
    include_str!("../../../src/core/migrations/0031-supplier-commitment-occurrence-matches.json"),
    include_str!("../../../src/core/migrations/0032-dimension-budget-and-provenance.json"),
    include_str!("../../../src/core/migrations/0033-bank-reconciliation-corrections.json"),
    include_str!("../../../src/core/migrations/0034-direct-bank-purchase-payable-corrections.json"),
    include_str!(
        "../../../src/core/migrations/0035-bank-reconciliation-account-role-fallback.json"
    ),
    include_str!("../../../src/core/migrations/0036-imported-receivables.json"),
];
#[derive(Deserialize)]
struct Images {
    baseline_artifact_sha256: String,
    baseline: Vec<SchemaObject>,
    current: Vec<SchemaObject>,
}
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
fn unsupported(message: &str) -> Error {
    Error::new("unsupported_schema", message)
}
fn failed(error: impl std::fmt::Display) -> Error {
    Error::new("migration_failed", error.to_string())
}
fn catalogue() -> Result<Vec<Migration>, Error> {
    serde_json::from_str(include_str!("legacy-schema.json")).map_err(invalid)
}
fn images(raw: &str, expected: &[Migration]) -> Result<Images, Error> {
    if format!("{:x}", Sha256::digest(raw.as_bytes()))
        != include_str!("legacy-company-schemas.sha256").trim()
    {
        return Err(unsupported("normalized schema image checksum differs"));
    }
    let images: Images = serde_json::from_str(raw).map_err(invalid)?;
    if images.baseline_artifact_sha256 != expected[0].checksum {
        return Err(unsupported("normalized baseline identity differs"));
    }
    Ok(images)
}
fn steps(raw: &[&str], expected: &[Migration]) -> Result<Vec<Step>, Error> {
    if raw.len() != 35 || expected.len() != 36 {
        return Err(unsupported("company catalogue revision differs"));
    }
    raw.iter()
        .zip(&expected[1..])
        .map(|(raw, m)| {
            let a: Artifact = serde_json::from_str(raw).map_err(invalid)?;
            if a.id.is_some_and(|id| id != m.id)
                || a.name.as_ref().is_some_and(|name| name != &m.name)
                || format!("{:x}", Sha256::digest(raw.as_bytes())) != m.checksum
            {
                return Err(unsupported("company artifact differs"));
            }
            Ok(Step {
                identity: Migration {
                    id: m.id,
                    name: m.name.clone(),
                    checksum: m.checksum.clone(),
                },
                sql: a.sql,
            })
        })
        .collect()
}
fn create_baseline(db: &Connection, images: &Images) -> Result<(), Error> {
    for object in &images.baseline {
        db.execute_batch(&object.sql).map_err(failed)?;
    }
    Ok(())
}
fn final_guards(db: &Connection, images: &Images) -> Result<(), Error> {
    for column in ["bic", "account_owner", "customer_no"] {
        let present: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('bank_accounts') WHERE name=?1)",
                [column],
                |r| r.get(0),
            )
            .map_err(invalid)?;
        if !present {
            db.execute_batch(&format!(
                "ALTER TABLE bank_accounts ADD COLUMN {column} TEXT"
            ))
            .map_err(failed)?;
        }
    }
    for object in images.current.iter().filter(|o| o.kind == "trigger") {
        db.execute_batch(&format!("DROP TRIGGER IF EXISTS {};", quote(&object.name)))
            .map_err(failed)?;
        db.execute_batch(&object.sql).map_err(failed)?;
    }
    Ok(())
}
fn reference(images: &Images, steps: &[Step], version: usize) -> Result<Vec<SchemaObject>, Error> {
    let db = Connection::open_in_memory().map_err(invalid)?;
    if version > 0 {
        create_baseline(&db, images)?;
        for step in steps.iter().take(version - 1) {
            db.execute_batch(&step.sql).map_err(failed)?;
        }
    }
    schema(&db)
}
fn stamp(db: &Connection, m: &Migration) -> Result<(), Error> {
    db.execute("INSERT INTO schema_migrations(id,name,checksum,applied_by_version,applied_by_commit) VALUES (?1,?2,?3,?4,NULL)",params![m.id,m.name,m.checksum,concat!("rentemester-rust/",env!("CARGO_PKG_VERSION"))]).map_err(failed)?;
    Ok(())
}
fn apply_step(db: &mut Connection, step: &Step, images: &Images) -> Result<(), Error> {
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(failed)?;
    tx.execute_batch(&step.sql).map_err(failed)?;
    if step.identity.id == 36 {
        final_guards(&tx, images)?;
    }
    #[cfg(test)]
    test_checkpoint(step.identity.id, "sql");
    stamp(&tx, &step.identity)?;
    #[cfg(test)]
    test_checkpoint(step.identity.id, "history");
    tx.commit().map_err(failed)
}
fn count(db: &Connection, sql: &str) -> Result<i64, Error> {
    db.query_row(sql, [], |r| r.get(0)).map_err(invalid)
}
fn introduced_invariants(db: &Connection, from: usize) -> Result<(), Error> {
    let mut violations = Vec::new();
    if from < 3 {
        violations.extend([
        "SELECT COUNT(*) FROM recurring_invoice_templates WHERE interval_count<>1 OR delivery_channel<>'manual'",
        "SELECT ABS((SELECT COUNT(*) FROM recurring_invoice_generation_claims)-(SELECT COUNT(*) FROM recurring_invoice_generations))",
        "SELECT COUNT(*) FROM recurring_invoice_generations g WHERE NOT EXISTS(SELECT 1 FROM recurring_invoice_generation_claims c WHERE c.template_id=g.template_id AND c.period_index=g.period_index)",
    ]);
    }
    if from < 11 {
        violations.push("SELECT COUNT(*) FROM exceptions WHERE resolution_key IS NOT NULL");
    }
    if from < 16 {
        violations.extend([
            "SELECT COUNT(*) FROM invoice_extraction_attempts WHERE initiated_by IS NOT NULL",
            "SELECT COUNT(*) FROM invoice_extraction_results WHERE initiated_by IS NOT NULL",
        ]);
    }
    if from < 21 {
        violations.push(
            "SELECT COUNT(*) FROM bookkeeping_batch_approval_policy WHERE self_approval_allowed<>0",
        );
    }
    if from < 30 {
        violations.extend(["SELECT COUNT(*) FROM accounting_dimension_definition_events WHERE status<>'active' OR supersedes_dimension_id IS NOT NULL","SELECT COUNT(*) FROM accounting_dimension_member_events WHERE supersedes_member_id IS NOT NULL"]);
    }
    if from < 32 {
        violations.push("SELECT COUNT(*) FROM accounting_dimension_assignment_events WHERE source_ref IS NOT NULL");
    }
    for sql in violations {
        if count(db, sql)? != 0 {
            return Err(invalid("introduced company migration defaults differ"));
        }
    }
    if count(
        db,
        "SELECT COUNT(*) FROM ledger_identity WHERE id=1 AND length(ledger_uuid)=32 AND ledger_uuid NOT GLOB '*[^0-9a-f]*'",
    )? != 1
        || count(
            db,
            "SELECT COUNT(*) FROM bookkeeping_batch_approval_policy WHERE id=1",
        )? != 1
    {
        return Err(invalid("required company singleton missing"));
    }
    Ok(())
}
pub(super) fn rehearse(db: &mut Connection) -> Result<RehearsalReport, Error> {
    let expected = catalogue()?;
    let images = images(include_str!("legacy-company-schemas.json"), &expected)?;
    let steps = steps(&RAW, &expected)?;
    db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA trusted_schema=OFF;PRAGMA synchronous=FULL")
        .map_err(invalid)?;
    let old = if schema(db)?.is_empty() {
        Vec::new()
    } else {
        strict_history(db, "schema_migrations", &expected)?
    };
    let from = old.len();
    if from >= 32 || schema(db)? != reference(&images, &steps, from)? {
        return Err(unsupported(
            "source is not a known normalized early company prefix",
        ));
    }
    integrity(db)?;
    let tables: Vec<_> = schema(db)?
        .into_iter()
        .filter(|o| o.kind == "table" && o.name != "schema_migrations")
        .map(|o| o.name)
        .collect();
    let columns = projection(db, &tables)?;
    let before = project_digest(db, &columns)?;
    let mut applied = Vec::new();
    if from == 0 {
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(failed)?;
        create_baseline(&tx, &images)?;
        #[cfg(test)]
        test_checkpoint(1, "sql");
        stamp(&tx, &expected[0])?;
        #[cfg(test)]
        test_checkpoint(1, "history");
        tx.commit().map_err(failed)?;
        applied.push(AppliedMigration {
            id: 1,
            name: expected[0].name.clone(),
            checksum: expected[0].checksum.clone(),
        });
    }
    for step in steps.iter().filter(|s| s.identity.id > from.max(1) as i64) {
        apply_step(db, step, &images)?;
        applied.push(AppliedMigration {
            id: step.identity.id,
            name: step.identity.name.clone(),
            checksum: step.identity.checksum.clone(),
        });
    }
    integrity(db)?;
    introduced_invariants(db, from)?;
    let current = schema(db)?;
    let mut expected_schema = images.current;
    expected_schema.sort_by(|a, b| a.name.cmp(&b.name));
    if current != expected_schema {
        return Err(unsupported("completed company schema differs"));
    }
    let new = strict_history(db, "schema_migrations", &expected)?;
    let after = project_digest(db, &columns)?;
    if new.len() != 36 || new[..from] != old || before != after {
        return Err(invalid(
            "company upgrade changed original projections/provenance",
        ));
    }
    Ok(RehearsalReport {
        schema_scope: "full-normalized-early-prefix",
        from_version: from,
        to_version: 36,
        applied,
        affected_schema: current,
        business_data_digest_before: before,
        business_data_digest_after: after,
        private_copy_discarded: true,
        writer_authority: false,
    })
}
#[cfg(test)]
fn test_checkpoint(step: i64, phase: &str) {
    if std::env::var("RM_COMPANY_BASE_TEST_STEP").ok().as_deref() == Some(step.to_string().as_str())
        && std::env::var("RM_COMPANY_BASE_TEST_PHASE").as_deref() == Ok(phase)
    {
        std::fs::write(std::env::var("RM_COMPANY_BASE_TEST_MARKER").unwrap(), phase).unwrap();
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
    fn prefix(db: &Connection, version: usize) {
        let expected = catalogue().unwrap();
        let images = images(include_str!("legacy-company-schemas.json"), &expected).unwrap();
        let steps = steps(&RAW, &expected).unwrap();
        if version > 0 {
            create_baseline(db, &images).unwrap();
            stamp(db, &expected[0]).unwrap();
            for s in steps.iter().take(version - 1) {
                db.execute_batch(&s.sql).unwrap();
                stamp(db, &s.identity).unwrap();
            }
        }
    }
    #[test]
    fn fresh_schema_is_complete_and_images_are_byte_bound() {
        let expected = catalogue().unwrap();
        let raw = include_str!("legacy-company-schemas.json");
        assert!(images(&format!("{raw} "), &expected).is_err());
        let mut db = Connection::open_in_memory().unwrap();
        let report = rehearse(&mut db).unwrap();
        assert_eq!(report.applied.len(), 36);
        assert_eq!(report.from_version, 0);
        assert_eq!(schema(&db).unwrap().len(), 455);
        db.execute_batch("INSERT INTO documents(id,source,sha256_hash) VALUES(1,'synthetic','synthetic-native');INSERT INTO purchase_posting_applications(id,document_id,application_key,status) VALUES(1,1,'synthetic','started')").unwrap();
        for sql in [
            "UPDATE purchase_posting_applications SET application_key='changed' WHERE id=1",
            "UPDATE purchase_posting_applications SET status='posted' WHERE id=1",
            "DELETE FROM purchase_posting_applications WHERE id=1",
        ] {
            assert!(db.execute(sql, []).is_err(), "{sql}");
        }

        assert_eq!(
            strict_history(&db, "schema_migrations", &expected)
                .unwrap()
                .len(),
            36
        );
    }
    #[test]
    fn original_column_digest_ignores_new_columns_but_detects_changed_old_cells() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE cells(a,b);INSERT INTO cells VALUES(1,X'00FF'),(1,X'00FF')")
            .unwrap();
        let cols = projection(&db, &["cells".into()]).unwrap();
        let before = project_digest(&db, &cols).unwrap();
        db.execute_batch("ALTER TABLE cells ADD COLUMN new_value TEXT DEFAULT 'new'")
            .unwrap();
        assert_eq!(project_digest(&db, &cols).unwrap(), before);
        db.execute_batch("UPDATE cells SET a=2 WHERE rowid=1")
            .unwrap();
        assert_ne!(project_digest(&db, &cols).unwrap(), before);
    }
    #[test]
    fn populated_recurring_columns_and_dimension_defaults_preserve_old_values() {
        let mut db = Connection::open_in_memory().unwrap();
        prefix(&db, 2);
        db.execute("INSERT INTO recurring_invoice_templates(name,interval,first_issue_date,next_issue_date,payload_json) VALUES ('synthetic','monthly','2026-01-01','2026-01-01','{}')",[]).unwrap();
        db.execute_batch("INSERT INTO documents(id,source,sha256_hash) VALUES(1,'synthetic','synthetic-generation');INSERT INTO recurring_invoice_generations(id,template_id,period_index,document_id,invoice_number,issue_date) VALUES(1,1,0,1,'SYNTHETIC-1','2026-01-01')").unwrap();
        let report = rehearse(&mut db).unwrap();
        assert_eq!(
            report.business_data_digest_before,
            report.business_data_digest_after
        );
        introduced_invariants(&db, 2).unwrap();
        let mut db = Connection::open_in_memory().unwrap();
        prefix(&db, 29);
        db.execute_batch("INSERT INTO accounting_dimension_definition_events(dimension_id,kind,name,event_type,actor,principal,created_at) VALUES('synthetic','project','Synthetic','defined','agent:synthetic','agent:synthetic','2026-01-01');INSERT INTO accounting_dimension_member_events(dimension_id,member_id,name,status,event_type,actor,principal,created_at) VALUES('synthetic','member','Synthetic','inactive','defined','agent:synthetic','agent:synthetic','2026-01-01')").unwrap();
        let report = rehearse(&mut db).unwrap();
        assert_eq!(
            report.business_data_digest_before,
            report.business_data_digest_after
        );
    }
    #[test]
    #[ignore = "subprocess helper selected by process-death parent"]
    fn crash_child() {
        let mut db = Connection::open(std::env::var("RM_COMPANY_BASE_TEST_DB").unwrap()).unwrap();
        rehearse(&mut db).unwrap();
    }
    struct Reaped(Child);
    impl Drop for Reaped {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    #[test]
    fn bootstrap_and_early_step_crashes_leave_complete_prefixes() {
        for (start, step, phase) in [
            (0, 1, "sql"),
            (0, 1, "history"),
            (2, 3, "sql"),
            (2, 3, "history"),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("private.sqlite");
            let marker = dir.path().join("phase");
            let db = Connection::open(&path).unwrap();
            db.execute_batch("PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL")
                .unwrap();
            prefix(&db, start);
            drop(db);
            let mut child = Reaped(
                Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "company_baseline::tests::crash_child",
                        "--ignored",
                        "--nocapture",
                    ])
                    .env("RM_COMPANY_BASE_TEST_DB", &path)
                    .env("RM_COMPANY_BASE_TEST_STEP", step.to_string())
                    .env("RM_COMPANY_BASE_TEST_PHASE", phase)
                    .env("RM_COMPANY_BASE_TEST_MARKER", &marker)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
            let now = Instant::now();
            while !marker.exists() {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "child stopped before checkpoint"
                );
                assert!(
                    now.elapsed() < Duration::from_secs(10),
                    "checkpoint timeout"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            child.0.kill().unwrap();
            assert!(!child.0.wait().unwrap().success());
            let mut db = Connection::open(&path).unwrap();
            let expected = catalogue().unwrap();
            if start == 0 {
                assert!(schema(&db).unwrap().is_empty());
            } else {
                assert_eq!(
                    strict_history(&db, "schema_migrations", &expected)
                        .unwrap()
                        .len(),
                    start
                );
            }
            rehearse(&mut db).unwrap();
        }
    }
}
