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
    (dir, owner, cookies, proof, uid)
}

#[test]
#[cfg(target_os = "macos")]
fn original_diagnostics118_host_dated_read_and_missing_secondary_health() {
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    let date = "2035-01-01";
    let before = owner
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .total_changes();
    let core = owner
        .retention_status_original118(&proof, &Fixed(NOW), Some(date))
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(core["asOf"], date);
    assert_eq!(core["ok"], true);
    assert_eq!(
        owner
            .held
            .0
            .as_ref()
            .unwrap()
            .company
            .as_ref()
            .unwrap()
            .total_changes(),
        before
    );
    assert_eq!(
        owner
            .retention_status_original118(&proof, &Fixed(NOW), Some(""))
            .unwrap()
            .outcome
            .unwrap()
            .value["ok"],
        false
    );
    // Owner's minimum SQL namespace is not the full original company layout.
    // The diagnostic must expose missing folders rather than fabricate health.
    let incomplete = owner.system_healthcheck_original118(&cookies, &proof, &Fixed(NOW), "a").unwrap().outcome.unwrap();
    assert_eq!(incomplete["ok"], false);
    assert_eq!(incomplete["missing"], json!(["documents", "config"]));
    use std::os::unix::fs::DirBuilderExt;
    for relative in ["a/documents/inbox", "a/config"] {
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(owner.root.join(relative)).unwrap();
    }
    let healthy = owner.system_healthcheck_original118(&cookies, &proof, &Fixed(NOW), "a").unwrap().outcome.unwrap();
    assert_eq!(healthy["ok"], true, "{healthy}");
    assert!(
        owner
            .system_healthcheck_original118(&cookies, &proof, &Fixed(NOW), "../escape")
            .unwrap()
            .outcome
            .is_err()
    );
    // Register a secondary with no ledger. Inspection must return diagnostic data,
    // rather than opening that ledger or replacing the healthy anchor.
    let input = serde_json::from_value(json!({"slug":"b","name":"Synthetic B"})).unwrap();
    owner
        .create_company(&cookies, &Fixed(NOW), &proof, &input)
        .unwrap()
        .outcome
        .unwrap();
    owner.select_company("a").unwrap();
    let ledger = owner.root.join("b/data/ledger.sqlite");
    std::fs::remove_file(&ledger).unwrap();
    let core = owner
        .system_healthcheck_original118(&cookies, &proof, &Fixed(NOW), "b")
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(core["ok"], false);
    assert!(core["checks"].is_array());
    assert!(!ledger.exists());
    std::fs::write(&ledger, b"corrupt synthetic ledger").unwrap();
    assert_eq!(
        owner
            .system_healthcheck_original118(&cookies, &proof, &Fixed(NOW), "b")
            .unwrap()
            .outcome
            .unwrap()["ok"],
        false
    );
    std::fs::remove_file(&ledger).unwrap();
    std::os::unix::fs::symlink(owner.root.join("a/data/ledger.sqlite"), &ledger).unwrap();
    assert_eq!(
        owner
            .system_healthcheck_original118(&cookies, &proof, &Fixed(NOW), "b")
            .unwrap()
            .outcome
            .unwrap()["ok"],
        false
    );
    owner.reusable = false;
    assert!(
        owner
            .system_healthcheck_original118(&cookies, &proof, &Fixed(NOW), "b")
            .is_err()
    );
}
#[test]
fn original_diagnostics118_explicit_date_does_not_replace_clock() {
    assert_eq!(diagnostic_date(&Fixed(NOW), None).unwrap(), "2026-01-01");
    for date in ["", "not a date", "2099-01-01"] {
        assert_eq!(diagnostic_date(&Fixed(NOW), Some(date)).unwrap(), date);
    }
}
