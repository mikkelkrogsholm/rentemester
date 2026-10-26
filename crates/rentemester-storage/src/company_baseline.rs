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
#[derive(Clone, Deserialize)]
pub(super) struct Images {
    baseline_artifact_sha256: String,
    pub(super) baseline: Vec<SchemaObject>,
    pub(super) current: Vec<SchemaObject>,
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
// All DDL/history for a fresh ledger is one transaction; no source normalization.
pub(super) fn initialize_empty(db: &mut Connection, company_name: &str) -> Result<(), Error> {
    if !schema(db)?.is_empty() {
        return Err(unsupported("native company requires an empty database"));
    }
    let expected = catalogue()?;
    let images = load_images()?;
    let steps = steps(&RAW, &expected)?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(failed)?;
    create_baseline(&tx, &images)?;
    stamp(&tx, &expected[0])?;
    for step in &steps {
        tx.execute_batch(&step.sql).map_err(failed)?;
        stamp(&tx, &step.identity)?;
    }
    final_guards(&tx, &images)?;
    tx.execute(
        "INSERT INTO companies(id,name) VALUES(1,?1)",
        [company_name],
    )
    .map_err(failed)?;
    integrity(&tx)?;
    introduced_invariants(&tx, 0)?;
    let mut current = images.current;
    current.sort_by(|a, b| a.name.cmp(&b.name));
    if schema(&tx)? != current || strict_history(&tx, "schema_migrations", &expected)?.len() != 36 {
        return Err(unsupported("fresh company schema or history differs"));
    }
    crate::asset_disposal_schema::install(&tx)?;
    crate::bank_statement_order::install(&tx)?;
    tx.commit().map_err(failed)
}
fn reference(images: &Images, steps: &[Step], version: usize) -> Result<Vec<SchemaObject>, Error> {
    let db = Connection::open_in_memory().map_err(invalid)?;
    if version > 0 {
        create_baseline(&db, images)?;
        for step in steps.iter().take(version - 1) {
            db.execute_batch(&step.sql).map_err(failed)?;
        }
    }
    if version == 36 {
        final_guards(&db, images)?;
    }
    schema(&db)
}
pub(super) fn recognizes(db: &Connection, version: usize) -> Result<bool, Error> {
    if version > 36 {
        return Ok(false);
    }
    let expected = catalogue()?;
    let images = load_images()?;
    let steps = steps(&RAW, &expected)?;
    Ok(schema(db)? == reference(&images, &steps, version)?)
}
pub(super) fn stamp(db: &Connection, m: &Migration) -> Result<(), Error> {
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
    rehearse_with_images(db, images)
}
pub(super) fn load_images() -> Result<Images, Error> {
    images(include_str!("legacy-company-schemas.json"), &catalogue()?)
}
pub(super) fn rehearse_with_images(
    db: &mut Connection,
    images: Images,
) -> Result<RehearsalReport, Error> {
    let expected = catalogue()?;
    let steps = steps(&RAW, &expected)?;
    db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA trusted_schema=OFF;PRAGMA synchronous=FULL")
        .map_err(invalid)?;
    let old = if schema(db)?.is_empty() {
        Vec::new()
    } else {
        strict_history(db, "schema_migrations", &expected)?
    };
    let from = old.len();
    if from > 36 || schema(db)? != reference(&images, &steps, from)? {
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
    let normalization = if from > 0 {
        Some(crate::company_normalization::normalize(db)?)
    } else {
        None
    };
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
        normalization,
        schema_scope: if from < 32 {
            "full-normalized-early-prefix"
        } else {
            "full-normalized-company-prefix"
        },
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
    fn retained_conversion_accepts_populated_full_prefixes_and_preserves_source() {
        for version in [1, 2, 31, 32, 36] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::create_dir(dir.path().join("data")).unwrap();
            let path = dir.path().join("data/ledger.sqlite");
            let db = Connection::open(&path).unwrap();
            prefix(&db, version);
            if version == 36 {
                final_guards(&db, &load_images().unwrap()).unwrap();
            }
            db.execute_batch("INSERT INTO companies(id,name) VALUES(1,'Synthetic adoption');INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('1890','Synthetic bank','asset','debit');INSERT INTO customers(name,payment_terms_days) VALUES('Synthetic buyer',30)").unwrap();
            crate::legacy_adoption::seed_test_journal(&db);
            drop(db);
            let original = std::fs::read(&path).unwrap();
            let converted =
                crate::legacy_adoption::convert_company_for_adoption(dir.path()).unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), original);
            assert_eq!(converted.source_files()[0].bytes(), original.len() as u64);
            assert_eq!(
                converted.source_files()[0].sha256(),
                format!("{:x}", Sha256::digest(&original))
            );
            assert_eq!(converted.source_files()[0].path(), "data/ledger.sqlite");
            assert_eq!(converted.inventory().ledger_uuid().len(), 32);
            let report = serde_json::to_value(converted.report()).unwrap();
            assert_eq!(report["writer_authority"], false);
            assert_eq!(report["source_retired"], false);
            assert_eq!(report["conversion"]["from_version"], version);
            let result_path = dir.path().join("converted.sqlite");
            std::fs::write(&result_path, converted.bytes()).unwrap();
            let result = Connection::open(&result_path).unwrap();
            crate::portable_backup::company(&result).unwrap();
            assert_eq!(
                crate::journal_hash::verify(&result)
                    .unwrap()
                    .entries_checked,
                1
            );

            assert_eq!(
                result
                    .query_row(
                        "SELECT COUNT(*) FROM accounts WHERE account_no='1890'",
                        [],
                        |r| r.get::<_, i64>(0)
                    )
                    .unwrap(),
                1
            );
            assert_eq!(
                result
                    .query_row(
                        "SELECT COUNT(*) FROM customers WHERE name='Synthetic buyer'",
                        [],
                        |r| r.get::<_, i64>(0)
                    )
                    .unwrap(),
                1
            );
        }
    }
    #[test]
    fn retained_conversion_refuses_empty_and_suffix_only_databases() {
        for suffix_only in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::create_dir(dir.path().join("data")).unwrap();
            let path = dir.path().join("data/ledger.sqlite");
            let db = Connection::open(&path).unwrap();
            if suffix_only {
                let images = load_images().unwrap();
                let history = images
                    .current
                    .iter()
                    .find(|o| o.name == "schema_migrations")
                    .unwrap();
                db.execute_batch(&history.sql).unwrap();
                for step in catalogue().unwrap() {
                    stamp(&db, &step).unwrap();
                }
            }
            drop(db);
            let before = std::fs::read(&path).unwrap();
            assert!(crate::legacy_adoption::convert_company_for_adoption(dir.path()).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
    }
    #[test]
    fn retained_conversion_refuses_unknown_schema_bad_history_and_missing_original() {
        for damage in [
            "CREATE TABLE unexplained(value)",
            "ALTER TABLE customers ADD COLUMN unexplained TEXT",
            "DROP TRIGGER schema_migrations_no_update; UPDATE schema_migrations SET checksum='bad' WHERE id=1",
            "INSERT INTO documents(source,sha256_hash) VALUES('synthetic','missing-original')",
        ] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::create_dir(dir.path().join("data")).unwrap();
            let path = dir.path().join("data/ledger.sqlite");
            let mut db = Connection::open(&path).unwrap();
            initialize_empty(&mut db, "Synthetic refused").unwrap();
            // History mutation needs the actual immutable trigger name, if any.
            if damage.starts_with("DROP TRIGGER") {
                for object in schema(&db)
                    .unwrap()
                    .into_iter()
                    .filter(|o| o.kind == "trigger" && o.table == "schema_migrations")
                {
                    db.execute_batch(&format!("DROP TRIGGER {}", quote(&object.name)))
                        .unwrap();
                }
                db.execute_batch("UPDATE schema_migrations SET checksum='bad' WHERE id=1")
                    .unwrap();
            } else {
                db.execute_batch(damage).unwrap();
            }
            drop(db);
            let original = std::fs::read(&path).unwrap();
            assert!(
                crate::legacy_adoption::convert_company_for_adoption(dir.path()).is_err(),
                "{damage}"
            );
            assert_eq!(std::fs::read(&path).unwrap(), original);
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
    #[test]
    fn normalization_seeds_audited_roles_backfills_only_missing_and_is_idempotent() {
        let mut db = Connection::open_in_memory().unwrap();
        prefix(&db, 2);
        db.execute_batch("INSERT INTO companies(id,name,fiscal_year_start_month) VALUES(1,'Synthetic',3);INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('2000','Synthetic bank','asset','debit'),('1200','Synthetic VAT','vat','credit');INSERT INTO documents(id,source,sha256_hash,invoice_date) VALUES(1,'synthetic','retention','2024-02-29');INSERT INTO documents(id,source,sha256_hash,invoice_date,retain_until) VALUES(2,'synthetic','preserved','2024-02-29','existing-invalid')").unwrap();
        let first = crate::company_normalization::normalize(&mut db).unwrap();
        let value = serde_json::to_value(first).unwrap();
        assert_eq!(value["role_mappings_inserted"], 3);
        assert_eq!(value["document_deadlines_filled"], 1);
        assert_eq!(
            db.query_row("SELECT retain_until FROM documents WHERE id=1", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            "2029-02-28"
        );
        assert_eq!(
            db.query_row("SELECT retain_until FROM documents WHERE id=2", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            "existing-invalid"
        );
        let second =
            serde_json::to_value(crate::company_normalization::normalize(&mut db).unwrap())
                .unwrap();
        assert_eq!(second["role_mappings_inserted"], 0);
        assert_eq!(second["document_deadlines_filled"], 0);
    }
    #[test]
    fn normalization_preserves_original_full_width_ids_with_sqlite_random_rowid_allocation() {
        let mut db = Connection::open_in_memory().unwrap();
        prefix(&db, 2);
        db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('2000','Synthetic','asset','debit');INSERT INTO audit_log(id,event_type,entity_type,message,actor,created_at) VALUES(9223372036854775807,'synthetic','synthetic','Must remain','agent:synthetic','2020-01-01 00:00:00')").unwrap();
        crate::company_normalization::normalize(&mut db).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT message FROM audit_log WHERE id=9223372036854775807",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "Must remain"
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM audit_log", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
    }
    #[test]
    fn normalization_guard_failure_rolls_back_seeds_and_prior_deadline_updates() {
        let mut db = Connection::open_in_memory().unwrap();
        prefix(&db, 2);
        db.execute_batch("INSERT INTO companies(id,name) VALUES(1,'Synthetic');INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('2000','Synthetic','asset','debit');INSERT INTO bank_transactions(id,transaction_date,text,amount,currency) VALUES(1,'2026-01-01','Synthetic',1,'DKK');INSERT INTO journal_entries(id,entry_no,transaction_date,text,source_bank_transaction_id,entry_hash,previous_hash,created_by,created_by_program,rule_version) VALUES(1,'SYN-1','2026-01-01','Synthetic',1,'synthetic','', 'system','synthetic','synthetic');INSERT INTO documents(id,source,sha256_hash,invoice_date) VALUES(1,'synthetic','rollback','2026-01-01')").unwrap();
        let tables: Vec<_> = schema(&db)
            .unwrap()
            .into_iter()
            .filter(|o| o.kind == "table")
            .map(|o| o.name)
            .collect();
        let cols = projection(&db, &tables).unwrap();
        let before = project_digest(&db, &cols).unwrap();
        assert_eq!(
            crate::company_normalization::normalize(&mut db)
                .unwrap_err()
                .code,
            "migration_failed"
        );
        assert_eq!(project_digest(&db, &cols).unwrap(), before);
    }
    #[test]
    #[ignore = "parity/process-death helper invoked by parent or independent TS harness"]
    fn normalization_child() {
        let mut db = Connection::open(std::env::var("RM_NORMALIZATION_DB").unwrap()).unwrap();
        db.execute_batch(
            "PRAGMA foreign_keys=ON;PRAGMA trusted_schema=OFF;PRAGMA synchronous=FULL",
        )
        .unwrap();
        let effects = crate::company_normalization::normalize(&mut db).unwrap();
        if let Ok(output) = std::env::var("RM_NORMALIZATION_OUTPUT") {
            let tables = crate::test_support::typed_tables(&db);
            std::fs::write(
                output,
                serde_json::to_vec(&serde_json::json!({"effects":effects,"tables":tables}))
                    .unwrap(),
            )
            .unwrap();
        }
    }
    #[test]
    fn normalization_process_death_rolls_back_all_data_effects() {
        for phase in ["roles", "retention"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("db");
            let marker = dir.path().join("phase");
            let db = Connection::open(&path).unwrap();
            db.execute_batch("PRAGMA journal_mode=WAL;PRAGMA synchronous=FULL")
                .unwrap();
            prefix(&db, 2);
            db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('2000','Synthetic','asset','debit');INSERT INTO documents(id,source,sha256_hash,invoice_date) VALUES(1,'synthetic','death','2026-01-01')").unwrap();
            let tables: Vec<_> = schema(&db)
                .unwrap()
                .into_iter()
                .filter(|o| o.kind == "table")
                .map(|o| o.name)
                .collect();
            let cols = projection(&db, &tables).unwrap();
            let before = project_digest(&db, &cols).unwrap();
            drop(db);
            let mut child = Reaped(
                Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "company_baseline::tests::normalization_child",
                        "--ignored",
                        "--nocapture",
                    ])
                    .env("RM_NORMALIZATION_DB", &path)
                    .env("RM_NORMALIZATION_PHASE", phase)
                    .env("RM_NORMALIZATION_MARKER", &marker)
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
            assert_eq!(project_digest(&db, &cols).unwrap(), before);
            crate::company_normalization::normalize(&mut db).unwrap();
        }
    }
}
