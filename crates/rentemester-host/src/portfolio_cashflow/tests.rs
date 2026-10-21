//! Synthetic unit fixtures only. Browser acceptance uses normal verification/MFA.
use super::*;
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
#[test]
fn overview_current_visibility_backup_fence_no_effects_and_restore() {
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    let before = owner
        .held
        .0
        .as_ref()
        .unwrap()
        .control
        .as_ref()
        .unwrap()
        .total_changes();
    let r = owner
        .portfolio_overview(&cookies, &Fixed(NOW), &proof, "2026-01-01", "2026-01-01")
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(r["companyCount"], 2);
    assert_eq!(r["companies"][0]["slug"], "a");
    assert_eq!(r["companies"][1]["slug"], "b");
    assert!(r.get("totals").is_none());
    assert_eq!(r["companies"][0]["backup"]["available"], true);
    assert!(r["companies"][0]["backup"]["backupDue"].is_boolean());
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
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','user:u'),('u','b','revoke',NULL,'user:u');").unwrap();
    let r = owner
        .portfolio_overview(&cookies, &Fixed(NOW), &proof, "2026-01-01", "2026-01-01")
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(r["companyCount"], 1);
    assert!(!r.to_string().contains("Synthetic B"));
    assert_eq!(r["companies"][0]["backup"]["available"], false);
    assert!(r["companies"][0]["backup"]["backupDue"].is_null());
    assert_eq!(owner.slug, "a");
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES('u','a','revoke','user:u')").unwrap();
    assert!(
        owner
            .portfolio_overview(&cookies, &Fixed(NOW), &proof, "2026-01-01", "2026-01-01")
            .unwrap()
            .outcome
            .is_err()
    );
    assert_eq!(owner.slug, "a");
}
#[test]
fn safe_domain_refusal_is_a_row_but_failed_source_admission_is_whole_failure() {
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    owner.select_company("b").unwrap();
    owner
        .held
        .0
        .as_mut()
        .unwrap()
        .company
        .as_mut()
        .unwrap()
        .execute("UPDATE companies SET currency='EUR' WHERE id=1", [])
        .unwrap();
    owner.select_company("a").unwrap();
    let r = owner
        .portfolio_overview(&cookies, &Fixed(NOW), &proof, "2026-01-01", "2026-01-01")
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(r["companies"][0]["ok"], true);
    assert_eq!(r["companies"][1]["ok"], false);
    assert_eq!(r["companies"][1]["error"], "company source unavailable");
    assert_eq!(owner.slug, "a");
    let source = owner.root.join("b/data/ledger.sqlite");
    let saved = owner.root.join("b/data/synthetic-saved.sqlite");
    std::fs::rename(&source, &saved).unwrap();
    let r = owner.portfolio_overview(&cookies, &Fixed(NOW), &proof, "2026-01-01", "2026-01-01");
    assert!(r.is_err() || r.unwrap().outcome.is_err());
    assert_eq!(owner.slug, "a");
    std::fs::rename(saved, source).unwrap();
}
