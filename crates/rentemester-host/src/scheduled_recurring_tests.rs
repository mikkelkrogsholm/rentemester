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
    rentemester_storage::portable_backup::company(db)
        .expect("complete scheduler history is validated in the signed SQLite snapshot");
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

#[test]
fn restored_scheduler_requires_current_admin_review_and_preserves_originals() {
    let (dir, mut owner, cookies, proof, uid) = setup();
    let id = template(&mut owner, &proof, "2025-12-18");
    enable(&mut owner, &proof, id);
    let p = plan(&mut owner, &proof, id);
    let select = serde_json::from_value(
        json!({"templateId":id,"planHash":p["planHash"],"periodIndexes":[0],"confirm":true}),
    )
    .unwrap();
    call(&mut owner, &proof, Action::CatchUp(&select)).unwrap();
    let issued = dispatch(&mut owner, &proof, id, 2).unwrap();
    let doc = issued["generation"]["documentId"].as_i64().unwrap();
    let source = plan(&mut owner, &proof, id);
    let raw = |owner: &Owner| -> Vec<String> {
        let db = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
        let mut q = db
            .prepare("SELECT canonical_payload FROM native_scheduled_recurring_events ORDER BY id")
            .unwrap();
        q.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };
    let original_events = raw(&owner);
    {
        let db = owner.held.0.as_mut().unwrap().company.as_mut().unwrap();
        rentemester_storage::scheduled_recurring::validate_backup(db).unwrap();
        let tx = db.transaction().unwrap();
        rentemester_storage::scheduled_recurring::append_restore_boundary(&tx, &"a".repeat(64))
            .unwrap();
        tx.commit().unwrap();
    }
    assert_eq!(raw(&owner), original_events);
    let restored = plan(&mut owner, &proof, id);
    assert_eq!(restored["restoreConsent"]["required"], true);
    assert_eq!(restored["restoreConsent"]["historicalOwnerUserId"], "u");
    assert_eq!(
        restored["restoreConsent"]["historicalQueued"][0]["periodIndex"],
        0
    );
    assert!(restored["normalDue"].is_null());
    assert_eq!(restored["queued"], json!([]));
    assert_ne!(source["planHash"], restored["planHash"]);
    assert!(dispatch(&mut owner, &proof, id, 0).is_err());
    let stale=serde_json::from_value(json!({"templateId":id,"planHash":source["planHash"],"reason":"Review restored source","confirm":true})).unwrap();
    assert!(call(&mut owner, &proof, Action::Reconsent(&stale)).is_err());
    let input=serde_json::from_value(json!({"templateId":id,"planHash":restored["planHash"],"reason":"Review restored source","confirm":true})).unwrap();
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('r','Restored owner','r@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('rf','synthetic','[]','r',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('rs','r','restored-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('r','activate','workspace_owner','user:u');").unwrap();
    let cookie = cookies.issue_session("restored-token").unwrap();
    let new_owner = cookies
        .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    // A current bookkeeper may read/generate under ordinary consent, but cannot create restored authority.
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('r','a','grant','bookkeeper','user:u')").unwrap();
    assert!(call(&mut owner, &new_owner, Action::Reconsent(&input)).is_err());
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('r','a','grant','owner','user:u')").unwrap();
    let result = call(&mut owner, &new_owner, Action::Reconsent(&input)).unwrap();
    let mut expected = restored["restoreConsent"].clone();
    expected["required"] = json!(false);
    assert_eq!(result["scheduledRecurring"]["restoreConsent"], expected);
    assert_eq!(result["scheduledRecurring"]["ownerUserId"], "r");
    assert!(dispatch(&mut owner, &proof, id, 0).is_err()); // historical queue never reactivated
    assert!(dispatch(&mut owner, &proof, id, 2).is_err()); // resolved period never duplicates
    assert_eq!(raw(&owner)[..original_events.len()], original_events);
    {
        let db = owner.held.0.as_mut().unwrap().company.as_mut().unwrap();
        rentemester_storage::scheduled_recurring::validate_backup(db).unwrap();
        let tx = db.transaction().unwrap();
        rentemester_storage::scheduled_recurring::append_restore_boundary(&tx, &"b".repeat(64))
            .unwrap();
        tx.commit().unwrap();
    }
    let again = call(&mut owner, &proof, Action::List).unwrap();
    assert_eq!(
        again["scheduledRecurring"]["rows"][0]["restoreConsent"]["required"],
        true
    );
    owner.try_close().unwrap();
    let mut owner = Owner::open_with_profile(
        &dir.path().canonicalize().unwrap().join("workspace"),
        uid,
        "w",
        "a",
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    assert_eq!(call(&mut owner, &proof, Action::List).unwrap(), again);
    assert!(
        !owner
            .document_original(&proof, &Fixed(NOW), doc)
            .unwrap()
            .outcome
            .unwrap()
            .value
            .bytes
            .is_empty()
    );
    let db = owner.held.0.as_mut().unwrap().company.as_mut().unwrap();
    // A registered PDF metadata cross-reference must not silently disappear from a signed backup.
    let (trigger, ddl):(String,String)=db.query_row("SELECT name,sql FROM sqlite_master WHERE type='trigger' AND sql LIKE '%issued invoice PDF evidence is immutable and cannot be updated%'",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    db.execute_batch(&format!("DROP TRIGGER {trigger}"))
        .unwrap();
    db.execute("UPDATE documents SET mime_type='application/octet-stream' WHERE document_type='issued_invoice_pdf'",[]).unwrap();
    db.execute_batch(&ddl).unwrap(); // restore exact physical schema; validator must detect the corrupted evidence

    assert!(rentemester_storage::scheduled_recurring::validate_backup(db).is_err());
}

#[test]
fn access_audit_original_pdf_denial_and_audit_failure_leave_originals_and_journal_unchanged() {
    let count=|owner:&Owner, table:&str|owner.held.0.as_ref().unwrap().company.as_ref().unwrap().query_row(&format!("SELECT count(*) FROM {table}"),[],|r|r.get::<_,i64>(0)).unwrap();

    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let id = template(&mut owner, &proof, "2026-01-01");
    enable(&mut owner, &proof, id);
    let issued = dispatch(&mut owner, &proof, id, 0).unwrap();
    let doc = issued["generation"]["documentId"].as_i64().unwrap();
    let company_before = {
        let db = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
        let mut q = db
            .prepare("SELECT id,sha256_hash,stored_path FROM documents ORDER BY id")
            .unwrap();
        q.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
    };
    let journal_before = count(&owner, "journal_entries");
    let bytes = owner
        .document_original(&proof, &Fixed(NOW), doc)
        .unwrap()
        .outcome
        .unwrap()
        .value
        .bytes;
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
    assert!(
        matches!(pdf,rentemester_application::sales::Output::Pdf(ref b) if b.starts_with(b"%PDF-"))
    );
    {
        let control = owner.held.0.as_mut().unwrap().control.as_mut().unwrap();
        assert_eq!(control.query_row("SELECT count(*) FROM rm_workspace_document_access_events WHERE actor='user:u' AND company_slug='a' AND resource_id=?1 AND outcome='served'",[doc],|r|r.get::<_,i64>(0)).unwrap(),2);
        control.execute_batch("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u');").unwrap();
    }
    assert!(
        owner
            .document_original(&proof, &Fixed(NOW), doc)
            .unwrap()
            .outcome
            .is_err()
    );
    assert!(
        owner
            .sales(
                &proof,
                &Fixed(NOW),
                rentemester_application::sales::Action::Pdf(doc),
                "2026-01-01",
                "2026-01-01T00:00:00Z",
                None
            )
            .unwrap()
            .outcome
            .is_err()
    );
    {
        let control = owner.held.0.as_mut().unwrap().control.as_mut().unwrap();
        assert_eq!(control.query_row("SELECT count(*) FROM rm_workspace_document_access_events WHERE outcome='denied' AND reason_code='authorization_denied'",[],|r|r.get::<_,i64>(0)).unwrap(),2);
        control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u');INSERT INTO rm_workspace_document_access_events(id,actor,company_slug,resource_type,resource_id,outcome,reason_code)VALUES(9223372036854775807,'user:synthetic','a','document_file',1,'served','authorized');").unwrap();
    }
    // Exhaustion is an admitted-schema audit write failure, not a weakened physical/schema check.
    assert!(
        owner
            .document_original(&proof, &Fixed(NOW), doc)
            .unwrap()
            .outcome
            .is_err()
    );
    assert!(
        owner
            .sales(
                &proof,
                &Fixed(NOW),
                rentemester_application::sales::Action::Pdf(doc),
                "2026-01-01",
                "2026-01-01T00:00:00Z",
                None
            )
            .unwrap()
            .outcome
            .is_err()
    );
    assert_eq!(count(&owner, "journal_entries"), journal_before);
    let slots = owner.held.0.as_mut().unwrap();
    let db = slots.company.as_ref().unwrap();
    for (id, hash, path) in company_before {
        let current: (String, String) = db
            .query_row(
                "SELECT sha256_hash,stored_path FROM documents WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(current, (hash.clone(), path.clone()));
        let original = slots.guard.read_document(&path, &hash).unwrap();
        if id == doc {
            assert_eq!(original, bytes);
        }
    }
}
