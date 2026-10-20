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
    invoice: i64,
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
    let mut guard = NamespaceGuard::create_fresh_with_profile(
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
    let invoice = {
        let tx = company
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .unwrap();
        let input = serde_json::from_value(json!({"issueDate":"2026-01-01","buyer":{"name":"Synthetic buyer","address":"Synthetic Street 2"},"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100}]})).unwrap();
        let payload =
            rentemester_storage::sales::prepare(&tx, &input, "2026-01-01T00:00:00.000Z", true)
                .unwrap();
        let id = crate::sales::issue_payload(&tx, &mut guard, &payload, "u").unwrap()["documentId"]
            .as_i64()
            .unwrap();
        tx.commit().unwrap();
        id
    };
    Fixture {
        _dir: dir,
        control,
        company,
        guard,
        key,
        subject,
        credential,
        invoice,
    }
}
impl Fixture {
    fn remaining(&self) -> i64 {
        self.control
            .query_row(
                "SELECT remaining FROM apikey WHERE id=?1",
                [&self.credential],
                |r| r.get(0),
            )
            .unwrap()
    }
    fn send(
        &mut self,
        to: &str,
        transport: &mut Fake,
    ) -> Result<ServiceResult<Value>, ServiceFailure> {
        let input = Send {
            invoice_document_id: self.invoice,
            to: to.into(),
            kind: None,
            confirm: true,
        };
        send_service(
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
            &input,
            transport,
        )
    }
    fn retry(
        &mut self,
        row: &Value,
        transport: &mut Fake,
    ) -> Result<ServiceResult<Value>, ServiceFailure> {
        let input = Retry {
            message_id: row["messageId"].as_str().unwrap().into(),
            expected_state_hash: row["stateHash"].as_str().unwrap().into(),
            reason: "Reviewed definite refusal with SMTP operator".into(),
            confirm: true,
        };
        retry_service(
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
            &input,
            transport,
        )
    }
    fn reconcile(&mut self, row: &Value) -> Result<ServiceResult<Value>, ServiceFailure> {
        let input = Reconcile {
            message_id: row["messageId"].as_str().unwrap().into(),
            expected_state_hash: row["stateHash"].as_str().unwrap().into(),
            resolution: "not_accepted".into(),
            reason: "Reviewed relay evidence".into(),
            evidence: "Synthetic SMTP operator confirms no acceptance".into(),
            confirm: true,
        };
        reconcile_service(
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
            &input,
        )
    }
    fn paths(&self) -> Vec<std::path::PathBuf> {
        vec![
            self._dir
                .path()
                .join("workspace/.rentemester/workspace-control.sqlite"),
            self._dir.path().join("workspace/a/data/ledger.sqlite"),
        ]
    }
}
struct Fake {
    sender: SenderIdentity,
    calls: usize,
    outcome: Handoff,
    paths: Vec<std::path::PathBuf>,
    after_send: Option<Box<dyn FnMut()>>,
}
impl Fake {
    fn new(f: &Fixture, outcome: Handoff) -> Self {
        Self {
            sender: SenderIdentity {
                host: "smtp.example.invalid".into(),
                port: 465,
                from_address: "sender@example.invalid".into(),
                from_name: Some("Synthetic sender".into()),
            },
            calls: 0,
            outcome,
            paths: f.paths(),
            after_send: None,
        }
    }
}
impl Transport for Fake {
    fn identity(&self) -> &SenderIdentity {
        &self.sender
    }
    fn send(&mut self, message: &Message) -> Handoff {
        assert_eq!(store::sha(&message.raw), message.body_sha256);
        for path in &self.paths {
            let mut db = Connection::open(path).unwrap();
            let tx = db
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            tx.rollback().unwrap();
        }
        self.calls += 1;
        if let Some(action) = &mut self.after_send {
            action();
        }
        self.outcome
    }
}
#[test]
fn service_mail_reader_and_bookkeeper_refused_before_quota_or_transport() {
    for role in ["reader", "bookkeeper"] {
        let mut f = fixture(role);
        let mut transport = Fake::new(&f, Handoff::Accepted);
        let before = f.remaining();
        let failure = f
            .send("buyer@example.invalid", &mut transport)
            .err()
            .unwrap();
        assert_eq!(failure.error.code, "access_denied");
        assert_eq!(failure.usage, UsageStage::NotCommitted);
        assert_eq!(transport.calls, 0);
        assert_eq!(f.remaining(), before);
        assert!(store::rows(&f.company).unwrap().is_empty());
    }
}
#[test]
fn service_mail_accepted_duplicate_preserves_source_and_spends_one_charge_each_call() {
    let mut f = fixture("owner");
    let mut transport = Fake::new(&f, Handoff::Accepted);
    let before = f.remaining();
    let first = f
        .send("buyer@example.invalid", &mut transport)
        .unwrap()
        .result["delivery"]
        .clone();
    assert_eq!(first["state"], "accepted");
    assert_eq!(transport.calls, 1);
    assert_eq!(f.remaining(), before - 1);
    assert!(
        first["history"]
            .as_array()
            .unwrap()
            .iter()
            .all(|event| event["actor"] == format!("service_account:{}", f.subject))
    );
    let duplicate = f
        .send("buyer@example.invalid", &mut transport)
        .unwrap()
        .result["delivery"]
        .clone();
    assert_eq!(duplicate["duplicate"], true);
    assert_eq!(duplicate["stateHash"], first["stateHash"]);
    assert_eq!(transport.calls, 1);
    assert_eq!(f.remaining(), before - 2);
    store::validate_backup(&f.company).unwrap();
    let before = f.remaining();
    let result = status_service(
        &mut f.control,
        &mut f.company,
        &mut f.guard,
        ServiceContext {
            key: &f.key,
            clock: &Fixed,
            selection: CompanySelection {
                workspace: "w",
                slug: "a",
                registered: true,
                archived: false,
            },
            max_future_days: 45,
        },
    )
    .unwrap();
    assert_eq!(result.result["rows"][0]["state"], "accepted");
    assert_eq!(f.remaining(), before - 1);
}
#[test]
fn service_mail_revocation_after_reservation_before_transport_is_unknown_and_never_sends() {
    let mut f = fixture("owner");
    let mut transport = Fake::new(&f, Handoff::Accepted);
    let before = f.remaining();
    let path = f.paths()[0].clone();
    let credential = f.credential.clone();
    BEFORE_READY.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(move || {
            Connection::open(path)
                .unwrap()
                .execute("UPDATE apikey SET enabled=0 WHERE id=?1", [credential])
                .unwrap();
        }))
    });
    let failure = f
        .send("buyer@example.invalid", &mut transport)
        .err()
        .unwrap();
    assert_eq!(failure.error.code, "invalid_credentials");
    assert_eq!(failure.business, BusinessStage::OutcomeUnknown);
    assert_eq!(failure.usage, UsageStage::Committed);
    assert_eq!(transport.calls, 0);
    assert_eq!(f.remaining(), before - 1);
    let rows = store::rows(&f.company).unwrap();
    assert_eq!(rows[0]["state"], "unknown");
    store::validate_backup(&f.company).unwrap();
}
#[test]
fn service_mail_unknown_handoff_requires_reconcile_before_retry_with_same_source() {
    let mut f = fixture("owner");
    let mut transport = Fake::new(&f, Handoff::Unknown);
    let before = f.remaining();
    let row = f
        .send("buyer@example.invalid", &mut transport)
        .unwrap()
        .result["delivery"]
        .clone();
    assert_eq!(row["state"], "unknown");
    assert_eq!(f.remaining(), before - 1);
    assert!(f.retry(&row, &mut transport).is_err());
    assert_eq!(transport.calls, 1);
    let reconciled = f.reconcile(&row).unwrap().result["delivery"].clone();
    assert_eq!(reconciled["state"], "refused");
    assert!(f.reconcile(&row).is_err());
    transport.outcome = Handoff::Accepted;
    let retried = f.retry(&reconciled, &mut transport).unwrap().result["delivery"].clone();
    assert_eq!(retried["state"], "accepted");
    assert_eq!(retried["messageId"], row["messageId"]);
    assert_eq!(retried["bodySha256"], row["bodySha256"]);
    assert_eq!(transport.calls, 2);
    store::validate_backup(&f.company).unwrap();
}
#[test]
fn service_mail_lost_post_handoff_authority_never_returns_safe_failure() {
    let mut f = fixture("owner");
    let mut transport = Fake::new(&f, Handoff::Accepted);
    let path = f.paths()[0].clone();
    let credential = f.credential.clone();
    transport.after_send = Some(Box::new(move || {
        Connection::open(&path)
            .unwrap()
            .execute("UPDATE apikey SET enabled=0 WHERE id=?1", [&credential])
            .unwrap();
    }));
    let failure = f
        .send("buyer@example.invalid", &mut transport)
        .err()
        .unwrap();
    assert_eq!(transport.calls, 1);
    assert_eq!(failure.business, BusinessStage::OutcomeUnknown);
    assert_eq!(failure.error.code, "invalid_credentials");
    assert_eq!(store::rows(&f.company).unwrap()[0]["state"], "unknown");
}
