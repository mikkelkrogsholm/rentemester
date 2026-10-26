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
fn action(bytes: &[u8]) -> Action<'_> {
    Action::ImportOrdered {
        statement_order:Some("ascending"),
        original:true,
        content: bytes,
        account: None,
        profile: None,
        now: "2026-01-01T00:00:00Z",
    }
}
#[test]
fn original_import_current_human_service_and_existing_source_integrity() {
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

    let csv = b"date,text,amount\n2026-01-01,Synthetic original,1.23\n";
    assert_eq!(
        execute(
            &mut control,
            &mut company,
            &mut guard,
            context(&cookie),
            action(csv)
        )
        .unwrap_err()
        .error
        .code,
        "access_denied"
    );
    control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','bookkeeper','user:u')").unwrap();
    let before = company.total_changes();
    let refused = execute(
        &mut control,
        &mut company,
        &mut guard,
        context(&cookie),
        action(b"bad CSV"),
    )
    .unwrap_err();
    assert_eq!(refused.error.code, "bank_import_invalid_csv");
    assert_eq!(refused.stage, control_fence::FailureStage::NotCommitted);
    assert!(refused.control_reusable && refused.company_reusable);
    assert_eq!(before, company.total_changes());
    let result = execute(
        &mut control,
        &mut company,
        &mut guard,
        context(&cookie),
        action(csv),
    )
    .unwrap()
    .value;
    assert_eq!(result["ok"], true);
    assert_eq!(result["data"]["imported"], 1);
    assert_eq!(result["data"]["exceptionsCreated"], 1);
    for field in ["bankAccountId", "bankAccountSlug", "profile"] {
        assert!(result["data"].get(field).is_none());
    }
    let duplicate = execute(
        &mut control,
        &mut company,
        &mut guard,
        context(&cookie),
        action(csv),
    )
    .unwrap()
    .value;
    assert_eq!(duplicate["data"]["imported"], 0);
    assert_eq!(duplicate["data"]["exceptionsCreated"], 0);
    control.execute_batch("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u')").unwrap();
    let receipt = crate::service_lifecycle::execute(
        &mut control,
        &verifier,
        &Fixed,
        &cookie,
        &crate::service_lifecycle::Operation::Create {
            display_name: "Synthetic bank importer".into(),
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
    let repeated = execute_service(
        &mut control,
        &mut company,
        &mut guard,
        service(),
        action(csv),
    )
    .unwrap();
    assert_eq!(repeated.result, duplicate);
    assert_eq!(repeated.usage, service_company::UsageStage::Committed);
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES(?1,'a','revoke','user:u')",[subject]).unwrap();
    assert_eq!(
        execute_service(
            &mut control,
            &mut company,
            &mut guard,
            service(),
            action(csv)
        )
        .err()
        .unwrap()
        .error
        .code,
        "access_denied"
    );
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','bookkeeper','user:u')",[subject]).unwrap();
    let original = root.join("a/bank/processed").join(format!(
        "{}.txt",
        result["data"]["sourceFileHash"].as_str().unwrap()
    ));
    std::fs::write(&original, b"changed source").unwrap();
    let before = company.total_changes();
    assert_eq!(
        execute(
            &mut control,
            &mut company,
            &mut guard,
            context(&cookie),
            action(csv)
        )
        .unwrap_err()
        .error
        .code,
        "bank_original_unavailable"
    );
    assert_eq!(
        execute_service(
            &mut control,
            &mut company,
            &mut guard,
            service(),
            action(csv)
        )
        .err()
        .unwrap()
        .error
        .code,
        "bank_original_unavailable"
    );
    assert_eq!(company.total_changes(), before);
    assert_eq!(
        company
            .query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
            .unwrap(),
        0
    );
}
#[test]
fn original_import_parser_mapping_is_finite_and_stage_scoped() {
    for code in [
        "invalid_bank_csv",
        "invalid_input",
        "invalid_currency",
        "invalid_rate",
    ] {
        assert_eq!(
            original_parser_error(Error::new(code, "ordinary parser validation")).code,
            "bank_import_invalid_csv"
        );
    }
    for code in [
        "bank_original_unavailable",
        "bank_refused",
        "access_denied",
        "transaction_required",
        "outcome_unknown",
    ] {
        assert_eq!(
            original_parser_error(Error::new(code, "not parser validation")).code,
            code
        );
    }
}
