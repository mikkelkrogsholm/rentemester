//! Synthetic unit fixtures only. Browser acceptance uses normal verification/MFA.
use super::*;
use rentemester_runtime::auth::{AuthError, CookieProfile, CookieVerifier};
use rentemester_runtime::invoice_mail::{Handoff, Message, SenderIdentity, Transport};
use serde_json::json;
struct Fixed(i64);
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(self.0)
    }
}
const NOW: i64 = 1767225600000;
fn setup() -> (
    tempfile::TempDir,
    Owner,
    CookieVerifier,
    VerifiedCookie,
    u32,
) {
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
    let mut owner = Owner::create_with_chart_profile(
        &root,
        uid,
        "w",
        "a",
        "Synthetic A",
        "2026-01-01T00:00:00.000Z",
        &[],
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','owner-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u');").unwrap();
    let cookies = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let cookie = cookies.issue_session("owner-token").unwrap();
    let proof = cookies
        .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    (dir, owner, cookies, proof, uid)
}
fn template(owner: &mut Owner, proof: &VerifiedCookie, first: &str) -> i64 {
    let tx = owner.held.0.as_mut().unwrap().company.as_mut().unwrap();
    tx.execute_batch("UPDATE companies SET address='Synthetic1',cvr='87654321',vat_period_type='quarter' WHERE id=1").unwrap();
    let customer = rentemester_storage::sales::Customer {
        ..serde_json::from_value(
            json!({"name":"Synthetic buyer","address":"Synthetic2","vatOrCvr":"DK12345678"}),
        )
        .unwrap()
    };
    let t = tx
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let customer = rentemester_storage::sales::create_customer(&t, &customer, "u").unwrap()["id"]
        .as_i64()
        .unwrap();
    t.commit().unwrap();
    let input=serde_json::from_value(json!({"name":"Synthetic scheduled service","interval":"weekly","intervalCount":1,"deliveryChannel":"manual","firstIssueDate":first,"paymentTermsDays":14,"vatRatePercent":25,"currency":"DKK","customerId":customer,"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100}],"confirm":true})).unwrap();
    owner
        .recurring_invoices(
            proof,
            &Fixed(NOW),
            rentemester_application::recurring_invoices::Action::Create(&input),
            "2026-01-01",
            "2026-01-01T00:00:00Z",
        )
        .unwrap()
        .outcome
        .unwrap()
        .value["templateId"]
        .as_i64()
        .unwrap()
}
fn invoice(o: &mut Owner, p: &VerifiedCookie) -> i64 {
    let id = template(o, p, "2026-01-01");
    let input = serde_json::from_value(json!({"asOfDate":"2026-01-01","confirm":true})).unwrap();
    o.recurring_invoices(
        p,
        &Fixed(NOW),
        rentemester_application::recurring_invoices::Action::Generate(id, &input),
        "2026-01-01",
        "2026-01-01T00:00:00Z",
    )
    .unwrap()
    .outcome
    .unwrap()
    .value["documentId"]
        .as_i64()
        .unwrap()
}
struct Fake {
    identity: SenderIdentity,
    outcome: Handoff,
    calls: usize,
    paths: Vec<std::path::PathBuf>,
}
impl Fake {
    fn new(outcome: Handoff) -> Self {
        Self {
            identity: SenderIdentity {
                host: "smtp.example.invalid".into(),
                port: 465,
                from_address: "sender@example.invalid".into(),
                from_name: Some("Synthetic sender".into()),
            },
            outcome,
            calls: 0,
            paths: vec![],
        }
    }
}
impl Transport for Fake {
    fn identity(&self) -> &SenderIdentity {
        &self.identity
    }
    fn send(&mut self, m: &Message) -> Handoff {
        assert!(!m.raw.is_empty());
        assert_eq!(store::sha(&m.raw), m.body_sha256);
        // Separate process proves neither protected database holds a SQLite write transaction.
        for p in &self.paths {
            assert!(std::process::Command::new("python3").arg("-c").arg("import sqlite3,sys; c=sqlite3.connect(sys.argv[1],timeout=0); c.execute('BEGIN IMMEDIATE'); c.rollback()").arg(p).status().unwrap().success());
        }
        self.calls += 1;
        self.outcome
    }
}
fn send(
    o: &mut Owner,
    p: &VerifiedCookie,
    id: i64,
    to: &str,
    f: &mut Fake,
) -> Result<Value, rentemester_application::control_fence::Failure> {
    let i = store::Send {
        invoice_document_id: id,
        to: to.into(),
        kind: None,
        confirm: true,
    };
    o.send_invoice_email(p, &Fixed(NOW), &i, f)
        .unwrap()
        .outcome
        .map(|v| v.value["delivery"].clone())
}
fn status(o: &mut Owner, p: &VerifiedCookie) -> Value {
    o.invoice_email_status(p, &Fixed(NOW))
        .unwrap()
        .outcome
        .unwrap()
        .value
}
fn retry(
    o: &mut Owner,
    p: &VerifiedCookie,
    row: &Value,
    f: &mut Fake,
) -> Result<Value, rentemester_application::control_fence::Failure> {
    let i=serde_json::from_value(json!({"messageId":row["messageId"],"expectedStateHash":row["stateHash"],"reason":"Reviewed definite refusal with SMTP operator","confirm":true})).unwrap();
    o.retry_invoice_email(p, &Fixed(NOW), &i, f)
        .unwrap()
        .outcome
        .map(|v| v.value["delivery"].clone())
}
fn reconcile(
    o: &mut Owner,
    p: &VerifiedCookie,
    row: &Value,
    resolution: &str,
) -> Result<Value, rentemester_application::control_fence::Failure> {
    let i=serde_json::from_value(json!({"messageId":row["messageId"],"expectedStateHash":row["stateHash"],"resolution":resolution,"reason":"Reviewed independent SMTP delivery evidence","evidence":"Synthetic operator log reference accepted or refused","confirm":true})).unwrap();
    o.reconcile_invoice_email(p, &Fixed(NOW), &i)
        .unwrap()
        .outcome
        .map(|v| v.value["delivery"].clone())
}
fn db(o: &Owner) -> &Connection {
    o.held.0.as_ref().unwrap().company.as_ref().unwrap()
}
fn count(o: &Owner, t: &str) -> i64 {
    db(o)
        .query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0))
        .unwrap()
}
#[test]
fn accepted_duplicate_refused_reviewed_retry_and_sender_collision() {
    let (_d, mut o, _c, p, _uid) = setup();
    let id = invoice(&mut o, &p);
    let mut f = Fake::new(Handoff::Accepted);
    // Exact owned SQLite files, discovered from their retained handles only.
    for c in [
        o.held.0.as_ref().unwrap().company.as_ref().unwrap(),
        o.held.0.as_ref().unwrap().control.as_ref().unwrap(),
    ] {
        let path: String = c
            .query_row("PRAGMA database_list", [], |r| r.get(2))
            .unwrap();
        f.paths.push(path.into());
    }
    let a = send(&mut o, &p, id, "accepted@example.invalid", &mut f).unwrap();
    assert_eq!(a["state"], "accepted");
    assert_eq!(a["humanResolution"], false);
    assert_eq!(a["history"].as_array().unwrap().len(), 2);
    assert_eq!(f.calls, 1);
    assert_eq!(
        send(&mut o, &p, id, "accepted@example.invalid", &mut f).unwrap()["duplicate"],
        true
    );
    assert_eq!(f.calls, 1);
    assert_eq!(count(&o, "email_send_log"), 1);
    f.identity.from_name = Some("Different sender name".into());
    assert!(send(&mut o, &p, id, "accepted@example.invalid", &mut f).is_err());
    assert_eq!(f.calls, 1);
    f.identity.from_name = Some("Synthetic sender".into());
    f.outcome = Handoff::Refused;
    let r = send(&mut o, &p, id, "refused@example.invalid", &mut f).unwrap();
    assert_eq!(r["state"], "refused");
    assert_eq!(count(&o, "email_send_log"), 1);
    assert_eq!(
        send(&mut o, &p, id, "refused@example.invalid", &mut f).unwrap()["state"],
        "refused"
    );
    assert_eq!(f.calls, 2);
    f.identity.host = "another.example.invalid".into();
    assert!(retry(&mut o, &p, &r, &mut f).is_err());
    f.identity.host = "smtp.example.invalid".into();
    f.outcome = Handoff::Accepted;
    assert_eq!(retry(&mut o, &p, &r, &mut f).unwrap()["state"], "accepted");
    assert_eq!(f.calls, 3);
    assert!(retry(&mut o, &p, &r, &mut f).is_err());
    assert_eq!(f.calls, 3);
    assert_eq!(count(&o, "journal_entries"), 0);
    assert_eq!(count(&o, "documents"), 2);
    store::validate_backup(db(&o)).unwrap();
}
#[test]
fn unknown_restart_reconcile_never_sends_and_current_revocation_fences() {
    let (d, mut o, _c, p, uid) = setup();
    let id = invoice(&mut o, &p);
    let mut f = Fake::new(Handoff::Unknown);
    let u = send(&mut o, &p, id, "unknown@example.invalid", &mut f).unwrap();
    assert_eq!(u["state"], "unknown");
    assert_eq!(count(&o, "email_send_log"), 0);
    let saved = status(&mut o, &p);
    o.try_close().unwrap();
    let mut o = Owner::open_with_profile(
        &d.path().canonicalize().unwrap().join("workspace"),
        uid,
        "w",
        "a",
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    assert_eq!(status(&mut o, &p), saved);
    assert_eq!(
        send(&mut o, &p, id, "unknown@example.invalid", &mut f).unwrap()["state"],
        "unknown"
    );
    assert_eq!(f.calls, 1);
    assert!(retry(&mut o, &p, &u, &mut f).is_err());
    let r = reconcile(&mut o, &p, &u, "not_accepted").unwrap();
    assert_eq!(r["state"], "refused");
    assert_eq!(r["humanResolution"], true);
    assert_eq!(f.calls, 1);
    assert!(reconcile(&mut o, &p, &u, "accepted").is_err());
    f.outcome = Handoff::Accepted;
    assert_eq!(retry(&mut o, &p, &r, &mut f).unwrap()["state"], "accepted");
    assert_eq!(f.calls, 2);
    f.outcome = Handoff::Unknown;
    let u = send(&mut o, &p, id, "attested@example.invalid", &mut f).unwrap();
    assert_eq!(
        reconcile(&mut o, &p, &u, "accepted").unwrap()["state"],
        "accepted"
    );
    assert_eq!(f.calls, 3);
    assert_eq!(count(&o, "email_send_log"), 2);
    o.held.0.as_mut().unwrap().control.as_mut().unwrap().execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u')",[]).unwrap();
    assert!(send(&mut o, &p, id, "revoked@example.invalid", &mut f).is_err());
    assert_eq!(f.calls, 3);
    assert_eq!(store::rows(db(&o)).unwrap().len(), 2);
}
#[test]
fn legacy_accepted_rendering_change_never_resends_and_ambiguous_log_refuses() {
    let (_d, mut o, _c, p, _uid) = setup();
    let id = invoice(&mut o, &p);
    let mut f = Fake::new(Handoff::Accepted);
    let no: String = db(&o)
        .query_row("SELECT invoice_no FROM documents WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .unwrap();
    // Historical TS rendering may differ from current immutable-PDF MIME. Its accepted evidence remains authoritative.
    let old = "<11111111111111111111111111111111@rentemester.local>";
    db(&o).execute("INSERT INTO email_send_log(invoice_document_id,invoice_no,kind,recipient,sender,subject,message_id,body_sha256,smtp_host,created_at)VALUES(?1,?2,'invoice','legacy@example.invalid','sender@example.invalid','Historical rendered invoice',?3,?4,'old.example.invalid','2025-01-01T00:00:00Z')",rusqlite::params![id,no,old,"1".repeat(64)]).unwrap();
    let r = send(&mut o, &p, id, "legacy@example.invalid", &mut f).unwrap();
    assert_eq!(r["state"], "accepted");
    assert_eq!(r["duplicate"], true);
    assert_eq!(r["legacy"], true);
    assert!(r["pdfSha256"].is_null());
    assert_eq!(r["messageId"], old);
    assert_eq!(f.calls, 0);
    db(&o).execute("INSERT INTO email_send_log(invoice_document_id,invoice_no,kind,recipient,sender,subject,message_id,body_sha256,smtp_host,created_at)VALUES(?1,?2,'invoice','legacy@example.invalid','sender@example.invalid','Conflicting old delivery','<22222222222222222222222222222222@rentemester.local>',?3,'old.example.invalid','2025-01-02T00:00:00Z')",rusqlite::params![id,no,"2".repeat(64)]).unwrap();
    assert!(send(&mut o, &p, id, "legacy@example.invalid", &mut f).is_err());
    assert_eq!(f.calls, 0);
    assert_eq!(count(&o, "email_send_log"), 2);
    store::validate_backup(db(&o)).unwrap(); // Retain both historical records, without claiming unambiguous resend authority.
    db(&o).execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('native_invoice_email_unknown','native_invoice_email_v1','forged','{}','user:u')",[]).unwrap();
    assert!(store::validate_backup(db(&o)).is_err());
}
fn key() -> rentemester_runtime::backup_artifact::SigningKey {
    // Public RFC8032 test-vector material only.
    let h = concat!(
        "3051020101300506032b657004220420",
        "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
        "812100",
        "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
    );
    let d = (0..h.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    rentemester_runtime::backup_artifact::SigningKey::from_pkcs8(&d).unwrap()
}
#[test]
fn signed_backup_preserves_exact_history_originals_without_dispatch_or_old_authority() {
    let (d, mut o, c, p, uid) = setup();
    let id = invoice(&mut o, &p);
    let mut f = Fake::new(Handoff::Accepted);
    send(&mut o, &p, id, "accepted@example.invalid", &mut f).unwrap();
    f.outcome = Handoff::Unknown;
    let u = send(&mut o, &p, id, "unknown@example.invalid", &mut f).unwrap();
    assert_eq!(u["state"], "unknown");
    let saved = store::rows(db(&o)).unwrap();
    {
        let slots = o.held.0.as_mut().unwrap();
        let db = slots.company.as_ref().unwrap();
        let mut source = store::sources(db).unwrap().remove(0);
        let json = slots
            .guard
            .read_document(&source.json_path, &source.json_sha256)
            .unwrap();
        let pdf = slots
            .guard
            .read_document(&source.pdf_path, &source.pdf_sha256)
            .unwrap();
        app::verify_source_bytes(db, &source, &json, &pdf).unwrap();
        assert!(app::verify_source_bytes(db, &source, &json, b"different PDF bytes").is_err());
        source.body_sha256 = "0".repeat(64);
        assert!(app::verify_source_bytes(db, &source, &json, &pdf).is_err());
    }
    let key = key();
    let trust = backup_artifact::TrustedPublicKey::from_bytes(&key.public_key_bytes()).unwrap();
    let bytes = o
        .backup_workspace(&c, &Fixed(NOW), &p, &key)
        .unwrap()
        .outcome
        .unwrap();
    let a = backup_artifact::verify(&bytes, &trust).unwrap();
    Owner::validate_backup(&a).unwrap();
    let target = d.path().canonicalize().unwrap().join("restored");
    let op = rentemester_runtime::bootstrap::Operator::from_policy(
        "w",
        "system:synthetic-restore",
        &["system:synthetic-restore".into()],
    )
    .unwrap();
    let mut r = Owner::restore_new(
        &target,
        uid,
        "w",
        "a",
        NamespaceProfile::LocalUserMacos,
        &a,
        &op,
    )
    .unwrap();
    assert_eq!(store::rows(db(&r)).unwrap(), saved);
    assert_eq!(count(&r, "email_send_log"), 1);
    assert_eq!(count(&r, "documents"), 2);
    assert_eq!(count(&r, "journal_entries"), 0);
    {
        let slots = r.held.0.as_mut().unwrap();
        validate_live_sources(slots.company.as_ref().unwrap(), &mut slots.guard).unwrap();
    }
    // No old sessions/grants are imported. Read-only recovery does not invoke a transport.
    assert!(
        r.invoice_email_status(&p, &Fixed(NOW))
            .unwrap()
            .outcome
            .is_err()
    );
    assert_eq!(f.calls, 2);
    r.try_close().unwrap();
    let r =
        Owner::open_with_profile(&target, uid, "w", "a", NamespaceProfile::LocalUserMacos).unwrap();
    assert_eq!(store::rows(db(&r)).unwrap(), saved);
    assert_eq!(f.calls, 2);
}
