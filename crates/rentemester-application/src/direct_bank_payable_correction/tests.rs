use super::*;

use crate::documents::Context;
use crate::{control_fence::CompanySelection, service_company};
use rentemester_runtime::{
    auth::{AuthError, Clock, CookieProfile, CookieVerifier, VerifiedCookie},
    namespace::NamespaceProfile,
    service_key::ServiceKey,
};
use rusqlite::Connection;
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1_792_368_000_000)
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
fn original_direct_current_principal_service_revoke_and_source_replay() {
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
        "2026-10-19T00:00:00.000Z",
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
    control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-10-19','2026-10-19');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-10-19T00:00:00.000Z','2026-10-19T00:00:00.000Z','2026-10-19T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','user:u');").unwrap();
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

    company.execute_batch("UPDATE companies SET vat_period_type='quarter' WHERE id=1;INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('A','Synthetic bank','asset','debit'),('X','Synthetic expense','expense','debit'),('V','Synthetic VAT','vat','debit'),('C','Synthetic creditor','liability','credit');INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by)VALUES('bank','A','confirmed',1,'synthetic'),('input_vat','V','confirmed',1,'synthetic'),('creditors','C','confirmed',1,'synthetic');INSERT INTO bank_accounts(slug,name,ledger_account_no)VALUES('synthetic','Synthetic bank','A');").unwrap();
    control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','bookkeeper','user:u')").unwrap();
    let file = rentemester_runtime::document::Prepared::new(
        "synthetic.txt".into(),
        b"Synthetic immutable purchase original".to_vec(),
    )
    .unwrap();
    let metadata=serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","issueDate":"2026-06-16","deliveryDescription":"Synthetic supplies","invoiceNo":"SYN-145","amountIncVat":125,"vatAmount":25,"sender":{"name":"Synthetic supplier","address":"Synthetic1","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic buyer","address":"Synthetic2","vatOrCvr":"DK87654321"}})).unwrap();
    crate::documents::ingest(
        &mut control,
        &mut company,
        &mut guard,
        context(&cookie),
        crate::documents::Ingest {
            file: &file,
            metadata: &metadata,
            today: "2026-10-19",
        },
    )
    .unwrap();
    crate::bank::execute(
        &mut control,
        &mut company,
        &mut guard,
        context(&cookie),
        crate::bank::Action::Import {
            content: "date;text;amount\n2026-07-02;Synthetic SYN-145;-125\n",
            account: Some("synthetic"),
            profile: None,
            now: "2026-10-19T00:00:00Z",
        },
    )
    .unwrap();
    let expense = rentemester_storage::expense::Input {
        document_id: 1,
        bank_transaction_id: 1,
        expense_account_no: "X".into(),
        vat_treatment: Some("standard".into()),
        payment_account_no: None,
        transaction_date: None,
        text: None,
    };
    crate::expense::execute(
        &mut control,
        &mut company,
        &mut guard,
        context(&cookie),
        crate::expense::Action::Post(&expense),
    )
    .unwrap();
    let input = PlanInput {
        document_id: 1,
        bank_transaction_id: 1,
        bill_date: "2026-06-16".into(),
        due_date: "2026-07-16".into(),
        expense_account_no: "X".into(),
        vat_treatment: Some("standard".into()),
        vendor_id: None,
        note: None,
    };
    let planned = execute(
        &mut control,
        &mut company,
        &mut guard,
        context(&cookie),
        Action::OriginalPlan(&input),
    )
    .unwrap()
    .value;
    let native = execute(
        &mut control,
        &mut company,
        &mut guard,
        context(&cookie),
        Action::Plan(&input),
    )
    .unwrap()
    .value;
    assert_eq!(planned["data"]["plan"], native["plan"]);
    let mut value = serde_json::to_value(&input).unwrap();
    value["planHash"] = planned["data"]["plan"]["planHash"].clone();
    value["reason"] = json!(" Synthetic reviewed correction ");
    value["confirm"] = json!(true);
    value["idempotencyKey"] = json!("synthetic-original-direct-145");
    let apply: ApplyInput = serde_json::from_value(value).unwrap();
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

    let wrong = rentemester_storage::bank_original::ExpectedPrincipal {
        kind: "user".into(),
        subject_id: "u".into(),
    };
    assert_eq!(
        execute_service(
            &mut control,
            &mut company,
            &mut guard,
            service(),
            Action::OriginalApply {
                input: &apply,
                expected: Some(&wrong)
            }
        )
        .err()
        .unwrap()
        .error
        .code,
        "access_denied"
    );
    let first = execute_service(
        &mut control,
        &mut company,
        &mut guard,
        service(),
        Action::OriginalApply {
            input: &apply,
            expected: None,
        },
    )
    .unwrap()
    .result;
    assert_eq!(first["data"]["sourcePrincipal"]["subjectId"], subject);
    assert!(first["data"].get("ok").is_none());
    assert!(first["data"].get("errors").is_none());
    assert_eq!(first["data"]["idempotency"]["replayed"], false);
    let again = execute_service(
        &mut control,
        &mut company,
        &mut guard,
        service(),
        Action::OriginalApply {
            input: &apply,
            expected: None,
        },
    )
    .unwrap()
    .result;
    assert_eq!(again["data"]["idempotency"]["replayed"], true);
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES(?1,'a','revoke','user:u')",[subject]).unwrap();
    assert_eq!(
        execute_service(
            &mut control,
            &mut company,
            &mut guard,
            service(),
            Action::OriginalApply {
                input: &apply,
                expected: None
            }
        )
        .err()
        .unwrap()
        .error
        .code,
        "access_denied"
    );
    let document = root.join("a").join(file.relative_path());
    std::fs::write(document, b"changed original").unwrap();
    assert_eq!(
        execute(
            &mut control,
            &mut company,
            &mut guard,
            context(&cookie),
            Action::OriginalApply {
                input: &apply,
                expected: None
            }
        )
        .unwrap_err()
        .error
        .code,
        "document_evidence_unavailable"
    );
}
