//! Physical current-role checks between provider discovery and registration.
//! Synthetic unit fixture only; no provider I/O or ordinary-login qualification.
use super::*;
use app::{Discovery, Request};
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

fn pending(next: Next) -> Admission {
    match next {
        Next::Pending(a) => *a,
        Next::Complete(_) => panic!("pending phase required"),
    }
}
fn revoke(owner: &mut Owner) {
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u')",[]).unwrap();
}
#[test]
fn revoked_company_discovery_cannot_admit_registration_post() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let phase = pending(
        owner
            .start_public_einvoice(&proof, &Fixed(NOW), Intent::Setup, "test")
            .unwrap()
            .outcome
            .unwrap()
            .value,
    );
    assert!(matches!(phase.request(), Request::FindCompany { .. }));
    revoke(&mut owner);
    let result = owner
        .finish_public_einvoice(
            &proof,
            &Fixed(NOW),
            phase,
            Outcome::CompanyFound(Discovery::Missing),
        )
        .unwrap()
        .outcome;
    assert!(result.is_err());
    let count: i64 = owner
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM digisense_companies", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}
#[test]
fn revoked_outbound_discovery_cannot_admit_participant_post() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let phase = pending(
        owner
            .start_public_einvoice(&proof, &Fixed(NOW), Intent::Setup, "test")
            .unwrap()
            .outcome
            .unwrap()
            .value,
    );
    let phase = pending(
        owner
            .finish_public_einvoice(
                &proof,
                &Fixed(NOW),
                phase,
                Outcome::CompanyFound(Discovery::Found("synthetic-key".into())),
            )
            .unwrap()
            .outcome
            .unwrap()
            .value,
    );
    assert!(matches!(phase.request(), Request::FindOutbound { .. }));
    revoke(&mut owner);
    assert!(
        owner
            .finish_public_einvoice(
                &proof,
                &Fixed(NOW),
                phase,
                Outcome::OutboundFound(Discovery::Missing)
            )
            .unwrap()
            .outcome
            .is_err()
    );
    let count: i64 = owner
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM digisense_participants", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(count, 0);
}
