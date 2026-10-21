use super::*;
use crate::bank::{Action as BankAction, execute, execute_service};
use crate::documents::Context;
use crate::{control_fence::CompanySelection, service_company};
use rentemester_runtime::{
    auth::{AuthError, Clock, CookieProfile, CookieVerifier, VerifiedCookie},
    namespace::NamespaceProfile,
    service_key::ServiceKey,
};
use rusqlite::{Connection, TransactionBehavior};
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
fn original_bank_link_current_human_service_source_and_principal_fences() {
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

    company.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('1751','Synthetic bank','asset','debit'),('8751','Synthetic equity','equity','credit');INSERT INTO bank_accounts(slug,name,ledger_account_no,currency)VALUES('synthetic','Synthetic bank','1751','DKK');").unwrap();
    let input = store::Link {
        confirm: true,
        bank_transaction_id: 1,
        journal_entry_id: 1,
        match_method: "source-reference".into(),
        source_reference: Some("synthetic:current".into()),
        note: None,
    };
    assert_eq!(
        execute(
            &mut control,
            &mut company,
            &mut guard,
            context(&cookie),
            BankAction::Original(Action::Link(&input))
        )
        .unwrap_err()
        .error
        .code,
        "access_denied"
    );
    control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','bookkeeper','user:u')").unwrap();
    let csv = b"transaction_date,text,amount,currency\n2026-01-01,Synthetic original,12.34,DKK\n";
    let imported = execute(
        &mut control,
        &mut company,
        &mut guard,
        context(&cookie),
        BankAction::ImportOriginal {
            content: csv,
            account: Some("synthetic"),
            profile: None,
            now: "2026-01-01T00:00:00Z",
        },
    )
    .unwrap()
    .value;
    let tx = company
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let journal=rentemester_storage::journal_post::post(&tx,&serde_json::from_value(json!({"transactionDate":"2026-01-01","text":"Synthetic posting","currency":"DKK","lines":[{"accountNo":"1751","debitAmount":"12.34"},{"accountNo":"8751","creditAmount":"12.34"}]})).unwrap(),&rentemester_storage::journal_preview::manual_journal_rule_version().unwrap(),rentemester_engine::fiscal_year::Date::parse("2026-01-01").unwrap(),rentemester_storage::journal_post::Actor{subject:"user:u",program:"rentemester-rust"}).unwrap();
    assert_eq!(journal["ok"], true);
    assert_eq!(journal["entryId"], 1);
    tx.commit().unwrap();
    let first = execute(
        &mut control,
        &mut company,
        &mut guard,
        context(&cookie),
        BankAction::Original(Action::Link(&input)),
    )
    .unwrap()
    .value;
    assert_eq!(first["data"]["idempotent"], false);
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
        BankAction::Original(Action::Link(&input)),
    )
    .unwrap();
    assert_eq!(repeated.result["data"]["idempotent"], true);
    let wrong = store::Correct {
        input: native::correction::Apply {
            confirm: true,
            bank_transaction_id: 1,
            replacement_journal_entry_id: 3,
            expected_reconciliation_id: "append-only:1".into(),
            plan_hash: "a".repeat(64),
            reason: "Synthetic reason".into(),
            idempotency_key: "synthetic-assertion".into(),
        },
        expected_principal: Some(store::ExpectedPrincipal {
            kind: "user".into(),
            subject_id: "u".into(),
        }),
    };
    assert_eq!(
        execute_service(
            &mut control,
            &mut company,
            &mut guard,
            service(),
            BankAction::Original(Action::Correct(&wrong))
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
            BankAction::Original(Action::Link(&input))
        )
        .err()
        .unwrap()
        .error
        .code,
        "access_denied"
    );
    let original = root.join("a/bank/processed").join(format!(
        "{}.txt",
        imported["data"]["sourceFileHash"].as_str().unwrap()
    ));
    std::fs::write(original, b"changed source").unwrap();
    let before = company.total_changes();
    let refused = execute(
        &mut control,
        &mut company,
        &mut guard,
        context(&cookie),
        BankAction::Original(Action::Link(&input)),
    )
    .unwrap_err();
    assert!(matches!(
        refused.error.code,
        "bank_statement_original_unavailable"
            | "namespace_source_changed"
            | "bank_original_unavailable"
    ));
    assert_eq!(before, company.total_changes());
}
