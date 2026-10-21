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
#[test]
fn original_invoice_current_human_service_unposted_source_and_readonly() {
    let mut f = fixture("bookkeeper");
    let source=serde_json::from_value(serde_json::json!({"issueDate":"2026-01-01","buyer":{"name":"Synthetic Buyer","address":"Synthetic2","vatOrCvr":"DK12345678"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100}]})).unwrap();
    let issued = json_output(f.run(Action::Issue(&source)).unwrap());
    let id = issued["documentId"].as_i64().unwrap();
    let query = Query::default();
    let changes = f.company.total_changes();
    let read = json_output(f.run(Action::Original(Kind::List, &query)).unwrap());
    assert_eq!(read["data"]["asOfDate"], "2026-01-02");
    assert_eq!(read["data"]["count"], 1);
    assert_eq!(read["data"]["rows"][0]["status"], "open");
    assert_eq!(f.count("journal_entries"), 0);
    assert_eq!(f.company.total_changes(), changes);
    let q = Query {
        document_id: Some(id),
        ..Query::default()
    };
    assert_eq!(
        json_output(f.run(Action::Original(Kind::Status, &q)).unwrap())["data"]["payments"],
        serde_json::json!([])
    );
    f.run(Action::Post(&rentemester_storage::sales::Post {
        invoice_document_id: id,
        revenue_account_no: "S".into(),
        confirm: true,
    }))
    .unwrap();
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
            action: Action::Original(Kind::Status, &q),
            today: "",
            now: "",
            year: None,
        },
    )
    .unwrap()
    .value;
    let Output::Json(human) = human else { panic!() };
    assert_eq!(
        human,
        json_output(f.run(Action::Original(Kind::Status, &q)).unwrap())
    );
    f.control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES(?1,'a','revoke','user:u')",[&f.subject]).unwrap();
    assert_eq!(
        f.run(Action::Original(Kind::List, &query))
            .err()
            .unwrap()
            .error
            .code,
        "access_denied"
    );
    let path: String = f
        .company
        .query_row("SELECT stored_path FROM documents WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .unwrap();
    std::fs::write(
        f._dir
            .path()
            .canonicalize()
            .unwrap()
            .join("workspace/a")
            .join(path),
        b"changed source",
    )
    .unwrap();
    let result = execute(
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
            action: Action::Original(Kind::Status, &q),
            today: "",
            now: "",
            year: None,
        },
    );
    assert!(result.is_err());
}
