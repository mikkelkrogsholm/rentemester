//! Synthetic unit fixtures only. Browser acceptance uses normal verification/MFA.
use super::*;
use rentemester_engine::cfo_analytics::Query;
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

fn query(scope: &str, companies: &[&str]) -> Query {
    Query {
        scope: scope.into(),
        from: "2026-01-01".into(),
        to: "2026-12-31".into(),
        company_slugs: companies.iter().map(|v| (*v).into()).collect(),
        group_profile_id: None,
        account: None,
        party: None,
        dimension: None,
        currency: None,
        cursor: None,
        aggregate: "sum".into(),
        limit: 100,
    }
}
fn journal(owner: &mut Owner, slug: &str, text: &str) {
    owner.select_company(slug).unwrap();
    let db = owner.held.0.as_mut().unwrap().company.as_mut().unwrap();
    db.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('1000','Synthetic bank','asset','debit'),('8000','Synthetic equity','equity','credit')").unwrap();
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let input=serde_json::from_value(json!({"transactionDate":"2026-01-01","text":text,"currency":"DKK","lines":[{"accountNo":"1000","debitAmount":"12.34"},{"accountNo":"8000","creditAmount":"12.34"}]})).unwrap();
    rentemester_storage::journal_post::post(
        &tx,
        &input,
        &rentemester_storage::journal_preview::manual_journal_rule_version().unwrap(),
        rentemester_engine::fiscal_year::Date::parse("2026-01-01").unwrap(),
        rentemester_storage::journal_post::Actor {
            subject: "user:synthetic",
            program: "test",
        },
    )
    .unwrap();
    tx.commit().unwrap();
}
#[test]
fn current_visibility_partial_totals_no_mutations_and_selected_company_restored() {
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    journal(&mut owner, "a", "Visible synthetic source");
    journal(&mut owner, "b", "Hidden synthetic source");
    owner.select_company("a").unwrap();
    let before = owner
        .held
        .0
        .as_ref()
        .unwrap()
        .control
        .as_ref()
        .unwrap()
        .total_changes();
    let report = owner
        .cfo_analytics(
            &cookies,
            &Fixed(NOW),
            &proof,
            &query("portfolio", &["a", "b"]),
        )
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(report["companies"], json!(["a", "b"]));
    assert_eq!(report["partial"], false);
    assert_eq!(report["rows"].as_array().unwrap().len(), 4);
    assert_eq!(owner.slug, "a");
    assert_eq!(
        before,
        owner
            .held
            .0
            .as_ref()
            .unwrap()
            .control
            .as_ref()
            .unwrap()
            .total_changes()
    );
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','b','revoke',NULL,'user:synthetic');").unwrap();
    let report = owner
        .cfo_analytics(
            &cookies,
            &Fixed(NOW),
            &proof,
            &query("portfolio", &["a", "b"]),
        )
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(report["companies"], json!(["a"]));
    assert_eq!(report["partial"], true);
    assert_eq!(report["rows"].as_array().unwrap().len(), 2);
    assert!(!report.to_string().contains("Hidden synthetic source"));
    assert!(report["reconciliation"].get("amountByCurrency").is_none());
    assert_eq!(owner.slug, "a");
    assert_eq!(
        owner
            .cfo_analytics(&cookies, &Fixed(NOW), &proof, &query("company", &["b"]))
            .unwrap()
            .outcome
            .unwrap_err()
            .error
            .code,
        "access_denied"
    );
    assert_eq!(owner.slug, "a");
}
#[test]
fn unavailable_physical_source_never_becomes_ready_zero_and_restores_selection() {
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    let root = owner.root.clone();
    let original = root.join("b/data/ledger.sqlite");
    let hidden = root.join("b/data/synthetic-hidden.sqlite");
    std::fs::rename(&original, &hidden).unwrap();
    let result = owner.cfo_analytics(
        &cookies,
        &Fixed(NOW),
        &proof,
        &query("portfolio", &["a", "b"]),
    );
    assert!(result.is_err() || result.as_ref().unwrap().outcome.is_err());
    assert_eq!(owner.slug, "a");
    std::fs::rename(&hidden, &original).unwrap();
    // Namespace failure may retire the Owner; no fabricated successful rows may escape.
}
struct Expires(std::sync::atomic::AtomicUsize);
impl Clock for Expires {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(
            if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                NOW
            } else {
                NOW + 24 * 60 * 60 * 1000
            },
        )
    }
}
#[test]
fn final_current_session_recheck_refuses_expiry_before_yield() {
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    let report = owner
        .cfo_analytics(
            &cookies,
            &Expires(std::sync::atomic::AtomicUsize::new(0)),
            &proof,
            &query("company", &["a"]),
        )
        .unwrap();
    assert!(report.outcome.is_err());
    assert_eq!(owner.slug, "a");
}

#[test]
fn absent_group_profile_is_truthful_unsupported_without_figures() {
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    let mut request = query("group", &["a"]);
    request.group_profile_id = Some("synthetic-missing-profile".into());
    let report = owner
        .cfo_analytics(&cookies, &Fixed(NOW), &proof, &request)
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(report["status"], "unsupported");
    assert!(report["group"]["consolidatedFigures"].is_null());
    assert_eq!(owner.slug, "a");
}

