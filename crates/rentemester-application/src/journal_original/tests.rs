use super::*;
use rentemester_runtime::{
    auth::{AuthError, CookieProfile, CookieVerifier},
    service_key::ServiceKey,
};
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1767225600000)
    }
}
fn selection() -> CompanySelection<'static> {
    CompanySelection {
        workspace: "w",
        slug: "a",
        registered: true,
        archived: false,
    }
}
fn action() -> Action {
    parse("dry-run",&serde_json::to_vec(&json!({"input":{"transactionDate":"2026-01-01","text":"Synthetic capital","lines":[{"accountNo":"2000","debitAmount":10},{"accountNo":"5000","creditAmount":10}]}})).unwrap()).unwrap()
}
fn fixture() -> (
    tempfile::TempDir,
    Connection,
    Connection,
    CookieVerifier,
    VerifiedCookie,
) {
    let dir = tempfile::tempdir().unwrap();
    let mut control = Connection::open(dir.path().join("control.sqlite")).unwrap();
    let mut company = Connection::open(dir.path().join("company.sqlite")).unwrap();
    rentemester_storage::fresh::initialize_empty_pair(
        &mut control,
        &mut company,
        "Synthetic original journal",
    )
    .unwrap();
    control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u');").unwrap();
    company.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('2000','Synthetic bank','asset','debit'),('5000','Synthetic equity','equity','credit')").unwrap();
    let cookies = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let issued = cookies.issue_session("synthetic-token").unwrap();
    let proof = cookies
        .verify(Some(issued.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    (dir, control, company, cookies, proof)
}
fn no_journals(company: &Connection) {
    for table in ["journal_entries", "journal_lines", "audit_log", "sequences"] {
        let count: i64 = company
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "{table}");
    }
}
#[test]
fn original_human_current_grant_revocation_and_no_identity_claims() {
    let (_dir, mut control, mut company, _cookies, proof) = fixture();
    // A workspace owner is not required: native current company membership suffices.
    control.execute_batch("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','bookkeeper','user:u')").unwrap();
    let result = human(
        &mut control,
        &mut company,
        &proof,
        &Fixed,
        selection(),
        action(),
    )
    .unwrap();
    assert_eq!(result.value["ok"], true);
    no_journals(&company);
    control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u')").unwrap();
    let denied = human(
        &mut control,
        &mut company,
        &proof,
        &Fixed,
        selection(),
        action(),
    )
    .unwrap_err();
    assert_eq!(denied.fence.error.code, "access_denied");
    no_journals(&company);
    control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','bookkeeper','user:u');DELETE FROM session WHERE id='s'").unwrap();
    assert!(
        human(
            &mut control,
            &mut company,
            &proof,
            &Fixed,
            selection(),
            action()
        )
        .is_err()
    );
    no_journals(&company);
}
#[test]
fn original_service_native_credential_grant_and_revocation() {
    let (_dir, mut control, mut company, cookies, proof) = fixture();
    let receipt = crate::service_lifecycle::execute(
        &mut control,
        &cookies,
        &Fixed,
        &proof,
        &crate::service_lifecycle::Operation::Create {
            display_name: "Synthetic original journal service".into(),
            operation_id: None,
            expires_in_seconds: None,
        },
    )
    .unwrap();
    let issued: Value = serde_json::from_str(receipt.as_json()).unwrap();
    let subject = issued["serviceAccountId"].as_str().unwrap();
    let credential = issued["credentialId"].as_str().unwrap();
    let key = ServiceKey::new("w", issued["secret"].as_str().unwrap()).unwrap();
    control.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES(?1,'activate','member','user:u')",[subject]).unwrap();
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','bookkeeper','user:u')",[subject]).unwrap();
    control
        .execute("UPDATE apikey SET remaining=30 WHERE id=?1", [credential])
        .unwrap();
    let context = || service_company::Context {
        key: &key,
        clock: &Fixed,
        selection: selection(),
        max_future_days: 45,
    };
    let result = service(&mut control, &mut company, context(), action()).unwrap();
    assert_eq!(result.result["ok"], true);
    assert!(result.receipt.is_none());
    no_journals(&company);
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','revoke',NULL,'user:u')",[subject]).unwrap();
    let denied = service(&mut control, &mut company, context(), action())
        .err()
        .unwrap();
    assert_eq!(denied.error.code, "access_denied");
    no_journals(&company);
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','bookkeeper','user:u')",[subject]).unwrap();
    control
        .execute("UPDATE apikey SET enabled=0 WHERE id=?1", [credential])
        .unwrap();
    assert!(service(&mut control, &mut company, context(), action()).is_err());
    no_journals(&company);
}
#[test]
fn transport_confirmation_and_identity_refusal() {
    let wire = json!({"input":{"transactionDate":"2026-01-01","text":"Synthetic","createdBy":"user:other","lines":[]},"confirm":true});
    assert_eq!(
        parse("post", &serde_json::to_vec(&wire).unwrap())
            .err()
            .unwrap()
            .code,
        "journal_input_refused"
    );
    assert!(parse("post", b"{\"input\":{}}").is_err());
    assert!(
        parse(
            "reverse",
            b"{\"confirm\":false,\"transactionDate\":\"2026-01-01\",\"reason\":\"Synthetic\"}"
        )
        .is_err()
    );
}

fn keyed_post(key: &str, amount: i64) -> Action {
    parse("keyed-post", &serde_json::to_vec(&json!({"confirm":true,"idempotencyKey":key,"input":{"transactionDate":"2026-01-01","text":"Synthetic keyed capital","lines":[{"accountNo":"2000","debitAmount":amount},{"accountNo":"5000","creditAmount":amount}]}})).unwrap()).unwrap()
}
fn keyed_reverse(key: &str, entry_no: &str, ignored: &str) -> Action {
    parse("keyed-reverse", &serde_json::to_vec(&json!({"confirm":true,"idempotencyKey":key,"entryNo":entry_no,"matchText":ignored,"transactionDate":"2026-01-01","reason":"Synthetic correction"})).unwrap()).unwrap()
}
fn count(company: &Connection, table: &str) -> i64 {
    company
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn run(
    control: &mut Connection,
    company: &mut Connection,
    proof: &VerifiedCookie,
    action: Action,
) -> Value {
    human(control, company, proof, &Fixed, selection(), action)
        .unwrap()
        .value
}
#[test]
fn journal_original_keyed_post_closed_replay_changed_intent_and_rejected_no_receipt() {
    let (_dir, mut control, mut company, _cookies, proof) = fixture();
    let first = run(&mut control, &mut company, &proof, keyed_post("post", 10));
    assert_eq!(first["ok"], true);
    assert_eq!(first["idempotency"]["replayed"], false);
    assert_eq!(first["idempotency"].as_object().unwrap().len(), 4);
    company.execute_batch("INSERT INTO accounting_periods(period_start,period_end,kind,status)VALUES('2026-01-01','2026-01-31','custom','closed')").unwrap();
    let replay = run(&mut control, &mut company, &proof, keyed_post("post", 10));
    let mut expected = first.clone();
    expected["idempotency"]["replayed"] = json!(true);
    assert_eq!(replay, expected);
    let conflict = run(&mut control, &mut company, &proof, keyed_post("post", 11));
    assert_eq!(conflict["code"], "IDEMPOTENCY_CONFLICT");
    let refusal = human(
        &mut control,
        &mut company,
        &proof,
        &Fixed,
        selection(),
        keyed_post("closed", 10),
    )
    .unwrap_err();
    assert_eq!(refusal.business.as_ref().unwrap()["ok"], false);
    assert_eq!(count(&company, "journal_entries"), 1);
    assert_eq!(count(&company, "journal_lines"), 2);
    assert_eq!(count(&company, "mutation_idempotency_tombstones"), 1);
    assert_eq!(count(&company, "mutation_idempotency_outcomes"), 1);
    // Current human grant is checked before even an exact accepted replay.
    control.execute_batch("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','disable',NULL,'user:u')").unwrap();
    assert!(
        human(
            &mut control,
            &mut company,
            &proof,
            &Fixed,
            selection(),
            keyed_post("post", 10)
        )
        .is_err()
    );
}
#[test]
fn journal_original_keyed_reversal_replays_changed_source_and_hashes_ignored_selectors() {
    let (_dir, mut control, mut company, _cookies, proof) = fixture();
    let first = run(&mut control, &mut company, &proof, keyed_post("post", 10));
    let no = first["entryNo"].as_str().unwrap();
    let reversed = run(
        &mut control,
        &mut company,
        &proof,
        keyed_reverse("reverse", no, "ignored"),
    );
    assert_eq!(reversed["ok"], true);
    assert_eq!(reversed["originalEntryId"], first["entryId"]);
    company.execute_batch("INSERT INTO accounting_periods(period_start,period_end,kind,status)VALUES('2026-01-01','2026-01-31','custom','closed')").unwrap();
    let replay = run(
        &mut control,
        &mut company,
        &proof,
        keyed_reverse("reverse", no, "ignored"),
    );
    let mut expected = reversed;
    expected["idempotency"]["replayed"] = json!(true);
    assert_eq!(replay, expected);
    let conflict = run(
        &mut control,
        &mut company,
        &proof,
        keyed_reverse("reverse", no, "changed ignored selector"),
    );
    assert_eq!(conflict["code"], "IDEMPOTENCY_CONFLICT");
    assert_eq!(count(&company, "journal_entries"), 2);
    assert_eq!(count(&company, "journal_lines"), 4);
    assert_eq!(count(&company, "mutation_idempotency_tombstones"), 2);
}
#[test]
fn journal_original_keyed_expired_tombstone_and_selector_refusal_never_reexecute() {
    let (_dir, mut control, mut company, _cookies, proof) = fixture();
    run(&mut control, &mut company, &proof, keyed_post("post", 10));
    company
        .execute_batch("DELETE FROM mutation_idempotency_outcomes")
        .unwrap();
    let expired = run(&mut control, &mut company, &proof, keyed_post("post", 10));
    assert_eq!(expired["code"], "IDEMPOTENCY_OUTCOME_EXPIRED");
    let refusal = human(
        &mut control,
        &mut company,
        &proof,
        &Fixed,
        selection(),
        keyed_reverse("missing", "missing", "Synthetic keyed capital"),
    )
    .unwrap_err();
    assert_eq!(
        refusal.business.as_ref().unwrap()["code"],
        "journal_selection_usage"
    );
    assert_eq!(count(&company, "journal_entries"), 1);
    assert_eq!(count(&company, "mutation_idempotency_tombstones"), 1);
}
#[test]
fn journal_original_keyed_required_key_and_raw_numeric_intent() {
    for operation in ["keyed-post", "keyed-reverse"] {
        assert!(parse(operation, b"{}").is_err());
    }
    let raw = |debit: Value| json!({"confirm":true,"idempotencyKey":"raw","input":{"transactionDate":"2026-01-01","text":"Synthetic","lines":[{"accountNo":"2000","debitAmount":debit},{"accountNo":"5000","creditAmount":10}]}});
    let intent =
        |wire: Value| match parse("keyed-post", &serde_json::to_vec(&wire).unwrap()).unwrap() {
            Action::Keyed { intent, .. } => intent,
            _ => panic!("expected keyed action"),
        };
    // Quantized native amounts can agree while original numeric intent differs.
    assert_ne!(intent(raw(json!(10))), intent(raw(json!(10.001))));
    let mut unkeyed = raw(json!(10));
    assert!(parse("post", &serde_json::to_vec(&unkeyed).unwrap()).is_err());
    unkeyed["idempotencyKey"] = json!(" ");
    assert!(parse("keyed-post", &serde_json::to_vec(&unkeyed).unwrap()).is_err());
    assert!(parse("keyed-post", &serde_json::to_vec(&raw(json!(-1))).unwrap()).is_err());
}
#[test]
fn journal_original_keyed_restored_boundary_requires_reconciliation_without_effects() {
    let (_dir, mut control, mut company, _cookies, proof) = fixture();
    run(
        &mut control,
        &mut company,
        &proof,
        keyed_post("source-key", 10),
    );
    company.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor)VALUES('native_restore_receipt_boundary','mutation_idempotency_tombstones','1','Imported receipt high-water mark; archive aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa; original keys require manual reconciliation under fresh identities','system:portable-restore')", []).unwrap();
    let refusal = human(
        &mut control,
        &mut company,
        &proof,
        &Fixed,
        selection(),
        keyed_post("source-key", 10),
    )
    .unwrap_err();
    assert_eq!(
        refusal.fence.error.code,
        "RESTORED_KEY_RECONCILIATION_REQUIRED"
    );
    assert_eq!(count(&company, "journal_entries"), 1);
    assert_eq!(count(&company, "mutation_idempotency_tombstones"), 1);
    assert_eq!(count(&company, "mutation_idempotency_outcomes"), 1);
}
#[test]
fn journal_original_keyed_service_revoked_current_grant_denies_replay() {
    let (_dir, mut control, mut company, cookies, proof) = fixture();
    let issued_receipt = crate::service_lifecycle::execute(
        &mut control,
        &cookies,
        &Fixed,
        &proof,
        &crate::service_lifecycle::Operation::Create {
            display_name: "Synthetic keyed journal service".into(),
            operation_id: None,
            expires_in_seconds: None,
        },
    )
    .unwrap();
    let issued: Value = serde_json::from_str(issued_receipt.as_json()).unwrap();
    let subject = issued["serviceAccountId"].as_str().unwrap();
    control.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES(?1,'activate','member','user:u')", [subject]).unwrap();
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','bookkeeper','user:u')", [subject]).unwrap();
    control
        .execute(
            "UPDATE apikey SET remaining=30 WHERE id=?1",
            [issued["credentialId"].as_str().unwrap()],
        )
        .unwrap();
    let key = ServiceKey::new("w", issued["secret"].as_str().unwrap()).unwrap();
    let context = || service_company::Context {
        key: &key,
        clock: &Fixed,
        selection: selection(),
        max_future_days: 45,
    };
    let first = service(
        &mut control,
        &mut company,
        context(),
        keyed_post("service-post", 10),
    )
    .unwrap();
    assert_eq!(first.result["ok"], true);
    let replay = service(
        &mut control,
        &mut company,
        context(),
        keyed_post("service-post", 10),
    )
    .unwrap();
    assert_eq!(replay.result["idempotency"]["replayed"], true);
    assert_eq!(
        replay.result["idempotency"]["receiptId"],
        first.result["idempotency"]["receiptId"]
    );
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','revoke',NULL,'user:u')", [subject]).unwrap();
    let refusal = service(
        &mut control,
        &mut company,
        context(),
        keyed_post("service-post", 10),
    )
    .err()
    .unwrap();
    assert_eq!(refusal.error.code, "access_denied");
    assert_eq!(count(&company, "journal_entries"), 1);
    assert_eq!(count(&company, "mutation_idempotency_tombstones"), 1);
}
