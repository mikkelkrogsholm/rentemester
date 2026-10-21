use super::*;
use rentemester_runtime::auth::{AuthError, Clock, CookieProfile, CookieVerifier};
use serde_json::json;
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1_767_225_600_000)
    }
}
#[test]
fn current_write_authority_precedes_source_access_and_revocation() {
    let dir = tempfile::tempdir().unwrap();
    let mut control = Connection::open(dir.path().join("control.sqlite")).unwrap();
    let mut company = Connection::open(dir.path().join("company.sqlite")).unwrap();
    for db in [&control, &company] {
        db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL")
            .unwrap();
    }
    rentemester_storage::fresh::initialize_empty_pair(
        &mut control,
        &mut company,
        "Synthetic writeoff access",
    )
    .unwrap();
    control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','user:u');").unwrap();
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
    let context = || Context {
        cookie: &cookie,
        clock: &Fixed,
        selection: control_fence::CompanySelection {
            workspace: "w",
            slug: "a",
            registered: true,
            archived: false,
        },
    };
    let input: WriteOff=serde_json::from_value(json!({"invoiceDocumentId":1,"writeOffDate":"2026-01-01","expenseAccountNo":"E","confirm":true})).unwrap();
    let before = (control.total_changes(), company.total_changes());
    let error = write_off_with_sources(&mut control, &mut company, context(), &input, |_, _| {
        panic!("reader cannot reach writeoff sources")
    })
    .unwrap_err();
    assert_eq!(error.error.code, "access_denied");
    assert_eq!((control.total_changes(), company.total_changes()), before);
    control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','bookkeeper','user:u')").unwrap();
    let mut reached = false;
    let error = write_off_with_sources(&mut control, &mut company, context(), &input, |tx, _| {
        reached = true;
        assert_eq!(
            tx.transaction_state(Some(rusqlite::MAIN_DB)).unwrap(),
            rusqlite::TransactionState::Write
        );
        Err(Error::new(
            "synthetic_source_check",
            "synthetic source unavailable",
        ))
    })
    .unwrap_err();
    assert!(reached, "{}", error.error.code);
    assert_eq!(error.error.code, "synthetic_source_check");
    control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES('u','a','revoke','user:u')").unwrap();
    let error = write_off_with_sources(&mut control, &mut company, context(), &input, |_, _| {
        panic!("revoked reader cannot reach sources")
    })
    .unwrap_err();
    assert_eq!(error.error.code, "access_denied");
}