#[test]
fn actual_delegate_plan_cannot_escape_preflight_company_scope_or_collect_outside_source() {
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    let manifest:rentemester_engine::intercompany::Manifest=serde_json::from_value(json!({"version":1,"groups":[{"id":"synthetic","name":"Synthetic","memberships":[{"id":"a","companySlug":"a","validFrom":"2026-01-01"},{"id":"b","companySlug":"b","validFrom":"2026-01-01"}],"ownership":[{"id":"owns-b","parentCompanySlug":"a","childCompanySlug":"b","basisPoints":10000,"evidenceRefs":["synthetic"],"validFrom":"2026-01-01"}]}]})).unwrap();
    owner
        .intercompany(
            &cookies,
            &Fixed(NOW),
            &proof,
            Some("a"),
            rentemester_application::intercompany::Action::Manifest(&manifest),
        )
        .unwrap()
        .outcome
        .unwrap();
    let profile=serde_json::from_value(json!({"version":1,"id":"synthetic-profile","groupId":"synthetic","currency":"DKK","reportingLines":[{"id":"assets","label":"Assets","section":"asset","displayOrder":1},{"id":"debt","label":"Debt","section":"liability","displayOrder":2},{"id":"equity","label":"Equity","section":"equity","displayOrder":3},{"id":"result","label":"Result","section":"equity","displayOrder":4,"role":"current-result"},{"id":"sales","label":"Sales","section":"income","displayOrder":5},{"id":"costs","label":"Costs","section":"expense","displayOrder":6}],"accountMappings":[{"id":"a-bank","companySlug":"a","accountNo":"1000","reportingLineId":"assets","validFrom":"2026-01-01"},{"id":"a-sales","companySlug":"a","accountNo":"8000","reportingLineId":"equity","validFrom":"2026-01-01"},{"id":"b-bank","companySlug":"b","accountNo":"1000","reportingLineId":"assets","validFrom":"2026-01-01"},{"id":"b-sales","companySlug":"b","accountNo":"8000","reportingLineId":"equity","validFrom":"2026-01-01"}],"evidenceRefs":["synthetic"],"validFrom":"2026-01-01"})).unwrap();
    let proposed = owner
        .consolidation(
            &cookies,
            &Fixed(NOW),
            &proof,
            Some("a"),
            rentemester_application::consolidation::Action::Propose(&profile),
        )
        .unwrap()
        .outcome
        .unwrap();
    let exact = rentemester_application::consolidation::Exact {
        profile_id: "synthetic-profile".into(),
        profile_hash: proposed["profileHash"].as_str().unwrap().into(),
    };
    assert!(
        owner
            .consolidation(
                &cookies,
                &Fixed(NOW),
                &proof,
                Some("a"),
                rentemester_application::consolidation::Action::Approve(&exact)
            )
            .unwrap()
            .outcome
            .is_err()
    );
    let c = owner.held.0.as_mut().unwrap().control.as_mut().unwrap();
    c.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('r','Reviewer','r@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('rf','synthetic','[]','r',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('rs','r','reviewer-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('r','activate','workspace_owner','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('r','a','grant','owner','user:u'),('r','b','grant','owner','user:u');").unwrap();
    let cookie = cookies.issue_session("reviewer-token").unwrap();
    let review = cookies
        .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    let wrong = rentemester_application::consolidation::Exact {
        profile_id: exact.profile_id.clone(),
        profile_hash: "0".repeat(64),
    };
    assert!(
        owner
            .consolidation(
                &cookies,
                &Fixed(NOW),
                &review,
                Some("a"),
                rentemester_application::consolidation::Action::Approve(&wrong)
            )
            .unwrap()
            .outcome
            .is_err()
    );
    owner
        .consolidation(
            &cookies,
            &Fixed(NOW),
            &review,
            Some("a"),
            rentemester_application::consolidation::Action::Approve(&exact),
        )
        .unwrap()
        .outcome
        .unwrap();

    // Simulate a wider approved profile becoming current after CFO preflight.
    // A missing outside source makes any attempted collection observable as a failure.
    let outside = owner.root.join("b/data/ledger.sqlite");
    let saved = owner.root.join("b/data/synthetic-saved.sqlite");
    std::fs::rename(&outside, &saved).unwrap();
    let allowed = std::collections::BTreeSet::from(["a".to_owned()]);
    let result = owner
        .consolidation_bound(
            &cookies,
            &Fixed(NOW),
            &proof,
            None,
            rentemester_application::consolidation::Action::Plan {
                profile_id: "synthetic-profile",
                from: "2026-01-01",
                as_of: "2026-12-31",
            },
            Some(&allowed),
        )
        .unwrap()
        .outcome
        .unwrap();
    std::fs::rename(&saved, &outside).unwrap();
    assert_eq!(result["status"], "blocked");
    assert!(result["consolidatedFigures"].is_null());
    assert_eq!(result["sourceSnapshots"], json!([]));
    assert_eq!(result["rawCompanySums"], json!([]));
    assert!(!result.to_string().contains("Synthetic B"));
    assert!(!result.to_string().contains("synthetic-profile"));
    assert_eq!(owner.slug, "a");
}
