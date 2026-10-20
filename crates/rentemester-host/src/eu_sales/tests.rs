//! Physical current-role checks between explicit lookup and VIES persistence.
//! Synthetic unit fixture only; no provider I/O or ordinary-login qualification.
use super::*;
use rentemester_runtime::auth::{AuthError, CookieProfile, CookieVerifier};
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

fn observation(vat: &str) -> Value {
    serde_json::json!({"countryCode":"DE","vatNumber":vat,"normalized":format!("DE{vat}"),"valid":true,"name":"Synthetic buyer","address":null,"rawResponse":"{}"})
}
fn count(owner: &Owner) -> i64 {
    owner
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .query_row("SELECT count(*) FROM vies_validations", [], |r| r.get(0))
        .unwrap()
}
#[test]
fn revoked_vies_lookup_cannot_persist_observation() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let admission = owner
        .start_vies(&proof, &Fixed(NOW), "DE123456789")
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(count(&owner), 0);
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u')",[]).unwrap();
    let result = owner
        .finish_vies(&proof, &Fixed(NOW), admission, &observation("123456789"))
        .unwrap()
        .outcome;
    assert_eq!(result.err().unwrap().error.code, "access_denied");
    assert_eq!(count(&owner), 0);
}
#[test]
fn vies_response_identity_cannot_be_rebound() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let admission = owner
        .start_vies(&proof, &Fixed(NOW), "DE123456789")
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let result = owner
        .finish_vies(&proof, &Fixed(NOW), admission, &observation("987654321"))
        .unwrap()
        .outcome;
    assert_eq!(result.err().unwrap().error.code, "vies_admission_changed");
    assert_eq!(count(&owner), 0);
}
#[test]
fn explicit_vies_observation_stores_trusted_time_and_no_journal() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let admission = owner
        .start_vies(&proof, &Fixed(NOW), "DE123456789")
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let result = owner
        .finish_vies(&proof, &Fixed(NOW), admission, &observation("123456789"))
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(
        result["validation"]["validatedAt"],
        "2026-01-01T00:00:00.000Z"
    );
    assert_eq!(
        result["validation"]["expiresAt"],
        "2026-04-01T00:00:00.000Z"
    );
    assert_eq!(count(&owner), 1);
    let journals: i64 = owner
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .query_row("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
        .unwrap();
    assert_eq!(journals, 0);
}
