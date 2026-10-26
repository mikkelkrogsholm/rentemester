//! Synthetic unit fixtures only. Browser acceptance uses normal verification/MFA.
use super::*;
use rentemester_runtime::auth::{AuthError, CookieProfile, CookieVerifier};
use rentemester_storage::workspace_transfer as store;
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
            json!({"name":format!("Synthetic buyer {}", tx.query_row("SELECT count(*) FROM customers", [], |r| r.get::<_, i64>(0)).unwrap()),"address":"Synthetic2","vatOrCvr":"DK12345678"}),
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
fn status(o: &mut Owner, c: &CookieVerifier, p: &VerifiedCookie) -> Value {
    o.workspace_transfer_status(c, &Fixed(NOW), p)
        .unwrap()
        .outcome
        .unwrap()
}
fn freeze(o: &mut Owner, c: &CookieVerifier, p: &VerifiedCookie) -> Value {
    let s = status(o, c, p);
    let i=serde_json::from_value(json!({"expectedStateHash":s["stateHash"],"destination":"Reviewed synthetic target","reason":"Explicit synthetic source migration","confirm":true})).unwrap();
    o.freeze_workspace(c, &Fixed(NOW), p, &i)
        .unwrap()
        .outcome
        .unwrap()
}
fn count(o: &Owner, t: &str) -> i64 {
    o.held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0))
        .unwrap()
}
#[test]
fn source_cancel_only_before_offer_and_business_fence_survives_reopen() {
    let (d, mut o, c, p, uid) = setup();
    let id = invoice(&mut o, &p);
    let before = count(&o, "documents");
    let create_service = rentemester_application::service_lifecycle::Operation::Create {
        display_name: "Synthetic transfer service".into(),
        operation_id: None,
        expires_in_seconds: None,
    };
    let issued = o
        .service_lifecycle(&c, &Fixed(NOW), &p, &create_service)
        .unwrap()
        .outcome
        .unwrap();
    let issued: Value = serde_json::from_str(issued.as_json()).unwrap();
    let subject = issued["serviceAccountId"].as_str().unwrap();
    let service = ServiceKey::new("w", issued["secret"].as_str().unwrap()).unwrap();
    let control = o.held.0.as_mut().unwrap().control.as_mut().unwrap();
    control.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES(?1,'activate','member','user:u')",[subject]).unwrap();
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','bookkeeper','user:u')",[subject]).unwrap();
    let s = freeze(&mut o, &c, &p);
    assert_eq!(s["mode"], "frozen");
    let member = install_target_identity(&mut o, &c);
    o.held.0.as_mut().unwrap().control.as_mut().unwrap().execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('target','activate','member','user:target')",[]).unwrap();
    let hidden = status(&mut o, &c, &member);
    assert_eq!(hidden["mode"], "frozen");
    assert_eq!(hidden["stateHash"], s["stateHash"]);
    assert!(hidden["destination"].is_null());
    assert!(hidden["challenge"].is_null());
    assert!(hidden["history"].as_array().unwrap().is_empty());

    let input = rentemester_storage::sales::Customer {
        ..serde_json::from_value(
            json!({"name":"Must not write","address":"Synthetic address","vatOrCvr":"DK12345678"}),
        )
        .unwrap()
    };
    let r = o.sales(
        &p,
        &Fixed(NOW),
        rentemester_application::sales::Action::Customers(&input),
        "2026-01-01",
        "2026-01-01T00:00:00Z",
        None,
    );
    assert!(r.is_err() || r.as_ref().unwrap().outcome.is_err());
    assert!(o.documents(&p, &Fixed(NOW)).unwrap().outcome.is_ok());
    assert!(
        o.current_session(&c, &Fixed(NOW), &p)
            .unwrap()
            .outcome
            .is_ok()
    );
    let post = || {
        serde_json::from_value(json!({"format":rentemester_application::journal_post::FORMAT,"expectedRuleVersion":"synthetic","request":{"transactionDate":"2026-01-01","text":"Synthetic guarded post","currency":"DKK","lines":[]}})).unwrap()
    };
    assert_eq!(
        o.post_human(
            &p,
            &Fixed(NOW),
            &rentemester_application::journal_post::Policy::default(),
            post()
        )
        .unwrap()
        .outcome
        .err()
        .unwrap()
        .fence
        .error
        .code,
        "workspace_transfer_fenced"
    );
    let reverse = || {
        serde_json::from_value(json!({"format":rentemester_application::journal_reversal::FORMAT,"expectedRuleVersion":"synthetic","request":{"entryId":"1","transactionDate":"2026-01-01","reason":"Synthetic guarded reversal"}})).unwrap()
    };
    assert_eq!(
        o.reverse_human(
            &p,
            &Fixed(NOW),
            &rentemester_application::journal_reversal::Policy::default(),
            reverse()
        )
        .unwrap()
        .outcome
        .err()
        .unwrap()
        .fence
        .error
        .code,
        "workspace_transfer_fenced"
    );
    let changes = o
        .held
        .0
        .as_ref()
        .unwrap()
        .control
        .as_ref()
        .unwrap()
        .total_changes();
    assert!(
        o.service_context(&Fixed(NOW), &service)
            .unwrap()
            .outcome
            .is_ok()
    );
    assert_eq!(
        o.held
            .0
            .as_ref()
            .unwrap()
            .control
            .as_ref()
            .unwrap()
            .total_changes(),
        changes
    );
    assert_eq!(
        o.post_service(&service, &Fixed(NOW), 45, post())
            .unwrap()
            .outcome
            .err()
            .unwrap()
            .error
            .code,
        "workspace_transfer_fenced"
    );
    assert_eq!(
        o.reverse_service(&service, &Fixed(NOW), 45, reverse())
            .unwrap()
            .outcome
            .err()
            .unwrap()
            .error
            .code,
        "workspace_transfer_fenced"
    );
    assert_eq!(
        o.dispatch_hosted_recurring(&service, &Fixed(NOW), 1)
            .unwrap()
            .outcome
            .err()
            .unwrap()
            .error
            .code,
        "workspace_transfer_fenced"
    );
    let create = rentemester_application::service_lifecycle::Operation::Create {
        display_name: "Must not create".into(),
        operation_id: None,
        expires_in_seconds: None,
    };
    assert_eq!(
        o.service_lifecycle(&c, &Fixed(NOW), &p, &create)
            .unwrap()
            .outcome
            .err()
            .unwrap()
            .error
            .code,
        "workspace_transfer_fenced"
    );
    let cancel=serde_json::from_value(json!({"expectedStateHash":s["stateHash"],"reason":"Abort before any archive was offered","confirm":true})).unwrap();
    assert_eq!(
        o.cancel_workspace_transfer(&c, &Fixed(NOW), &p, &cancel)
            .unwrap()
            .outcome
            .unwrap()["mode"],
        "active"
    );
    let s = freeze(&mut o, &c, &p);
    assert_eq!(count(&o, "documents"), before);
    assert_eq!(count(&o, "journal_entries"), 0);
    assert!(id > 0);
    o.try_close().unwrap();
    let mut o = Owner::open_with_profile(
        &d.path().canonicalize().unwrap().join("workspace"),
        uid,
        "w",
        "a",
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    assert_eq!(status(&mut o, &c, &p), s);
    assert!(
        store::require_business_write(o.held.0.as_ref().unwrap().control.as_ref().unwrap())
            .is_err()
    );
}
fn install_target_identity(o: &mut Owner, c: &CookieVerifier) -> VerifiedCookie {
    // Synthetic unit setup only; root actual qualification provisions a normal new MFA owner.
    o.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('target','Synthetic target','target@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('target-factor','synthetic','[]','target',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('target-session','target','target-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('target','activate','workspace_owner','user:target');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('target','a','grant','owner','user:target');").unwrap();
    let cookie = c.issue_session("target-token").unwrap();
    c.verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
        .unwrap()
}
#[test]
fn offered_snapshot_staged_nonce_signed_release_activation_restart_and_no_original_mutation() {
    let (d, mut source, c, p, uid) = setup();
    let id = invoice(&mut source, &p);
    struct NoReceipt {
        identity: rentemester_runtime::invoice_mail::SenderIdentity,
        calls: u32,
    }
    impl rentemester_runtime::invoice_mail::Transport for NoReceipt {
        fn identity(&self) -> &rentemester_runtime::invoice_mail::SenderIdentity {
            &self.identity
        }
        fn send(
            &mut self,
            _: &rentemester_runtime::invoice_mail::Message,
        ) -> rentemester_runtime::invoice_mail::Handoff {
            self.calls += 1;
            rentemester_runtime::invoice_mail::Handoff::Unknown
        }
    }
    let mut mail = NoReceipt {
        identity: rentemester_runtime::invoice_mail::SenderIdentity {
            host: "smtp.example.invalid".into(),
            port: 465,
            from_address: "sender@example.invalid".into(),
            from_name: None,
        },
        calls: 0,
    };
    let send = rentemester_storage::invoice_email::Send {
        invoice_document_id: id,
        to: "recipient@example.invalid".into(),
        kind: None,
        confirm: true,
    };
    source
        .send_invoice_email(&p, &Fixed(NOW), &send, &mut mail)
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(mail.calls, 1);
    let key = key();
    let trusted = TrustedPublicKey::from_bytes(&key.public_key_bytes()).unwrap();
    let frozen = freeze(&mut source, &c, &p);
    assert_eq!(frozen["pendingEffects"].as_array().unwrap().len(), 1);
    assert!(
        source
            .send_invoice_email(&p, &Fixed(NOW), &send, &mut mail)
            .unwrap()
            .outcome
            .is_err()
    );
    assert_eq!(mail.calls, 1);
    let input =
        serde_json::from_value(json!({"expectedStateHash":frozen["stateHash"],"confirm":true}))
            .unwrap();
    let bytes = source
        .export_workspace_transfer(&c, &Fixed(NOW), &p, &input, &key)
        .unwrap()
        .outcome
        .unwrap();
    let offered = status(&mut source, &c, &p);
    let hash = store::digest(&bytes);
    assert_eq!(offered["offeredArchiveHashes"][0], hash);
    let cancel=serde_json::from_value(json!({"expectedStateHash":offered["stateHash"],"reason":"Cannot abort after offering archive","confirm":true})).unwrap();
    assert!(
        source
            .cancel_workspace_transfer(&c, &Fixed(NOW), &p, &cancel)
            .unwrap()
            .outcome
            .is_err()
    );
    let artifact = rentemester_runtime::backup_artifact::verify(&bytes, &trusted).unwrap();
    let op = rentemester_runtime::bootstrap::Operator::from_policy(
        "w",
        "system:synthetic-transfer",
        &["system:synthetic-transfer".into()],
    )
    .unwrap();
    let root = d.path().canonicalize().unwrap();
    assert!(
        Owner::restore_new(
            &root.join("ordinary-refused"),
            uid,
            "w",
            "a",
            NamespaceProfile::LocalUserMacos,
            &artifact,
            &op
        )
        .is_err()
    );
    assert!(!root.join("ordinary-refused").exists());
    let target_path = root.join("target");
    let mut target = Owner::stage_transfer_new(
        &target_path,
        uid,
        "w",
        "a",
        NamespaceProfile::LocalUserMacos,
        &artifact,
        &op,
        &hash,
        "https://target.example.invalid",
    )
    .unwrap();
    let target_proof = install_target_identity(&mut target, &c);
    let staged = status(&mut target, &c, &target_proof);
    assert_eq!(staged["mode"], "staged");
    assert_eq!(staged["pendingEffects"], frozen["pendingEffects"]);
    assert!(
        target
            .current_session(&c, &Fixed(NOW), &p)
            .unwrap()
            .outcome
            .is_err()
    );
    let mut other = Owner::stage_transfer_new(
        &root.join("other-target"),
        uid,
        "w",
        "a",
        NamespaceProfile::LocalUserMacos,
        &artifact,
        &op,
        &hash,
        "https://target.example.invalid",
    )
    .unwrap();
    let other_proof = install_target_identity(&mut other, &c);
    let other_staged = status(&mut other, &c, &other_proof);
    assert_ne!(
        staged["challenge"]["destinationInstanceId"],
        other_staged["challenge"]["destinationInstanceId"]
    );
    let release:store::Release=serde_json::from_value(json!({"expectedStateHash":offered["stateHash"],"challenge":staged["challenge"],"reason":"Independently reviewed exact synthetic target identity","confirm":true})).unwrap();
    let moved = source
        .release_workspace_transfer(&c, &Fixed(NOW), &p, &release, &key)
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(moved["mode"], "moved");
    assert_eq!(
        source
            .release_workspace_transfer(&c, &Fixed(NOW), &p, &release, &key)
            .unwrap()
            .outcome
            .unwrap(),
        moved
    );
    let activate:store::Activate=serde_json::from_value(json!({"expectedStateHash":staged["stateHash"],"release":moved["release"],"reason":"Reviewed trusted source release before activation","confirm":true})).unwrap();
    let wrong:store::Activate=serde_json::from_value(json!({"expectedStateHash":other_staged["stateHash"],"release":moved["release"],"reason":"Wrong cloned staged destination must refuse","confirm":true})).unwrap();
    assert!(
        other
            .activate_workspace_transfer(&c, &Fixed(NOW), &other_proof, &wrong, &trusted)
            .unwrap()
            .outcome
            .is_err()
    );
    let wrong_key = TrustedPublicKey::from_bytes(&[7u8; 32]).unwrap();
    assert!(
        target
            .activate_workspace_transfer(&c, &Fixed(NOW), &target_proof, &activate, &wrong_key)
            .unwrap()
            .outcome
            .is_err()
    );
    assert_eq!(status(&mut target, &c, &target_proof), staged);
    target.try_close().unwrap();
    let mut target = Owner::open_with_profile(
        &target_path,
        uid,
        "w",
        "a",
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    assert_eq!(status(&mut target, &c, &target_proof), staged);
    let active = target
        .activate_workspace_transfer(&c, &Fixed(NOW), &target_proof, &activate, &trusted)
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(active["mode"], "active");
    assert_eq!(active["pendingEffects"], frozen["pendingEffects"]);
    assert_eq!(mail.calls, 1);
    assert_eq!(
        target
            .activate_workspace_transfer(&c, &Fixed(NOW), &target_proof, &activate, &trusted)
            .unwrap()
            .outcome
            .unwrap(),
        active
    );
    assert_eq!(count(&target, "documents"), 2);
    assert_eq!(count(&target, "journal_entries"), 0);
    target.try_close().unwrap();
    let mut target = Owner::open_with_profile(
        &target_path,
        uid,
        "w",
        "a",
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    assert_eq!(status(&mut target, &c, &target_proof), active);
    let second_hop = freeze(&mut target, &c, &target_proof);
    assert_eq!(second_hop["mode"], "frozen");
    assert_ne!(second_hop["transferId"], active["transferId"]);
    assert!(second_hop["challenge"].is_null());
    assert!(second_hop["release"].is_null());
    assert!(
        second_hop["offeredArchiveHashes"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        second_hop["history"].as_array().unwrap().len(),
        active["history"].as_array().unwrap().len() + 1
    );
    assert_eq!(count(&target, "journal_entries"), 0);
    source.try_close().unwrap();
    let mut source = Owner::open_with_profile(
        &root.join("workspace"),
        uid,
        "w",
        "a",
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    assert_eq!(status(&mut source, &c, &p), moved);
    assert!(
        store::require_business_write(source.held.0.as_ref().unwrap().control.as_ref().unwrap())
            .is_err()
    );
}

fn scheduled_value(
    o: &mut Owner,
    p: &VerifiedCookie,
    action: rentemester_application::scheduled_recurring::Action<'_>,
) -> Value {
    o.scheduled_recurring(
        p,
        &Fixed(NOW),
        action,
        "2026-01-01",
        "2026-01-01T00:00:00.000Z",
    )
    .unwrap()
    .outcome
    .unwrap()
    .value
}
fn scheduled_current(o: &Owner, id: i64) -> Value {
    rentemester_storage::scheduled_recurring::current(
        o.held.0.as_ref().unwrap().company.as_ref().unwrap(),
        id,
        "2026-01-01",
    )
    .unwrap()
}
fn scheduled_dispatch(
    o: &mut Owner,
    p: &VerifiedCookie,
    id: i64,
    index: u32,
) -> rentemester_engine::scheduled_recurring::Dispatch {
    use rentemester_application::scheduled_recurring::Action;
    let plan = scheduled_value(o, p, Action::Plan(id));
    serde_json::from_value(json!({"templateId":id,"planHash":plan["plan"]["planHash"],"periodIndex":index,"confirm":true})).unwrap()
}
fn generation_history(o: &Owner) -> Vec<String> {
    let db = o.held.0.as_ref().unwrap().company.as_ref().unwrap();
    let mut query = db.prepare("SELECT json_array(id,template_id,period_index,document_id,invoice_number,issue_date,delivery_period_start,delivery_period_end,created_at) FROM recurring_invoice_generations ORDER BY id").unwrap();
    query
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}
fn assert_transfer_job_fence(o: &mut Owner, p: &VerifiedCookie, id: i64) {
    use rentemester_application::scheduled_recurring::Action;
    let before = scheduled_current(o, id);
    let generations = count(o, "recurring_invoice_generations");
    let documents = count(o, "documents");
    let dispatch = scheduled_dispatch(o, p, id, 0);
    let failure = o
        .scheduled_recurring(
            p,
            &Fixed(NOW),
            Action::Dispatch(&dispatch),
            "2026-01-01",
            "2026-01-01T00:00:00.000Z",
        )
        .unwrap()
        .outcome
        .err()
        .unwrap();
    assert_eq!(failure.error.code, "workspace_transfer_fenced");
    let customer = serde_json::from_value(
        json!({"name":"Fenced synthetic buyer","address":"Synthetic3","vatOrCvr":"DK12345678"}),
    )
    .unwrap();
    assert!(
        o.sales(
            p,
            &Fixed(NOW),
            rentemester_application::sales::Action::Customers(&customer),
            "2026-01-01",
            "2026-01-01T00:00:00.000Z",
            None
        )
        .unwrap()
        .outcome
        .is_err()
    );
    assert_eq!(scheduled_current(o, id), before);
    assert_eq!(count(o, "recurring_invoice_generations"), generations);
    assert_eq!(count(o, "documents"), documents);
}

#[test]
fn nonempty_jobs_handoff_reconsent_and_target_only_journal_source_unavailable_recovery() {
    use rentemester_application::scheduled_recurring::Action;
    use rentemester_storage::scheduled_recurring as scheduled;
    let (d, mut source, c, p, uid) = setup();
    let mut ids = Vec::new();
    for _ in 0..3 {
        let id = template(&mut source, &p, "2025-12-18");
        let plan = scheduled_value(&mut source, &p, Action::Plan(id));
        let enable = serde_json::from_value(
            json!({"templateId":id,"templateHash":plan["plan"]["templateHash"],"confirm":true}),
        )
        .unwrap();
        scheduled_value(&mut source, &p, Action::Enable(&enable));
        let plan = scheduled_value(&mut source, &p, Action::Plan(id));
        let select = serde_json::from_value(json!({"templateId":id,"planHash":plan["plan"]["planHash"],"periodIndexes":[0],"confirm":true})).unwrap();
        scheduled_value(&mut source, &p, Action::CatchUp(&select));
        ids.push(id);
    }
    let (queued, completed, unresolved) = (ids[0], ids[1], ids[2]);
    let dispatch = scheduled_dispatch(&mut source, &p, completed, 0);
    let done = scheduled_value(&mut source, &p, Action::Dispatch(&dispatch));
    assert_eq!(done["dispatch"]["status"], "succeeded");
    // Typed fixture seam models an attempt whose outcome was not recorded; no forged events/provider.
    let dispatch = scheduled_dispatch(&mut source, &p, unresolved, 0);
    let db = source.held.0.as_mut().unwrap().company.as_mut().unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    scheduled::begin(
        &tx,
        &dispatch,
        "u",
        "2026-01-01",
        "2026-01-01T00:00:00.000Z",
    )
    .unwrap();
    tx.commit().unwrap();
    let original_generations = generation_history(&source);
    let original: Vec<_> = ids
        .iter()
        .map(|id| scheduled_current(&source, *id))
        .collect();
    assert_eq!(original[0]["queued"].as_array().unwrap().len(), 1);
    assert_eq!(original[2]["error"], "unresolved_attempt");
    assert_eq!(count(&source, "recurring_invoice_generations"), 1);
    let key = key();
    let trusted = TrustedPublicKey::from_bytes(&key.public_key_bytes()).unwrap();
    let frozen = freeze(&mut source, &c, &p);
    assert_transfer_job_fence(&mut source, &p, queued);
    let input =
        serde_json::from_value(json!({"expectedStateHash":frozen["stateHash"],"confirm":true}))
            .unwrap();
    let wire = source
        .export_workspace_transfer(&c, &Fixed(NOW), &p, &input, &key)
        .unwrap()
        .outcome
        .unwrap();
    let artifact = rentemester_runtime::backup_artifact::verify(&wire, &trusted).unwrap();
    Owner::validate_backup(&artifact).unwrap();
    let offered = status(&mut source, &c, &p);
    let root = d.path().canonicalize().unwrap();
    let target_path = root.join("jobs-target");
    let op = rentemester_runtime::bootstrap::Operator::from_policy(
        "w",
        "system:synthetic-transfer",
        &["system:synthetic-transfer".into()],
    )
    .unwrap();
    let mut target = Owner::stage_transfer_new(
        &target_path,
        uid,
        "w",
        "a",
        NamespaceProfile::LocalUserMacos,
        &artifact,
        &op,
        &store::digest(&wire),
        "https://jobs.example.invalid",
    )
    .unwrap();
    let tp = install_target_identity(&mut target, &c);
    let staged = status(&mut target, &c, &tp);
    assert_eq!(generation_history(&target), original_generations);
    let restored: Vec<_> = ids
        .iter()
        .map(|id| scheduled_current(&target, *id))
        .collect();
    for (before, after) in original.iter().zip(&restored) {
        assert_eq!(after["history"], before["history"]);
        assert_eq!(after["restoreConsent"]["required"], true);
        assert!(after["ownerUserId"].is_null());
    }
    assert_transfer_job_fence(&mut target, &tp, queued);
    let release = serde_json::from_value(json!({"expectedStateHash":offered["stateHash"],"challenge":staged["challenge"],"reason":"Reviewed exact synthetic job target","confirm":true})).unwrap();
    let moved = source
        .release_workspace_transfer(&c, &Fixed(NOW), &p, &release, &key)
        .unwrap()
        .outcome
        .unwrap();
    assert_transfer_job_fence(&mut source, &p, queued);
    assert_eq!(status(&mut source, &c, &p), moved);
    source.try_close().unwrap();
    // Source stays moved and unavailable throughout target activity/recovery.
    let activate = serde_json::from_value(json!({"expectedStateHash":staged["stateHash"],"release":moved["release"],"reason":"Reviewed source release for jobs","confirm":true})).unwrap();
    target
        .activate_workspace_transfer(&c, &Fixed(NOW), &tp, &activate, &trusted)
        .unwrap()
        .outcome
        .unwrap();
    target.try_close().unwrap();
    let mut target = Owner::open_with_profile(
        &target_path,
        uid,
        "w",
        "a",
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    assert_eq!(generation_history(&target), original_generations);
    for (id, before) in ids.iter().zip(&restored) {
        assert_eq!(&scheduled_current(&target, *id), before);
        let dispatch = scheduled_dispatch(&mut target, &tp, *id, 0);
        assert!(
            target
                .scheduled_recurring(
                    &tp,
                    &Fixed(NOW),
                    Action::Dispatch(&dispatch),
                    "2026-01-01",
                    "2026-01-01T00:00:00.000Z"
                )
                .unwrap()
                .outcome
                .is_err()
        );
    }
    for id in [queued, completed, unresolved] {
        let before = scheduled_current(&target, id);
        let plan = scheduled_value(&mut target, &tp, Action::Plan(id));
        let consent = serde_json::from_value(json!({"templateId":id,"planHash":plan["plan"]["planHash"],"reason":"Fresh synthetic target owner review","confirm":true})).unwrap();
        let result = target
            .scheduled_recurring(
                &tp,
                &Fixed(NOW),
                Action::Reconsent(&consent),
                "2026-01-01",
                "2026-01-01T00:00:00.000Z",
            )
            .unwrap()
            .outcome;
        if id == unresolved {
            assert!(result.is_err());
            assert_eq!(scheduled_current(&target, id), before);
        } else {
            result.unwrap();
            assert_eq!(scheduled_current(&target, id)["ownerUserId"], "target");
            assert_eq!(
                scheduled_current(&target, id)["restoreConsent"]["required"],
                false
            );
        }
    }
    let dispatch = scheduled_dispatch(&mut target, &tp, completed, 0);
    assert!(
        target
            .scheduled_recurring(
                &tp,
                &Fixed(NOW),
                Action::Dispatch(&dispatch),
                "2026-01-01",
                "2026-01-01T00:00:00.000Z"
            )
            .unwrap()
            .outcome
            .is_err()
    );
    assert_eq!(count(&target, "recurring_invoice_generations"), 1);
    assert_eq!(generation_history(&target), original_generations);
    // Restore consent does not silently reinstate the historical queued selection.
    let plan = scheduled_value(&mut target, &tp, Action::Plan(queued));
    let select = serde_json::from_value(json!({"templateId":queued,"planHash":plan["plan"]["planHash"],"periodIndexes":[0],"confirm":true})).unwrap();
    scheduled_value(&mut target, &tp, Action::CatchUp(&select));
    let dispatch = scheduled_dispatch(&mut target, &tp, queued, 0);
    assert_eq!(
        scheduled_value(&mut target, &tp, Action::Dispatch(&dispatch))["dispatch"]["status"],
        "succeeded"
    );
    assert_eq!(count(&target, "recurring_invoice_generations"), 2);

    // An accepted balanced post exists only on the current target, never on the moved source.
    target.held.0.as_mut().unwrap().company.as_mut().unwrap().execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('1800','Synthetic recovery bank','asset','debit'),('8000','Synthetic recovery equity','equity','credit')").unwrap();
    let post = serde_json::from_value(json!({"format":rentemester_application::journal_post::FORMAT,"expectedRuleVersion":rentemester_storage::journal_preview::manual_journal_rule_version().unwrap(),"request":{"transactionDate":"2026-01-01","text":"Synthetic target-only accepted journal","currency":"DKK","lines":[{"accountNo":"1800","debitAmount":"12.34"},{"accountNo":"8000","creditAmount":"12.34"}]}})).unwrap();
    let accepted = target
        .post_human(
            &tp,
            &Fixed(NOW),
            &rentemester_application::journal_post::Policy::default(),
            post,
        )
        .unwrap()
        .outcome
        .unwrap()
        .value
        .result;
    assert_eq!(count(&target, "journal_entries"), 1);
    let history: Vec<_> = ids
        .iter()
        .map(|id| scheduled_current(&target, *id)["history"].clone())
        .collect();
    let target_generations = generation_history(&target);
    let target_journal = target
        .journal_history(&tp, &Fixed(NOW), Default::default())
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let wire = target
        .backup_workspace(&c, &Fixed(NOW), &tp, &key)
        .unwrap()
        .outcome
        .unwrap();
    let fresh = rentemester_runtime::backup_artifact::verify(&wire, &trusted).unwrap();
    Owner::validate_backup(&fresh).unwrap();
    target.try_close().unwrap();
    let recovery_path = root.join("jobs-source-unavailable-recovery");
    let mut recovery = Owner::restore_new(
        &recovery_path,
        uid,
        "w",
        "a",
        NamespaceProfile::LocalUserMacos,
        &fresh,
        &op,
    )
    .unwrap();
    assert_eq!(count(&recovery, "journal_entries"), 1);
    assert_eq!(count(&recovery, "recurring_invoice_generations"), 2);
    assert_eq!(generation_history(&recovery), target_generations);
    let rp = install_target_identity(&mut recovery, &c);
    for (id, history) in ids.iter().zip(&history) {
        let current = scheduled_current(&recovery, *id);
        assert_eq!(&current["history"], history);
        assert_eq!(current["restoreConsent"]["required"], true);
    }
    // Compare the accepted entry through the ordinary journal read model, not only its count.
    let journal = recovery
        .journal_history(&rp, &Fixed(NOW), Default::default())
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert!(
        journal
            .to_string()
            .contains("Synthetic target-only accepted journal")
    );
    assert_eq!(journal, target_journal);
    assert_eq!(accepted["ok"], true);
    recovery.try_close().unwrap();
    let recovery = Owner::open_with_profile(
        &recovery_path,
        uid,
        "w",
        "a",
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    assert_eq!(count(&recovery, "journal_entries"), 1);
    assert_eq!(
        scheduled_current(&recovery, unresolved)["error"],
        "unresolved_attempt"
    );
}

// Same bounded child-process pattern as company_baseline's process-death tests.
// These barriers observe before the operation and after its durable commit;
// they do not claim to inject inside SQLite's commit implementation.
#[test]
#[ignore = "activation process helper invoked only by its parent test"]
fn activation_process_child() {
    let phase = std::env::var("RM_TRANSFER_ACTIVATION_PHASE").unwrap();
    let path = std::path::PathBuf::from(std::env::var("RM_TRANSFER_ACTIVATION_ROOT").unwrap());
    let uid = std::env::var("RM_TRANSFER_ACTIVATION_UID")
        .unwrap()
        .parse()
        .unwrap();
    let mut target =
        Owner::open_with_profile(&path, uid, "w", "a", NamespaceProfile::LocalUserMacos).unwrap();
    let cookies = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let cookie = cookies.issue_session("target-token").unwrap();
    let proof = cookies
        .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    assert_eq!(status(&mut target, &cookies, &proof)["mode"], "staged");
    if phase == "after" {
        let input = serde_json::from_slice(
            &std::fs::read(std::env::var("RM_TRANSFER_ACTIVATION_INPUT").unwrap()).unwrap(),
        )
        .unwrap();
        let trusted = TrustedPublicKey::from_bytes(&key().public_key_bytes()).unwrap();
        let activated = target
            .activate_workspace_transfer(&cookies, &Fixed(NOW), &proof, &input, &trusted)
            .unwrap()
            .outcome
            .unwrap();
        assert_eq!(activated["mode"], "active");
    } else {
        assert_eq!(phase, "before");
    }
    let observed = status(&mut target, &cookies, &proof);
    std::fs::write(
        std::env::var("RM_TRANSFER_ACTIVATION_MARKER").unwrap(),
        serde_json::to_vec(&observed).unwrap(),
    )
    .unwrap();
    loop {
        std::thread::park();
    }
}

#[test]
fn activation_process_death_reopens_known_authority_without_old_source_writer() {
    use std::os::unix::process::ExitStatusExt;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    struct Reaped(std::process::Child);
    impl Drop for Reaped {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    for phase in ["before", "after"] {
        let (d, mut source, cookies, proof, uid) = setup();
        let key = key();
        let trusted = TrustedPublicKey::from_bytes(&key.public_key_bytes()).unwrap();
        let frozen = freeze(&mut source, &cookies, &proof);
        let export =
            serde_json::from_value(json!({"expectedStateHash":frozen["stateHash"],"confirm":true}))
                .unwrap();
        let wire = source
            .export_workspace_transfer(&cookies, &Fixed(NOW), &proof, &export, &key)
            .unwrap()
            .outcome
            .unwrap();
        let artifact = rentemester_runtime::backup_artifact::verify(&wire, &trusted).unwrap();
        let offered = status(&mut source, &cookies, &proof);
        let root = d.path().canonicalize().unwrap();
        let target_path = root.join("activation-target");
        let op = rentemester_runtime::bootstrap::Operator::from_policy(
            "w",
            "system:synthetic-transfer",
            &["system:synthetic-transfer".into()],
        )
        .unwrap();
        let mut target = Owner::stage_transfer_new(
            &target_path,
            uid,
            "w",
            "a",
            NamespaceProfile::LocalUserMacos,
            &artifact,
            &op,
            &store::digest(&wire),
            "https://activation.example.invalid",
        )
        .unwrap();
        let target_proof = install_target_identity(&mut target, &cookies);
        let staged = status(&mut target, &cookies, &target_proof);
        let release = serde_json::from_value(json!({"expectedStateHash":offered["stateHash"],"challenge":staged["challenge"],"reason":"Reviewed synthetic activation-crash target","confirm":true})).unwrap();
        let moved = source
            .release_workspace_transfer(&cookies, &Fixed(NOW), &proof, &release, &key)
            .unwrap()
            .outcome
            .unwrap();
        source.try_close().unwrap();
        target.try_close().unwrap();
        let input = json!({"expectedStateHash":staged["stateHash"],"release":moved["release"],"reason":"Reviewed source release after activation interruption","confirm":true});
        let input_path = root.join("activation-input.json");
        std::fs::write(&input_path, serde_json::to_vec(&input).unwrap()).unwrap();
        let marker = root.join("activation.ready");
        let log = std::fs::File::create(root.join("activation-child.log")).unwrap();
        let mut child = Reaped(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "workspace_transfer::tests::activation_process_child",
                    "--ignored",
                    "--nocapture",
                ])
                .env("RM_TRANSFER_ACTIVATION_PHASE", phase)
                .env("RM_TRANSFER_ACTIVATION_ROOT", &target_path)
                .env("RM_TRANSFER_ACTIVATION_UID", uid.to_string())
                .env("RM_TRANSFER_ACTIVATION_INPUT", &input_path)
                .env("RM_TRANSFER_ACTIVATION_MARKER", &marker)
                .stdout(Stdio::null())
                .stderr(Stdio::from(log))
                .spawn()
                .unwrap(),
        );
        let started = Instant::now();
        while !marker.exists() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "activation child stopped before checkpoint"
            );
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "bounded activation checkpoint deadline"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        let observed: Value = serde_json::from_slice(&std::fs::read(&marker).unwrap()).unwrap();
        let expected_mode = if phase == "before" {
            "staged"
        } else {
            "active"
        };
        assert_eq!(observed["mode"], expected_mode);
        child.0.kill().unwrap();
        assert_eq!(child.0.wait().unwrap().signal(), Some(9));
        let recover = Instant::now();
        let mut target = Owner::open_with_profile(
            &target_path,
            uid,
            "w",
            "a",
            NamespaceProfile::LocalUserMacos,
        )
        .unwrap();
        assert_eq!(status(&mut target, &cookies, &target_proof), observed);
        let recovered_ms = recover.elapsed().as_millis();
        assert_eq!(count(&target, "journal_entries"), 0);
        let mut source = Owner::open_with_profile(
            &root.join("workspace"),
            uid,
            "w",
            "a",
            NamespaceProfile::LocalUserMacos,
        )
        .unwrap();
        assert_eq!(status(&mut source, &cookies, &proof), moved);
        assert!(
            store::require_business_write(
                source.held.0.as_ref().unwrap().control.as_ref().unwrap()
            )
            .is_err()
        );
        // A pre-operation crash needs the original reviewed activation once.
        // An accepted operation with a lost process/reply is read back, never blindly repeated.
        if phase == "before" {
            assert!(
                store::require_business_write(
                    target.held.0.as_ref().unwrap().control.as_ref().unwrap()
                )
                .is_err()
            );
            let activate = serde_json::from_value(input).unwrap();
            target
                .activate_workspace_transfer(
                    &cookies,
                    &Fixed(NOW),
                    &target_proof,
                    &activate,
                    &trusted,
                )
                .unwrap()
                .outcome
                .unwrap();
        }
        assert_eq!(
            status(&mut target, &cookies, &target_proof)["mode"],
            "active"
        );
        assert_eq!(count(&source, "journal_entries"), 0);
        eprintln!(
            "activation_process_recovery phase={phase} observed={expected_mode} recovery_ms={recovered_ms} journal_entries_lost=0 source_mode=moved"
        );
        source.try_close().unwrap();
        target.try_close().unwrap();
    }
}

#[test]
#[ignore = "explicit original726 raw pre-freeze copy qualification; never infer from staged nonce tests"]
fn pre_freeze_raw_copy_cannot_admit_a_second_writer_after_original_moved() {
    fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
        std::fs::create_dir(to).unwrap();
        std::fs::set_permissions(to, std::fs::metadata(from).unwrap().permissions()).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            assert!(
                !kind.is_symlink(),
                "synthetic clone contains no symlink substitution"
            );
            let target = to.join(entry.file_name());
            if kind.is_dir() {
                copy_tree(&entry.path(), &target);
            } else {
                assert!(kind.is_file());
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }
    let (d, source, cookies, proof, uid) = setup();
    let root = d.path().canonicalize().unwrap();
    let source_path = root.join("workspace");
    let clone_path = root.join("pre-freeze-copy");
    source.try_close().unwrap();
    copy_tree(&source_path, &clone_path); // Complete stopped, current-schema copy; no forged data.
    let mut source = Owner::open_with_profile(
        &source_path,
        uid,
        "w",
        "a",
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    let frozen = freeze(&mut source, &cookies, &proof);
    let key = key();
    let trusted = TrustedPublicKey::from_bytes(&key.public_key_bytes()).unwrap();
    let export =
        serde_json::from_value(json!({"expectedStateHash":frozen["stateHash"],"confirm":true}))
            .unwrap();
    let wire = source
        .export_workspace_transfer(&cookies, &Fixed(NOW), &proof, &export, &key)
        .unwrap()
        .outcome
        .unwrap();
    let artifact = rentemester_runtime::backup_artifact::verify(&wire, &trusted).unwrap();
    let offered = status(&mut source, &cookies, &proof);
    let op = rentemester_runtime::bootstrap::Operator::from_policy(
        "w",
        "system:synthetic-transfer",
        &["system:synthetic-transfer".into()],
    )
    .unwrap();
    let mut target = Owner::stage_transfer_new(
        &root.join("clone-probe-target"),
        uid,
        "w",
        "a",
        NamespaceProfile::LocalUserMacos,
        &artifact,
        &op,
        &store::digest(&wire),
        "https://clone-probe.example.invalid",
    )
    .unwrap();
    let target_proof = install_target_identity(&mut target, &cookies);
    let staged = status(&mut target, &cookies, &target_proof);
    let release = serde_json::from_value(json!({"expectedStateHash":offered["stateHash"],"challenge":staged["challenge"],"reason":"Reviewed raw-copy refusal qualification target","confirm":true})).unwrap();
    let moved = source
        .release_workspace_transfer(&cookies, &Fixed(NOW), &proof, &release, &key)
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(moved["mode"], "moved");
    let activate = serde_json::from_value(json!({"expectedStateHash":staged["stateHash"],"release":moved["release"],"reason":"Reviewed sole-authority target activation","confirm":true})).unwrap();
    target
        .activate_workspace_transfer(&cookies, &Fixed(NOW), &target_proof, &activate, &trusted)
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(
        status(&mut target, &cookies, &target_proof)["mode"],
        "active"
    );
    assert!(
        store::require_business_write(source.held.0.as_ref().unwrap().control.as_ref().unwrap())
            .is_err()
    );
    match Owner::open_with_profile(&clone_path, uid, "w", "a", NamespaceProfile::LocalUserMacos) {
        Err(reason) => eprintln!("pre_freeze_copy_probe admission_refused={reason:?}"),
        Ok(mut clone) => {
            let mode = status(&mut clone, &cookies, &proof)["mode"].clone();
            let writable = store::require_business_write(
                clone.held.0.as_ref().unwrap().control.as_ref().unwrap(),
            )
            .is_ok();
            eprintln!(
                "pre_freeze_copy_probe original=moved target=active clone_mode={mode} clone_write_gate={writable} financial_effects=0"
            );
            clone.try_close().unwrap();
            source.try_close().unwrap();
            target.try_close().unwrap();
            if writable {
                let kept = d.keep();
                eprintln!(
                    "pre_freeze_copy_probe preserved_synthetic_fixture={}",
                    kept.display()
                );
                panic!(
                    "Original726 refuses a second writer: ordinary pre-freeze copy retained active write admission after source moved"
                );
            }
        }
    }
}
