//! Synthetic unit identities only; no ordinary-login or provider qualification claim.
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
    owner.held.0.as_mut().unwrap().company.as_mut().unwrap().execute("UPDATE companies SET cvr='12345678',name='Synthetic Public Company',address='Synthetic 1',country='DK' WHERE id=1",[]).unwrap();
    (dir, owner, cookies, proof, uid)
}

fn report(
    owner: &mut Owner,
    proof: &VerifiedCookie,
) -> Result<Value, rentemester_application::control_fence::Failure> {
    let attempt = owner
        .oss_report(
            proof,
            &Fixed(NOW + 2 * 60 * 60 * 1000),
            Date::parse("2026-01-01").unwrap(),
            Date::parse("2026-03-31").unwrap(),
        )
        .unwrap();
    assert!(!attempt.retired);
    attempt.outcome.map(|v| v.value)
}
fn membership(owner: &mut Owner, role: Option<&str>) {
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute(
        "INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a',?1,?2,'user:u')",
        rusqlite::params![if role.is_some() { "grant" } else { "revoke" }, role],
    ).unwrap();
}
fn audits(owner: &Owner) -> (i64, i64) {
    let slots = owner.held.0.as_ref().unwrap();
    (
        slots
            .control
            .as_ref()
            .unwrap()
            .query_row("SELECT count(*) FROM workspace_audit", [], |r| r.get(0))
            .unwrap(),
        slots
            .company
            .as_ref()
            .unwrap()
            .query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0))
            .unwrap(),
    )
}
#[test]
fn oss_report_rechecks_reviewer_and_revocation_without_writes_or_fresh_session() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    membership(&mut owner, Some("reviewer"));
    let before = audits(&owner);
    let first = report(&mut owner, &proof).unwrap();
    assert_eq!(first["ok"], true);
    assert_eq!(first["submission"], false);
    assert_eq!(first["entryCount"], 0);
    assert_eq!(report(&mut owner, &proof).unwrap(), first);
    assert_eq!(audits(&owner), before);
    membership(&mut owner, None);
    let before = audits(&owner);
    assert_eq!(
        report(&mut owner, &proof).unwrap_err().error.code,
        "access_denied"
    );
    assert_eq!(audits(&owner), before);
}
#[test]
fn oss_report_requires_registered_original_bytes_and_preserves_audit() {
    use rentemester_application::documents::ScanPolicy;
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let file = rentemester_runtime::document::Prepared::new(
        "synthetic.txt".into(),
        b"Synthetic supporting OSS evidence".to_vec(),
    )
    .unwrap();
    let admission = owner
        .prepare_document_ingest(
            &proof,
            &Fixed(NOW),
            file,
            serde_json::from_value(
                json!({"source":"synthetic-oss-unit","documentType":"cash_register_receipt"}),
            )
            .unwrap(),
            ScanPolicy::Off,
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    owner
        .finish_document_ingest_registered(&proof, &Fixed(NOW), admission, None)
        .unwrap()
        .outcome
        .unwrap();
    let before = audits(&owner);
    assert_eq!(report(&mut owner, &proof).unwrap()["ok"], true);
    assert_eq!(audits(&owner), before);
    let originals = owner.root.join("a/documents/originals");
    let original = std::fs::read_dir(originals)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    std::fs::remove_file(original).unwrap();
    assert_eq!(
        report(&mut owner, &proof).unwrap_err().error.code,
        "oss_report_source_unavailable"
    );
    assert_eq!(audits(&owner), before);
}
