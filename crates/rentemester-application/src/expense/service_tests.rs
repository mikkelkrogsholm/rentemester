#![cfg(target_os = "macos")]
use super::*;
use crate::{
    control_fence::CompanySelection,
    service_company::{
        BusinessStage, Context as ServiceContext, ServiceFailure, ServiceResult, UsageStage,
    },
};
use rentemester_runtime::{
    auth::{AuthError, Clock, CookieProfile, CookieVerifier},
    namespace::NamespaceProfile,
    service_key::ServiceKey,
};
use serde_json::json;
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1767225600000)
    }
}
struct Fixture {
    _dir: tempfile::TempDir,
    control: Connection,
    company: Connection,
    guard: NamespaceGuard,
    key: ServiceKey,
    subject: String,
    credential: String,
}
fn fixture(role: &str) -> Fixture {
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
    let guard = NamespaceGuard::create_fresh_with_profile(
        &root,
        uid,
        "w",
        "a",
        "Synthetic service sales",
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
        "Synthetic service seller",
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
            display_name: "Synthetic sales service".into(),
            operation_id: None,
            expires_in_seconds: None,
        },
    )
    .unwrap();
    let issued: Value = serde_json::from_str(receipt.as_json()).unwrap();
    let subject = issued["serviceAccountId"].as_str().unwrap().to_owned();
    let credential = issued["credentialId"].as_str().unwrap().to_owned();
    let key = ServiceKey::new("w", issued["secret"].as_str().unwrap()).unwrap();
    control.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES(?1,'activate','member','user:u')",[&subject]).unwrap();
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant',?2,'user:u')",[&subject,role]).unwrap();
    control
        .execute("UPDATE apikey SET remaining=30 WHERE id=?1", [&credential])
        .unwrap();
    company.execute_batch("UPDATE companies SET address='Synthetic Street 1',cvr='87654321',vat_period_type='quarter' WHERE id=1;INSERT INTO accounts(account_no,name,type,normal_balance,default_vat_code)VALUES('E','Synthetic expense','expense','debit','DK_PURCHASE_25'),('V','Synthetic input VAT','vat','debit',NULL),('C','Synthetic creditor','liability','credit',NULL),('B','Synthetic bank','asset','debit',NULL);INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('creditors','C',1,'synthetic'),('input_vat','V',1,'synthetic');INSERT INTO bank_accounts(slug,name,currency,ledger_account_no)VALUES('synthetic','Synthetic bank','DKK','B');").unwrap();
    Fixture {
        _dir: dir,
        control,
        company,
        guard,
        key,
        subject,
        credential,
    }
}
impl Fixture {
    fn run(&mut self, action: Action<'_>) -> Result<ServiceResult<Value>, ServiceFailure> {
        execute_service(
            &mut self.control,
            &mut self.company,
            &mut self.guard,
            ServiceContext {
                key: &self.key,
                clock: &Fixed,
                selection: CompanySelection {
                    workspace: "w",
                    slug: "a",
                    registered: true,
                    archived: false,
                },
                max_future_days: 45,
            },
            action,
        )
    }
    fn count(&self, table: &str) -> i64 {
        self.company
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }
    fn remaining(&self) -> i64 {
        self.control
            .query_row(
                "SELECT remaining FROM apikey WHERE id=?1",
                [&self.credential],
                |r| r.get(0),
            )
            .unwrap()
    }
    fn original(&mut self) -> i64 {
        let file = rentemester_runtime::document::Prepared::new(
            "synthetic-purchase.txt".into(),
            b"Synthetic supplier original bytes".to_vec(),
        )
        .unwrap();
        let path = self.guard.publish_document(&file).unwrap();
        let metadata=serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","issueDate":"2026-01-01","invoiceNo":"SYNTHETIC-BILL-1","deliveryDescription":"Synthetic supplies","amountIncVat":125,"vatAmount":25,"sender":{"name":"Synthetic Supplier","address":"Synthetic Street 2","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic service seller","address":"Synthetic Street 1","vatOrCvr":"DK87654321"}})).unwrap();
        let tx = self
            .company
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let result = rentemester_storage::documents::register(
            &tx,
            rentemester_storage::documents::Original {
                filename: file.filename(),
                path: &path,
                hash: file.hash(),
                mime: file.mime(),
            },
            &metadata,
            "synthetic-human",
            "2026-01-01",
        )
        .unwrap();
        let csv = "date;text;amount;currency\n2026-01-01;Synthetic supplier payment;-125;DKK\n";
        rentemester_storage::bank::import(
            &tx,
            csv,
            &rentemester_engine::bank::parse(csv).unwrap(),
            Some("synthetic"),
            "synthetic-human",
            "2026-01-01T00:00:00.000Z",
            "synthetic-bank-original",
        )
        .unwrap();
        tx.commit().unwrap();
        result["document"]["id"].as_i64().unwrap()
    }
}
fn registration(document: i64) -> rentemester_storage::payables::Register {
    serde_json::from_value(json!({"documentId":document,"billDate":"2026-01-01","dueDate":"2026-01-15","expenseAccountNo":"E","confirm":true})).unwrap()
}
#[test]
fn service_payable_domain_flow_uses_current_fence_exact_original_and_attribution() {
    let mut f = fixture("bookkeeper");
    let document = f.original();
    let before = f.company.total_changes();
    f.run(Action::Options(document)).unwrap();
    assert_eq!(f.company.total_changes(), before);
    let registered = f
        .run(Action::Register(&registration(document)))
        .unwrap()
        .result;
    let id = registered["payableId"].as_i64().unwrap();
    assert_eq!(registered["grossAmount"], 125.0);
    assert_eq!(f.count("journal_entries"), 1);
    let before = f.company.total_changes();
    let listed = f
        .run(Action::Payables {
            as_of: "2026-01-01",
            status: "open",
        })
        .unwrap()
        .result;
    assert_eq!(listed["totalOpenBalance"], 125.0);
    assert_eq!(f.company.total_changes(), before);
    let bank = f
        .company
        .query_row("SELECT id FROM bank_transactions", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap();
    let payment = serde_json::from_value(json!({"bankTransactionId":bank,"confirm":true})).unwrap();
    let paid = f.run(Action::Pay(id, &payment)).unwrap().result;
    assert_eq!(paid["openBalance"], 0.0);
    assert_eq!(f.count("journal_entries"), 2);
    assert_eq!(f.count("payable_payments"), 1);
    let actor = format!("service_account:{}", f.subject);
    assert_eq!(f.company.query_row::<i64,_,_>("SELECT count(*) FROM audit_log WHERE event_type IN ('payable_register','payable_payment_apply') AND actor=?1",[actor],|r|r.get(0)).unwrap(),2);
    assert_eq!(f.company.query_row::<i64,_,_>("SELECT count(*) FROM journal_entries WHERE created_by=?1 AND created_by_program='service-principal'",[format!("user:{}",f.subject)],|r|r.get(0)).unwrap(),2);
    let failure = f.run(Action::Pay(id, &payment)).err().unwrap();
    assert_eq!(failure.business, BusinessStage::NotCommitted);
    assert_eq!(f.count("journal_entries"), 2);
}
#[test]
fn service_payable_reader_and_unrelated_action_refuse_before_quota_without_retiring_owner() {
    let mut f = fixture("reader");
    let document = f.original();
    let failure = f
        .run(Action::Register(&registration(document)))
        .err()
        .unwrap();
    assert_eq!(failure.error.code, "access_denied");
    assert_eq!(failure.usage, UsageStage::NotCommitted);
    assert!(failure.control_reusable && failure.company_reusable);
    assert_eq!(f.remaining(), 30);
    assert_eq!(f.count("journal_entries"), 0);
    let input: Input = serde_json::from_value(
        json!({"documentId":document,"bankTransactionId":1,"expenseAccountNo":"E"}),
    )
    .unwrap();
    let failure = f.run(Action::Post(&input)).err().unwrap();
    assert_eq!(failure.business, BusinessStage::NotAttempted);
    assert_eq!(failure.usage, UsageStage::NotCommitted);
    assert!(failure.control_reusable && failure.company_reusable);
    assert_eq!(f.remaining(), 30);
    f.run(Action::Payables {
        as_of: "2026-01-01",
        status: "all",
    })
    .unwrap();
}
#[test]
fn service_payable_changed_original_or_revoked_key_never_creates_journal() {
    let mut f = fixture("bookkeeper");
    let document = f.original();
    let path: String = f
        .company
        .query_row(
            "SELECT stored_path FROM documents WHERE id=?1",
            [document],
            |r| r.get(0),
        )
        .unwrap();
    std::fs::write(
        f._dir.path().join("workspace/a").join(path),
        b"Changed synthetic original",
    )
    .unwrap();
    let failure = f
        .run(Action::Register(&registration(document)))
        .err()
        .unwrap();
    assert_eq!(failure.error.code, "document_evidence_unavailable");
    assert_eq!(failure.usage, UsageStage::Committed);
    assert_eq!(f.count("journal_entries"), 0);
    assert_eq!(f.count("payables"), 0);
    f.control
        .execute("UPDATE apikey SET enabled=0 WHERE id=?1", [&f.credential])
        .unwrap();
    let remaining = f.remaining();
    let failure = f
        .run(Action::Register(&registration(document)))
        .err()
        .unwrap();
    assert_eq!(failure.error.code, "invalid_credentials");
    assert_eq!(failure.usage, UsageStage::NotCommitted);
    assert_eq!(f.remaining(), remaining);
}
