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
fn bank_queries_current_human_service_and_unmatched_original_integrity() {
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

    company.execute_batch("INSERT INTO bank_accounts(slug,name,currency)VALUES('synthetic','Synthetic statement','DKK')").unwrap();
    let csv = b"transaction_date,text,amount,currency\n2025-12-31,Synthetic unmatched,1.23,DKK\n";
    let archive = guard.publish_bank_statement_bytes(csv).unwrap();
    {
        let tx = company
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        rentemester_storage::bank::import(
            &tx,
            std::str::from_utf8(csv).unwrap(),
            &rentemester_engine::bank::parse(std::str::from_utf8(csv).unwrap()).unwrap(),
            Some("synthetic"),
            "u",
            "2026-01-01T00:00:00Z",
            &archive,
        )
        .unwrap();
        tx.commit().unwrap();
    }
    let q = ListQuery::default();
    let before = (control.total_changes(), company.total_changes());
    let value = execute(
        &mut control,
        &mut company,
        &mut guard,
        context(&cookie),
        Action::Transactions(&q),
    )
    .unwrap()
    .value;
    assert_eq!(value["data"]["count"], 1);
    assert_eq!(value["data"]["rows"][0]["amount"], 1.23);
    assert_eq!(before, (control.total_changes(), company.total_changes()));
    control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES('u','a','revoke','user:u')").unwrap();
    assert_eq!(
        execute(
            &mut control,
            &mut company,
            &mut guard,
            context(&cookie),
            Action::Transactions(&q)
        )
        .err()
        .unwrap()
        .error
        .code,
        "access_denied"
    );
    control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','user:u');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u')").unwrap();
    let receipt = crate::service_lifecycle::execute(
        &mut control,
        &verifier,
        &Fixed,
        &cookie,
        &crate::service_lifecycle::Operation::Create {
            display_name: "Synthetic bank reader".into(),
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
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','reader','user:u')",[subject]).unwrap();
    control
        .execute("UPDATE apikey SET remaining=10 WHERE id=?1", [credential])
        .unwrap();
    let service = || service_company::Context {
        key: &key,
        clock: &Fixed,
        selection: selection(),
        max_future_days: 45,
    };
    let result = execute_service(
        &mut control,
        &mut company,
        &mut guard,
        service(),
        Action::Transactions(&q),
    )
    .unwrap();
    assert_eq!(result.result, value);
    assert_eq!(result.usage, service_company::UsageStage::Committed);
    let report = ReportQuery {
        from: "2025-01-01".into(),
        to: "2026-12-31".into(),
        filters: Default::default(),
    };
    assert_eq!(
        execute_service(
            &mut control,
            &mut company,
            &mut guard,
            service(),
            Action::Reconciliation(&report)
        )
        .unwrap()
        .result["data"]["unmatchedAmountTotal"],
        1.23
    );
    assert_eq!(
        execute_service(
            &mut control,
            &mut company,
            &mut guard,
            service(),
            Action::AccountList(&AccountsQuery::default())
        )
        .unwrap()
        .result["data"]["count"],
        1
    );
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES(?1,'a','revoke','user:u')",[subject]).unwrap();
    assert_eq!(
        execute_service(
            &mut control,
            &mut company,
            &mut guard,
            service(),
            Action::Transactions(&q)
        )
        .err()
        .unwrap()
        .error
        .code,
        "access_denied"
    );
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','reader','user:u')",[subject]).unwrap();
    let original = root.join("a").join(archive);
    std::fs::write(&original, b"changed original").unwrap();
    assert_eq!(
        execute(
            &mut control,
            &mut company,
            &mut guard,
            context(&cookie),
            Action::Transactions(&q)
        )
        .err()
        .unwrap()
        .error
        .code,
        "bank_query_sources_unavailable"
    );
    assert_eq!(
        execute_service(
            &mut control,
            &mut company,
            &mut guard,
            service(),
            Action::Transactions(&q)
        )
        .err()
        .unwrap()
        .error
        .code,
        "bank_query_sources_unavailable"
    );
    std::fs::write(&original, csv).unwrap();
    assert_eq!(
        execute(
            &mut control,
            &mut company,
            &mut guard,
            context(&cookie),
            Action::Transactions(&q)
        )
        .unwrap()
        .value,
        value
    );
}
