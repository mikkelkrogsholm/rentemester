//! Unit-only human/session facts; executor credentials are created/admitted by the real lifecycle.
//! Actual human enrollment/MFA/browser-closed server qualification is separate integration evidence.
use super::*;
use rentemester_application::{
    scheduled_recurring::Action as App,
    service_lifecycle::{self, Operation},
};
use rentemester_runtime::auth::{AuthError, CookieProfile, CookieVerifier};
use serde_json::{Value, json};
const TODAY: &str = "2026-01-01";
const AT: &str = "2026-01-01T00:00:00.000Z";
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1767225600000)
    }
}
struct Fixture {
    _dir: tempfile::TempDir,
    owner: Owner,
    cookies: CookieVerifier,
    proof: VerifiedCookie,
    key: ServiceKey,
    subject: String,
    credential: String,
    uid: u32,
}
fn lifecycle(f: &mut Fixture, op: &Operation) -> Value {
    let r = f
        .owner
        .control_auth("w", |c| {
            service_lifecycle::execute(c, &f.cookies, &Fixed, &f.proof, op)
        })
        .unwrap()
        .outcome
        .unwrap();
    serde_json::from_str(r.as_json()).unwrap()
}
fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
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
        &dir.path().canonicalize().unwrap().join("source"),
        uid,
        "w",
        "a",
        "Synthetic hosted",
        AT,
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
    let receipt = owner
        .control_auth("w", |c| {
            service_lifecycle::execute(
                c,
                &cookies,
                &Fixed,
                &proof,
                &Operation::Create {
                    display_name: "Synthetic hosted executor".into(),
                    operation_id: None,
                    expires_in_seconds: None,
                },
            )
        })
        .unwrap()
        .outcome
        .unwrap();
    let r: Value = serde_json::from_str(receipt.as_json()).unwrap();
    let subject = r["serviceAccountId"].as_str().unwrap().to_owned();
    let credential = r["credentialId"].as_str().unwrap().to_owned();
    let key = ServiceKey::new("w", r["secret"].as_str().unwrap()).unwrap();
    let db = owner.held.0.as_mut().unwrap().control.as_mut().unwrap();
    db.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES(?1,'activate','member','user:u')",[&subject]).unwrap();
    db.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','bookkeeper','user:u')",[&subject]).unwrap();
    Fixture {
        _dir: dir,
        owner,
        cookies,
        proof,
        key,
        subject,
        credential,
        uid,
    }
}
fn app(f: &mut Fixture, action: App<'_>) -> Value {
    f.owner
        .scheduled_recurring(&f.proof, &Fixed, action, TODAY, AT)
        .unwrap()
        .outcome
        .unwrap()
        .value
}
fn template(f: &mut Fixture, first: &str) -> i64 {
    let db = f.owner.held.0.as_mut().unwrap().company.as_mut().unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    tx.execute_batch("UPDATE companies SET address='Synthetic1',cvr='87654321',vat_period_type='quarter' WHERE id=1").unwrap();
    let existing: Option<i64> = {
        use rusqlite::OptionalExtension;
        tx.query_row("SELECT id FROM customers ORDER BY id LIMIT 1", [], |r| {
            r.get(0)
        })
        .optional()
        .unwrap()
    };
    let customer = if let Some(id) = existing {
        id
    } else {
        rentemester_storage::sales::create_customer(
            &tx,
            &serde_json::from_value(
                json!({"name":"Synthetic buyer","address":"Synthetic2","vatOrCvr":"DK12345678"}),
            )
            .unwrap(),
            "u",
        )
        .unwrap()["id"]
            .as_i64()
            .unwrap()
    };
    tx.commit().unwrap();
    let i=serde_json::from_value(json!({"name":"Synthetic hosted service","interval":"weekly","intervalCount":1,"deliveryChannel":"manual","firstIssueDate":first,"paymentTermsDays":14,"vatRatePercent":25,"currency":"DKK","customerId":customer,"lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":100}],"confirm":true})).unwrap();
    let id = f
        .owner
        .recurring_invoices(
            &f.proof,
            &Fixed,
            rentemester_application::recurring_invoices::Action::Create(&i),
            TODAY,
            AT,
        )
        .unwrap()
        .outcome
        .unwrap()
        .value["templateId"]
        .as_i64()
        .unwrap();
    let p = app(f, App::Plan(id))["plan"].clone();
    let enable = serde_json::from_value(
        json!({"templateId":id,"templateHash":p["templateHash"],"confirm":true}),
    )
    .unwrap();
    app(f, App::Enable(&enable));
    id
}
fn plan(f: &mut Fixture, id: i64) -> Value {
    f.owner
        .hosted_recurring(
            &f.proof,
            &Fixed,
            Some(&f.key),
            hosted_recurring::Action::Plan(id),
        )
        .unwrap()
        .outcome
        .unwrap()
        .value["hostedPlan"]
        .clone()
}
fn consent(f: &mut Fixture, id: i64) {
    let p = plan(f, id);
    let input=serde_json::from_value(json!({"templateId":id,"serviceAccountId":f.subject,"credentialId":f.credential,"planHash":p["planHash"],"reason":"Explicit reviewed normal-due-only delegation","confirm":true})).unwrap();
    let r = f
        .owner
        .hosted_recurring(
            &f.proof,
            &Fixed,
            Some(&f.key),
            hosted_recurring::Action::Consent(&input),
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(r["scheduledRecurring"]["executionMode"], "hosted");
}
fn count(f: &Fixture, table: &str) -> i64 {
    f.owner
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn usage(f: &Fixture) -> (i64, Option<i64>, Option<String>) {
    f.owner
        .held
        .0
        .as_ref()
        .unwrap()
        .control
        .as_ref()
        .unwrap()
        .query_row(
            "SELECT requestCount,remaining,lastRequest FROM apikey WHERE id=?1",
            [&f.credential],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap()
}
fn candidates(f: &mut Fixture) -> Value {
    f.owner
        .hosted_recurring_candidates(&f.key, &Fixed, 32)
        .unwrap()
        .outcome
        .unwrap()
        .value
}
#[test]
fn hosted_real_service_admission_normal_due_exact_originals_and_app_audit_regression() {
    let mut f = fixture();
    let id = template(&mut f, TODAY);
    let before = usage(&f);
    let _ = plan(&mut f, id);
    assert_eq!(usage(&f), before);
    consent(&mut f, id);
    assert_eq!(count(&f, "documents"), 0);
    assert_eq!(candidates(&mut f)["rows"].as_array().unwrap().len(), 1);
    assert_eq!(usage(&f), before);
    let p = app(&mut f, App::Plan(id))["plan"].clone();
    let manual = serde_json::from_value(
        json!({"templateId":id,"planHash":p["planHash"],"periodIndex":0,"confirm":true}),
    )
    .unwrap();
    assert!(
        f.owner
            .scheduled_recurring(&f.proof, &Fixed, App::Dispatch(&manual), TODAY, AT)
            .unwrap()
            .outcome
            .is_err()
    );
    let done = f
        .owner
        .dispatch_hosted_recurring(&f.key, &Fixed, id)
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(done.result["dispatch"]["status"], "succeeded");
    assert!(matches!(
        done.usage,
        rentemester_application::service_company::UsageStage::Committed
    ));
    assert_eq!(count(&f, "documents"), 2);
    assert_eq!(count(&f, "recurring_invoice_generations"), 1);
    assert_eq!(count(&f, "journal_entries"), 0);
    let document = done.result["generation"]["documentId"].as_i64().unwrap();
    let pdf = f
        .owner
        .sales(
            &f.proof,
            &Fixed,
            rentemester_application::sales::Action::Pdf(document),
            TODAY,
            AT,
            None,
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    match pdf {
        rentemester_application::sales::Output::Pdf(bytes) => assert!(bytes.starts_with(b"%PDF-")),
        _ => panic!("expected immutable PDF"),
    }
    let charged = usage(&f);
    assert!(
        f.owner
            .dispatch_hosted_recurring(&f.key, &Fixed, id)
            .unwrap()
            .outcome
            .is_err()
    );
    assert_eq!(usage(&f), charged);
    assert_eq!(count(&f, "documents"), 2);
    let db = f.owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
    assert_eq!(db.query_row("SELECT count(*) FROM audit_log WHERE event_type IN ('invoice_issue','invoice_render_pdf','recurring_invoice_generate') AND actor=?1",[format!("service_account:{}",f.subject)],|r|r.get::<_,i64>(0)).unwrap(),3);
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM audit_log WHERE actor=?1",
            [format!("user:{}", f.subject)],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    let human_id = template(&mut f, TODAY);
    let p = app(&mut f, App::Plan(human_id))["plan"].clone();
    let i = serde_json::from_value(
        json!({"templateId":human_id,"planHash":p["planHash"],"periodIndex":0,"confirm":true}),
    )
    .unwrap();
    assert_eq!(
        app(&mut f, App::Dispatch(&i))["dispatch"]["status"],
        "succeeded"
    );
    assert_eq!(f.owner.held.0.as_ref().unwrap().company.as_ref().unwrap().query_row("SELECT count(*) FROM audit_log WHERE event_type IN ('invoice_issue','invoice_render_pdf','recurring_invoice_generate') AND actor='user:u'",[],|r|r.get::<_,i64>(0)).unwrap(),3);
}
#[test]
fn hosted_identity_hash_grants_rotation_and_revoke_without_configured_executor() {
    let mut f = fixture();
    let id = template(&mut f, TODAY);
    let p = plan(&mut f, id);
    for field in ["credentialId", "serviceAccountId", "planHash"] {
        let mut i = json!({"templateId":id,"serviceAccountId":f.subject,"credentialId":f.credential,"planHash":p["planHash"],"reason":"Reviewed delegation","confirm":true});
        i[field] = json!("wrong");
        assert!(
            f.owner
                .hosted_recurring(
                    &f.proof,
                    &Fixed,
                    Some(&f.key),
                    hosted_recurring::Action::Consent(&serde_json::from_value(i).unwrap())
                )
                .unwrap()
                .outcome
                .is_err()
        );
    }
    consent(&mut f, id);
    let db = f.owner.held.0.as_mut().unwrap().control.as_mut().unwrap();
    db.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES(?1,'a','revoke','user:u')",[&f.subject]).unwrap();
    assert!(
        f.owner
            .dispatch_hosted_recurring(&f.key, &Fixed, id)
            .unwrap()
            .outcome
            .is_err()
    );
    let db = f.owner.held.0.as_mut().unwrap().control.as_mut().unwrap();
    db.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','bookkeeper','user:u')",[&f.subject]).unwrap();
    assert!(candidates(&mut f)["rows"].as_array().unwrap().is_empty());
    consent(&mut f, id);
    let subject = f.subject.clone();
    let credential = f.credential.clone();
    let r = lifecycle(
        &mut f,
        &Operation::Rotate {
            subject,
            credential,
            operation_id: None,
            expires_in_seconds: None,
        },
    );
    assert!(
        f.owner
            .dispatch_hosted_recurring(&f.key, &Fixed, id)
            .unwrap()
            .outcome
            .is_err()
    );
    f.key = ServiceKey::new("w", r["secret"].as_str().unwrap()).unwrap();
    f.credential = r["credentialId"].as_str().unwrap().into();
    assert!(candidates(&mut f)["rows"].as_array().unwrap().is_empty());
    consent(&mut f, id);
    let subject = f.subject.clone();
    let credential = f.credential.clone();
    lifecycle(
        &mut f,
        &Operation::Revoke {
            subject,
            credential,
            operation_id: None,
        },
    );
    let p = app(&mut f, App::Plan(id))["plan"].clone();
    let i=serde_json::from_value(json!({"templateId":id,"expectedStateHash":p["stateHash"],"reason":"Explicit revoke without executor key","confirm":true})).unwrap();
    let r = f
        .owner
        .hosted_recurring(&f.proof, &Fixed, None, hosted_recurring::Action::Revoke(&i))
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(r["scheduledRecurring"]["executionMode"], "app");
    assert_eq!(r["scheduledRecurring"]["hostedConsent"]["active"], false);
    assert_eq!(count(&f, "documents"), 0);
}
#[test]
fn hosted_missed_work_and_grantor_revoke_regrant_never_auto_resume() {
    let mut f = fixture();
    let missed = template(&mut f, "2025-12-18");
    let p = plan(&mut f, missed);
    let i=serde_json::from_value(json!({"templateId":missed,"serviceAccountId":f.subject,"credentialId":f.credential,"planHash":p["planHash"],"reason":"Must not assume catchup","confirm":true})).unwrap();
    assert!(
        f.owner
            .hosted_recurring(
                &f.proof,
                &Fixed,
                Some(&f.key),
                hosted_recurring::Action::Consent(&i)
            )
            .unwrap()
            .outcome
            .is_err()
    );
    let id = template(&mut f, TODAY);
    consent(&mut f, id);
    let db = f.owner.held.0.as_mut().unwrap().control.as_mut().unwrap();
    db.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES('u','a','revoke','user:u')").unwrap();
    assert!(candidates(&mut f)["rows"].as_array().unwrap().is_empty());
    let db = f.owner.held.0.as_mut().unwrap().control.as_mut().unwrap();
    db.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u')").unwrap();
    assert!(candidates(&mut f)["rows"].as_array().unwrap().is_empty());
    assert!(
        f.owner
            .dispatch_hosted_recurring(&f.key, &Fixed, id)
            .unwrap()
            .outcome
            .is_err()
    );
    assert_eq!(count(&f, "documents"), 0);
    consent(&mut f, id);
    let p = app(&mut f, App::Plan(id))["plan"].clone();
    let change=serde_json::from_value(json!({"templateId":id,"expectedStateHash":p["stateHash"],"reason":"Explicit pause invalidates delegation","confirm":true})).unwrap();
    let row = app(&mut f, App::Pause(&change))["scheduledRecurring"].clone();
    assert_eq!(row["hostedConsent"]["active"], false);
    let change=serde_json::from_value(json!({"templateId":id,"expectedStateHash":row["stateHash"],"reason":"Resume only ordinary app mode","confirm":true})).unwrap();
    assert_eq!(
        app(&mut f, App::Resume(&change))["scheduledRecurring"]["executionMode"],
        "app"
    );
    assert!(candidates(&mut f)["rows"].as_array().unwrap().is_empty());
    consent(&mut f, id);
    let p = app(&mut f, App::Plan(id))["plan"].clone();
    let change=serde_json::from_value(json!({"templateId":id,"expectedStateHash":p["stateHash"],"reason":"Explicit terminal cancel","confirm":true})).unwrap();
    let row = app(&mut f, App::Cancel(&change))["scheduledRecurring"].clone();
    assert_eq!(row["hostedConsent"]["active"], false);
    assert_eq!(row["state"], "cancelled");
    assert!(candidates(&mut f)["rows"].as_array().unwrap().is_empty());
}
#[test]
fn hosted_v3_signed_backup_restore_preserves_generations_and_suspends_all_historical_authority() {
    let mut f = fixture();
    let id = template(&mut f, TODAY);
    consent(&mut f, id);
    f.owner
        .dispatch_hosted_recurring(&f.key, &Fixed, id)
        .unwrap()
        .outcome
        .unwrap();
    let raw: Vec<String> = f
        .owner
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .prepare("SELECT canonical_payload FROM native_scheduled_recurring_events ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(raw.len(), 4);
    assert!(raw[1].contains("\"version\":3"));
    let hex = concat!(
        "3051020101300506032b657004220420",
        "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
        "812100",
        "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
    );
    let der = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    let key = rentemester_runtime::backup_artifact::SigningKey::from_pkcs8(&der).unwrap();
    let trust =
        rentemester_runtime::backup_artifact::TrustedPublicKey::from_bytes(&key.public_key_bytes())
            .unwrap();
    let bytes = f
        .owner
        .backup_workspace(&f.cookies, &Fixed, &f.proof, &key)
        .unwrap()
        .outcome
        .unwrap();
    let archive = rentemester_runtime::backup_artifact::verify(&bytes, &trust).unwrap();
    Owner::validate_backup(&archive).unwrap();
    let target = f._dir.path().canonicalize().unwrap().join("restored");
    let op = rentemester_runtime::bootstrap::Operator::from_policy(
        "w",
        "system:synthetic-restore",
        &["system:synthetic-restore".into()],
    )
    .unwrap();
    let mut restored = Owner::restore_new(
        &target,
        f.uid,
        "w",
        "a",
        NamespaceProfile::LocalUserMacos,
        &archive,
        &op,
    )
    .unwrap();
    let slots = restored.held.0.as_mut().unwrap();
    let db = slots.company.as_mut().unwrap();
    let after: Vec<String> = db
        .prepare("SELECT canonical_payload FROM native_scheduled_recurring_events ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(after, raw);
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let row = rentemester_storage::scheduled_recurring::current(&tx, id, TODAY).unwrap();
    assert_eq!(row["executionMode"], "app");
    assert_eq!(row["hostedConsent"]["active"], false);
    assert_eq!(row["restoreConsent"]["required"], true);
    assert_eq!(row["lastRun"]["status"], "succeeded");
    tx.rollback().unwrap();
    let originals: Vec<(String, String)> = db
        .prepare("SELECT stored_path,sha256_hash FROM documents ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(originals.len(), 2);
    for (path, hash) in originals {
        assert!(!slots.guard.read_document(&path, &hash).unwrap().is_empty());
    }
    assert_eq!(
        slots
            .control
            .as_ref()
            .unwrap()
            .query_row("SELECT count(*) FROM apikey", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(
        restored
            .dispatch_hosted_recurring(&f.key, &Fixed, id)
            .unwrap()
            .outcome
            .is_err()
    );
}
