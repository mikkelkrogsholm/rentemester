use super::*;
use rusqlite::params;
use serde_json::{Value, json};
use std::path::Path;
fn company(db: &Connection) {
    db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL;CREATE TABLE sequences(kind TEXT,scope TEXT,value INTEGER,PRIMARY KEY(kind,scope));CREATE TABLE companion(n INTEGER PRIMARY KEY CHECK(n>0));CREATE TABLE audit_log(id INTEGER PRIMARY KEY,event_type TEXT,entity_type TEXT,entity_id TEXT,message TEXT,actor TEXT,created_at TEXT DEFAULT CURRENT_TIMESTAMP);").unwrap();
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
    let denied_admin = execute(
        &mut c, &mut l, principal(), selection(), Permission::CompanyAdmin,
        |_| -> Result<(), Error> { panic!("bookkeeper cannot administer company") },
    ).unwrap_err();
    assert_eq!(denied_admin.error.code, "access_denied");
    assert!(denied_admin.control_reusable && denied_admin.company_reusable);
    let still_readable = execute(
        &mut c, &mut l, principal(), selection(), Permission::CompanyRead,
        |tx| tx.query_row("SELECT count(*) FROM companion", [], |r| r.get::<_, i64>(0))
            .map_err(|_| error("synthetic_sql")),
    ).unwrap();
    assert_eq!(still_readable.value, 1);
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
    assert!(denied.control_reusable && denied.company_reusable);
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

struct FixedClock(i64);
impl rentemester_runtime::auth::Clock for FixedClock {
    fn now_unix_ms(&self) -> Result<i64, rentemester_runtime::auth::AuthError> {
        Ok(self.0)
    }
}
#[test]
#[ignore = "synthetic cookie/fence process helper selected by session-cookie harness"]
fn cookie_child() {
    use rentemester_runtime::auth::{CookieProfile, CookieVerifier};
    let config: Value = serde_json::from_str(&std::env::var("RM_FENCE_REQUEST").unwrap()).unwrap();
    let mut c = Connection::open(std::env::var("RM_FENCE_CONTROL").unwrap()).unwrap();
    let mut l = Connection::open(std::env::var("RM_FENCE_COMPANY").unwrap()).unwrap();
    for db in [&c, &l] {
        db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL;")
            .unwrap();
        db.busy_timeout(std::time::Duration::ZERO).unwrap();
    }
    let verifier =
        CookieVerifier::new("w", CookieProfile::Secure, config["key"].as_str().unwrap()).unwrap();
    let proof = verifier.verify(config["cookie"].as_str(), config["service"].as_str());
    let result = match proof {
        Err(e) => Err(initial(Error::new(e.code(), "credential refused"))),
        Ok(proof) => {
            checkpoint("cookie_verified");
            let clock = FixedClock(config["now"].as_i64().unwrap());
            let requirement = if config["fresh"] == true {
                SessionRequirement::Fresh
            } else {
                SessionRequirement::Current
            };
            if let Some(expected) = config["expectedSubject"].as_str() {
                let tx = c
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .unwrap();
                let identity = rentemester_storage::cookie_session::resolve(
                    &tx,
                    proof.token(),
                    clock.0,
                    requirement,
                )
                .unwrap();
                assert_eq!(identity.subject, expected);
                assert_eq!(
                    identity.session_id,
                    config["expectedSession"].as_str().unwrap()
                );
                tx.rollback().unwrap();
            }
            execute_cookie(
                &mut c,
                &mut l,
                CookieAuthorization {
                    cookie: &proof,
                    clock: &clock,
                    requirement,
                },
                selection(),
                Permission::CompanyLedgerPost,
                effect,
            )
        }
    };
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

#[test]
fn cookie_clock_failure_workspace_binding_and_admission_time() {
    use rentemester_runtime::auth::{AuthError, Clock, CookieProfile, CookieVerifier};
    use std::cell::Cell;
    struct MovingClock(Cell<i64>);
    impl Clock for MovingClock {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Ok(self.0.get())
        }
    }
    struct BrokenClock;
    impl Clock for BrokenClock {
        fn now_unix_ms(&self) -> Result<i64, AuthError> {
            Err(AuthError::InvalidClock)
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let mut c = Connection::open(dir.path().join("c")).unwrap();
    control(&c);
    let mut l = Connection::open(dir.path().join("l")).unwrap();
    company(&l);
    c.execute_batch("UPDATE session SET createdAt='2026-01-01T00:00:00.000Z',expiresAt='2026-01-01T12:00:00.000Z'").unwrap();
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../rentemester-runtime/fixtures/session-cookie.json"
    ))
    .unwrap();
    let key = corpus["key"].as_str().unwrap();
    let header = corpus["vectors"][0]["header"].as_str().unwrap();
    let proof = CookieVerifier::new("w", CookieProfile::Secure, key)
        .unwrap()
        .verify(Some(header), None)
        .unwrap();
    let failure = execute_cookie(
        &mut c,
        &mut l,
        CookieAuthorization {
            cookie: &proof,
            clock: &BrokenClock,
            requirement: SessionRequirement::Current,
        },
        selection(),
        Permission::CompanyLedgerPost,
        |_| -> Result<(), Error> { panic!("clock refusal must precede work") },
    )
    .unwrap_err();
    assert_eq!(failure.error.code, "invalid_auth_clock");
    assert!(failure.control_reusable);
    let wrong = CookieVerifier::new("other", CookieProfile::Secure, key)
        .unwrap()
        .verify(Some(header), None)
        .unwrap();
    let clock = MovingClock(Cell::new(1_767_226_199_999));
    let failure = execute_cookie(
        &mut c,
        &mut l,
        CookieAuthorization {
            cookie: &wrong,
            clock: &clock,
            requirement: SessionRequirement::Fresh,
        },
        selection(),
        Permission::CompanyLedgerPost,
        effect,
    )
    .unwrap_err();
    assert_eq!(failure.error.code, "access_denied");
    // Crossing freshness after authorization does not undo admitted synchronous work.
    let result = execute_cookie(
        &mut c,
        &mut l,
        CookieAuthorization {
            cookie: &proof,
            clock: &clock,
            requirement: SessionRequirement::Fresh,
        },
        selection(),
        Permission::CompanyLedgerPost,
        |tx| {
            clock.0.set(1_767_226_200_000);
            effect(tx)
        },
    )
    .unwrap();
    assert_eq!(result.value, 1);
    assert_eq!(
        execute_cookie(
            &mut c,
            &mut l,
            CookieAuthorization {
                cookie: &proof,
                clock: &clock,
                requirement: SessionRequirement::Fresh
            },
            selection(),
            Permission::CompanyLedgerPost,
            effect
        )
        .unwrap_err()
        .error
        .code,
        "invalid_credentials"
    );
}

#[test]
fn durable_receipt_replay_is_below_current_authorization_fence() {
    use rentemester_storage::idempotency::{self, Input, Operation, Outcome, PrincipalKind};
    let dir = tempfile::tempdir().unwrap();
    let mut c = Connection::open(dir.path().join("control.sqlite")).unwrap();
    control(&c);
    let mut l = Connection::open(dir.path().join("company.sqlite")).unwrap();
    company(&l);
    // Exact receipt DDL; company fixture here qualifies authorization composition only.
    for raw in [
        include_str!("../../../../src/core/migrations/0020-mutation-idempotency-receipts.json"),
        include_str!("../../../../src/core/migrations/0023-local-idempotency-tombstones.json"),
    ] {
        let v: Value = serde_json::from_str(raw).unwrap();
        l.execute_batch(v["sql"].as_str().unwrap()).unwrap();
    }
    l.execute_batch("CREATE TABLE IF NOT EXISTS audit_log(id INTEGER PRIMARY KEY,event_type TEXT,entity_type TEXT,entity_id TEXT,message TEXT,actor TEXT,created_at TEXT DEFAULT CURRENT_TIMESTAMP)").unwrap();
    let payload = json!({"amount":"100.00"});
    for replay in [false, true] {
        let committed = execute(
            &mut c,
            &mut l,
            principal(),
            selection(),
            Permission::CompanyLedgerPost,
            |tx| {
                idempotency::execute(
                    tx,
                    Input {
                        key: "synthetic-retry",
                        operation: Operation::JournalPost,
                        principal_kind: PrincipalKind::User,
                        subject: "u",
                        payload: &payload,
                        created_by: "user:u",
                        created_by_program: "test",
                        now_ms: 1_767_225_600_000,
                    },
                    |tx| {
                        if replay {
                            panic!("replay callback");
                        }
                        Ok(json!({"ok":true,"effect":effect(tx)?}))
                    },
                )
                .map_err(|failure| failure.error)
            },
        )
        .unwrap();
        assert!(
            matches!(committed.value,Outcome::Accepted{receipt,..} if receipt.replayed==replay)
        );
    }
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES('u','a','revoke',NULL,'agent:synthetic')",[]).unwrap();
    let before = l.total_changes();
    let denied = execute(
        &mut c,
        &mut l,
        principal(),
        selection(),
        Permission::CompanyLedgerPost,
        |_| -> Result<(), Error> {
            panic!("revoked principal must not look up or replay protected receipt")
        },
    )
    .err()
    .unwrap();
    assert_eq!(denied.error.code, "access_denied");
    assert_eq!(l.total_changes(), before);
    assert_eq!(
        l.query_row("SELECT COUNT(*) FROM companion", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn control_owned_metadata_commits_but_company_changes_are_rolled_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = Connection::open(dir.path().join("control.sqlite")).unwrap();
    control(&c);
    let mut l = Connection::open(dir.path().join("company.sqlite")).unwrap();
    company(&l);
    let insert = |tx: &Transaction<'_>| {
        tx.execute("INSERT INTO workspace_audit(event_type,entity_type,entity_id,actor) VALUES('synthetic_metadata','synthetic','synthetic','user:u')", []).map_err(|_| error("synthetic_sql"))?;
        Ok::<(), Error>(())
    };
    execute_authorized_both(&mut c, &mut l, None, true, |_| Ok("u".into()), |control, _, _| insert(control)).unwrap();
    let count = |db: &Connection| db.query_row("SELECT count(*) FROM workspace_audit WHERE event_type='synthetic_metadata'", [], |row| row.get::<_, i64>(0)).unwrap();
    assert_eq!(count(&c), 1);
    let failure = execute_authorized_both(&mut c, &mut l, None, true, |_| Ok("u".into()), |control, ledger, _| {
        insert(control)?;
        effect(ledger)?;
        Ok(())
    }).unwrap_err();
    assert_eq!(failure.error.code, "transaction_escaped");
    assert_eq!(count(&c), 1);
    assert_eq!(l.query_row("SELECT count(*) FROM companion", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
}
