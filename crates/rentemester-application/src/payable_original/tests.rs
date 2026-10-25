#![cfg(target_os = "macos")]
use super::*;
use crate::expense::{Action as ExpenseAction, execute_service};
use crate::{
    control_fence::CompanySelection,
    service_company::{Context as ServiceContext, ServiceFailure, ServiceResult},
};
use rentemester_runtime::{
    auth::{AuthError, Clock, CookieProfile, CookieVerifier},
    namespace::NamespaceProfile,
    service_key::ServiceKey,
};
use rentemester_storage::payable_original as storage;
use rusqlite::Connection;
use serde_json::json;
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1767308400000)
    }
}
struct Fixture {
    _dir: tempfile::TempDir,
    control: Connection,
    company: Connection,
    guard: NamespaceGuard,
    key: ServiceKey,
    subject: String,
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
    control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T23:00:00.000Z','2026-01-01T23:00:00.000Z','2026-01-02T11:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u');").unwrap();
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
    }
}
impl Fixture {
    fn run(&mut self, action: ExpenseAction<'_>) -> Result<ServiceResult<Value>, ServiceFailure> {
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
        let archive = self
            .guard
            .publish_bank_statement_bytes(csv.as_bytes())
            .unwrap();
        rentemester_storage::bank::import(
            &tx,
            csv,
            &rentemester_engine::bank::parse(csv).unwrap(),
            Some("synthetic"),
            "synthetic-human",
            "2026-01-01T00:00:00.000Z",
            &archive,
        )
        .unwrap();
        tx.commit().unwrap();
        result["document"]["id"].as_i64().unwrap()
    }
}
#[test]
fn original_payable_service_current_sources_receipts_and_revocation() {
    let mut f = fixture("bookkeeper");
    let document = f.original();
    let register:storage::Register=serde_json::from_value(json!({"documentId":document,"billDate":"2026-01-01","dueDate":"2026-01-15","expenseAccount":"E","confirm":true,"idempotencyKey":"synthetic-register"})).unwrap();
    let first = f
        .run(ExpenseAction::Original(Action::Register(&register)))
        .unwrap()
        .result;
    assert_eq!(first["data"]["payableId"], 1);
    assert_eq!(
        f.run(ExpenseAction::Original(Action::Register(&register)))
            .unwrap()
            .result["data"]["idempotency"]["replayed"],
        true
    );
    let query = storage::Query::default();
    let listed = f
        .run(ExpenseAction::Original(Action::List(&query)))
        .unwrap()
        .result;
    assert_eq!(listed["data"]["asOfDate"], "2026-01-01");
    assert_eq!(listed["data"]["totalOpenBalance"], 125.0);
    assert!(listed["data"].get("company").is_none());
    let pay:storage::Pay=serde_json::from_value(json!({"payableId":1,"bankTransactionId":1,"confirm":true,"idempotencyKey":"synthetic-pay"})).unwrap();
    assert_eq!(
        f.run(ExpenseAction::Original(Action::Pay(&pay)))
            .unwrap()
            .result["data"]["openBalance"],
        0.0
    );
    assert_eq!(
        f.run(ExpenseAction::Original(Action::Pay(&pay)))
            .unwrap()
            .result["data"]["idempotency"]["replayed"],
        true
    );
    assert_eq!(f.count("journal_entries"), 2);
    assert_eq!(f.count("payable_payments"), 1);
    let actor = format!("service_account:{}", f.subject);
    assert_eq!(
        f.company
            .query_row::<i64, _, _>(
                "SELECT count(*) FROM journal_entries WHERE created_by=?1",
                [&actor],
                |r| r.get(0)
            )
            .unwrap(),
        2
    );
    let receipt_actor = format!("{actor} via rentemester-original-payable");
    assert_eq!(f.company.query_row::<i64,_,_>("SELECT count(*) FROM audit_log WHERE entity_type='idempotency_receipt' AND actor=?1",[&receipt_actor],|r|r.get(0)).unwrap(),4);

    let root = f._dir.path().canonicalize().unwrap().join("workspace/a");
    let hash: String = f
        .company
        .query_row(
            "SELECT source_file_hash FROM bank_transactions WHERE id=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let bank = root.join("bank/processed").join(format!("{hash}.txt"));
    let bytes = std::fs::read(&bank).unwrap();
    std::fs::write(&bank, b"changed source").unwrap();
    assert_eq!(
        f.run(ExpenseAction::Original(Action::Pay(&pay)))
            .err()
            .unwrap()
            .error
            .code,
        "bank_statement_original_unavailable"
    );
    std::fs::write(bank, bytes).unwrap();
    f.control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES(?1,'a','revoke','user:u')",[&f.subject]).unwrap();
    assert_eq!(
        f.run(ExpenseAction::Original(Action::Register(&register)))
            .err()
            .unwrap()
            .error
            .code,
        "access_denied"
    );
    f.control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','bookkeeper','user:u')",[&f.subject]).unwrap();
    let path: String = f
        .company
        .query_row(
            "SELECT stored_path FROM documents WHERE id=?1",
            [document],
            |r| r.get(0),
        )
        .unwrap();
    std::fs::write(root.join(path), b"changed document").unwrap();
    assert_eq!(
        f.run(ExpenseAction::Original(Action::Register(&register)))
            .err()
            .unwrap()
            .error
            .code,
        "document_evidence_unavailable"
    );
}
