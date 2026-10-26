//! All control migrations, one pending-suffix transaction, private captures only.
use crate::migration::{digest, integrity, schema, strict_history};
use crate::{Migration, Snapshot, invalid};
use rentemester_engine::Error;
use rusqlite::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

pub(crate) const ARTIFACTS: [&str; 31] = [
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
];
#[derive(Deserialize)]
struct Artifact {
    id: i64,
    name: String,
    sql: String,
}
struct Step {
    identity: Migration,
    sql: String,
}
#[derive(Serialize)]
struct Applied {
    id: i64,
    name: String,
    checksum: String,
}
#[derive(Serialize)]
pub struct ControlRehearsalReport {
    from_version: usize,
    to_version: usize,
    applied: Vec<Applied>,
    canonical_schema_sha256: String,
    schema_objects: usize,
    prior_data_preserved: bool,
    prior_provenance_preserved: bool,
    sessions_invalidated: i64,
    mfa_events_backfilled: usize,
    private_copy_discarded: bool,
    writer_authority: bool,
}
fn unsupported(message: &str) -> Error {
    Error::new("unsupported_schema", message)
}
fn failed(error: impl std::fmt::Display) -> Error {
    Error::new("migration_failed", error.to_string())
}
fn catalogue() -> Result<Vec<Migration>, Error> {
    native_catalogue().map(|mut entries| { entries.truncate(31); entries })
}
fn native_catalogue() -> Result<Vec<Migration>, Error> {
    serde_json::from_str(include_str!("legacy-workspace-schema.json")).map_err(invalid)
}
fn native_steps() -> Result<Vec<Step>, Error> {
    let mut raw = ARTIFACTS.to_vec();
    raw.push(include_str!("native-workspace-migrations/0032-namespace-custody-v32.json"));
    raw.push(include_str!("native-workspace-migrations/0033-workspace-tasks-v33.json"));
    raw.push(include_str!("native-workspace-migrations/0034-knowledge-pages-v34.json"));
    steps(&raw, &native_catalogue()?)
}
fn steps(raw: &[&str], expected: &[Migration]) -> Result<Vec<Step>, Error> {
    if expected.is_empty() || raw.len() != expected.len() {
        return Err(unsupported("control catalogue size differs"));
    }
    raw.iter()
        .zip(expected)
        .enumerate()
        .map(|(index, (bytes, m))| {
            let a: Artifact = serde_json::from_str(bytes)
                .map_err(|_| unsupported("malformed control artifact"))?;
            if m.id != index as i64 + 1
                || a.id != m.id
                || a.name != m.name
                || format!("{:x}", Sha256::digest(bytes.as_bytes())) != m.checksum
                || a.sql.is_empty()
            {
                return Err(unsupported("control artifact identity differs"));
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
// Native empty-file construction, not source adoption or the rehearsal API.
pub(super) fn initialize_empty(db: &mut Connection) -> Result<(), Error> {
    if !schema(db)?.is_empty() {
        return Err(unsupported("native control requires an empty database"));
    }
    let expected = native_catalogue()?;
    let steps = native_steps()?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(failed)?;
    for step in &steps {
        tx.execute_batch(&step.sql).map_err(failed)?;
        tx.execute("INSERT INTO workspace_schema_migrations(id,name,checksum,applied_by_version,applied_by_commit) VALUES (?1,?2,?3,?4,NULL)",params![step.identity.id,step.identity.name,step.identity.checksum,concat!("rentemester-rust/",env!("CARGO_PKG_VERSION"))]).map_err(failed)?;
    }
    integrity(&tx)?;
    if schema(&tx)? != reference(&steps, expected.len())?
        || strict_history(&tx, "workspace_schema_migrations", &expected)?.len() != expected.len()
    {
        return Err(unsupported("fresh control schema or history differs"));
    }
    tx.commit().map_err(failed)
}
fn reference(steps: &[Step], version: usize) -> Result<Vec<crate::migration::SchemaObject>, Error> {
    let db = Connection::open_in_memory().map_err(invalid)?;
    db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA trusted_schema=OFF;")
        .map_err(invalid)?;
    for s in steps.iter().take(version) {
        db.execute_batch(&s.sql).map_err(failed)?;
    }
    schema(&db)
}
fn history(db: &Connection, expected: &[Migration]) -> Result<Vec<Vec<String>>, Error> {
    if schema(db)?.is_empty() {
        return Ok(Vec::new());
    }
    let rows = strict_history(db, "workspace_schema_migrations", expected)?;
    if rows.is_empty() {
        return Err(unsupported("populated control database has empty history"));
    }
    Ok(rows)
}
fn enabled_users(db: &Connection) -> Result<Vec<String>, Error> {
    db.prepare("SELECT id FROM user WHERE COALESCE(twoFactorEnabled,0)=1 ORDER BY id")
        .map_err(invalid)?
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)
}
fn session_count(db: &Connection) -> Result<i64, Error> {
    db.query_row("SELECT COUNT(*) FROM session", [], |r| r.get(0))
        .map_err(invalid)
}
fn verify_backfill(db: &Connection, expected: &[String]) -> Result<(), Error> {
    let mut query=db.prepare("SELECT user_id,event_type,actor,created_at FROM rm_workspace_mfa_events ORDER BY user_id").map_err(invalid)?;
    let mut rows = query.query([]).map_err(invalid)?;
    let mut users = Vec::new();
    while let Some(row) = rows.next().map_err(invalid)? {
        let user: String = row.get(0).map_err(invalid)?;
        let event: String = row.get(1).map_err(invalid)?;
        let actor: String = row.get(2).map_err(invalid)?;
        let at: String = row.get(3).map_err(invalid)?;
        if event != "mfa_enabled" || actor != "system:better-auth" || at.is_empty() {
            return Err(invalid("MFA backfill fields differ"));
        }
        users.push(user);
    }
    if users != expected {
        return Err(invalid("MFA backfill user set differs"));
    }
    Ok(())
}
fn rehearse(
    db: &mut Connection,
    steps: &[Step],
    expected: &[Migration],
) -> Result<ControlRehearsalReport, Error> {
    db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA trusted_schema=OFF; PRAGMA synchronous=FULL;")
        .map_err(invalid)?;
    let old = history(db, expected)?;
    let version = old.len();
    if schema(db)? != reference(steps, version)? {
        return Err(unsupported("full control prefix schema differs"));
    }
    integrity(db)?;
    let pending_mfa = version < 5;
    let users = if pending_mfa && version >= 2 {
        enabled_users(db)?
    } else {
        Vec::new()
    };
    let sessions = if pending_mfa && version >= 2 {
        session_count(db)?
    } else {
        0
    };
    let tables: Vec<_> = schema(db)?
        .into_iter()
        .filter(|o| {
            o.kind == "table"
                && o.name != "workspace_schema_migrations"
                && !(pending_mfa && o.name == "session")
        })
        .map(|o| o.name)
        .collect();
    let before = digest(db, &tables)?;
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(failed)?;
    let mut applied = Vec::new();
    for step in steps.iter().skip(version) {
        tx.execute_batch(&step.sql).map_err(failed)?;
        #[cfg(test)]
        test_checkpoint(step.identity.id, "sql");
        tx.execute("INSERT INTO workspace_schema_migrations(id,name,checksum,applied_by_version,applied_by_commit) VALUES (?1,?2,?3,?4,NULL)",params![step.identity.id,step.identity.name,step.identity.checksum,concat!("rentemester-rust/",env!("CARGO_PKG_VERSION"))]).map_err(failed)?;
        #[cfg(test)]
        test_checkpoint(step.identity.id, "history");
        applied.push(Applied {
            id: step.identity.id,
            name: step.identity.name.clone(),
            checksum: step.identity.checksum.clone(),
        });
    }
    integrity(&tx)?;
    let current_schema = schema(&tx)?;
    if current_schema != reference(steps, ARTIFACTS.len())? {
        return Err(unsupported("final control schema differs"));
    }
    let new = history(&tx, expected)?;
    if new.len() != ARTIFACTS.len() || new[..version] != old || digest(&tx, &tables)? != before {
        return Err(invalid(
            "control migration changed original data/provenance",
        ));
    }
    if pending_mfa {
        if session_count(&tx)? != 0 {
            return Err(invalid("legacy sessions were not invalidated"));
        }
        verify_backfill(&tx, &users)?;
    }
    let fingerprint = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&current_schema).map_err(invalid)?)
    );
    #[cfg(test)]
    test_checkpoint(31, "before_commit");
    tx.commit().map_err(failed)?;
    #[cfg(test)]
    test_checkpoint(31, "committed");
    Ok(ControlRehearsalReport {
        from_version: version,
        to_version: ARTIFACTS.len(),
        applied,
        canonical_schema_sha256: fingerprint,
        schema_objects: current_schema.len(),
        prior_data_preserved: true,
        prior_provenance_preserved: true,
        sessions_invalidated: sessions,
        mfa_events_backfilled: users.len(),
        private_copy_discarded: true,
        writer_authority: false,
    })
}
/// Unforgeable in-process observation of the exact main31 disposable copy; no writer grant.
pub(super) struct MainKnowledge31Projection<'a,'db>{transaction:&'a rusqlite::Transaction<'db>}
impl MainKnowledge31Projection<'_,'_>{pub(super)fn transaction(&self)->&rusqlite::Transaction<'_>{self.transaction}}
/// Conversion-only private copy. Reuses the exact full-prefix rehearsal guards;
/// never grants writer authority or retains the old control authentication state.
pub(super) fn convert_adoption_copy(
    db: &mut Connection,
) -> Result<(usize, crate::portable_backup::Control), Error> {
    // Main knowledge31 is not native mail31: validate its own exact lineage/shape,
    // then project business on this disposable copy without rewriting source history.
    let has_history: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='workspace_schema_migrations')",[],|r|r.get(0)).map_err(failed)?;
    let source_main: bool = has_history && db.query_row("SELECT EXISTS(SELECT 1 FROM workspace_schema_migrations WHERE id=31 AND name='rentemester-knowledge-pages-v31')",[],|r|r.get(0)).map_err(failed)?;
    if source_main {
        let raw=include_str!("../../../src/core/workspace-migrations/0031-knowledge-pages-v31.json");
        let artifact:Artifact=serde_json::from_str(raw).map_err(failed)?;
        let mut source_catalog=catalogue()?;source_catalog.truncate(30);
        source_catalog.push(Migration{id:31,name:artifact.name,checksum:format!("{:x}",Sha256::digest(raw.as_bytes()))});
        let mut source_artifacts=ARTIFACTS[..30].to_vec();source_artifacts.push(raw);
        let source_steps=steps(&source_artifacts,&source_catalog)?;
        if history(db,&source_catalog)?.len()!=31||crate::document_access::strip_schema(db,&schema(db)?)?!=reference(&source_steps,31)?{return Err(unsupported("exact main knowledge31 lineage required"))}
        integrity(db)?;
        let tx=db.transaction_with_behavior(TransactionBehavior::Immediate).map_err(failed)?;
        // Empty native-only task tables make the closed business exporter applicable.
        // No migration-history row or historical authority is reinterpreted or imported.
        tx.execute_batch(&native_steps()?[32].sql).map_err(failed)?;
        let business=crate::portable_backup::export_main31_projection(&MainKnowledge31Projection{transaction:&tx})?;
        tx.rollback().map_err(failed)?;
        return Ok((31,business));
    }
    let expected = catalogue()?;
    let steps = steps(&ARTIFACTS, &expected)?;
    let report = rehearse(db, &steps, &expected)?;
    {
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate).map_err(failed)?;
        for step in native_steps()?.iter().skip(31) {
            tx.execute_batch(&step.sql).map_err(failed)?;
            tx.execute("INSERT INTO workspace_schema_migrations(id,name,checksum,applied_by_version) VALUES(?1,?2,?3,'native-private-conversion')", params![step.identity.id,step.identity.name,step.identity.checksum]).map_err(failed)?;
        }
        tx.commit().map_err(failed)?;
    }
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(failed)?;
    let business = crate::portable_backup::export_control(&tx)?;
    tx.rollback().map_err(failed)?;
    Ok((report.from_version, business))
}
/// Explicit operator-only extension under an already held namespace lifetime.
/// Accept only the immediately preceding complete schema or the current one;
/// never use this as an adoption route for older source databases.
pub fn upgrade_mail_control(db: &mut Connection) -> Result<(), Error> {
    let native = native_catalogue()?;
    let version = history(db, &native)?.len();
    if version >= 32 {
        if crate::document_access::strip_schema(db, &schema(db)?)? != reference(&native_steps()?, version)? {
            return Err(unsupported("native control prefix schema differs"));
        }
        return integrity(db);
    }
    let expected = catalogue()?;
    let steps = steps(&ARTIFACTS, &expected)?;
    let version = history(db, &expected)?.len();
    if version == expected.len() {
        return validate_v31(db);
    }
    if version != 30 || schema(db)? != reference(&steps, 30)? {
        return Err(unsupported("mail schema upgrade requires complete v30 control"));
    }
    rehearse(db, &steps, &expected).map(|_| ())
}
fn validate_v31(db: &Connection) -> Result<(), Error> {
    let expected = catalogue()?;
    let steps = steps(&ARTIFACTS, &expected)?;
    if history(db, &expected)?.len() != 31
        || crate::document_access::strip_schema(db, &schema(db)?)? != reference(&steps, 31)?
    {
        return Err(unsupported("complete v31 required"));
    }
    integrity(db)
}
/// Private new schema or explicit stopped enrollment only. Migration and binding
/// share one transaction; no ordinary admission calls this function.
pub(crate) fn bind_namespace(db: &mut Connection, identity: crate::namespace_custody::Identity, enroll: bool) -> Result<(), Error> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate).map_err(failed)?;
    let expected = native_catalogue()?;
    let all = native_steps()?;
    let version = history(&tx, &expected)?.len();
    if !(31..=expected.len()).contains(&version)
        || crate::document_access::strip_schema(&tx, &schema(&tx)?)? != reference(&all, version)? {
        return Err(unsupported("complete native control prefix required"));
    }
    integrity(&tx)?;
    if enroll { crate::workspace_transfer::require_business_write(&tx)?; }
    // Existing custody is checked BEFORE any suffix migration. Never rebind a copied namespace.
    if version >= 32 && (enroll || version < expected.len()) { crate::namespace_custody::verify(&tx, identity)?; }
    for step in all.iter().skip(version) {
        tx.execute_batch(&step.sql).map_err(failed)?;
        tx.execute("INSERT INTO workspace_schema_migrations(id,name,checksum,applied_by_version) VALUES(?1,?2,?3,?4)", params![step.identity.id,step.identity.name,step.identity.checksum,concat!("rentemester-rust/",env!("CARGO_PKG_VERSION"))]).map_err(failed)?;
    }
    validate_current_control(&tx)?;
    if enroll { crate::workspace_transfer::require_business_write(&tx)?; }
    if enroll && version >= 32 { crate::namespace_custody::verify(&tx, identity)?; }
    else { crate::namespace_custody::insert_or_verify(&tx, identity)?; }
    tx.commit().map_err(failed)
}
/// Deliberately sanitize every public refusal: control copies may contain credentials.
fn public_error(error: Error) -> Error {
    Error::new(
        error.code,
        format!("workspace control rehearsal refused ({})", error.code),
    )
}
pub fn rehearse_control_migration(workspace: &Path) -> Result<ControlRehearsalReport, Error> {
    (|| {
        let expected = catalogue()?;
        let steps = steps(&ARTIFACTS, &expected)?;
        Snapshot::capture_control(workspace)?
            .with_writable_copy(|db| rehearse(db, &steps, &expected))
    })()
    .map_err(public_error)
}
#[cfg(test)]
fn test_checkpoint(step: i64, phase: &str) {
    if std::env::var("RM_CONTROL_TEST_STEP").ok().as_deref() == Some(step.to_string().as_str())
        && std::env::var("RM_CONTROL_TEST_PHASE").as_deref() == Ok(phase)
    {
        std::fs::write(std::env::var("RM_CONTROL_TEST_MARKER").unwrap(), phase).unwrap();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}

/// Complete current control admission for an already held transaction, without migration.
pub(crate) fn validate_current_control(db: &Connection) -> Result<(), Error> {
    let expected = native_catalogue()?;
    let steps = native_steps()?;
    if history(db, &expected)?.len() != expected.len()
        || crate::document_access::strip_schema(db, &schema(db)?)? != reference(&steps, expected.len())?
    {
        return Err(unsupported(
            "complete current control schema/history required",
        ));
    }
    integrity(db)?;
    // These immutable streams derive authority from numeric ID order. Once the
    // largest ROWID exists SQLite can allocate later IDs below it. No chronology
    // can then be inferred safely, including for other subjects/companies.
    let exhausted: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM main.rm_workspace_user_access_events WHERE id=9223372036854775807) OR EXISTS(SELECT 1 FROM main.rm_company_membership_events WHERE id=9223372036854775807) OR EXISTS(SELECT 1 FROM main.rm_workspace_security_events WHERE id=9223372036854775807)",
        [], |row| row.get(0),
    ).map_err(failed)?;
    if exhausted {
        return Err(unsupported("ordered control history is exhausted"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native33_knowledge_suffix_keeps_history_and_verifies_custody_before_migration(){
        let mut db=native32();let all=native_steps().unwrap();let task=&all[32];db.execute_batch(&task.sql).unwrap();db.execute("INSERT INTO workspace_schema_migrations(id,name,checksum,applied_by_version)VALUES(?1,?2,?3,'historical-native33')",params![task.identity.id,task.identity.name,task.identity.checksum]).unwrap();let old:Vec<(i64,String,String)>=db.prepare("SELECT id,name,checksum FROM workspace_schema_migrations ORDER BY id").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap().collect::<Result<_,_>>().unwrap();
        assert!(bind_namespace(&mut db,crate::namespace_custody::Identity{device:7,inode:9},true).is_err());assert_eq!(db.query_row::<i64,_,_>("SELECT count(*) FROM workspace_schema_migrations",[],|r|r.get(0)).unwrap(),33);bind_namespace(&mut db,crate::namespace_custody::Identity{device:7,inode:8},true).unwrap();let current:Vec<(i64,String,String)>=db.prepare("SELECT id,name,checksum FROM workspace_schema_migrations ORDER BY id").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap().collect::<Result<_,_>>().unwrap();assert_eq!(&current[..33],old);assert_eq!(current.len(),34);
    }
    fn native32() -> Connection {
        let db=Connection::open_in_memory().unwrap();
        for step in native_steps().unwrap().iter().take(32) {
            db.execute_batch(&step.sql).unwrap();
            db.execute("INSERT INTO workspace_schema_migrations(id,name,checksum,applied_by_version) VALUES(?1,?2,?3,'historical-native32')",params![step.identity.id,step.identity.name,step.identity.checksum]).unwrap();
        }
        crate::namespace_custody::insert_or_verify(&db,crate::namespace_custody::Identity{device:7,inode:8}).unwrap();db
    }
    #[test]
    fn task_suffix_requires_existing_custody_before_any_change_and_retains_prefix() {
        let mut db=native32();
        let old=history(&db,&native_catalogue().unwrap()).unwrap();
        let changes=db.total_changes();
        assert!(crate::namespace_custody::enroll_stopped(&mut db,crate::namespace_custody::Identity{device:7,inode:9}).is_err());
        assert_eq!(db.total_changes(),changes);
        assert_eq!(history(&db,&native_catalogue().unwrap()).unwrap(),old);
        assert!(db.prepare("SELECT * FROM rm_task_events").is_err());
        crate::namespace_custody::enroll_stopped(&mut db,crate::namespace_custody::Identity{device:7,inode:8}).unwrap();
        let current=history(&db,&native_catalogue().unwrap()).unwrap();
        assert_eq!(&current[..32],old); assert_eq!(current.len(),34);
        validate_current_control(&db).unwrap();
        assert_eq!(db.query_row::<i64,_,_>("SELECT count(*) FROM rm_task_events",[],|r|r.get(0)).unwrap(),0);
    }
    #[test]
    fn main_knowledge31_is_not_native_mail31_or_a_task_upgrade_source() {
        let mut db=Connection::open_in_memory().unwrap();
        for step in native_steps().unwrap().iter().take(30) {
            db.execute_batch(&step.sql).unwrap();db.execute("INSERT INTO workspace_schema_migrations(id,name,checksum,applied_by_version) VALUES(?1,?2,?3,'synthetic-main')",params![step.identity.id,step.identity.name,step.identity.checksum]).unwrap();
        }
        let raw=include_str!("../../../src/core/workspace-migrations/0031-knowledge-pages-v31.json");
        let a:Artifact=serde_json::from_str(raw).unwrap();db.execute_batch(&a.sql).unwrap();
        db.execute("INSERT INTO workspace_schema_migrations(id,name,checksum,applied_by_version) VALUES(31,?1,?2,'synthetic-main')",params![a.name,format!("{:x}",Sha256::digest(raw.as_bytes()))]).unwrap();
        let changes=db.total_changes();
        assert!(crate::namespace_custody::enroll_stopped(&mut db,crate::namespace_custody::Identity{device:7,inode:8}).is_err());
        assert_eq!(db.total_changes(),changes);
        assert!(db.prepare("SELECT * FROM rm_namespace_custody").is_err());
    }
    use std::{
        process::{Child, Command, Stdio},
        time::{Duration, Instant},
    };
    fn fixture(db: &Connection, version: usize) {
        db.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )
        .unwrap();
        let expected = catalogue().unwrap();
        let steps = steps(&ARTIFACTS, &expected).unwrap();
        for s in steps.iter().take(version) {
            db.execute_batch(&s.sql).unwrap();
            db.execute("INSERT INTO workspace_schema_migrations(id,name,checksum,applied_by_version) VALUES (?1,?2,?3,'synthetic-prior')",params![s.identity.id,s.identity.name,s.identity.checksum]).unwrap();
        }
    }
    fn users(db: &Connection) {
        db.execute_batch("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES ('synthetic-enabled','Enabled','enabled@example.invalid',1,'2026-01-01','2026-01-01',1),('synthetic-disabled','Disabled','disabled@example.invalid',1,'2026-01-01','2026-01-01',0);
            INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES ('s1','2026-12-31','synthetic-token-one','2026-01-01','2026-01-01','synthetic-enabled'),('s2','2026-12-31','synthetic-token-two','2026-01-01','2026-01-01','synthetic-disabled');
            INSERT INTO workspace_audit(event_type,entity_type,actor) VALUES ('synthetic','test','agent:synthetic');").unwrap();
    }
    #[test]
    fn namespace_custody_enrollment_refuses_frozen_and_missing_v32_without_repair() {
        use crate::namespace_custody::{Identity,enroll_stopped};
        let id=Identity{device:7,inode:9};
        let mut db=Connection::open_in_memory().unwrap(); fixture(&db,31);
        let initial=crate::workspace_transfer::current(&db).unwrap();
        let event=serde_json::json!({"version":1,"previousStateHash":initial.state_hash,"actor":"user:synthetic","at":"2026-01-01T00:00:00.000Z","fact":{"kind":"frozen","transfer_id":"a".repeat(32),"source_instance_id":"b".repeat(32),"workspace_id":"w","destination":"Synthetic destination","reason":"Explicit synthetic transfer review","companies":[{"slug":"synthetic","ledgerUuid":"c".repeat(32)}]}});
        // Event serialization order is itself checked by the typed reader.
        #[derive(Deserialize,Serialize)]
        #[serde(rename_all="camelCase")]
        struct FixtureEvent {version:u32,previous_state_hash:String,actor:String,at:String,fact:crate::workspace_transfer::Fact}
        let event:FixtureEvent=serde_json::from_value(event).unwrap();
        let raw=serde_json::to_string(&event).unwrap();
        db.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor,created_at) VALUES('native_workspace_transfer_frozen','native_workspace_transfer_v1',?1,'user:synthetic','2026-01-01T00:00:00.000Z')",[raw]).unwrap();
        assert_eq!(crate::workspace_transfer::current(&db).unwrap().mode,"frozen");
        let before=schema(&db).unwrap(); assert!(enroll_stopped(&mut db,id).is_err()); assert_eq!(schema(&db).unwrap(),before);
        let mut db=Connection::open_in_memory().unwrap(); initialize_empty(&mut db).unwrap();
        assert!(enroll_stopped(&mut db,id).is_err());
        assert_eq!(db.query_row("SELECT count(*) FROM rm_namespace_custody",[],|r|r.get::<_,i64>(0)).unwrap(),0);
    }
    #[test]
    fn namespace_custody_v31_enrollment_preserves_history_sessions_and_is_atomic() {
        use crate::namespace_custody::{Identity, enroll_stopped, verify};
        let id=Identity{device:7,inode:9};
        let mut db=Connection::open_in_memory().unwrap();
        fixture(&db,31); users(&db);
        let old=history(&db,&catalogue().unwrap()).unwrap();
        let before=digest(&db,&["user".into(),"session".into(),"workspace_audit".into()]).unwrap();
        // A real SQL fault after DDL must roll back migration and history too.
        db.execute_batch("CREATE TEMP TRIGGER reject_custody_history BEFORE INSERT ON main.workspace_schema_migrations WHEN NEW.id=32 BEGIN SELECT RAISE(ABORT,'synthetic fault'); END;").unwrap();
        assert!(enroll_stopped(&mut db,id).is_err());
        assert_eq!(history(&db,&catalogue().unwrap()).unwrap(),old);
        assert_eq!(schema(&db).unwrap(),reference(&steps(&ARTIFACTS,&catalogue().unwrap()).unwrap(),31).unwrap());
        db.execute_batch("DROP TRIGGER reject_custody_history").unwrap();
        enroll_stopped(&mut db,id).unwrap(); verify(&db,id).unwrap();
        assert_eq!(digest(&db,&["user".into(),"session".into(),"workspace_audit".into()]).unwrap(),before);
        assert_eq!(history(&db,&native_catalogue().unwrap()).unwrap()[..31],old);
        let changes=db.total_changes(); enroll_stopped(&mut db,id).unwrap(); assert_eq!(changes,db.total_changes());
        assert!(enroll_stopped(&mut db,Identity{device:7,inode:10}).is_err());
        verify(&db,id).unwrap();
    }
    #[test]
    fn namespace_custody_v31_service_access_extension_survives_enrollment_and_current_mail_noop() {
        let mut current = Connection::open_in_memory().unwrap();
        initialize_empty(&mut current).unwrap();
        let tx = current.transaction().unwrap();
        crate::document_access::install(&tx).unwrap();
        tx.commit().unwrap();
        let mut extension: Vec<_> = schema(&current).unwrap().into_iter()
            .filter(|o| o.name.starts_with("native_service_document_access_")).collect();
        extension.sort_by_key(|o| (o.kind != "table", o.name.clone()));
        assert_eq!(extension.len(), 3);
        let mut db = Connection::open_in_memory().unwrap();
        fixture(&db, 31); users(&db);
        for object in extension { db.execute_batch(&object.sql).unwrap(); }
        db.execute("INSERT INTO native_service_document_access_events(actor,company_slug,resource_type,resource_id,outcome,reason_code) VALUES('service_account:synthetic','synthetic','document_file',1,'served','authorized')", []).unwrap();
        let tables: Vec<_> = schema(&db).unwrap().into_iter()
            .filter(|o| o.kind == "table" && o.name != "workspace_schema_migrations")
            .map(|o| o.name).collect();
        let before = digest(&db, &tables).unwrap();
        let id = crate::namespace_custody::Identity { device: 7, inode: 9 };
        crate::namespace_custody::enroll_stopped(&mut db, id).unwrap();
        assert_eq!(digest(&db, &tables).unwrap(), before);
        crate::namespace_custody::verify(&db, id).unwrap();
        let changes = db.total_changes();
        upgrade_mail_control(&mut db).unwrap();
        assert_eq!(db.total_changes(), changes);
        assert_eq!(digest(&db, &tables).unwrap(), before);
        assert!(db.execute("DELETE FROM native_service_document_access_events", []).is_err());
    }
    #[test]
    fn ordered_event_exhaustion_is_sticky_and_does_not_reject_counted_evidence() {
        for (table, insert) in [
            (
                "rm_workspace_user_access_events",
                "INSERT INTO rm_workspace_user_access_events(id,user_id,event_type,workspace_role,actor) VALUES(?1,'synthetic-enabled','activate','member','agent:synthetic')",
            ),
            (
                "rm_company_membership_events",
                "INSERT INTO rm_company_membership_events(id,user_id,company_slug,event_type,company_role,actor) VALUES(?1,'synthetic-enabled','synthetic-company','grant','reader','agent:synthetic')",
            ),
            (
                "rm_workspace_security_events",
                "INSERT INTO rm_workspace_security_events(id,scope,event_type,actor) VALUES(?1,'workspace','session_invalidate','agent:synthetic')",
            ),
        ] {
            let mut db = Connection::open_in_memory().unwrap();
            initialize_empty(&mut db).unwrap();
            users(&db);
            db.execute(insert, [i64::MAX - 1]).unwrap();
            validate_current_control(&db).unwrap();
            db.execute(insert, [rusqlite::types::Value::Null]).unwrap();
            let before = db.total_changes();
            assert_eq!(
                validate_current_control(&db).unwrap_err().code,
                "unsupported_schema"
            );
            assert_eq!(db.total_changes(), before);
            db.execute(insert, [rusqlite::types::Value::Null]).unwrap();
            let latest: i64 = db
                .query_row(&format!("SELECT max(id) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(latest, i64::MAX);
            let before = db.total_changes();
            assert!(validate_current_control(&db).is_err());
            assert_eq!(db.total_changes(), before);
        }
        let mut db = Connection::open_in_memory().unwrap();
        initialize_empty(&mut db).unwrap();
        users(&db);
        db.execute("INSERT INTO rm_workspace_auth_state_events(id,state_transition,user_id) VALUES(?1,'credential_updated','synthetic-enabled')", [i64::MAX]).unwrap();
        validate_current_control(&db).unwrap();
    }
    #[test]
    fn mail_extension_preserves_v30_inbox_and_append_only_guards() {
        let mut db = Connection::open_in_memory().unwrap();
        fixture(&db, 30);
        db.execute_batch("INSERT INTO rm_workspace_inbox_sources VALUES('source','synthetic','key',X'01','hash','original.pdf','application/pdf','upload',NULL,'2026-01-01T00:00:00Z','{}','user:synthetic','2026-01-01T00:00:00Z');
            INSERT INTO rm_workspace_inbox_events VALUES(7,'source','ingested','hash','{}','user:synthetic','2026-01-01T00:00:00Z');
            INSERT INTO rm_workspace_inbox_exceptions VALUES('source','INBOX_NO_CANDIDATE','review_and_assign','2026-01-01T00:00:00Z',NULL);").unwrap();
        let expected = catalogue().unwrap();
        let steps = steps(&ARTIFACTS, &expected).unwrap();
        let report = rehearse(&mut db, &steps, &expected).unwrap();
        assert_eq!(report.from_version, 30);
        assert_eq!(report.to_version, 31);
        assert_eq!(report.applied.len(), 1);
        assert!(report.prior_data_preserved && report.prior_provenance_preserved);
        assert_eq!(db.query_row("SELECT id FROM rm_workspace_inbox_events", [], |r| r.get::<_,i64>(0)).unwrap(), 7);
        db.execute("INSERT INTO rm_workspace_inbox_events(source_id,event_type,payload_hash,canonical_payload,actor,created_at) VALUES('source','metadata_reviewed','hash','{}','user:synthetic','2026-01-02T00:00:00Z')", []).unwrap();
        assert!(db.execute("UPDATE rm_workspace_inbox_events SET actor='changed' WHERE id=7", []).is_err());
        assert!(db.execute("DELETE FROM rm_workspace_inbox_events WHERE id=7", []).is_err());
        assert!(db.execute("INSERT INTO rm_workspace_inbox_events(source_id,event_type,payload_hash,canonical_payload,actor,created_at) VALUES('source','unknown_kind','hash','{}','user:synthetic','2026-01-02T00:00:00Z')", []).is_err());
        integrity(&db).unwrap();
    }
    #[test]
    fn explicit_mail_upgrade_accepts_only_exact_previous_or_current_schema() {
        for version in [29, 30, 31] {
            let mut db = Connection::open_in_memory().unwrap();
            fixture(&db, version);
            let before = schema(&db).unwrap();
            let result = upgrade_mail_control(&mut db);
            if version == 29 {
                assert!(result.is_err());
                assert_eq!(schema(&db).unwrap(), before);
            } else {
                result.unwrap();
                validate_v31(&db).unwrap();
                let changes = db.total_changes();
                upgrade_mail_control(&mut db).unwrap();
                assert_eq!(db.total_changes(), changes);
            }
        }
        let mut db = Connection::open_in_memory().unwrap();
        fixture(&db, 30);
        db.execute_batch("CREATE TABLE unexpected(value TEXT);").unwrap();
        let before = schema(&db).unwrap();
        assert!(upgrade_mail_control(&mut db).is_err());
        assert_eq!(schema(&db).unwrap(), before);
    }
    #[test]
    fn all_control_artifacts_are_bound_and_public_errors_are_opaque() {
        let expected = catalogue().unwrap();
        assert_eq!(steps(&ARTIFACTS, &expected).unwrap().len(), ARTIFACTS.len());
        let bytes = format!("{} ", ARTIFACTS[0]);
        let mut raw = ARTIFACTS.to_vec();
        raw[0] = &bytes;
        assert_eq!(
            steps(&raw, &expected).err().unwrap().code,
            "unsupported_schema"
        );
        let error = public_error(failed("synthetic-credential-CANARY"));
        assert_eq!(error.code, "migration_failed");
        assert!(!error.message.contains("CANARY"));
    }
    #[test]
    fn legal_sqlite_like_user_objects_are_not_internal_or_empty() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE sqliteShadow(secret TEXT)")
            .unwrap();
        assert_eq!(schema(&db).unwrap().len(), 1);
        assert_eq!(
            history(&db, &catalogue().unwrap()).unwrap_err().code,
            "unsupported_schema"
        );
    }
    #[test]
    fn mfa_upgrade_and_immutable_guards_match_real_sql() {
        let mut db = Connection::open_in_memory().unwrap();
        fixture(&db, 4);
        users(&db);
        let expected = catalogue().unwrap();
        let steps = steps(&ARTIFACTS, &expected).unwrap();
        let report = rehearse(&mut db, &steps, &expected).unwrap();
        assert_eq!(report.sessions_invalidated, 2);
        assert_eq!(report.mfa_events_backfilled, 1);
        assert_eq!(session_count(&db).unwrap(), 0);
        verify_backfill(&db, &["synthetic-enabled".into()]).unwrap();
        for table in ["workspace_audit", "workspace_schema_migrations"] {
            assert!(
                db.execute(&format!("UPDATE {table} SET id=id WHERE id=1"), [])
                    .is_err()
            );
            assert!(
                db.execute(&format!("DELETE FROM {table} WHERE id=1"), [])
                    .is_err()
            );
        }
        db.execute_batch("INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES ('s3','2026-12-31','synthetic-three','2026-01-01','2026-01-01','synthetic-enabled'),('s4','2026-12-31','synthetic-four','2026-01-01','2026-01-01','synthetic-disabled');UPDATE user SET twoFactorEnabled=1 WHERE id='synthetic-disabled';").unwrap();
        assert_eq!(session_count(&db).unwrap(), 1);
        let user: String = db
            .query_row("SELECT userId FROM session", [], |r| r.get(0))
            .unwrap();
        assert_eq!(user, "synthetic-enabled");
        let again = rehearse(&mut db, &steps, &expected).unwrap();
        assert_eq!(again.sessions_invalidated, 0);
        assert_eq!(again.mfa_events_backfilled, 0);
        assert!(again.applied.is_empty());
    }
    #[test]
    fn real_history_insert_failure_rolls_back_entire_suffix_including_sessions() {
        let mut db = Connection::open_in_memory().unwrap();
        fixture(&db, 4);
        users(&db);
        let expected = catalogue().unwrap();
        let mut steps = steps(&ARTIFACTS, &expected).unwrap();
        // Test-only SQL fault after several real migrations, before runtime's own history insert.
        steps[11].sql.push_str("INSERT INTO workspace_schema_migrations(id,name,checksum,applied_by_version) VALUES (12,'synthetic-collision','synthetic','synthetic');");
        assert_eq!(
            rehearse(&mut db, &steps, &expected).err().unwrap().code,
            "migration_failed"
        );
        assert_eq!(history(&db, &expected).unwrap().len(), 4);
        assert_eq!(session_count(&db).unwrap(), 2);
        assert_eq!(schema(&db).unwrap(), reference(&steps, 4).unwrap());
    }
    #[test]
    #[ignore = "subprocess helper selected explicitly by process-death parent"]
    fn crash_child() {
        let mut db = Connection::open(std::env::var("RM_CONTROL_TEST_DB").unwrap()).unwrap();
        let expected = catalogue().unwrap();
        let steps = steps(&ARTIFACTS, &expected).unwrap();
        rehearse(&mut db, &steps, &expected).unwrap();
    }
    struct Reaped(Child);
    impl Drop for Reaped {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    #[test]
    fn process_death_restores_whole_prefix_and_sessions_or_commits_all() {
        for (step, phase) in [
            (5, "sql"),
            (12, "history"),
            (31, "before_commit"),
            (31, "committed"),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("private.sqlite");
            let marker = dir.path().join("phase");
            let db = Connection::open(&path).unwrap();
            fixture(&db, 4);
            users(&db);
            drop(db);
            let mut child = Reaped(
                Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "control_migration::tests::crash_child",
                        "--ignored",
                        "--nocapture",
                    ])
                    .env("RM_CONTROL_TEST_DB", &path)
                    .env("RM_CONTROL_TEST_MARKER", &marker)
                    .env("RM_CONTROL_TEST_STEP", step.to_string())
                    .env("RM_CONTROL_TEST_PHASE", phase)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
            let start = Instant::now();
            while !marker.exists() {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "child exited before {step}/{phase}"
                );
                assert!(
                    start.elapsed() < Duration::from_secs(10),
                    "checkpoint timeout"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(std::fs::read_to_string(&marker).unwrap(), phase);
            child.0.kill().unwrap();
            assert!(!child.0.wait().unwrap().success());
            let mut db = Connection::open(&path).unwrap();
            let expected = catalogue().unwrap();
            let steps = steps(&ARTIFACTS, &expected).unwrap();
            let version = if phase == "committed" { ARTIFACTS.len() } else { 4 };
            assert_eq!(history(&db, &expected).unwrap().len(), version);
            assert_eq!(schema(&db).unwrap(), reference(&steps, version).unwrap());
            integrity(&db).unwrap();
            assert_eq!(
                session_count(&db).unwrap(),
                if version == 4 { 2 } else { 0 }
            );
            let report = rehearse(&mut db, &steps, &expected).unwrap();
            assert_eq!(report.from_version, version);
        }
    }
}
