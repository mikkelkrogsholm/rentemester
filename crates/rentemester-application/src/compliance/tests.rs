use super::*;
use crate::control_fence::CompanySelection;
use rentemester_runtime::auth::{AuthError, Clock, CookieProfile, CookieVerifier};
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1_767_225_600_000)
    }
}
#[test]
fn current_reader_company_boundary_revocation_and_no_writes() {
    let dir = tempfile::tempdir().unwrap();
    let mut c = Connection::open(dir.path().join("c.sqlite")).unwrap();
    let mut l = Connection::open(dir.path().join("l.sqlite")).unwrap();
    rentemester_storage::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic Compliance")
        .unwrap();
    c.execute_batch("INSERT INTO user(id,name,email,emailVerified,createdAt,updatedAt,twoFactorEnabled)VALUES('u','Synthetic','u@example.invalid',1,'2026-01-01','2026-01-01',1);INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId)VALUES('s','2026-01-01T12:00:00.000Z','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','u');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','agent:synthetic');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','agent:synthetic');").unwrap();
    l.execute_batch("INSERT INTO customers(name,vat_or_cvr,email)VALUES('Synthetic Subject','SYN-GDPR','subject@example.invalid');").unwrap();
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../rentemester-runtime/fixtures/session-cookie.json"
    ))
    .unwrap();
    let cookie = CookieVerifier::new("w", CookieProfile::Secure, fixture["key"].as_str().unwrap())
        .unwrap()
        .verify(fixture["vectors"][0]["header"].as_str(), None)
        .unwrap();
    let context = |slug| Context {
        cookie: &cookie,
        clock: &Fixed,
        selection: CompanySelection {
            workspace: "w",
            slug,
            registered: true,
            archived: false,
        },
    };

    let before = l.total_changes();
    let control_before = c.total_changes();
    let result = execute(&mut c, &mut l, context("a"), "2026-01-01")
        .unwrap()
        .value;
    assert_eq!(result["data"]["company"]["name"], "Synthetic Compliance");
    assert_eq!(
        result["sha256"],
        rentemester_engine::compliance_report::fingerprint(result["html"].as_str().unwrap())
    );
    assert!(execute(&mut c, &mut l, context("other"), "2026-01-01").is_err());
    assert!(execute(&mut c, &mut l, context("a"), "2026-02-30").is_err());
    assert_eq!(l.total_changes(), before);
    assert_eq!(c.total_changes(), control_before);
    c.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'agent:synthetic');").unwrap();
    assert!(execute(&mut c, &mut l, context("a"), "2026-01-01").is_err());
    assert_eq!(l.total_changes(), before);
}
