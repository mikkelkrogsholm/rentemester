use super::*;
use rentemester_runtime::auth::AuthError;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
struct FixedClock {
    now: i64,
    step: i64,
    calls: AtomicUsize,
}
impl Clock for FixedClock {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(self.now
            + if self.calls.fetch_add(1, Ordering::SeqCst) > 0 {
                self.step
            } else {
                0
            })
    }
}
#[test]
fn failure_debug_never_discloses_error_detail_or_business_payload() {
    let mut f = initial(Error::new("synthetic", "SYNTHETIC_SECRET_DETAIL"));
    f.rejection = Some(json!({"secret":"SYNTHETIC_SECRET_PAYLOAD"}));
    let shown = format!("{f:?}");
    assert!(!shown.contains("SYNTHETIC_SECRET"));
    assert!(shown.contains("NotAttempted"));
}
#[test]
#[ignore = "canonical synthetic service company quota/post/reversal process helper"]
fn child() {
    let v: Value = serde_json::from_str(&std::env::var("RM_SERVICE_REQUEST").unwrap()).unwrap();
    let mut control = Connection::open(std::env::var("RM_SERVICE_CONTROL").unwrap()).unwrap();
    let mut company = Connection::open(std::env::var("RM_SERVICE_COMPANY").unwrap()).unwrap();
    for db in [&control, &company] {
        db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL")
            .unwrap();
        db.busy_timeout(std::time::Duration::from_millis(
            v["busyMs"].as_u64().unwrap_or(100),
        ))
        .unwrap();
    }
    if v["controlQueryOnly"] == true {
        control.execute_batch("PRAGMA query_only=ON").unwrap();
    }
    if v["companyQueryOnly"] == true {
        company.execute_batch("PRAGMA query_only=ON").unwrap();
    }
    if v["controlTransaction"] == true {
        control.execute_batch("BEGIN").unwrap();
    }
    if v["companyTransaction"] == true {
        company.execute_batch("BEGIN").unwrap();
    }
    let key = ServiceKey::new(
        v["credentialWorkspace"].as_str().unwrap_or("w"),
        v["secret"].as_str().unwrap(),
    )
    .unwrap();
    let clock = FixedClock {
        now: v["now"].as_i64().unwrap(),
        step: v["clockStep"].as_i64().unwrap_or(0),
        calls: AtomicUsize::new(0),
    };
    let context = Context {
        key: &key,
        clock: &clock,
        selection: CompanySelection {
            workspace: v["workspace"].as_str().unwrap_or("w"),
            slug: v["slug"].as_str().unwrap_or("a"),
            registered: v["registered"].as_bool().unwrap_or(true),
            archived: v["archived"].as_bool().unwrap_or(false),
        },
        max_future_days: 45,
    };
    let result = if v["operation"] == "reverse" {
        execute_reversal(
            &mut control,
            &mut company,
            context,
            serde_json::from_value(v["input"].clone()).unwrap(),
        )
    } else {
        execute_post(
            &mut control,
            &mut company,
            context,
            serde_json::from_value(v["input"].clone()).unwrap(),
        )
    };
    let output = match result {
        Ok(r) => {
            json!({"ok":true,"usage":r.usage,"business":r.business,"result":r.result,"receipt":r.receipt,"controlReusable":r.control_reusable})
        }
        Err(f) => {
            json!({"ok":false,"usage":f.usage,"business":f.business,"code":f.error.code,"rejection":f.rejection,"controlReusable":f.control_reusable,"companyReusable":f.company_reusable})
        }
    };
    std::fs::write(
        std::env::var("RM_SERVICE_OUTPUT").unwrap(),
        serde_json::to_vec(&output).unwrap(),
    )
    .unwrap();
}

struct ReadFixture {
    _dir: tempfile::TempDir,
    control: Connection,
    company: Connection,
    key: ServiceKey,
    subject: String,
    credential: String,
}

