use super::*;
use crate::control_fence::CompanySelection;
use rentemester_runtime::{
    auth::{AuthError, Clock, CookieProfile, CookieVerifier},
    service_key::ServiceKey,
};
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1767225600000)
    }
}
#[test]
fn exception_original_current_service_reader_bookkeeper_reviewer_and_revoked_noop() {
    let dir = tempfile::tempdir().unwrap();
    let mut control = Connection::open(dir.path().join("control.sqlite")).unwrap();
    let mut company = Connection::open(dir.path().join("company.sqlite")).unwrap();
    rentemester_storage::fresh::initialize_empty_pair(
        &mut control,
        &mut company,
        "Synthetic Exceptions",
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
        &Fixed,
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

    company.execute_batch("INSERT INTO exceptions(id,type,severity,message) VALUES(1,'SYNTHETIC','high','Synthetic service review');").unwrap();
    let input = rentemester_storage::exceptions::Resolve {
        note: Some(" first service note ".into()),
    };
    let context = || crate::service_company::Context {
        key: &key,
        clock: &Fixed,
        max_future_days: 45,
        selection: CompanySelection {
            workspace: "w",
            slug: "a",
            registered: true,
            archived: false,
        },
    };
    assert_eq!(
        execute_service(
            &mut control,
            &mut company,
            context(),
            Action::ListWithArchived("open", false)
        )
        .unwrap()
        .result["count"],
        1
    );
    assert!(
        execute_service(
            &mut control,
            &mut company,
            context(),
            Action::Resolve(1, &input)
        )
        .is_err()
    );
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES(?1,'a','grant','bookkeeper','user:u')", [&subject]).unwrap();
    assert!(
        execute_service(
            &mut control,
            &mut company,
            context(),
            Action::Resolve(1, &input)
        )
        .is_err()
    );
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES(?1,'a','grant','reviewer','user:u')", [&subject]).unwrap();
    assert_eq!(
        execute_service(
            &mut control,
            &mut company,
            context(),
            Action::Resolve(1, &input)
        )
        .unwrap()
        .result["resolved"],
        true
    );
    assert_eq!(
        execute_service(
            &mut control,
            &mut company,
            context(),
            Action::Resolve(1, &input)
        )
        .unwrap()
        .result["resolved"],
        false
    );
    let actor: String = company
        .query_row("SELECT resolved_by FROM exceptions WHERE id=1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(actor, format!("service-account:{subject}"));
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES(?1,'a','revoke',NULL,'user:u')", [&subject]).unwrap();
    let changes = company.total_changes();
    assert!(
        execute_service(
            &mut control,
            &mut company,
            context(),
            Action::Resolve(1, &input)
        )
        .is_err()
    );
    assert!(
        execute_service(
            &mut control,
            &mut company,
            context(),
            Action::Resolve(999, &input)
        )
        .is_err()
    );
    assert_eq!(company.total_changes(), changes);
    assert_eq!(
        company
            .query_row(
                "SELECT count(*) FROM audit_log WHERE event_type='exception_resolve'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor) VALUES(?1,'a','grant','reviewer','user:u')", [&subject]).unwrap();
    control
        .execute("UPDATE apikey SET enabled=0 WHERE id=?1", [&credential])
        .unwrap();
    assert!(
        execute_service(
            &mut control,
            &mut company,
            context(),
            Action::Resolve(1, &input)
        )
        .is_err()
    );
    assert!(execute_service(&mut control, &mut company, context(), Action::List("open")).is_err());
    assert_eq!(company.total_changes(), changes);
}
