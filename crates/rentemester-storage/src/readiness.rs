//! Sanitized read-only schema admission on caller-held connections. Never migrates.
use crate::{migration, Migration};
use rusqlite::Connection;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Code { Current, SetupRequired, NotConfigured, MigrationRequired, UnsupportedSchema, StorageUnavailable }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Readiness {
    pub status: &'static str,
    pub code: Code,
}
impl Readiness {
    pub fn new(code: Code) -> Self {
        Self { status: match code {
            Code::Current => "ready",
            Code::SetupRequired | Code::NotConfigured => "not_ready",
            _ => "unavailable",
        }, code }
    }
    pub fn unusable(self) -> bool {
        matches!(self.code, Code::MigrationRequired | Code::UnsupportedSchema | Code::StorageUnavailable)
    }
}

const CONTROL_ARTIFACTS: [&str; 33] = [
    include_str!("../../../src/core/workspace-migrations/0001-workspace-control-baseline.json"),
    include_str!("../../../src/core/workspace-migrations/0002-better-auth-foundation.json"),
    include_str!("../../../src/core/workspace-migrations/0003-workspace-access-events.json"),
    include_str!("../../../src/core/workspace-migrations/0004-workspace-bootstrap-saga-v4.json"),
    include_str!("../../../src/core/workspace-migrations/0005-session-assurance-v5.json"),
    include_str!("../../../src/core/workspace-migrations/0006-auth-audit-v6.json"),
    include_str!("../../../src/core/workspace-migrations/0007-group-structure-v7.json"),
    include_str!("../../../src/core/workspace-migrations/0008-document-access-audit-v8.json"),
    include_str!("../../../src/core/workspace-migrations/0009-intercompany-mapping-v9.json"),
    include_str!("../../../src/core/workspace-migrations/0010-consolidation-elimination-v10.json"),
    include_str!("../../../src/core/workspace-migrations/0011-consolidation-profile-v11.json"),
    include_str!("../../../src/core/workspace-migrations/0012-auth-recovery-telemetry-v12.json"),
    include_str!("../../../src/core/workspace-migrations/0013-authorization-denial-audit-v13.json"),
    include_str!("../../../src/core/workspace-migrations/0014-workspace-invitations-v14.json"),
    include_str!("../../../src/core/workspace-migrations/0015-party-registry-v15.json"),
    include_str!("../../../src/core/workspace-migrations/0016-corporate-records-v16.json"),
    include_str!("../../../src/core/workspace-migrations/0017-service-principals-v17.json"),
    include_str!("../../../src/core/workspace-migrations/0018-service-principal-saga-v18.json"),
    include_str!("../../../src/core/workspace-migrations/0019-party-assertions-v19.json"),
    include_str!(
        "../../../src/core/workspace-migrations/0020-corporate-record-governance-v20.json"
    ),
    include_str!("../../../src/core/workspace-migrations/0021-company-knowledge-v21.json"),
    include_str!("../../../src/core/workspace-migrations/0022-ownership-graph-v22.json"),
    include_str!(
        "../../../src/core/workspace-migrations/0023-corporate-record-scope-integrity-v23.json"
    ),
    include_str!("../../../src/core/workspace-migrations/0024-ownership-fact-lifecycle-v24.json"),
    include_str!("../../../src/core/workspace-migrations/0025-workspace-document-inbox-v25.json"),
    include_str!(
        "../../../src/core/workspace-migrations/0026-workspace-inbox-handoff-claims-v26.json"
    ),
    include_str!("../../../src/core/workspace-migrations/0027-intercompany-dispositions-v27.json"),
    include_str!(
        "../../../src/core/workspace-migrations/0028-intercompany-disposition-lifecycle-v28.json"
    ),
    include_str!(
        "../../../src/core/workspace-migrations/0029-workspace-inbox-single-assignment-v29.json"
    ),
    include_str!(
        "../../../src/core/workspace-migrations/0030-document-party-role-taxonomy-v30.json"
    ),
    include_str!("../../../src/core/workspace-migrations/0031-workspace-mail-intake-v31.json"),
    include_str!("native-workspace-migrations/0032-namespace-custody-v32.json"),
    include_str!("native-workspace-migrations/0033-workspace-tasks-v33.json"),
];

