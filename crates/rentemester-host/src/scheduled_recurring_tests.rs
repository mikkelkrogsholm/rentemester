//! Synthetic unit fixtures only. Browser acceptance uses normal verification/MFA.
use super::*;
use rentemester_application::scheduled_recurring::Action;
use rentemester_runtime::auth::{AuthError, CookieProfile, CookieVerifier};
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
fn call(
    owner: &mut Owner,
    proof: &VerifiedCookie,
    action: Action<'_>,
) -> Result<serde_json::Value, rentemester_application::control_fence::Failure> {
    owner
        .scheduled_recurring(
            proof,
            &Fixed(NOW),
            action,
            "2026-01-01",
            "2026-01-01T00:00:00Z",
        )
        .unwrap()
        .outcome
        .map(|c| c.value)
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
fn plan(owner: &mut Owner, proof: &VerifiedCookie, id: i64) -> serde_json::Value {
    call(owner, proof, Action::Plan(id)).unwrap()["plan"].clone()
}
fn enable(owner: &mut Owner, proof: &VerifiedCookie, id: i64) {
    let p = plan(owner, proof, id);
    let input = serde_json::from_value(
        json!({"templateId":id,"templateHash":p["templateHash"],"confirm":true}),
    )
    .unwrap();
    call(owner, proof, Action::Enable(&input)).unwrap();
    let db = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
    let error = rentemester_storage::portable_backup::company(db).err().expect("nonempty scheduling cannot be silently omitted from backup");
    assert_eq!(error.code, "portable_backup_refused");
    assert!(error.message.contains("scheduled recurring history"));
}
fn dispatch(
    owner: &mut Owner,
    proof: &VerifiedCookie,
    id: i64,
    index: u32,
) -> Result<serde_json::Value, rentemester_application::control_fence::Failure> {
    let p = plan(owner, proof, id);
    let i = serde_json::from_value(
        json!({"templateId":id,"planHash":p["planHash"],"periodIndex":index,"confirm":true}),
    )
    .unwrap();
    call(owner, proof, Action::Dispatch(&i))
}
#[test]
fn scheduled_recurring_owned_catchup_skip_pause_current_authority_originals_and_restart() {
    let (dir, mut owner, _cookies, proof, uid) = setup();
    let id = template(&mut owner, &proof, "2025-12-18");
    let count = |owner: &Owner, table: &str| {
        owner
            .held
            .0
            .as_ref()
            .unwrap()
            .company
            .as_ref()
            .unwrap()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap()
    };
    let before = call(&mut owner, &proof, Action::List).unwrap();
    assert_eq!(before["scheduledRecurring"]["rows"][0]["state"], "disabled");
    assert!(
        !rentemester_storage::scheduled_recurring_schema::installed(
            owner.held.0.as_ref().unwrap().company.as_ref().unwrap()
        )
        .unwrap()
    );
    enable(&mut owner, &proof, id);
    let p = plan(&mut owner, &proof, id);
    assert_eq!(p["missedCount"], 2);
    assert_eq!(p["normalDue"]["periodIndex"], 2);
    let stale: rentemester_engine::scheduled_recurring::Dispatch = serde_json::from_value(
        json!({"templateId":id,"planHash":p["planHash"],"periodIndex":2,"confirm":true}),
    )
    .unwrap();
    let select = serde_json::from_value(
        json!({"templateId":id,"planHash":p["planHash"],"periodIndexes":[0],"confirm":true}),
    )
    .unwrap();
    call(&mut owner, &proof, Action::CatchUp(&select)).unwrap();
    assert!(call(&mut owner, &proof, Action::Dispatch(&stale)).is_err());
    let p = plan(&mut owner, &proof, id);
    let skip=serde_json::from_value(json!({"templateId":id,"planHash":p["planHash"],"periodIndexes":[1],"reason":"Explicit skip of historical occurrence","confirm":true})).unwrap();
    call(&mut owner, &proof, Action::Skip(&skip)).unwrap();
    let p = plan(&mut owner, &proof, id);
    let change=serde_json::from_value(json!({"templateId":id,"expectedStateHash":p["stateHash"],"reason":"Explicit pause","confirm":true})).unwrap();
    call(&mut owner, &proof, Action::Pause(&change)).unwrap();
    assert!(dispatch(&mut owner, &proof, id, 0).is_err());
    let p = plan(&mut owner, &proof, id);
    let change=serde_json::from_value(json!({"templateId":id,"expectedStateHash":p["stateHash"],"reason":"Explicit resume","confirm":true})).unwrap();
    call(&mut owner, &proof, Action::Resume(&change)).unwrap();
    // Membership revoke commits before dispatch; failed authorization must leave this Owner reusable.
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u')").unwrap();
    assert!(call(&mut owner, &proof, Action::Dispatch(&stale)).is_err());
    assert_eq!(count(&owner, "documents"), 0);
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u')").unwrap();
    let queued = dispatch(&mut owner, &proof, id, 0).unwrap();
    assert_eq!(queued["dispatch"]["status"], "succeeded");
    let doc = queued["generation"]["documentId"].as_i64().unwrap();
    let json_original = owner
        .document_original(&proof, &Fixed(NOW), doc)
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert!(!json_original.bytes.is_empty());
    let pdf = owner
        .sales(
            &proof,
            &Fixed(NOW),
            rentemester_application::sales::Action::Pdf(doc),
            "2026-01-01",
            "2026-01-01T00:00:00Z",
            None,
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    if let rentemester_application::sales::Output::Pdf(bytes) = pdf {
        assert!(bytes.starts_with(b"%PDF-"));
    } else {
        panic!("missing immutable PDF");
    }
    let due = dispatch(&mut owner, &proof, id, 2).unwrap();
    assert_eq!(due["dispatch"]["status"], "succeeded");
    assert!(dispatch(&mut owner, &proof, id, 2).is_err());
    assert!(dispatch(&mut owner, &proof, id, 1).is_err());
    assert_eq!(count(&owner, "documents"), 4);
    assert_eq!(count(&owner, "journal_entries"), 0);
    let saved = call(&mut owner, &proof, Action::List).unwrap();
    assert_eq!(
        saved["scheduledRecurring"]["rows"][0]["nextOccurrence"]["issueDate"],
        "2026-01-08"
    );
    let p = plan(&mut owner, &proof, id);
    assert_eq!(p["missedCount"], 0);
    assert!(p["normalDue"].is_null());
    owner.try_close().unwrap();
    let mut owner = Owner::open_with_profile(
        &dir.path().canonicalize().unwrap().join("workspace"),
        uid,
        "w",
        "a",
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    assert_eq!(call(&mut owner, &proof, Action::List).unwrap(), saved);
    assert_eq!(count(&owner, "documents"), 4);
    assert!(dispatch(&mut owner, &proof, id, 0).is_err());
    let p = plan(&mut owner, &proof, id);
    let cancel=serde_json::from_value(json!({"templateId":id,"expectedStateHash":p["stateHash"],"reason":"Explicit cancellation","confirm":true})).unwrap();
    call(&mut owner, &proof, Action::Cancel(&cancel)).unwrap();
    assert!(call(&mut owner, &proof, Action::Resume(&cancel)).is_err());
}
#[test]
fn scheduled_recurring_failure_is_durable_terminal_not_retry_and_cancel_has_no_invoice() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let id = template(&mut owner, &proof, "2026-01-01");
    enable(&mut owner, &proof, id);
    owner
        .held
        .0
        .as_mut()
        .unwrap()
        .company
        .as_mut()
        .unwrap()
        .execute_batch("UPDATE customers SET archived=1")
        .unwrap();
    let failed = dispatch(&mut owner, &proof, id, 0).unwrap();
    assert_eq!(failed["dispatch"]["status"], "failed");
    assert!(failed["scheduledRecurring"]["error"].is_string());
    assert!(dispatch(&mut owner, &proof, id, 0).is_err());
    let c = owner.held.0.as_mut().unwrap().company.as_mut().unwrap();
    assert_eq!(
        c.query_row("SELECT count(*) FROM documents", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM recurring_invoice_generation_claims",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    let p = plan(&mut owner, &proof, id);
    let change=serde_json::from_value(json!({"templateId":id,"expectedStateHash":p["stateHash"],"reason":"Cancel after visible failed attempt","confirm":true})).unwrap();
    call(&mut owner, &proof, Action::Cancel(&change)).unwrap();
    assert!(dispatch(&mut owner, &proof, id, 0).is_err());
}
#[test]
fn scheduled_recurring_other_authorized_user_can_stop_but_cannot_inherit_consent() {
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    let id = template(&mut owner, &proof, "2026-01-01");
    enable(&mut owner, &proof, id);
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('r','Reviewer','r@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('rf','synthetic','[]','r',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('rs','r','reviewer-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('r','activate','workspace_owner','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('r','a','grant','owner','user:u')").unwrap();
    let cookie = cookies.issue_session("reviewer-token").unwrap();
    let other = cookies
        .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    assert!(dispatch(&mut owner, &other, id, 0).is_err());
    let p = plan(&mut owner, &other, id);
    let pause=serde_json::from_value(json!({"templateId":id,"expectedStateHash":p["stateHash"],"reason":"Other current owner stops generation","confirm":true})).unwrap();
    call(&mut owner, &other, Action::Pause(&pause)).unwrap();
    let p = plan(&mut owner, &other, id);
    let resume=serde_json::from_value(json!({"templateId":id,"expectedStateHash":p["stateHash"],"reason":"May not inherit another user's consent","confirm":true})).unwrap();
    assert!(call(&mut owner, &other, Action::Resume(&resume)).is_err());
    call(&mut owner, &proof, Action::Resume(&resume)).unwrap();
    let p = plan(&mut owner, &other, id);
    let cancel=serde_json::from_value(json!({"templateId":id,"expectedStateHash":p["stateHash"],"reason":"Other current owner cancels generation","confirm":true})).unwrap();
    call(&mut owner, &other, Action::Cancel(&cancel)).unwrap();
    assert!(dispatch(&mut owner, &proof, id, 0).is_err());
}
