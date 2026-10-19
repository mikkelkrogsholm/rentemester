//! Synthetic unit fixtures only. Browser acceptance uses normal verification/MFA.
use super::*;
use rentemester_application::archive_import::{Action as ImportAction, Output};
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
    let input = serde_json::from_value(json!({"slug":"b","name":"Synthetic B"})).unwrap();
    assert!(
        owner
            .create_company(&cookies, &Fixed(NOW), &proof, &input)
            .unwrap()
            .outcome
            .is_ok()
    );
    owner.select_company("a").unwrap();
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
fn report(
    owner: &mut Owner,
    proof: &VerifiedCookie,
    year: Option<&str>,
    source: Option<&str>,
    resource: Resource,
) -> Result<Value, rentemester_application::control_fence::Failure> {
    owner
        .archive_reports(
            proof,
            &Fixed(NOW),
            Request {
                today: "2026-01-01",
                year,
                source,
                account: None,
                as_of: None,
                resource,
            },
        )
        .unwrap()
        .outcome
        .map(|v| v.value)
}
fn chart(owner: &mut Owner, proof: &VerifiedCookie) {
    for (no, kind, normal) in [("1000", "asset", "debit"), ("8000", "equity", "credit")] {
        let input=serde_json::from_value(json!({"accountNo":no,"name":format!("Synthetic {no}"),"type":kind,"normalBalance":normal})).unwrap();
        owner
            .company_setup(
                proof,
                &Fixed(NOW),
                rentemester_application::company_setup::Action::Create(&input),
            )
            .unwrap()
            .outcome
            .unwrap();
    }
}
fn changes(owner: &Owner) -> (u64, u64) {
    let held = owner.held.0.as_ref().unwrap();
    (
        held.control.as_ref().unwrap().total_changes(),
        held.company.as_ref().unwrap().total_changes(),
    )
}
#[test]
fn actual_live_archive_collision_five_views_exact_sources_readonly_and_reopen() {
    let (_dir, mut owner, _cookies, proof, uid) = setup();
    chart(&mut owner, &proof);
    let input = serde_json::from_value(json!({
        "format":journal_post::FORMAT,
        "expectedRuleVersion":rentemester_storage::manual_journal_rule_version().unwrap(),
        "idempotencyKey":"synthetic-live-collision",
        "request":{"transactionDate":"2024-02-01","text":"Synthetic LIVE source","currency":"DKK",
        "lines":[{"accountNo":"1000","debitAmount":"9.87","creditAmount":"0.00"},
        {"accountNo":"8000","debitAmount":"0.00","creditAmount":"9.87"}]}
    }))
    .unwrap();
    owner
        .post_human(&proof, &Fixed(NOW), &journal_post::Policy::default(), input)
        .unwrap()
        .outcome
        .unwrap();
    install(&mut owner, &proof);
    let before = changes(&owner);
    let live = report(
        &mut owner,
        &proof,
        Some("2024"),
        None,
        Resource::TrialBalance,
    )
    .unwrap();
    assert_eq!(live["source"], "live");
    assert_eq!(live["totalDebit"], 9.87);
    assert_eq!(
        live,
        report(
            &mut owner,
            &proof,
            Some("2024"),
            Some("live"),
            Resource::TrialBalance
        )
        .unwrap()
    );
    let mut archived = Vec::new();
    for resource in [
        Resource::Journal,
        Resource::TrialBalance,
        Resource::IncomeStatement,
        Resource::Balance,
        Resource::Overview,
    ] {
        let value = report(&mut owner, &proof, Some("2024"), Some("archive"), resource).unwrap();
        assert_eq!(value["source"], "archive");
        assert_eq!(value["declaredCurrency"], "DKK");
        assert!(value["periodStart"].is_null() && value["periodEnd"].is_null());
        assert_eq!(value["comparison"]["status"], "unavailable");
        assert_eq!(value["completeness"]["status"], "ready");
        assert_eq!(
            value["fiscalYears"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|y| y["label"] == "2024")
                .count(),
            2
        );
        archived.push((resource, value));
    }
    assert_eq!(archived[1].1["totalDebit"], "1.24");
    assert!(archived[0].1["entries"][0]["id"].is_null());
    assert_eq!(
        archived[0].1["entries"][0]["lines"][0]["sourceAmount"],
        "1.235000"
    );
    let filtered = owner
        .archive_reports(
            &proof,
            &Fixed(NOW),
            Request {
                today: "2026-01-01",
                year: Some("2024"),
                source: Some("archive"),
                account: Some("1000"),
                as_of: None,
                resource: Resource::Journal,
            },
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(filtered["entries"][0]["lines"].as_array().unwrap().len(), 2);
    assert_eq!(
        report(&mut owner, &proof, None, None, Resource::Journal).unwrap()["selectedYear"],
        "2026"
    );
    assert_eq!(
        report(&mut owner, &proof, None, Some("live"), Resource::Journal).unwrap()["selectedYear"],
        "2026"
    );
    let years = owner
        .archive_report_years(&proof, &Fixed(NOW), "2026-01-01")
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert!(
        years["years"]
            .as_array()
            .unwrap()
            .iter()
            .all(|y| y["label"] != "2025")
    );
    assert_eq!(changes(&owner), before);
    let root = owner.root.clone();
    drop(owner);
    let mut owner =
        Owner::open_with_profile(&root, uid, "w", "a", NamespaceProfile::LocalUserMacos).unwrap();
    for (resource, expected) in archived {
        assert_eq!(
            report(&mut owner, &proof, Some("2024"), Some("archive"), resource).unwrap(),
            expected
        );
    }
}
#[test]
fn current_chart_company_permission_and_physical_source_never_fall_back() {
    let (_dir, mut owner, _cookies, proof, uid) = setup();
    let file = install(&mut owner, &proof);
    assert_eq!(
        report(&mut owner, &proof, Some("2024"), None, Resource::Journal).unwrap()["source"],
        "archive"
    );
    assert_eq!(
        report(
            &mut owner,
            &proof,
            Some("2024"),
            Some("live"),
            Resource::Journal
        )
        .unwrap_err()
        .error
        .code,
        "invalid_year"
    );
    let tb = report(
        &mut owner,
        &proof,
        Some("2024"),
        Some("archive"),
        Resource::TrialBalance,
    )
    .unwrap();
    assert_eq!(tb["totalDebit"], "1.24");
    let pnl = report(
        &mut owner,
        &proof,
        Some("2024"),
        Some("archive"),
        Resource::IncomeStatement,
    )
    .unwrap();
    assert_eq!(pnl["completeness"]["status"], "incomplete");
    assert!(pnl["result"].is_null());
    assert_eq!(
        pnl["completeness"]["unclassifiedAccounts"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    chart(&mut owner, &proof);
    assert_eq!(
        report(
            &mut owner,
            &proof,
            Some("2024"),
            Some("archive"),
            Resource::IncomeStatement
        )
        .unwrap()["completeness"]["status"],
        "ready"
    );
    assert_eq!(
        report(
            &mut owner,
            &proof,
            Some("2025"),
            Some("archive"),
            Resource::Journal
        )
        .unwrap_err()
        .error
        .code,
        "invalid_year"
    );
    assert_eq!(
        owner
            .archive_reports(
                &proof,
                &Fixed(NOW),
                Request {
                    today: "2026-01-01",
                    year: Some("2024"),
                    source: Some("archive"),
                    account: None,
                    as_of: Some("2024-12-31"),
                    resource: Resource::Overview,
                }
            )
            .unwrap()
            .outcome
            .unwrap_err()
            .error
            .code,
        "archive_date_report_unsupported"
    );
    owner.select_company("b").unwrap();
    assert_eq!(
        report(
            &mut owner,
            &proof,
            Some("2024"),
            Some("archive"),
            Resource::Journal
        )
        .unwrap_err()
        .error
        .code,
        "invalid_year"
    );
    owner.select_company("a").unwrap();
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u');").unwrap();
    assert_eq!(
        report(
            &mut owner,
            &proof,
            Some("2024"),
            Some("archive"),
            Resource::Journal
        )
        .unwrap_err()
        .error
        .code,
        "access_denied"
    );
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u');").unwrap();
    let path = owner.root.join("a").join(file.relative_path());
    let root = owner.root.clone();
    drop(owner);
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[0] = b'!';
    std::fs::write(&path, bytes).unwrap();
    let mut owner =
        Owner::open_with_profile(&root, uid, "w", "a", NamespaceProfile::LocalUserMacos).unwrap();
    let before = changes(&owner);
    for resource in [
        Resource::Journal,
        Resource::TrialBalance,
        Resource::IncomeStatement,
        Resource::Balance,
        Resource::Overview,
    ] {
        let error =
            report(&mut owner, &proof, Some("2024"), Some("archive"), resource).unwrap_err();
        assert_eq!(error.error.code, "report_source_unavailable");
    }
    assert_eq!(changes(&owner), before);
}
