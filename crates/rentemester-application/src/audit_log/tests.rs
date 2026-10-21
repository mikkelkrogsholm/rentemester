use super::*;
use rentemester_runtime::{
    auth::{AuthError, Clock, CookieProfile, CookieVerifier, VerifiedCookie},
    service_key::ServiceKey,
};
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1_767_225_600_000)
    }
}
fn setup() -> (
    tempfile::TempDir,
    Connection,
    Connection,
    CookieVerifier,
    VerifiedCookie,
) {
    let dir = tempfile::tempdir().unwrap();
    let mut c = Connection::open(dir.path().join("control.sqlite")).unwrap();
    let mut l = Connection::open(dir.path().join("company.sqlite")).unwrap();
    rentemester_storage::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic audit").unwrap();
    c.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','user:u');").unwrap();
    l.execute_batch("INSERT INTO audit_log(event_type,entity_type,message,actor,created_at)VALUES('synthetic','journal','<script>plain text</script>','user:u','2026-01-01T00:00:00.000Z')").unwrap();
    let verifier = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let issued = verifier.issue_session("synthetic-token").unwrap();
    let cookie = verifier
        .verify(Some(issued.request_header()), None)
        .unwrap();
    (dir, c, l, verifier, cookie)
}
fn selection() -> control_fence::CompanySelection<'static> {
    control_fence::CompanySelection {
        workspace: "w",
        slug: "a",
        registered: true,
        archived: false,
    }
}
fn context(cookie: &VerifiedCookie) -> Context<'_> {
    Context {
        cookie,
        clock: &Fixed,
        selection: selection(),
    }
}
#[test]
fn current_human_reader_gets_exact_read_only_history_but_revoked_membership_and_session_refuse() {
    let (_dir, mut c, mut l, _, cookie) = setup();
    let before = (c.total_changes(), l.total_changes());
    let result = read(&mut c, &mut l, context(&cookie), &Query::default()).unwrap();
    assert_eq!(
        result.value["rows"][0]["message"],
        "<script>plain text</script>"
    );
    assert_eq!(result.value["total"], 1);
    assert!(result.control_reusable);
    assert_eq!((c.total_changes(), l.total_changes()), before);
    assert!(c.is_autocommit() && l.is_autocommit());
    c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES('u','a','revoke','user:u')").unwrap();
    assert_eq!(
        read(&mut c, &mut l, context(&cookie), &Query::default())
            .unwrap_err()
            .error
            .code,
        "access_denied"
    );
    c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','user:u');DELETE FROM session WHERE id='s'").unwrap();
    assert_eq!(
        read(&mut c, &mut l, context(&cookie), &Query::default())
            .unwrap_err()
            .error
            .code,
        "invalid_credentials"
    );
    assert_eq!(l.total_changes(), before.1);
}
#[test]
fn current_service_reader_uses_quota_and_rechecks_membership_and_key_before_disclosure() {
    let (_dir, mut c, mut l, verifier, cookie) = setup();
    c.execute_batch("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u')").unwrap();
    let receipt = crate::service_lifecycle::execute(
        &mut c,
        &verifier,
        &Fixed,
        &cookie,
        &crate::service_lifecycle::Operation::Create {
            display_name: "Synthetic audit reader".into(),
            operation_id: None,
            expires_in_seconds: None,
        },
    )
    .unwrap();
    let issued: Value = serde_json::from_str(receipt.as_json()).unwrap();
    let subject = issued["serviceAccountId"].as_str().unwrap();
    let credential = issued["credentialId"].as_str().unwrap();
    let key = ServiceKey::new("w", issued["secret"].as_str().unwrap()).unwrap();
    c.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES(?1,'activate','member','user:u')",[subject]).unwrap();
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','reader','user:u')",[subject]).unwrap();
    c.execute("UPDATE apikey SET remaining=3 WHERE id=?1", [credential])
        .unwrap();
    let context = || service_company::Context {
        key: &key,
        clock: &Fixed,
        selection: selection(),
        max_future_days: 45,
    };
    let before = l.total_changes();
    let result = read_service(&mut c, &mut l, context(), &Query::default()).unwrap();
    assert_eq!(result.result["total"], 1);
    assert!(result.receipt.is_none());
    assert_eq!(result.usage, service_company::UsageStage::Committed);
    let remaining = |db: &Connection| {
        db.query_row(
            "SELECT remaining FROM apikey WHERE id=?1",
            [credential],
            |r| r.get::<_, i64>(0),
        )
        .unwrap()
    };
    assert_eq!(remaining(&c), 2);
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES(?1,'a','revoke','user:u')",[subject]).unwrap();
    let failure = read_service(&mut c, &mut l, context(), &Query::default())
        .err()
        .unwrap();
    assert_eq!(failure.error.code, "access_denied");
    assert_eq!(failure.usage, service_company::UsageStage::NotCommitted);
    assert_eq!(remaining(&c), 2);
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','reader','user:u')",[subject]).unwrap();
    c.execute("UPDATE apikey SET enabled=0 WHERE id=?1", [credential])
        .unwrap();
    assert_eq!(
        read_service(&mut c, &mut l, context(), &Query::default())
            .err()
            .unwrap()
            .error
            .code,
        "invalid_credentials"
    );
    assert_eq!(remaining(&c), 2);
    assert_eq!(l.total_changes(), before);
    assert!(c.is_autocommit() && l.is_autocommit());
}
#[test]
fn invalid_query_preserves_books_and_current_authorization_precedes_query_disclosure() {
    let (_dir, mut c, mut l, _, cookie) = setup();
    let before = l.total_changes();
    let query = Query {
        limit: Some(0),
        ..Query::default()
    };
    assert_eq!(
        read(&mut c, &mut l, context(&cookie), &query)
            .unwrap_err()
            .error
            .code,
        "invalid_audit_log_query"
    );
    c.execute_batch("DELETE FROM session").unwrap();
    assert_eq!(
        read(&mut c, &mut l, context(&cookie), &query)
            .unwrap_err()
            .error
            .code,
        "invalid_credentials"
    );
    assert_eq!(l.total_changes(), before);
    assert!(c.is_autocommit() && l.is_autocommit());
}
