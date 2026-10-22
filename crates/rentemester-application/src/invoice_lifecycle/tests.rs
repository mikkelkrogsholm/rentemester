#![cfg(target_os = "macos")]
use super::*;
use crate::sales::{Action, Output, Request, execute, execute_service};
use crate::{
    control_fence::CompanySelection,
    service_company::{Context as ServiceContext, ServiceFailure, ServiceResult},
};
use rentemester_runtime::{
    auth::{AuthError, Clock, CookieProfile, CookieVerifier},
    namespace::NamespaceProfile,
    service_key::ServiceKey,
};
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
    control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u')").unwrap();
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
    company.execute_batch("UPDATE companies SET address='Synthetic Street 1',cvr='87654321',vat_period_type='quarter' WHERE id=1;INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('R','Synthetic receivable','asset','debit'),('S','Synthetic revenue','income','credit'),('V','Synthetic VAT','vat','credit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('debtors','R',1,'synthetic'),('output_vat','V',1,'synthetic');").unwrap();
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
    fn run(&mut self, action: Action<'_>) -> Result<ServiceResult<Output>, ServiceFailure> {
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
            Request {
                action,
                today: "2026-01-01",
                now: "2026-01-01T00:00:00.000Z",
                year: Some("2026"),
            },
        )
    }
    fn count(&self, table: &str) -> i64 {
        self.company
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }
}
fn json_output(result: ServiceResult<Output>) -> Value {
    assert!(result.receipt.is_none());
    match result.result {
        Output::Json(value) => value,
        _ => panic!("expected JSON"),
    }
}
impl Fixture {
    fn bank(
        &mut self,
        action: crate::bank::Action<'_>,
    ) -> Result<ServiceResult<Value>, ServiceFailure> {
        crate::bank::execute_service(
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
    fn setup_bank(&mut self) {
        self.company.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('B','Synthetic bank','asset','debit');INSERT INTO bank_accounts(slug,name,currency,ledger_account_no)VALUES('synthetic','Synthetic Bank','DKK','B');").unwrap();
    }
    fn import_bank(
        &mut self,
        bytes: &[u8],
        profile: Option<&str>,
    ) -> Result<ServiceResult<Value>, ServiceFailure> {
        self.bank(crate::bank::Action::ImportBytes {
            content: bytes,
            account: Some("synthetic"),
            profile,
            now: "2026-01-01T00:00:00.000Z",
        })
    }
}
#[test]
fn original_service_issue_render_post_settle_credit_refund_current_authority_and_sources() {
    let mut f = fixture("bookkeeper");
    f.setup_bank();
    f.import_bank(b"date;text;amount;currency;reference\n2026-01-01;Synthetic receipt;125;DKK;paid\n2026-01-01;Synthetic refund;-25;DKK;refund\n",None).unwrap();
    let issue = json!({"confirm":true,"payload":{"invoiceType":"full","issueDate":"2026-01-01","buyer":{"name":"Synthetic buyer","address":"Synthetic street2"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100,"lineTotalExVat":100}],"totals":{"netAmount":100,"vatRate":0.25,"vatAmount":25,"grossAmount":125}}});
    let issued = json_output(f.run(Action::Lifecycle(Kind::Issue, &issue)).unwrap());
    assert_eq!(issued["ok"], true);
    let id = issued["data"]["documentId"].as_i64().unwrap();
    let number = issued["data"]["invoiceNumber"].clone();
    assert!(
        issued["data"]["storedPath"]
            .as_str()
            .unwrap()
            .starts_with("/api/companies/a/")
    );
    let render = json!({"confirm":true,"documentId":id,"invoiceNumber":"ignored"});
    let before = f.company.total_changes();
    let rendered = json_output(f.run(Action::Lifecycle(Kind::Render, &render)).unwrap());
    assert_eq!(rendered["data"]["sha256"], issued["data"]["pdfSha256"]);
    assert_eq!(before, f.company.total_changes());
    let missing = json!({"confirm":true,"documentId":id});
    assert_eq!(
        json_output(f.run(Action::Lifecycle(Kind::Post, &missing)).unwrap())["ok"],
        false
    );
    assert_eq!(f.count("journal_entries"), 0);
    let post = json!({"confirm":true,"invoiceNumber":number,"revenueAccountNo":"S"});
    assert_eq!(
        json_output(f.run(Action::Lifecycle(Kind::Post, &post)).unwrap())["ok"],
        true
    );
    let settle = json!({"confirm":true,"payload":{"invoiceNumber":number,"bankTransactionReference":"paid","bankAccountNo":"B","receivableAccountNo":"R"}});
    let payment = json_output(f.run(Action::Lifecycle(Kind::SettleBank, &settle)).unwrap());
    assert_eq!(payment["data"]["openBalance"], 0.0);
    assert!(payment["data"]["entryNo"].is_string());
    let credit = json!({"confirm":true,"payload":{"originalInvoiceNumber":number,"issueDate":"2026-01-01","reason":"Synthetic correction","grossAmount":25}});
    let credit = json_output(f.run(Action::Lifecycle(Kind::CreditNote, &credit)).unwrap());
    assert!(credit["data"]["journalEntryId"].is_i64());
    let refund = json!({"confirm":true,"payload":{"invoiceDocumentId":id,"bankTransactionReference":"refund","bankAccountNo":"B","receivableAccountNo":"R"}});
    let refund = json_output(f.run(Action::Lifecycle(Kind::RefundBank, &refund)).unwrap());
    assert_eq!(refund["data"]["remainingCreditBalance"], 0.0);
    assert_eq!(f.count("journal_entries"), 4);
    let actor = format!("service_account:{}", f.subject);
    assert_eq!(f.company.query_row::<i64,_,_>("SELECT count(*) FROM audit_log WHERE actor=?1 AND event_type IN ('invoice_issue','invoice_render_pdf','invoice_post','invoice_payment_apply','credit_note_issue','invoice_refund_apply')",[actor],|r|r.get(0)).unwrap(),6);
    assert_eq!(f.company.query_row::<i64,_,_>("SELECT count(*) FROM journal_entries WHERE created_by=?1 AND created_by_program='service-principal'",[format!("user:{}",f.subject)],|r|r.get(0)).unwrap(),4);
    let cookies = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let issued_cookie = cookies.issue_session("synthetic-token").unwrap();
    let cookie = cookies
        .verify(Some(issued_cookie.request_header()), None)
        .unwrap();
    let human = execute(
        &mut f.control,
        &mut f.company,
        &mut f.guard,
        crate::documents::Context {
            cookie: &cookie,
            clock: &Fixed,
            selection: CompanySelection {
                workspace: "w",
                slug: "a",
                registered: true,
                archived: false,
            },
        },
        Request {
            action: Action::Lifecycle(Kind::Render, &render),
            today: "",
            now: "",
            year: None,
        },
    )
    .unwrap()
    .value;
    let Output::Json(human) = human else { panic!() };
    assert_eq!(human, rendered);
    f.control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES(?1,'a','revoke','user:u')",[&f.subject]).unwrap();
    assert_eq!(
        f.run(Action::Lifecycle(Kind::Render, &render))
            .err()
            .unwrap()
            .error
            .code,
        "access_denied"
    );
    let path: String = f
        .company
        .query_row(
            "SELECT stored_path FROM documents WHERE id=?1",
            [issued["data"]["pdfDocumentId"].as_i64().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    std::fs::write(f._dir.path().join("workspace/a").join(path), b"changed PDF").unwrap();
    assert!(
        execute(
            &mut f.control,
            &mut f.company,
            &mut f.guard,
            crate::documents::Context {
                cookie: &cookie,
                clock: &Fixed,
                selection: CompanySelection {
                    workspace: "w",
                    slug: "a",
                    registered: true,
                    archived: false
                }
            },
            Request {
                action: Action::Lifecycle(Kind::Render, &render),
                today: "",
                now: "",
                year: None
            }
        )
        .is_err()
    );
    assert_eq!(f.count("journal_entries"), 4);
}
#[test]
fn original_apply_payment_service_bankless_and_candidate_keep_sources_and_current_rights() {
    let mut f = fixture("bookkeeper");
    f.company.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('B','Synthetic bank','asset','debit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('bank','B',1,'synthetic')").unwrap();
    let issue = json!({"confirm":true,"payload":{"invoiceType":"full","issueDate":"2026-01-01","buyer":{"name":"Synthetic buyer","address":"Synthetic street2"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100,"lineTotalExVat":100}],"totals":{"netAmount":100,"vatRate":0.25,"vatAmount":25,"grossAmount":125}}});
    let issued = json_output(f.run(Action::Lifecycle(Kind::Issue, &issue)).unwrap());
    let id = issued["data"]["documentId"].as_i64().unwrap();
    let post = json!({"confirm":true,"documentId":id,"revenueAccountNo":"S"});
    assert_eq!(
        json_output(f.run(Action::Lifecycle(Kind::Post, &post)).unwrap())["ok"],
        true
    );
    let pay = json!({"confirm":true,"payload":{"invoiceDocumentId":id,"paymentDate":"2026-01-01","amount":50,"note":"Synthetic partial receipt"}});
    let first = json_output(f.run(Action::Lifecycle(Kind::ApplyPayment, &pay)).unwrap());
    assert_eq!(first["data"]["openBalance"], 75.0);
    assert_eq!(
        first["appliedRules"],
        json!([
            "DK-INVOICE-PAYMENT-001",
            "DK-INVOICE-CORRECTION-BALANCE-001"
        ])
    );
    assert!(first["data"].get("appliedRules").is_none());
    assert!(first["data"]["journalEntryId"].is_i64());
    assert_eq!(f.count("journal_entries"), 2);
    assert_eq!(f.count("bank_transactions"), 0);
    let candidate = {
        let tx = f
            .company
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let request=serde_json::from_value(json!({"transactionDate":"2026-01-01","text":"Synthetic posted receipt to classify","currency":"DKK","documentId":id.to_string(),"lines":[{"accountNo":"B","debitAmount":"75.00"},{"accountNo":"R","creditAmount":"75.00"}]})).unwrap();
        let r = rentemester_storage::journal_post::post(
            &tx,
            &request,
            &rentemester_storage::journal_preview::manual_journal_rule_version().unwrap(),
            Date::parse("2026-01-02").unwrap(),
            rentemester_storage::journal_post::Actor {
                subject: "user:u",
                program: "rentemester-rust",
            },
        )
        .unwrap();
        tx.commit().unwrap();
        r["entryId"].as_i64().unwrap()
    };
    let classify = json!({"confirm":true,"payload":{"invoiceDocumentId":id,"journalEntryId":candidate,"paymentDate":"2026-01-01","amount":75}});
    let second = json_output(
        f.run(Action::Lifecycle(Kind::ApplyPayment, &classify))
            .unwrap(),
    );
    assert_eq!(second["data"]["journalEntryId"], candidate);
    assert_eq!(second["data"]["openBalance"], 0.0);
    assert_eq!(f.count("journal_entries"), 3);
    assert_eq!(f.count("invoice_payments"), 2);
    assert_eq!(f.count("bank_transactions"), 0);
    let tx = f.company.transaction().unwrap();
    let audit = rentemester_storage::audit_invoice::verify(&tx).unwrap();
    assert_eq!(audit["ok"], true, "{audit}");
    tx.rollback().unwrap();
    assert_eq!(f.company.query_row::<i64,_,_>("SELECT count(*) FROM audit_log WHERE event_type='invoice_payment_apply' AND actor=?1",[format!("service_account:{}",f.subject)],|r|r.get(0)).unwrap(),2);
    f.control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES(?1,'a','revoke','user:u')",[&f.subject]).unwrap();
    assert_eq!(
        f.run(Action::Lifecycle(Kind::ApplyPayment, &classify))
            .err()
            .unwrap()
            .error
            .code,
        "access_denied"
    );
    let path: String = f
        .company
        .query_row(
            "SELECT stored_path FROM documents WHERE id=?1",
            [issued["data"]["pdfDocumentId"].as_i64().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    std::fs::write(f._dir.path().join("workspace/a").join(path), b"changed PDF").unwrap();
    let cookies = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let issued_cookie = cookies.issue_session("synthetic-token").unwrap();
    let cookie = cookies
        .verify(Some(issued_cookie.request_header()), None)
        .unwrap();
    let failure = execute(
        &mut f.control,
        &mut f.company,
        &mut f.guard,
        crate::documents::Context {
            cookie: &cookie,
            clock: &Fixed,
            selection: CompanySelection {
                workspace: "w",
                slug: "a",
                registered: true,
                archived: false,
            },
        },
        Request {
            action: Action::Lifecycle(Kind::ApplyPayment, &classify),
            today: "",
            now: "",
            year: None,
        },
    )
    .err()
    .unwrap();
    assert_eq!(failure.error.code, "invoice_evidence_unavailable");
    assert_eq!(f.count("invoice_payments"), 2);
    assert_eq!(f.count("journal_entries"), 3);
}
