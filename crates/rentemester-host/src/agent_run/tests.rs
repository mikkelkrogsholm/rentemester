use super::*;
use rentemester_runtime::auth::{AuthError, CookieProfile, CookieVerifier};
use serde_json::{Value, json};
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
fn policy(asset: bool, treatment: &str) -> Policy {
    serde_json::from_value(json!({"supplierRules":[{"token":"synthetic","expenseAccount":"X","vatTreatment":treatment,"label":"Supplies","assetLike":asset}]})).unwrap()
}
fn request() -> Request {
    serde_json::from_value(json!({"confirm":true,"asOf":"2026-01-01"})).unwrap()
}
fn chart(o: &mut Owner) {
    o.held.0.as_mut().unwrap().company.as_mut().unwrap().execute_batch("UPDATE companies SET address='Synthetic buyer street',cvr='87654321',vat_period_type='quarter' WHERE id=1;INSERT INTO accounts(account_no,name,type,normal_balance) VALUES('A','Synthetic bank','asset','debit'),('X','Synthetic expense','expense','debit'),('V','Synthetic VAT','vat','debit'),('L','Synthetic payable','liability','credit');INSERT INTO account_role_mappings(role,account_no,status,version,confirmed_by)VALUES('input_vat','V','confirmed',1,'synthetic'),('creditors','L','confirmed',1,'synthetic'),('bank','A','confirmed',1,'synthetic');INSERT INTO bank_accounts(slug,name,ledger_account_no)VALUES('synthetic','Synthetic bank','A');").unwrap();
}
fn intake(o: &mut Owner, c: &VerifiedCookie, gross: i64, invoice: &str) -> i64 {
    let m=serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale","issueDate":"2026-01-01","deliveryDescription":"Synthetic supplies","invoiceNo":invoice,"amountIncVat":gross,"vatAmount":gross/5,"sender":{"name":"Synthetic supplier","address":"Synthetic1","vatOrCvr":"DK12345678"},"recipient":{"name":"Synthetic buyer","address":"Synthetic2","vatOrCvr":"DK87654321"}})).unwrap();
    let f = rentemester_runtime::document::Prepared::new(
        format!("{invoice}.txt"),
        format!("Synthetic original {invoice}").into_bytes(),
    )
    .unwrap();
    let a = human(
        o.prepare_documents_original(
            c,
            &Fixed(NOW),
            f,
            m,
            rentemester_application::documents::ScanPolicy::Off,
            rentemester_application::documents::OriginalOptions {
                original_api: true,
                ..Default::default()
            },
        )
        .unwrap(),
    )
    .unwrap();
    let receipt = human(
        o.finish_document_ingest_registered(c, &Fixed(NOW), a, None)
            .unwrap(),
    )
    .unwrap()
    .receipt;
    receipt["document"]["id"]
        .as_i64()
        .or_else(|| receipt["id"].as_i64())
        .expect(&receipt.to_string())
}
fn bank(o: &mut Owner, c: &VerifiedCookie, gross: i64, text: &str) {
    let csv = format!("date;text;amount\n2026-01-01;{text};-{gross}\n");
    let v = human(
        o.bank(
            c,
            &Fixed(NOW),
            rentemester_application::bank::Action::ImportOriginal {
                content: csv.as_bytes(),
                account: Some("synthetic"),
                profile: None,
                now: "2026-01-01T00:00:00Z",
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(v["ok"], true, "{v}");
}
fn run(o: &mut Owner, c: &VerifiedCookie, p: &Policy) -> Report {
    let mut r = o.agent_run_start(c, &Fixed(NOW), &request(), p).unwrap();
    r.phases.push("ingest".into());
    o.agent_run_finish(c, &Fixed(NOW), p, r).unwrap()
}
fn journal(o: &Owner) -> (i64, String, String) {
    o.held.0.as_ref().unwrap().company.as_ref().unwrap().query_row("SELECT count(*),COALESCE((SELECT created_by FROM journal_entries ORDER BY id DESC LIMIT 1),''),COALESCE((SELECT created_by_program FROM journal_entries ORDER BY id DESC LIMIT 1),'') FROM journal_entries",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap()
}
#[test]
fn agent_run_actual_human_writer_full_phases_and_repeat() {
    let (_d, mut o, _cookies, c, _) = setup();
    chart(&mut o);
    intake(&mut o, &c, 125, "SYN-1");
    bank(&mut o, &c, 125, "Synthetic supplier SYN-1");
    let r = run(&mut o, &c, &policy(false, "standard"));
    assert_eq!(r.phases, rentemester_engine::agent_run::PHASES);
    assert_eq!(r.actor, "user:u");
    assert_eq!(r.company, "a");
    assert_eq!(r.expenses_booked.len(), 1, "{}", json!(r));
    assert!(r.expenses_booked[0].journal_entry_no.is_some());
    assert_eq!(
        journal(&o),
        (
            1,
            "user:u".into(),
            rentemester_engine::agent_run::PROGRAM.into()
        )
    );
    let repeated = run(&mut o, &c, &policy(false, "standard"));
    assert!(repeated.expenses_booked.is_empty());
    assert_eq!(journal(&o).0, 1);
}
#[test]
fn agent_run_no_rule_fixed_asset_and_vat_refusal_never_post() {
    for (gross, policy, kind) in [
        (125, Policy::default(), "AGENT_NO_ACCOUNT_RULE"),
        (5000, policy(true, "standard"), "AGENT_POSSIBLE_FIXED_ASSET"),
        (
            125,
            policy(false, "reverse_charge"),
            "AGENT_BOOKING_BLOCKED",
        ),
    ] {
        let (_d, mut o, _, c, _) = setup();
        chart(&mut o);
        intake(&mut o, &c, gross, "SYN-1");
        bank(&mut o, &c, gross, "Synthetic supplier SYN-1");
        let r = run(&mut o, &c, &policy);
        assert!(r.expenses_booked.is_empty());
        assert!(
            r.open_exceptions.iter().any(|e| e.kind == kind),
            "{}",
            json!(r)
        );
        assert_eq!(journal(&o).0, 0);
    }
}
#[test]
fn agent_run_revocation_and_reader_role_refuse_before_effect() {
    for reader in [false, true] {
        let (_d, mut o, _, c, _) = setup();
        chart(&mut o);
        intake(&mut o, &c, 125, "SYN-1");
        bank(&mut o, &c, 125, "Synthetic supplier SYN-1");
        let p = policy(false, "standard");
        let r = o.agent_run_start(&c, &Fixed(NOW), &request(), &p).unwrap();
        o.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch(if reader{"INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','user:u')"}else{"INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u')"}).unwrap();
        let f = o.agent_run_finish(&c, &Fixed(NOW), &p, r).err().unwrap();
        assert!(f.authority());
        assert_eq!(journal(&o).0, 0);
    }
}
#[test]
fn agent_run_service_actual_principal_and_revocation() {
    use rentemester_application::service_lifecycle::Operation;
    let (_d, mut o, cookies, c, _) = setup();
    chart(&mut o);
    intake(&mut o, &c, 125, "SYN-1");
    bank(&mut o, &c, 125, "Synthetic supplier SYN-1");
    let receipt = o
        .service_lifecycle(
            &cookies,
            &Fixed(NOW),
            &c,
            &Operation::Create {
                display_name: "Synthetic agent".into(),
                operation_id: None,
                expires_in_seconds: None,
            },
        )
        .unwrap()
        .outcome
        .unwrap();
    let v: Value = serde_json::from_str(receipt.as_json()).unwrap();
    let subject = v["serviceAccountId"].as_str().unwrap();
    let key = ServiceKey::new("w", v["secret"].as_str().unwrap()).unwrap();
    let ctl = o.held.0.as_mut().unwrap().control.as_mut().unwrap();
    ctl.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES(?1,'activate','member','user:u')",[subject]).unwrap();
    ctl.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','bookkeeper','user:u')",[subject]).unwrap();
    let p = policy(false, "standard");
    let r = o
        .agent_run_start_service(&key, &Fixed(NOW), &request(), &p)
        .unwrap();
    let r = o
        .agent_run_finish_service(&key, &Fixed(NOW), &p, r)
        .unwrap();
    assert_eq!(r.expenses_booked.len(), 1, "{}", json!(r));
    assert_eq!(r.actor, format!("service_account:{subject}"));
    assert_eq!(
        journal(&o),
        (
            1,
            r.actor.clone(),
            rentemester_engine::agent_run::PROGRAM.into()
        )
    );
    o.service_lifecycle(
        &cookies,
        &Fixed(NOW),
        &c,
        &Operation::Revoke {
            subject: subject.into(),
            credential: v["credentialId"].as_str().unwrap().into(),
            operation_id: None,
        },
    )
    .unwrap()
    .outcome
    .unwrap();
    assert!(
        o.agent_run_start_service(&key, &Fixed(NOW), &request(), &p)
            .is_err()
    );
    assert_eq!(journal(&o).0, 1);
}
#[test]
fn agent_run_unknown_and_authority_never_become_known_success() {
    let f = human::<()>(Attempt {
        retired: false,
        outcome: Err(rentemester_application::control_fence::Failure {
            stage: rentemester_application::control_fence::FailureStage::OutcomeUnknown,
            error: rentemester_engine::Error::new("write_failed", "uncertain"),
            control_reusable: true,
            company_reusable: true,
        }),
    })
    .unwrap_err();
    assert!(f.unknown);
}
#[test]
fn agent_run_admitted_payable_receipt_and_amount_only_refusal() {
    for corroborates in [true, false] {
        let (_d, mut o, _, c, _) = setup();
        chart(&mut o);
        let doc = intake(&mut o, &c, 250, "BILL-2");
        let i=serde_json::from_value(json!({"documentId":doc,"billDate":"2026-01-01","dueDate":"2026-01-01","expenseAccount":"X","vatTreatment":"standard","confirm":true})).unwrap();
        let registered = human(
            o.expense(
                &c,
                &Fixed(NOW),
                rentemester_application::expense::Action::Original(
                    rentemester_storage::payable_original::Action::Register(&i),
                ),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(registered["ok"], true, "{registered}");
        bank(
            &mut o,
            &c,
            250,
            if corroborates {
                "Synthetic supplier BILL-2"
            } else {
                "Owner unrelated transfer"
            },
        );
        let before = journal(&o).0;
        let r = run(&mut o, &c, &policy(false, "standard"));
        if corroborates {
            assert_eq!(r.payables_matched.len(), 1, "{}", json!(r));
            let journal_id: i64 = o
                .held
                .0
                .as_ref()
                .unwrap()
                .company
                .as_ref()
                .unwrap()
                .query_row("SELECT max(id) FROM journal_entries", [], |r| r.get(0))
                .unwrap();
            assert_eq!(
                r.payables_matched[0].journal_entry_no,
                Some(journal_id.to_string())
            );
            assert_eq!(journal(&o).0, before + 1);
            assert_eq!(journal(&o).2, rentemester_engine::agent_run::PROGRAM);
            assert!(
                run(&mut o, &c, &policy(false, "standard"))
                    .payables_matched
                    .is_empty()
            );
        } else {
            assert!(r.payables_matched.is_empty());
            assert_eq!(journal(&o).0, before);
            assert!(
                r.open_exceptions
                    .iter()
                    .any(|p| p.kind == "AGENT_PAYABLE_MATCH_UNCERTAIN")
            );
        }
    }
}
#[test]
fn agent_run_physical_original_tamper_refuses_financial_effect() {
    let (_d, mut o, _, c, _) = setup();
    chart(&mut o);
    let doc = intake(&mut o, &c, 125, "SYN-1");
    bank(&mut o, &c, 125, "Synthetic supplier SYN-1");
    let path: String = o
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .query_row(
            "SELECT stored_path FROM documents WHERE id=?1",
            [doc],
            |r| r.get(0),
        )
        .unwrap();
    std::fs::write(o.root.join("a").join(path), b"changed original").unwrap();
    let r = run(&mut o, &c, &policy(false, "standard"));
    assert!(r.expenses_booked.is_empty());
    assert_eq!(journal(&o).0, 0);
    assert!(
        r.open_exceptions
            .iter()
            .any(|p| p.kind == "AGENT_BOOKING_BLOCKED")
    );
}
#[test]
fn agent_run_bad_date_report_has_actual_actor_and_no_phases_or_effects() {
    let (_d, mut o, _, c, _) = setup();
    let mut input = request();
    input.as_of = "2026-02-30".into();
    let r = o
        .agent_run_start(&c, &Fixed(NOW), &input, &Policy::default())
        .unwrap();
    assert!(!r.ok);
    assert_eq!(r.actor, "user:u");
    assert!(r.phases.is_empty());
    assert_eq!(journal(&o).0, 0);
}
#[test]
fn agent_run_bank_import_uses_current_explicit_binding_and_retains_prior_intake() {
    let (_d, mut o, _, c, _) = setup();
    chart(&mut o);
    intake(&mut o, &c, 125, "SYN-1");
    let p = policy(false, "standard");
    let mut r = o.agent_run_start(&c, &Fixed(NOW), &request(), &p).unwrap();
    r.phases.push("ingest".into());
    r.documents_ingested = 1;
    let f = o
        .agent_run_import_bank(
            &c,
            &Fixed(NOW),
            "not a bank csv",
            "2026-01-01T00:00:00Z",
            &r,
        )
        .unwrap_err();
    assert!(!f.unknown);
    assert_eq!(f.code, "bank_import_invalid_csv");
    assert_eq!(
        o.held
            .0
            .as_ref()
            .unwrap()
            .company
            .as_ref()
            .unwrap()
            .query_row::<i64, _, _>("SELECT count(*) FROM documents", [], |r| r.get(0))
            .unwrap(),
        1
    );
    assert_eq!(journal(&o).0, 0);
    let v = o
        .agent_run_import_bank(
            &c,
            &Fixed(NOW),
            "date;text;amount\n2026-01-01;Synthetic supplier SYN-1;-125\n",
            "2026-01-01T00:00:00Z",
            &r,
        )
        .unwrap();
    assert_eq!(v["data"]["imported"], 1);
    r.bank_transactions_imported = 1;
    let r = o.agent_run_finish(&c, &Fixed(NOW), &p, r).unwrap();
    assert_eq!(r.expenses_booked.len(), 1);
    assert_eq!(r.documents_ingested, 1);
    assert_eq!(r.bank_transactions_imported, 1);
}
