//! Synthetic unit fixtures only. Browser acceptance uses normal verification/MFA.
use super::*;
use rentemester_application::archive_import::{Action as ImportAction, Output};
use rentemester_engine::opening_balance::{Apply, Input};
use rentemester_runtime::auth::{AuthError, CookieProfile, CookieVerifier};
use rentemester_runtime::document::Prepared;
use rentemester_storage::archive_reports::Resource;
use serde_json::Value;
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
    let input = serde_json::from_value(json!({"slug":"b","name":"Synthetic B","fiscalYearStartMonth":"7","fiscalYearLabelStrategy":"start-year"})).unwrap();
    assert!(
        owner
            .create_company(&cookies, &Fixed(NOW), &proof, &input)
            .unwrap()
            .outcome
            .is_ok()
    );
    owner.select_company("b").unwrap();
    (dir, owner, cookies, proof, uid)
}

// Byte-exact synthetic CRLF posting CSV: bank/equity +/-1.235000; optional LF balances.
// The identical 2025 posting pair is the excluded cut-over marker, never installed.
fn source() -> Prepared {
    Prepared::new("synthetic-history.json".into(),br#"{"format":"rentemester-dinero-archive-source/1","currency":"DKK","files":[{"path":"2024/Posteringer.csv","contentBase64":"S29udG87S29udG9uYXZuO0RhdG87QmlsYWc7QmlsYWdzdHlwZTtUZWtzdDtNb21zdHlwZTtCZWzDuGI7U2FsZG8NCjEwMDA7U3ludGhldGljIGJhbms7MjAyNC0wMi0wMTsxO0ZpbmFucztTeW50aGV0aWMgaGlzdG9yeTs7MSwyMzUwMDA7MSwyMzUwMDANCjgwMDA7U3ludGhldGljIGVxdWl0eTsyMDI0LTAyLTAxOzE7RmluYW5zO1N5bnRoZXRpYyBoaXN0b3J5OzstMSwyMzUwMDA7MA0K"},{"path":"2024/SaldoBalance.csv","contentBase64":"S29udG87S29udG9uYXZuO0JlbMO4YgoxMDAwO1N5bnRoZXRpYyBiYW5rOzEsMjM1MDAwCjgwMDA7U3ludGhldGljIGVxdWl0eTstMSwyMzUwMDAK"},{"path":"2025/Posteringer.csv","contentBase64":"S29udG87S29udG9uYXZuO0RhdG87QmlsYWc7QmlsYWdzdHlwZTtUZWtzdDtNb21zdHlwZTtCZWzDuGI7U2FsZG8NCjEwMDA7U3ludGhldGljIGJhbms7MjAyNS0wMi0wMTsxO0ZpbmFucztTeW50aGV0aWMgaGlzdG9yeTs7MSwyMzUwMDA7MSwyMzUwMDANCjgwMDA7U3ludGhldGljIGVxdWl0eTsyMDI1LTAyLTAxOzE7RmluYW5zO1N5bnRoZXRpYyBoaXN0b3J5OzstMSwyMzUwMDA7MA0K"}]}"#.to_vec()).unwrap()
}

fn install(owner: &mut Owner, proof: &VerifiedCookie) -> Prepared {
    let file = source();
    let plan = owner
        .archive_import(proof, &Fixed(NOW), ImportAction::Plan(&file))
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let Output::Json(plan) = plan else {
        panic!("JSON plan expected")
    };
    let input = rentemester_storage::archive_import::Apply {
        plan_hash: plan["plan"]["planHash"].as_str().unwrap().into(),
        idempotency_key: "synthetic-report-install".into(),
        confirm: true,
    };
    owner
        .archive_import(proof, &Fixed(NOW), ImportAction::Apply(&file, &input))
        .unwrap()
        .outcome
        .unwrap();
    file
}
fn run(
    owner: &mut Owner,
    proof: &VerifiedCookie,
    action: Action<'_>,
) -> Result<Value, rentemester_application::control_fence::Failure> {
    owner
        .opening_balance(proof, &Fixed(NOW), action)
        .unwrap()
        .outcome
        .map(|v| v.value)
}
fn input(owner: &Owner) -> Input {
    let db = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
    let id: i64 = db
        .query_row(
            "SELECT id FROM audit_log WHERE event_type='native_closed_history_archive_import'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    serde_json::from_value(json!({"cutOverDate":"2024-08-01","lines":[
        {"accountNo":"1000","debitAmount":"100.00"},
        {"accountNo":"2000","creditAmount":"30.00"},
        {"accountNo":"8000","creditAmount":"70.00"}],"note":"Synthetic explicit adoption","sourceArchiveImportIds":[id]})).unwrap()
}
fn chart(owner: &mut Owner, proof: &VerifiedCookie) {
    for (no, kind, normal) in [
        ("1000", "asset", "debit"),
        ("2000", "liability", "credit"),
        ("3000", "expense", "debit"),
        ("8000", "equity", "credit"),
    ] {
        let account=serde_json::from_value(json!({"accountNo":no,"name":format!("Synthetic {no}"),"type":kind,"normalBalance":normal})).unwrap();
        owner
            .company_setup(
                proof,
                &Fixed(NOW),
                rentemester_application::company_setup::Action::Create(&account),
            )
            .unwrap()
            .outcome
            .unwrap();
    }
}
fn expense(owner: &mut Owner, proof: &VerifiedCookie, key: &str) {
    let original = Prepared::new(
        "synthetic-later-expense.txt".into(),
        format!("Synthetic expense original {key}").into_bytes(),
    )
    .unwrap();
    let metadata=serde_json::from_value(json!({"source":"cockpit-upload","documentType":"cash_register_receipt","issueDate":"2024-08-02","amountIncVat":10,"currency":"DKK"})).unwrap();
    let registered = owner
        .ingest_document(
            proof,
            &Fixed(NOW),
            rentemester_application::documents::Ingest {
                file: &original,
                metadata: &metadata,
                today: "2026-01-01",
            },
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let document_id = registered["document"]["id"].as_i64().unwrap().to_string();
    let input=serde_json::from_value(json!({"format":journal_post::FORMAT,"expectedRuleVersion":rentemester_storage::manual_journal_rule_version().unwrap(),"idempotencyKey":key,
    "request":{"documentId":document_id,"transactionDate":"2024-08-02","text":"Synthetic later expense","currency":"DKK","lines":[{"accountNo":"3000","debitAmount":"10.00"},{"accountNo":"1000","creditAmount":"10.00"}]}})).unwrap();
    owner
        .post_human(proof, &Fixed(NOW), &journal_post::Policy::default(), input)
        .unwrap()
        .outcome
        .unwrap_or_else(|f| panic!("Synthetic expense refusal: {:?}", f.business));
}
fn apply(input: Input, plan: &Value, key: &str) -> Apply {
    Apply {
        input,
        plan_hash: plan["plan"]["planHash"].as_str().unwrap().into(),
        idempotency_key: key.into(),
        confirm: true,
    }
}
fn counts(owner: &Owner) -> (i64, i64, i64) {
    let db = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
    (
        db.query_row("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
            .unwrap(),
        db.query_row("SELECT count(*) FROM opening_balances", [], |r| r.get(0))
            .unwrap(),
        db.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0))
            .unwrap(),
    )
}
#[test]
fn explicit_adoption_receipt_reopen_and_actual_dated_fiscal_report_proof() {
    let (_dir, mut owner, _cookies, proof, uid) = setup();
    chart(&mut owner, &proof);
    let file = install(&mut owner, &proof);
    assert!(run(&mut owner, &proof, Action::Read).unwrap()["openingBalance"].is_null());
    let input = input(&owner);
    let before = counts(&owner);
    let plan = run(&mut owner, &proof, Action::Plan(&input)).unwrap();
    assert_eq!(plan["plan"]["status"], "ready");
    assert_eq!(plan["plan"]["totals"]["debit"], "100.00");
    assert_eq!(counts(&owner), before);
    let request = apply(input, &plan, "synthetic-opening");
    let committed = run(&mut owner, &proof, Action::Apply(&request)).unwrap();
    assert_eq!(committed["openingBalance"]["actor"], "user:u");
    assert_eq!(committed["idempotency"]["replayed"], false);
    expense(&mut owner, &proof, "synthetic-post-cutover");
    let mut after = counts(&owner);
    let old = run(&mut owner, &proof, Action::Apply(&request)).unwrap();
    assert_eq!(old["openingBalance"], committed["openingBalance"]);
    assert_eq!(old["idempotency"]["replayed"], true);
    assert_eq!(counts(&owner), (after.0, after.1, after.2 + 1));
    let event: String = owner
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .query_row(
            "SELECT event_type FROM audit_log ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(event, "idempotency_replay");
    after = counts(&owner);
    let dated = owner
        .trial_balance(
            &proof,
            &Fixed(NOW),
            rentemester_engine::trial_balance::Period::new("2024-08-01", "2024-08-02").unwrap(),
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(
        dated
            .accounts
            .iter()
            .find(|r| r.account_no == "1000")
            .unwrap()
            .balance
            .decimal(),
        "90.00"
    );
    assert!(
        owner
            .trial_balance(
                &proof,
                &Fixed(NOW),
                rentemester_engine::trial_balance::Period::new("2024-07-31", "2024-08-02").unwrap()
            )
            .unwrap()
            .outcome
            .is_err()
    );
    let fiscal = owner
        .archive_reports(
            &proof,
            &Fixed(NOW),
            rentemester_application::archive_reports::Request {
                today: "2026-01-01",
                year: Some("2024"),
                source: Some("live"),
                account: None,
                as_of: None,
                resource: Resource::Balance,
            },
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(fiscal["totalAssets"], 90.0);
    assert_eq!(fiscal["openingBalanceBasis"]["cutOverDate"], "2024-08-01");
    assert_eq!(fiscal["liveCoverage"]["status"], "partial");
    let archive = owner
        .archive_reports(
            &proof,
            &Fixed(NOW),
            rentemester_application::archive_reports::Request {
                today: "2026-01-01",
                year: Some("2024"),
                source: Some("archive"),
                account: None,
                as_of: None,
                resource: Resource::TrialBalance,
            },
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(archive["totalDebit"], "1.24");
    let mut changed = serde_json::to_value(&request).unwrap();
    changed["input"]["note"] = json!("Changed retained intent");
    let changed = serde_json::from_value(changed).unwrap();
    assert_eq!(
        run(&mut owner, &proof, Action::Apply(&changed))
            .unwrap_err()
            .error
            .code,
        "IDEMPOTENCY_CONFLICT"
    );
    let root = owner.root.clone();
    drop(owner);
    let mut owner =
        Owner::open_with_profile(&root, uid, "w", "b", NamespaceProfile::LocalUserMacos).unwrap();
    assert_eq!(
        run(&mut owner, &proof, Action::Read).unwrap()["openingBalance"],
        committed["openingBalance"]
    );
    assert_eq!(
        run(&mut owner, &proof, Action::Apply(&request)).unwrap()["idempotency"]["replayed"],
        true
    );
    assert_eq!(counts(&owner), (after.0, after.1, after.2 + 1));
    after = counts(&owner);
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','b','grant','bookkeeper','user:u');").unwrap();
    assert_eq!(
        run(&mut owner, &proof, Action::Apply(&request))
            .unwrap_err()
            .error
            .code,
        "access_denied"
    );
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','b','grant','owner','user:u');").unwrap();
    let path = root.join("b").join(file.relative_path());
    drop(owner);
    std::fs::write(path, b"{}").unwrap();
    let mut owner =
        Owner::open_with_profile(&root, uid, "w", "b", NamespaceProfile::LocalUserMacos).unwrap();
    assert_eq!(
        run(&mut owner, &proof, Action::Apply(&request))
            .unwrap_err()
            .error
            .code,
        "archive_source_unavailable"
    );
    assert_eq!(counts(&owner), after);
}
#[test]
fn stale_plan_admin_ledger_conjunction_and_missing_citation_refuse_without_effects() {
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    chart(&mut owner, &proof);
    let file = install(&mut owner, &proof);
    let input = input(&owner);
    let plan = run(&mut owner, &proof, Action::Plan(&input)).unwrap();
    expense(&mut owner, &proof, "synthetic-later-before-opening");
    let stale = apply(input.clone(), &plan, "synthetic-stale-opening");
    let before = counts(&owner);
    assert!(run(&mut owner, &proof, Action::Apply(&stale)).is_err());
    assert_eq!(counts(&owner), before);
    let plan = run(&mut owner, &proof, Action::Plan(&input)).unwrap();
    let request = apply(input, &plan, "synthetic-reviewed-opening");
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','b','grant','bookkeeper','user:u');").unwrap();
    assert!(run(&mut owner, &proof, Action::Plan(&request.input)).is_ok());
    assert_eq!(
        run(&mut owner, &proof, Action::Apply(&request))
            .unwrap_err()
            .error
            .code,
        "access_denied"
    );
    assert_eq!(counts(&owner), before);
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','b','grant','owner','user:u');").unwrap();
    let state = owner
        .workspace_transfer_status(&cookies, &Fixed(NOW), &proof)
        .unwrap()
        .outcome
        .unwrap();
    let frozen = owner
        .freeze_workspace(
            &cookies,
            &Fixed(NOW),
            &proof,
            &rentemester_storage::workspace_transfer::Freeze {
                expected_state_hash: state["stateHash"].as_str().unwrap().into(),
                destination: "https://synthetic.example.invalid".into(),
                reason: "Synthetic opening freeze check".into(),
                confirm: true,
            },
        )
        .unwrap()
        .outcome
        .unwrap();
    assert!(run(&mut owner, &proof, Action::Plan(&request.input)).is_ok());
    assert_eq!(
        run(&mut owner, &proof, Action::Apply(&request))
            .unwrap_err()
            .error
            .code,
        "workspace_transfer_fenced"
    );
    assert_eq!(counts(&owner), before);
    owner
        .cancel_workspace_transfer(
            &cookies,
            &Fixed(NOW),
            &proof,
            &rentemester_storage::workspace_transfer::Cancel {
                expected_state_hash: frozen["stateHash"].as_str().unwrap().into(),
                reason: "Synthetic cancel before source test".into(),
                confirm: true,
            },
        )
        .unwrap()
        .outcome
        .unwrap();
    let path = owner.root.join("b").join(file.relative_path());
    std::fs::write(&path, b"{}").unwrap();
    assert_eq!(
        run(&mut owner, &proof, Action::Apply(&request))
            .unwrap_err()
            .error
            .code,
        "archive_source_unavailable"
    );
    assert_eq!(counts(&owner), before);
}
