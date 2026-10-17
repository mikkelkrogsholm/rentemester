//! All control migrations, one pending-suffix transaction, private captures only.
use crate::migration::{digest, integrity, schema, strict_history};
use crate::{Migration, Snapshot, invalid};
use rentemester_engine::Error;
use rusqlite::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

const ARTIFACTS: [&str; 30] = [
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
    serde_json::from_str(include_str!("legacy-workspace-schema.json")).map_err(invalid)
}
fn steps(raw: &[&str], expected: &[Migration]) -> Result<Vec<Step>, Error> {
    if expected.len() != 30 || raw.len() != 30 {
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
    if current_schema != reference(steps, 30)? {
        return Err(unsupported("final control schema differs"));
    }
    let new = history(&tx, expected)?;
    if new.len() != 30 || new[..version] != old || digest(&tx, &tables)? != before {
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
    test_checkpoint(30, "before_commit");
    tx.commit().map_err(failed)?;
    #[cfg(test)]
    test_checkpoint(30, "committed");
    Ok(ControlRehearsalReport {
        from_version: version,
        to_version: 30,
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

#[cfg(test)]
mod tests {
    use super::*;
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
    fn all_control_artifacts_are_bound_and_public_errors_are_opaque() {
        let expected = catalogue().unwrap();
        assert_eq!(steps(&ARTIFACTS, &expected).unwrap().len(), 30);
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
            (30, "before_commit"),
            (30, "committed"),
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
            let version = if phase == "committed" { 30 } else { 4 };
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
