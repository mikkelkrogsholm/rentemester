use super::*;
use crate::{control_fence::CompanySelection, service_company};
use rentemester_runtime::{
    auth::{AuthError, Clock, CookieProfile, CookieVerifier, VerifiedCookie},
    namespace::NamespaceProfile,
    service_key::ServiceKey,
};
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1_767_225_600_000)
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
fn context(cookie: &VerifiedCookie) -> Context<'_> {
    Context {
        cookie,
        clock: &Fixed,
        selection: selection(),
    }
}
#[test]
fn account_original_master_data_and_browser_admin_are_distinct_current_human_service_rights() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap().join("workspace");
    let uid = String::from_utf8(
        std::process::Command::new("id")
            .arg("-u")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .parse()
    .unwrap();
    let mut guard = NamespaceGuard::create_fresh_with_profile(
        &root,
        uid,
        "w",
        "a",
        "Synthetic HTML <company>",
        "2026-01-01T00:00:00.000Z",
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    let mut control = Connection::open(root.join(".rentemester/workspace-control.sqlite")).unwrap();
    let mut company = Connection::open(root.join("a/data/ledger.sqlite")).unwrap();
    for db in [&control, &company] {
        db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL")
            .unwrap();
    }
    rentemester_storage::fresh::initialize_empty_pair(
        &mut control,
        &mut company,
        "Synthetic HTML <company>",
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

    let data: AccountInput =
        serde_json::from_value(serde_json::json!({"name":"Synthetic original","accountNo":"123"}))
            .unwrap();
    assert_eq!(
        execute(
            &mut control,
            &mut company,
            &mut guard,
            context(&cookie),
            Action::CreateOriginal(&data)
        )
        .unwrap_err()
        .error
        .code,
        "access_denied"
    );
    control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','bookkeeper','user:u')").unwrap();
    let first = execute(
        &mut control,
        &mut company,
        &mut guard,
        context(&cookie),
        Action::CreateOriginal(&data),
    )
    .unwrap()
    .value;
    assert_eq!(first["data"]["account"]["slug"], "synthetic-original");
    let patch = AccountPatch {
        account_no: Some("".into()),
        ..AccountPatch::default()
    };
    assert_eq!(
        execute(
            &mut control,
            &mut company,
            &mut guard,
            context(&cookie),
            Action::UpdateOriginal {
                account: "synthetic-original",
                patch: &patch
            }
        )
        .unwrap()
        .value["data"]["account"]["accountNo"],
        Value::Null
    );
    assert_eq!(
        execute(
            &mut control,
            &mut company,
            &mut guard,
            context(&cookie),
            Action::Create(&data)
        )
        .unwrap_err()
        .error
        .code,
        "access_denied"
    );
    assert_eq!(
        execute(
            &mut control,
            &mut company,
            &mut guard,
            context(&cookie),
            Action::UpdateBrowser {
                account: "1",
                patch: &patch
            }
        )
        .unwrap_err()
        .error
        .code,
        "access_denied"
    );
    control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES('u','a','revoke','user:u')").unwrap();
    assert_eq!(
        execute(
            &mut control,
            &mut company,
            &mut guard,
            context(&cookie),
            Action::UpdateOriginal {
                account: "1",
                patch: &patch
            }
        )
        .unwrap_err()
        .error
        .code,
        "access_denied"
    );
    control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u')").unwrap();
    let browser = execute(
        &mut control,
        &mut company,
        &mut guard,
        context(&cookie),
        Action::UpdateBrowser {
            account: "1",
            patch: &patch,
        },
    )
    .unwrap()
    .value;
    assert_eq!(browser["id"], first["data"]["account"]["id"]);
    let receipt = crate::service_lifecycle::execute(
        &mut control,
        &verifier,
        &Fixed,
        &cookie,
        &crate::service_lifecycle::Operation::Create {
            display_name: "Synthetic account master data".into(),
            operation_id: None,
            expires_in_seconds: None,
        },
    )
    .unwrap();
    let issued: Value = serde_json::from_str(receipt.as_json()).unwrap();
    let subject = issued["serviceAccountId"].as_str().unwrap();
    let credential = issued["credentialId"].as_str().unwrap();
    let key = ServiceKey::new("w", issued["secret"].as_str().unwrap()).unwrap();
    control.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES(?1,'activate','member','user:u')",[subject]).unwrap();
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','bookkeeper','user:u')",[subject]).unwrap();
    control
        .execute("UPDATE apikey SET remaining=10 WHERE id=?1", [credential])
        .unwrap();
    let service = || service_company::Context {
        key: &key,
        clock: &Fixed,
        selection: selection(),
        max_future_days: 45,
    };
    let second: AccountInput =
        serde_json::from_value(serde_json::json!({"name":"Synthetic service bank"})).unwrap();
    assert_eq!(
        execute_service(
            &mut control,
            &mut company,
            &mut guard,
            service(),
            Action::CreateOriginal(&second)
        )
        .unwrap()
        .result["ok"],
        true
    );
    assert_eq!(
        execute_service(
            &mut control,
            &mut company,
            &mut guard,
            service(),
            Action::UpdateOriginal {
                account: "1",
                patch: &patch
            }
        )
        .unwrap()
        .result["ok"],
        true
    );
    assert_eq!(
        execute_service(
            &mut control,
            &mut company,
            &mut guard,
            service(),
            Action::UpdateBrowser {
                account: "1",
                patch: &patch
            }
        )
        .err()
        .unwrap()
        .error
        .code,
        "access_denied"
    );
    assert_eq!(
        execute_service(
            &mut control,
            &mut company,
            &mut guard,
            service(),
            Action::Create(&second)
        )
        .err()
        .unwrap()
        .error
        .code,
        "access_denied"
    );
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES(?1,'a','revoke','user:u')",[subject]).unwrap();
    assert_eq!(
        execute_service(
            &mut control,
            &mut company,
            &mut guard,
            service(),
            Action::UpdateOriginal {
                account: "1",
                patch: &patch
            }
        )
        .err()
        .unwrap()
        .error
        .code,
        "access_denied"
    );
    assert_eq!(company.query_row::<i64,_,_>("SELECT count(*) FROM audit_log WHERE event_type IN ('bank_account_add','bank_account_update') AND (entity_type!='bank_account' OR actor NOT LIKE '%:%')",[],|r|r.get(0)).unwrap(),0);
    assert_eq!(
        company
            .query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
            .unwrap(),
        0
    );
}
