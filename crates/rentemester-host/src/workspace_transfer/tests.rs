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