fn unsupported() -> rentemester_engine::Error {
    rentemester_engine::Error::new("unsupported_schema", "schema readiness refused")
}
fn control(db: &Connection) -> Result<bool, rentemester_engine::Error> {
    let expected: Vec<Migration> = serde_json::from_str(include_str!("legacy-workspace-schema.json")).map_err(crate::invalid)?;
    let history = migration::strict_history(db, "workspace_schema_migrations", &expected).map_err(|_| unsupported())?;
    if history.len() == expected.len() {
        crate::control_migration::validate_current_control(db)?;
        return Ok(false);
    }
    if history.is_empty() { return Err(unsupported()); }
    // Bind every artifact's bytes and identity, not merely the historical row count.
    use sha2::{Digest, Sha256};
    let reference = Connection::open_in_memory().map_err(crate::invalid)?;
    for (raw, identity) in CONTROL_ARTIFACTS.iter().zip(&expected).take(history.len()) {
        let artifact: serde_json::Value = serde_json::from_str(raw).map_err(crate::invalid)?;
        if format!("{:x}", Sha256::digest(raw.as_bytes())) != identity.checksum
            || artifact["id"].as_i64() != Some(identity.id)
            || artifact["name"].as_str() != Some(identity.name.as_str()) {
            return Err(unsupported());
        }
        reference.execute_batch(artifact["sql"].as_str().ok_or_else(unsupported)?).map_err(crate::invalid)?;
    }
    if crate::document_access::strip_schema(db, &migration::schema(db)?)? != migration::schema(&reference)? { return Err(unsupported()); }
    Ok(true)
}
fn company(db: &Connection) -> Result<bool, rentemester_engine::Error> {
    let expected: Vec<Migration> = serde_json::from_str(include_str!("legacy-schema.json")).map_err(crate::invalid)?;
    let history = migration::strict_history(db, "schema_migrations", &expected).map_err(|_| unsupported())?;
    if history.len() == expected.len() {
        crate::journal_preview::require_current(db)?;
        return Ok(false);
    }
    if history.is_empty() || !crate::company_baseline::recognizes(db, history.len())? {
        return Err(unsupported());
    }
    Ok(true)
}
/// No paths, identifiers or raw errors escape this boundary. Both databases must
/// pass history, exact physical schema and integrity before a pending classification.
pub fn probe(control_db: &Connection, company_db: &Connection) -> Readiness {
    let result = (|| {
        // Integrity takes precedence over migration, including for historical images.
        migration::integrity(control_db)?;
        migration::integrity(company_db)?;
        let control_pending = control(control_db)?;
        let company_pending = company(company_db)?;
        Ok::<_, rentemester_engine::Error>(control_pending || company_pending)
    })();
    Readiness::new(match result {
        Ok(true) => Code::MigrationRequired,
        Ok(false) => Code::Current,
        Err(e) if e.code == "unsupported_schema" => Code::UnsupportedSchema,
        Err(_) => Code::StorageUnavailable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pair() -> (Connection, Connection) {
        let mut c = Connection::open_in_memory().unwrap();
        let mut d = Connection::open_in_memory().unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut d, "Synthetic readiness").unwrap();
        (c, d)
    }
    fn control_prefix(version: usize) -> Connection {
        let db = Connection::open_in_memory().unwrap();
        let expected: Vec<Migration> = serde_json::from_str(include_str!("legacy-workspace-schema.json")).unwrap();
        for (raw, m) in CONTROL_ARTIFACTS.iter().zip(expected).take(version) {
            let artifact: serde_json::Value = serde_json::from_str(raw).unwrap();
            db.execute_batch(artifact["sql"].as_str().unwrap()).unwrap();
            db.execute("INSERT INTO workspace_schema_migrations(id,name,checksum,applied_by_version) VALUES (?1,?2,?3,'synthetic')", rusqlite::params![m.id,m.name,m.checksum]).unwrap();
        }
        db
    }
    #[test]
    fn readiness_current_is_read_only() {
        let (c, d) = pair();
        let before = (crate::test_support::typed_tables(&c), crate::test_support::typed_tables(&d));
        let changes = (c.total_changes(), d.total_changes());
        c.execute_batch("PRAGMA query_only=ON").unwrap();
        d.execute_batch("PRAGMA query_only=ON").unwrap();
        for _ in 0..2 { assert_eq!(probe(&c, &d).code, Code::Current); }
        assert_eq!(before, (crate::test_support::typed_tables(&c), crate::test_support::typed_tables(&d)));
        assert_eq!(changes, (c.total_changes(), d.total_changes()));
    }
    #[test]
    fn native32_task_upgrade_is_pending_with_known_service_audit_but_not_unknown_objects() {
        let (mut current,d)=pair();let c=control_prefix(32);
        let tx=current.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).unwrap();crate::document_access::install(&tx).unwrap();tx.commit().unwrap();
        let mut extension=migration::schema(&current).unwrap().into_iter().filter(|o|o.name.starts_with("native_service_document_access_")).collect::<Vec<_>>();extension.sort_by_key(|o|(o.kind!="table",o.name.clone()));
        for object in extension {c.execute_batch(&object.sql).unwrap();}
        let before=crate::test_support::typed_tables(&c);let changes=c.total_changes();
        assert_eq!(probe(&c,&d).code,Code::MigrationRequired);assert_eq!(c.total_changes(),changes);assert_eq!(crate::test_support::typed_tables(&c),before);
        c.execute_batch("CREATE TABLE unknown_extra(id INTEGER)").unwrap();assert_eq!(probe(&c,&d).code,Code::UnsupportedSchema);
    }
    #[test]
    fn readiness_requires_exact_history_and_physical_schema() {
        for sql in [
            "DROP TABLE schema_migrations",
            "DELETE FROM schema_migrations WHERE id=36",
            "UPDATE schema_migrations SET checksum='altered' WHERE id=1",
            "DELETE FROM schema_migrations",
            "UPDATE schema_migrations SET id=99 WHERE id=36",
            "CREATE TABLE unknown_object(id INTEGER)",
            "DROP TABLE imported_receivable_events",
        ] {
            let (c, d) = pair();
            d.execute_batch(sql).unwrap();
            assert_eq!(probe(&c, &d).code, Code::UnsupportedSchema, "{sql}");
        }
        let (_, d) = pair();
        let c = control_prefix(30);
        let before = crate::test_support::typed_tables(&c);
        assert_eq!(probe(&c, &d).code, Code::MigrationRequired);
        assert_eq!(before, crate::test_support::typed_tables(&c));
        c.execute_batch("CREATE TABLE unknown_object(id INTEGER)").unwrap();
        assert_eq!(probe(&c, &d).code, Code::UnsupportedSchema);
    }
    #[test]
    fn readiness_company_historical_prefix_and_retirement_fail_closed() {
        let (c, _) = pair();
        let d = Connection::open_in_memory().unwrap();
        for object in crate::company_baseline::load_images().unwrap().baseline {
            d.execute_batch(&object.sql).unwrap();
        }
        let expected: Vec<Migration> = serde_json::from_str(include_str!("legacy-schema.json")).unwrap();
        crate::company_baseline::stamp(&d, &expected[0]).unwrap();
        assert_eq!(probe(&c, &d).code, Code::MigrationRequired);
        d.execute_batch("CREATE TABLE unknown_object(id INTEGER)").unwrap();
        assert_eq!(probe(&c, &d).code, Code::UnsupportedSchema);
        let (mut c, d) = pair();
        crate::legacy_retirement::retire_owned(&mut c, &crate::legacy_retirement::RetirementBinding {
            operation_id: "a".repeat(64), source_workspace_id: "b".repeat(64),
            operator_actor: "system:synthetic".into(), source_stopped_confirmed: true,
        }).unwrap();
        assert_eq!(probe(&c, &d).code, Code::UnsupportedSchema);
    }
    #[test]
    fn readiness_integrity_failure_is_not_pending() {
        let (c, d) = pair();
        c.execute_batch("PRAGMA foreign_keys=OFF; INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES ('synthetic','2027','secret','2026','2026','missing')").unwrap();
        assert_eq!(probe(&c, &d).code, Code::StorageUnavailable);
    }
    #[test]
    fn readiness_file_bytes_and_sql_are_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let cp = dir.path().join("control.sqlite");
        let dp = dir.path().join("company.sqlite");
        let mut c = Connection::open(&cp).unwrap();
        let mut d = Connection::open(&dp).unwrap();
        crate::fresh::initialize_empty_pair(&mut c, &mut d, "Synthetic").unwrap();
        c.close().unwrap(); d.close().unwrap();
        let before = (std::fs::read(&cp).unwrap(), std::fs::read(&dp).unwrap());
        let c = Connection::open_with_flags(&cp, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let d = Connection::open_with_flags(&dp, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        assert_eq!(probe(&c, &d).code, Code::Current);
        assert_eq!(before, (std::fs::read(&cp).unwrap(), std::fs::read(&dp).unwrap()));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}