#[test]
#[ignore = "explicit synthetic source-control fixture preparation before baseline capture"]
fn upgrade_legacy_service_fixture_control() {
    let mut control = Connection::open(std::env::var("RM_SERVICE_CONTROL").unwrap()).unwrap();
    control
        .execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL")
        .unwrap();
    rentemester_storage::upgrade_mail_control(&mut control).unwrap();
}
struct ReadClock;
impl Clock for ReadClock {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1767225600000)
    }
}
fn read_fixture() -> ReadFixture {
    use rentemester_runtime::auth::{CookieProfile, CookieVerifier};
    let dir = tempfile::tempdir().unwrap();
    let mut control = Connection::open(dir.path().join("control.sqlite")).unwrap();
    let mut company = Connection::open(dir.path().join("company.sqlite")).unwrap();
    rentemester_storage::fresh::initialize_empty_pair(
        &mut control,
        &mut company,
        "Synthetic service read",
    )
    .unwrap();
    control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u');").unwrap();
    let cookies = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let cookie = cookies.issue_session("synthetic-token").unwrap();
    let proof = cookies
        .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    let receipt = crate::service_lifecycle::execute(
        &mut control,
        &cookies,
        &ReadClock,
        &proof,
        &crate::service_lifecycle::Operation::Create {
            display_name: "Synthetic reader".into(),
            operation_id: None,
            expires_in_seconds: None,
        },
    )
    .unwrap();
    let issued: Value = serde_json::from_str(receipt.as_json()).unwrap();
    let subject = issued["serviceAccountId"].as_str().unwrap().to_owned();
    let credential = issued["credentialId"].as_str().unwrap().to_owned();
    let key = ServiceKey::new("w", issued["secret"].as_str().unwrap()).unwrap();
    control.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES(?1,'activate','member','user:u')", [&subject]).unwrap();
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','reader','user:u')", [&subject]).unwrap();
    control
        .execute("UPDATE apikey SET remaining=3 WHERE id=?1", [&credential])
        .unwrap();
    ReadFixture {
        _dir: dir,
        control,
        company,
        key,
        subject,
        credential,
    }
}
fn read_context<'a>(key: &'a ServiceKey, clock: &'a dyn Clock) -> Context<'a> {
    Context {
        key,
        clock,
        selection: CompanySelection {
            workspace: "w",
            slug: "a",
            registered: true,
            archived: false,
        },
        max_future_days: 45,
    }
}
fn remaining(f: &ReadFixture) -> i64 {
    f.control
        .query_row(
            "SELECT remaining FROM apikey WHERE id=?1",
            [&f.credential],
            |r| r.get(0),
        )
        .unwrap()
}
#[test]
fn service_read_reader_is_charged_once_and_returns_reusable_snapshot() {
    let mut f = read_fixture();
    let result = execute_read(
        &mut f.control,
        &mut f.company,
        read_context(&f.key, &ReadClock),
        |_| Ok(json!({"synthetic":42})),
    )
    .unwrap();
    assert_eq!(result.result, json!({"synthetic":42}));
    assert_eq!(result.usage, UsageStage::Committed);
    assert!(result.control_reusable && result.receipt.is_none());
    assert_eq!(remaining(&f), 2);
}
#[test]
fn service_read_revoked_membership_prevents_callback_and_charge() {
    let mut f = read_fixture();
    f.control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','revoke',NULL,'user:u')", [&f.subject]).unwrap();
    let failure = execute_read(
        &mut f.control,
        &mut f.company,
        read_context(&f.key, &ReadClock),
        |_| panic!("unauthorized projection"),
    )
    .err()
    .unwrap();
    assert_eq!(failure.error.code, "access_denied");
    assert_eq!(failure.usage, UsageStage::NotCommitted);
    assert_eq!(remaining(&f), 3);
}
#[test]
fn service_read_disabled_key_prevents_callback_and_charge() {
    let mut f = read_fixture();
    f.control
        .execute("UPDATE apikey SET enabled=0 WHERE id=?1", [&f.credential])
        .unwrap();
    let failure = execute_read(
        &mut f.control,
        &mut f.company,
        read_context(&f.key, &ReadClock),
        |_| panic!("revoked projection"),
    )
    .err()
    .unwrap();
    assert_eq!(failure.error.code, "invalid_credentials");
    assert_eq!(remaining(&f), 3);
}
#[test]
fn service_read_callback_write_rolls_back_company_but_not_usage() {
    let mut f = read_fixture();
    f.company
        .execute_batch("CREATE TABLE synthetic_readonly(value INTEGER)")
        .unwrap();
    let failure = execute_read(
        &mut f.control,
        &mut f.company,
        read_context(&f.key, &ReadClock),
        |tx| {
            tx.execute("INSERT INTO synthetic_readonly VALUES(1)", [])
                .unwrap();
            Ok(json!({"invalid":true}))
        },
    )
    .err()
    .unwrap();
    assert_eq!(failure.error.code, "readonly_operation_changed_data");
    assert_eq!(failure.usage, UsageStage::Committed);
    assert_eq!(remaining(&f), 2);
    assert_eq!(
        f.company
            .query_row("SELECT count(*) FROM synthetic_readonly", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn service_read_expiry_between_phases_refuses_projection_after_charge() {
    let mut f = read_fixture();
    f.control
        .execute(
            "UPDATE apikey SET expiresAt='2026-01-01T00:00:00.001Z' WHERE id=?1",
            [&f.credential],
        )
        .unwrap();
    let clock = FixedClock {
        now: 1767225600000,
        step: 2,
        calls: AtomicUsize::new(0),
    };
    let failure = execute_read(
        &mut f.control,
        &mut f.company,
        read_context(&f.key, &clock),
        |_| panic!("expired projection"),
    )
    .err()
    .unwrap();
    assert_eq!(failure.error.code, "invalid_credentials");
    assert_eq!(failure.usage, UsageStage::Committed);
    assert_eq!(remaining(&f), 2);
}
#[test]
fn service_read_frozen_workspace_preserves_readonly_usage() {
    let mut f = read_fixture();
    let tx = f
        .control
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    rentemester_storage::workspace_transfer::append(
        &tx,
        rentemester_storage::workspace_transfer::Fact::Frozen {
            transfer_id: "a".repeat(32),
            source_instance_id: "b".repeat(32),
            workspace_id: "w".into(),
            destination: "https://synthetic.invalid".into(),
            reason: "Synthetic read-only transfer".into(),
            companies: vec![rentemester_storage::workspace_transfer::Company {
                slug: "a".into(),
                ledger_uuid: "c".repeat(32),
            }],
        },
        "user:u",
        "2026-01-01T00:00:00.000Z",
    )
    .unwrap();
    tx.commit().unwrap();
    let result = execute_read(
        &mut f.control,
        &mut f.company,
        read_context(&f.key, &ReadClock),
        |_| Ok(json!({"readonly":true})),
    )
    .unwrap();
    assert_eq!(result.usage, UsageStage::NotCommitted);
    assert_eq!(remaining(&f), 3);
}
