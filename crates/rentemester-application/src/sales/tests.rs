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
    company.execute_batch("UPDATE companies SET address='Synthetic Street 1',cvr='87654321',vat_period_type='quarter' WHERE id=1;INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('R','Synthetic receivable','asset','debit'),('S','Synthetic revenue','income','credit'),('V','Synthetic VAT','vat','credit');INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('debtors','R',1,'synthetic'),('output_vat','V',1,'synthetic');").unwrap();
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
    fn remaining(&self) -> i64 {
        self.control
            .query_row(
                "SELECT remaining FROM apikey WHERE id=?1",
                [&self.credential],
                |r| r.get(0),
            )
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
fn input(customer: i64) -> Input {
    serde_json::from_value(json!({"issueDate":"2026-01-01","customerId":customer,"lines":[{"description":"Synthetic service work","quantity":1,"unitPriceExVat":100}]})).unwrap()
}
fn customer() -> Customer {
    serde_json::from_value(
        json!({"name":"Synthetic buyer","address":"Synthetic Street 2","vatOrCvr":"12345678"}),
    )
    .unwrap()
}
#[test]
fn service_sales_complete_flow_preserves_pdf_money_and_stable_attribution() {
    let mut f = fixture("bookkeeper");
    let initial = json_output(f.run(Action::Contacts).unwrap());
    assert!(initial["customers"].as_array().unwrap().is_empty());
    let created = json_output(f.run(Action::Customers(&customer())).unwrap());
    let customer_id = created["id"].as_i64().unwrap();
    let source = input(customer_id);
    let before = f.company.total_changes();
    let preview = f.run(Action::Preview(&source)).unwrap();
    let Output::Pdf(preview) = preview.result else {
        panic!("expected PDF")
    };
    assert!(preview.starts_with(b"%PDF-"));
    assert_eq!(f.company.total_changes(), before);
    assert_eq!(f.count("documents"), 0);
    assert_eq!(f.count("journal_entries"), 0);
    let issued = json_output(f.run(Action::Issue(&source)).unwrap());
    let id = issued["documentId"].as_i64().unwrap();
    assert_eq!(issued["grossAmount"], 125.0);
    assert_eq!(f.count("journal_entries"), 0);
    let post = Post {
        invoice_document_id: id,
        revenue_account_no: "S".into(),
        confirm: true,
    };
    let posted = json_output(f.run(Action::Post(&post)).unwrap());
    assert_eq!(posted["ok"], true);
    let list = json_output(f.run(Action::List).unwrap());
    assert_eq!(list["invoices"][0]["posted"], true);
    assert_eq!(list["totalOpenDkk"], 125.0);
    let before = f.company.total_changes();
    let pdf = f.run(Action::Pdf(id)).unwrap();
    let Output::Pdf(pdf) = pdf.result else {
        panic!("expected PDF")
    };
    assert_eq!(pdf, preview);
    assert_eq!(f.company.total_changes(), before);
    assert_eq!(f.count("journal_entries"), 1);
    assert_eq!(f.remaining(), 23);
    let audit_actor = format!("service_account:{}", f.subject);
    assert_eq!(f.company.query_row::<i64,_,_>("SELECT count(*) FROM audit_log WHERE event_type IN ('customer_add','invoice_issue','invoice_render_pdf','invoice_post') AND actor=?1",[&audit_actor],|r|r.get(0)).unwrap(),4);
    let entry: (String, String) = f
        .company
        .query_row(
            "SELECT created_by,created_by_program FROM journal_entries",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        entry,
        (format!("user:{}", f.subject), "service-principal".into())
    );
}
#[test]
fn service_sales_unsupported_and_insufficient_role_refuse_before_usage_or_data() {
    let mut f = fixture("reader");
    let failure = f.run(Action::DeleteCustomer(1, true)).err().unwrap();
    assert_eq!(failure.error.code, "access_denied");
    assert_eq!(failure.usage, UsageStage::NotCommitted);
    assert_eq!(failure.business, BusinessStage::NotAttempted);
    assert_eq!(f.remaining(), 30);
    let failure = f.run(Action::Customers(&customer())).err().unwrap();
    assert_eq!(failure.error.code, "access_denied");
    assert_eq!(f.remaining(), 30);
    assert_eq!(f.count("customers"), 0);
    let source:Input=serde_json::from_value(json!({"issueDate":"2026-01-01","buyer":{"name":"Synthetic buyer","address":"Synthetic 2"},"lines":[{"description":"Synthetic","quantity":1,"unitPriceExVat":100}]})).unwrap();
    let failure = f.run(Action::Preview(&source)).err().unwrap();
    assert_eq!(failure.error.code, "access_denied");
    assert_eq!(f.remaining(), 30);
    assert_eq!(f.count("documents"), 0);
}
#[test]
fn service_sales_invalid_source_and_revoked_credential_never_create_business_effects() {
    let mut f = fixture("bookkeeper");
    let invalid: Customer = serde_json::from_value(json!({"name":" "})).unwrap();
    let before = f.company.total_changes();
    let failure = f.run(Action::Customers(&invalid)).err().unwrap();
    assert_eq!(failure.usage, UsageStage::Committed);
    assert_eq!(failure.business, BusinessStage::NotCommitted);
    assert_eq!(f.count("customers"), 0);
    assert_eq!(f.company.total_changes(), before);
    f.control
        .execute("UPDATE apikey SET enabled=0 WHERE id=?1", [&f.credential])
        .unwrap();
    let before = f.remaining();
    let failure = f.run(Action::Contacts).err().unwrap();
    assert_eq!(failure.error.code, "invalid_credentials");
    assert_eq!(failure.usage, UsageStage::NotCommitted);
    assert_eq!(f.remaining(), before);
    assert_eq!(f.count("documents"), 0);
}
#[test]
fn service_sales_pdf_and_post_refuse_changed_original_without_journal_effects() {
    let mut f = fixture("bookkeeper");
    let created = json_output(f.run(Action::Customers(&customer())).unwrap());
    let source = input(created["id"].as_i64().unwrap());
    let issued = json_output(f.run(Action::Issue(&source)).unwrap());
    let id = issued["documentId"].as_i64().unwrap();
    let path: String = f
        .company
        .query_row(
            "SELECT stored_path FROM documents WHERE document_type='issued_invoice_pdf'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    std::fs::write(
        f._dir.path().join("workspace/a").join(path),
        b"%PDF-synthetic-corrupt",
    )
    .unwrap();
    let failure = f.run(Action::Pdf(id)).err().unwrap();
    assert_eq!(failure.error.code, "invoice_evidence_unavailable");
    assert_eq!(failure.usage, UsageStage::Committed);
    let failure = f
        .run(Action::Post(&Post {
            invoice_document_id: id,
            revenue_account_no: "S".into(),
            confirm: true,
        }))
        .err()
        .unwrap();
    assert_eq!(failure.error.code, "invoice_evidence_unavailable");
    assert_eq!(f.count("journal_entries"), 0);
    assert_eq!(f.count("issued_invoice_postings"), 0);
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
    fn posted_invoice(&mut self) -> i64 {
        let buyer = json_output(self.run(Action::Customers(&customer())).unwrap());
        let source = input(buyer["id"].as_i64().unwrap());
        let issued = json_output(self.run(Action::Issue(&source)).unwrap());
        let id = issued["documentId"].as_i64().unwrap();
        self.run(Action::Post(&Post {
            invoice_document_id: id,
            revenue_account_no: "S".into(),
            confirm: true,
        }))
        .unwrap();
        id
    }
}
fn settlement(invoice: i64, bank: i64) -> rentemester_storage::invoice_settlement::Input {
    serde_json::from_value(
        json!({"invoiceDocumentId":invoice,"bankTransactionId":bank,"confirm":true}),
    )
    .unwrap()
}
#[test]
fn service_bank_bytes_and_payment_preserve_source_dedupe_and_historical_receivable() {
    let mut f = fixture("bookkeeper");
    f.setup_bank();
    let invoice = f.posted_invoice();
    f.company.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('R2','Synthetic current receivable','asset','debit');UPDATE account_role_mappings SET status='superseded' WHERE role='debtors';INSERT INTO account_role_mappings(role,account_no,version,confirmed_by)VALUES('debtors','R2',2,'synthetic');").unwrap();
    const BYTES: &[u8] =
        b"Dato;Tekst;Bel\xf8b;Valuta;Saldo\r\n01.01.2026;Synthetic r\xe5d;125,00;DKK;125,00\r\n";
    let imported = f.import_bank(BYTES, Some("danske-bank")).unwrap().result;
    assert_eq!(imported["imported"], 1);
    assert_eq!(f.count("journal_entries"), 1);
    assert_eq!(
        f.guard
            .read_bank_statement(imported["sourceFileHash"].as_str().unwrap())
            .unwrap(),
        BYTES
    );
    let repeated = f.import_bank(BYTES, Some("danske-bank")).unwrap().result;
    assert_eq!(repeated["imported"], 0);
    assert_eq!(repeated["skippedDuplicates"], 1);
    assert_eq!(f.count("bank_transactions"), 1);
    let before = f.company.total_changes();
    let accounts = f.bank(crate::bank::Action::Accounts).unwrap().result;
    assert_eq!(accounts["accounts"][0]["slug"], "synthetic");
    f.bank(crate::bank::Action::Read {
        today: "2026-01-01",
        year: Some("2026"),
    })
    .unwrap();
    assert_eq!(f.company.total_changes(), before);
    let bank = f
        .company
        .query_row("SELECT id FROM bank_transactions", [], |r| r.get(0))
        .unwrap();
    let payment = json_output(f.run(Action::Settle(&settlement(invoice, bank))).unwrap());
    assert_eq!(payment["openBalance"], 0.0);
    assert_eq!(payment["principalAmount"], 125.0);
    assert_eq!(f.count("journal_entries"), 2);
    assert_eq!(f.count("invoice_payments"), 1);
    let matched:bool=f.company.query_row("SELECT EXISTS(SELECT 1 FROM bank_journal_reconciliations WHERE bank_transaction_id=?1)",[bank],|r|r.get(0)).unwrap();
    assert!(matched);
    let latest = payment["entryId"].as_i64().unwrap();
    let credited:String=f.company.query_row("SELECT a.account_no FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=?1 AND l.credit_amount=125",[latest],|r|r.get(0)).unwrap();
    assert_eq!(credited, "R");
    let actor: (String, String) = f
        .company
        .query_row(
            "SELECT created_by,created_by_program FROM journal_entries WHERE id=?1",
            [latest],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        actor,
        (format!("user:{}", f.subject), "service-principal".into())
    );
    let audit = format!("service_account:{}", f.subject);
    assert_eq!(f.company.query_row::<i64,_,_>("SELECT count(*) FROM audit_log WHERE event_type='invoice_payment_apply' AND actor=?1",[&audit],|r|r.get(0)).unwrap(),1);
    assert_eq!(f.company.query_row::<i64,_,_>("SELECT count(*) FROM audit_log WHERE event_type IN ('bank_import','bank_import_profile') AND actor=?1",[&audit],|r|r.get(0)).unwrap(),3);
    let failure = f
        .run(Action::Settle(&settlement(invoice, bank)))
        .err()
        .unwrap();
    assert_eq!(failure.business, BusinessStage::NotCommitted);
    assert_eq!(f.count("journal_entries"), 2);
    assert_eq!(f.count("invoice_payments"), 1);
    let listed = json_output(f.run(Action::List).unwrap());
    assert_eq!(listed["totalOpenDkk"], 0.0);
}
#[test]
fn service_bank_admin_reader_malformed_and_revoked_requests_fail_closed() {
    let mut f = fixture("reader");
    f.setup_bank();
    let account: crate::bank::AccountInput =
        serde_json::from_value(json!({"name":"Unapproved Synthetic"})).unwrap();
    let failure = f.bank(crate::bank::Action::Create(&account)).err().unwrap();
    assert_eq!(failure.error.code, "access_denied");
    assert!(failure.control_reusable && failure.company_reusable);
    assert_eq!(failure.usage, UsageStage::NotCommitted);
    assert_eq!(f.remaining(), 30);
    assert_eq!(f.count("bank_accounts"), 1);
    let failure = f
        .import_bank(b"date;text;amount\n2026-01-01;Synthetic;125\n", None)
        .err()
        .unwrap();
    assert_eq!(failure.error.code, "access_denied");
    assert_eq!(f.remaining(), 30);
    assert_eq!(f.count("bank_transactions"), 0);
    f.control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','bookkeeper','user:u')",[&f.subject]).unwrap();
    let failure = f.import_bank(b"invalid\xff", None).err().unwrap();
    assert_eq!(failure.usage, UsageStage::Committed);
    assert_eq!(f.count("bank_transactions"), 0);
    assert_eq!(f.count("journal_entries"), 0);
    f.control
        .execute("UPDATE apikey SET enabled=0 WHERE id=?1", [&f.credential])
        .unwrap();
    let remaining = f.remaining();
    let failure = f.bank(crate::bank::Action::Accounts).err().unwrap();
    assert_eq!(failure.error.code, "invalid_credentials");
    assert_eq!(f.remaining(), remaining);
}
#[test]
fn service_payment_rejects_wrong_sign_currency_and_amount_without_posting() {
    let mut f = fixture("bookkeeper");
    f.setup_bank();
    let invoice = f.posted_invoice();
    f.import_bank(b"date;text;amount;currency;amount_dkk;fx_rate_to_dkk\n2026-01-01;Synthetic outgoing;-125;DKK;;\n2026-01-01;Synthetic foreign;125;EUR;125;1\n2026-01-01;Synthetic excess;126;DKK;;\n",None).unwrap();
    let ids: Vec<i64> = f
        .company
        .prepare("SELECT id FROM bank_transactions ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(ids.len(), 3);
    for bank in ids {
        let failure = f
            .run(Action::Settle(&settlement(invoice, bank)))
            .err()
            .unwrap();
        assert_eq!(failure.usage, UsageStage::Committed);
        assert_eq!(failure.business, BusinessStage::NotCommitted);
        assert_eq!(f.count("invoice_payments"), 0);
        assert_eq!(f.count("journal_entries"), 1);
    }
    assert_eq!(
        f.company
            .query_row::<i64, _, _>(
                "SELECT count(*) FROM bank_journal_reconciliations",
                [],
                |r| r.get(0)
            )
            .unwrap(),
        0
    );
}
