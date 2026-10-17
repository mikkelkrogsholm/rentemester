use super::*;
use rusqlite::params;
use serde_json::{Value, json};
use std::path::Path;
fn company(db: &Connection) {
    db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL;CREATE TABLE sequences(kind TEXT,scope TEXT,value INTEGER,PRIMARY KEY(kind,scope));CREATE TABLE companion(n INTEGER PRIMARY KEY CHECK(n>0));").unwrap();
}
fn control(db: &Connection) {
    db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL;")
        .unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let catalogue: Value = serde_json::from_str(
        &std::fs::read_to_string(
            root.join("crates/rentemester-storage/src/legacy-workspace-schema.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut paths: Vec<_> = std::fs::read_dir(root.join("src/core/workspace-migrations"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    paths.sort();
    for (path, identity) in paths.iter().zip(catalogue.as_array().unwrap()) {
        let artifact: Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(artifact["id"], identity["id"]);
        db.execute_batch(artifact["sql"].as_str().unwrap()).unwrap();
        db.execute("INSERT INTO workspace_schema_migrations(id,name,checksum,applied_by_version) VALUES(?1,?2,?3,'synthetic')",params![identity["id"].as_i64().unwrap(),identity["name"].as_str().unwrap(),identity["checksum"].as_str().unwrap()]).unwrap();
    }
    db.execute_batch("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled) VALUES('u','Synthetic','u@example.invalid',1,'2026-01-01','2026-01-01',1);
        INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES('s','2099-01-01','synthetic-token','2026-01-01','2026-01-01','u');
        INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor) VALUES('u','activate','member','agent:synthetic');
        INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','grant','bookkeeper','agent:synthetic');").unwrap();
}
fn principal() -> HumanSession<'static> {
    HumanSession {
        workspace: "w",
        subject: "u",
        session_id: "s",
        provider_authenticated: true,
    }
}
fn selection() -> CompanySelection<'static> {
    CompanySelection {
        workspace: "w",
        slug: "a",
        registered: true,
        archived: false,
    }
}
fn effect(tx: &Transaction<'_>) -> Result<i64, Error> {
    let n = rentemester_storage::sequences::next(tx, "journal", "synthetic", 0)?;
    tx.execute("INSERT INTO companion(n) VALUES(?1)", [n])
        .map_err(|_| error("synthetic_sql"))?;
    Ok(n)
}
#[test]
fn current_observation_commit_and_business_rollback() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = Connection::open(dir.path().join("control.sqlite")).unwrap();
    control(&c);
    let mut l = Connection::open(dir.path().join("company.sqlite")).unwrap();
    company(&l);
    let before = c.total_changes();
    let committed = execute(
        &mut c,
        &mut l,
        principal(),
        selection(),
        Permission::CompanyLedgerPost,
        effect,
    )
    .unwrap();
    assert_eq!(committed.value, 1);
    assert!(committed.control_reusable);
    assert_eq!(c.total_changes(), before);
    let failure = execute(
        &mut c,
        &mut l,
        principal(),
        selection(),
        Permission::CompanyLedgerPost,
        |tx| {
            effect(tx)?;
            Err::<(), _>(error("synthetic_business"))
        },
    )
    .unwrap_err();
    assert_eq!(failure.stage, FailureStage::NotCommitted);
    assert!(failure.control_reusable && failure.company_reusable);
    assert_eq!(
        l.query_row("SELECT value FROM sequences", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        l.query_row("SELECT count(*) FROM companion", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    c.execute_batch("DELETE FROM session WHERE id='s';")
        .unwrap();
    let denied = execute(
        &mut c,
        &mut l,
        principal(),
        selection(),
        Permission::CompanyLedgerPost,
        |_| -> Result<(), Error> { panic!("denial must not invoke work") },
    )
    .unwrap_err();
    assert_eq!(denied.error.code, "access_denied");
    assert!(c.is_autocommit() && l.is_autocommit());
}
#[test]
fn committed_cleanup_failure_keeps_committed_value() {
    let mut db = Connection::open_in_memory().unwrap();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("ROLLBACK").unwrap();
    let result = finish_committed(42, tx);
    assert_eq!(result.value, 42);
    assert!(!result.control_reusable);
}
#[test]
fn manual_commit_is_unknown_never_false_rollback() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = Connection::open(dir.path().join("control.sqlite")).unwrap();
    control(&c);
    let mut l = Connection::open(dir.path().join("company.sqlite")).unwrap();
    company(&l);
    let failure = execute(
        &mut c,
        &mut l,
        principal(),
        selection(),
        Permission::CompanyLedgerPost,
        |tx| {
            let n = effect(tx)?;
            tx.execute_batch("COMMIT").unwrap();
            Ok(n)
        },
    )
    .unwrap_err();
    assert_eq!(failure.stage, FailureStage::OutcomeUnknown);
    assert!(!failure.company_reusable && !failure.control_reusable);
    assert_eq!(
        l.query_row("SELECT count(*) FROM companion", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[test]
#[ignore = "native process fence helper selected by control-fence harness"]
fn child() {
    let config: Value = serde_json::from_str(&std::env::var("RM_FENCE_REQUEST").unwrap()).unwrap();
    let mut c = Connection::open_with_flags(
        std::env::var("RM_FENCE_CONTROL").unwrap(),
        if config["readonlyControl"].as_bool() == Some(true) {
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
        } else {
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
        },
    )
    .unwrap();
    let mut l = Connection::open_with_flags(
        std::env::var("RM_FENCE_COMPANY").unwrap(),
        if config["readonlyCompany"].as_bool() == Some(true) {
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
        } else {
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
        },
    )
    .unwrap();
    c.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL;")
        .unwrap();
    l.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL;")
        .unwrap();
    c.busy_timeout(std::time::Duration::ZERO).unwrap();
    l.busy_timeout(std::time::Duration::ZERO).unwrap();
    if let Some(sql) = config["controlSql"].as_str() {
        c.execute_batch(sql).unwrap();
    }
    if let Some(sql) = config["companySql"].as_str() {
        l.execute_batch(sql).unwrap();
    }
    for (db, field) in [(&c, "expectedControlMode"), (&l, "expectedCompanyMode")] {
        if let Some(expected) = config[field].as_str() {
            assert_eq!(
                db.pragma_query_value(None, "journal_mode", |r| r.get::<_, String>(0))
                    .unwrap(),
                expected,
                "requested unsafe mode must actually be installed"
            );
        }
    }
    let p = HumanSession {
        workspace: config["workspace"].as_str().unwrap_or("w"),
        subject: config["subject"].as_str().unwrap_or("u"),
        session_id: config["session"].as_str().unwrap_or("s"),
        provider_authenticated: config["authenticated"].as_bool().unwrap_or(true),
    };
    let selection = CompanySelection {
        workspace: config["companyWorkspace"].as_str().unwrap_or("w"),
        slug: config["slug"].as_str().unwrap_or("a"),
        registered: config["registered"].as_bool().unwrap_or(true),
        archived: config["archived"].as_bool().unwrap_or(false),
    };
    let permission: Permission = serde_json::from_value(
        config
            .get("permission")
            .cloned()
            .unwrap_or(json!("company.ledger.post")),
    )
    .unwrap();
    let result = execute(&mut c, &mut l, p, selection, permission, |tx| {
        let n = effect(tx)?;
        match config["work"].as_str().unwrap_or("success") {
            "reject" => Err(error("synthetic_business")),
            "sqlfail" => {
                tx.execute("INSERT INTO companion(n) VALUES(?1)", [n])
                    .map_err(|_| error("synthetic_sql"))?;
                Ok(n)
            }
            "escape" => {
                tx.execute_batch("COMMIT").unwrap();
                Ok(n)
            }
            _ => Ok(n),
        }
    });
    let output = match result {
        Ok(done) => {
            json!({"outcome":"committed","value":done.value,"controlReusable":done.control_reusable})
        }
        Err(f) => {
            json!({"outcome":if f.stage==FailureStage::NotCommitted {"not_committed"}else{"unknown"},"code":f.error.code,"controlReusable":f.control_reusable,"companyReusable":f.company_reusable})
        }
    };
    std::fs::write(
        std::env::var("RM_FENCE_OUTPUT").unwrap(),
        serde_json::to_string(&output).unwrap(),
    )
    .unwrap();
}
