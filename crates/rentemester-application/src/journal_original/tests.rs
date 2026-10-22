use super::*;
use rentemester_runtime::{
    auth::{AuthError, CookieProfile, CookieVerifier},
    service_key::ServiceKey,
};
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1767225600000)
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
fn action() -> Action {
    parse("dry-run",&serde_json::to_vec(&json!({"input":{"transactionDate":"2026-01-01","text":"Synthetic capital","lines":[{"accountNo":"2000","debitAmount":10},{"accountNo":"5000","creditAmount":10}]}})).unwrap()).unwrap()
}
fn fixture() -> (
    tempfile::TempDir,
    Connection,
    Connection,
    CookieVerifier,
    VerifiedCookie,
) {
    let dir = tempfile::tempdir().unwrap();
    let mut control = Connection::open(dir.path().join("control.sqlite")).unwrap();
    let mut company = Connection::open(dir.path().join("company.sqlite")).unwrap();
    rentemester_storage::fresh::initialize_empty_pair(
        &mut control,
        &mut company,
        "Synthetic original journal",
    )
    .unwrap();
    control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u');").unwrap();
    company.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('2000','Synthetic bank','asset','debit'),('5000','Synthetic equity','equity','credit')").unwrap();
    let cookies = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let issued = cookies.issue_session("synthetic-token").unwrap();
    let proof = cookies
        .verify(Some(issued.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    (dir, control, company, cookies, proof)
}
fn no_journals(company: &Connection) {
    for table in ["journal_entries", "journal_lines", "audit_log", "sequences"] {
        let count: i64 = company
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "{table}");
    }
}
#[test]
fn original_human_current_grant_revocation_and_no_identity_claims() {
    let (_dir, mut control, mut company, _cookies, proof) = fixture();
    // A workspace owner is not required: native current company membership suffices.
    control.execute_batch("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','bookkeeper','user:u')").unwrap();
    let result = human(
        &mut control,
        &mut company,
        &proof,
        &Fixed,
        selection(),
        action(),
    )
    .unwrap();
    assert_eq!(result.value["ok"], true);
    no_journals(&company);
    control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u')").unwrap();
    let denied = human(
        &mut control,
        &mut company,
        &proof,
        &Fixed,
        selection(),
        action(),
    )
    .unwrap_err();
    assert_eq!(denied.fence.error.code, "access_denied");
    no_journals(&company);
    control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','bookkeeper','user:u');DELETE FROM session WHERE id='s'").unwrap();
    assert!(
        human(
            &mut control,
            &mut company,
            &proof,
            &Fixed,
            selection(),
            action()
        )
        .is_err()
    );
    no_journals(&company);
}
#[test]
fn original_service_native_credential_grant_and_revocation() {
    let (_dir, mut control, mut company, cookies, proof) = fixture();
    let receipt = crate::service_lifecycle::execute(
        &mut control,
        &cookies,
        &Fixed,
        &proof,
        &crate::service_lifecycle::Operation::Create {
            display_name: "Synthetic original journal service".into(),
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
        .execute("UPDATE apikey SET remaining=30 WHERE id=?1", [credential])
        .unwrap();
    let context = || service_company::Context {
        key: &key,
        clock: &Fixed,
        selection: selection(),
        max_future_days: 45,
    };
    let result = service(&mut control, &mut company, context(), action()).unwrap();
    assert_eq!(result.result["ok"], true);
    assert!(result.receipt.is_none());
    no_journals(&company);
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','revoke',NULL,'user:u')",[subject]).unwrap();
    let denied = service(&mut control, &mut company, context(), action())
        .err()
        .unwrap();
    assert_eq!(denied.error.code, "access_denied");
    no_journals(&company);
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','bookkeeper','user:u')",[subject]).unwrap();
    control
        .execute("UPDATE apikey SET enabled=0 WHERE id=?1", [credential])
        .unwrap();
    assert!(service(&mut control, &mut company, context(), action()).is_err());
    no_journals(&company);
}
#[test]
fn transport_confirmation_and_identity_refusal() {
    let wire = json!({"input":{"transactionDate":"2026-01-01","text":"Synthetic","createdBy":"user:other","lines":[]},"confirm":true});
    assert_eq!(
        parse("post", &serde_json::to_vec(&wire).unwrap())
            .err()
            .unwrap()
            .code,
        "journal_input_refused"
    );
    assert!(parse("post", b"{\"input\":{}}").is_err());
    assert!(
        parse(
            "reverse",
            b"{\"confirm\":false,\"transactionDate\":\"2026-01-01\",\"reason\":\"Synthetic\"}"
        )
        .is_err()
    );
}
