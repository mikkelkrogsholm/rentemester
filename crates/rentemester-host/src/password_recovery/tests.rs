//! Synthetic owned-namespace unit fixtures; ordinary browser verification/MFA is separate acceptance.
use super::*;
use rentemester_runtime::{
    auth::{AuthError, CookieProfile, CookieVerifier},
    recovery::{DeliveryUnconfirmed, RecoveryLinks, ResetMessage},
};
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1767225600000)
    }
}
#[derive(Default)]
struct Sender {
    token: Option<String>,
    url: Option<String>,
    fail: bool,
    disabled: bool,
}
impl ResetSender for Sender {
    fn enabled(&self) -> bool {
        !self.disabled
    }
    fn send(&mut self, message: &ResetMessage<'_>) -> Result<(), DeliveryUnconfirmed> {
        assert_eq!(message.recipient(), "synthetic@example.invalid");
        self.token = Some(message.token().into());
        self.url = Some(message.action_url().into());
        if self.fail {
            Err(DeliveryUnconfirmed)
        } else {
            Ok(())
        }
    }
}
fn setup() -> (tempfile::TempDir, Owner, CookieVerifier, VerifiedCookie) {
    let dir = tempfile::tempdir().unwrap();
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
        &dir.path().canonicalize().unwrap().join("workspace"),
        uid,
        "w",
        "synthetic",
        "Synthetic recovery",
        "2026-01-01T00:00:00.000Z",
        &[],
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    let db = owner.held.0.as_mut().unwrap().control.as_mut().unwrap();
    db.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic recovery','synthetic@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','old-session','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','synthetic','grant','owner','user:u');").unwrap();
    let hash = rentemester_runtime::password::hash_password("synthetic-old-password").unwrap();
    db.execute("INSERT INTO account(id,userId,accountId,providerId,issuer,password,createdAt,updatedAt)VALUES('credential','u','u','credential','local:credential',?1,'2026-01-01','2026-01-01')",[hash.as_str()]).unwrap();
    let cookies = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let cookie = cookies.issue_session("old-session").unwrap();
    let proof = cookies
        .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    (dir, owner, cookies, proof)
}
fn counts(owner: &Owner) -> (i64, i64, i64) {
    let db = owner.held.0.as_ref().unwrap().control.as_ref().unwrap();
    (
        db.query_row("SELECT count(*) FROM verification", [], |r| r.get(0))
            .unwrap(),
        db.query_row("SELECT count(*) FROM workspace_audit", [], |r| r.get(0))
            .unwrap(),
        db.query_row("SELECT count(*) FROM session", [], |r| r.get(0))
            .unwrap(),
    )
}
#[test]
fn owned_request_readonly_callback_complete_invalidates_old_sessions_and_refuses_replay() {
    let (_dir, mut owner, cookies, proof) = setup();
    let links = RecoveryLinks::new(
        "https://example.invalid",
        &["https://example.invalid"],
        CookieProfile::Secure,
    )
    .unwrap();
    let runtime = reset_request::Runtime {
        cookies: &cookies,
        clock: &Fixed,
        links: &links,
        audit_key_version: 1,
    };
    let mut sender = Sender::default();
    owner
        .current_session(&cookies, &Fixed, &proof)
        .unwrap()
        .outcome
        .unwrap();
    let request = owner
        .request_password_reset(
            &runtime,
            &mut sender,
            "synthetic@example.invalid",
            Some("/reset"),
        )
        .unwrap();
    assert!(!request.retired);
    request.outcome.unwrap();
    let token = sender.token.as_deref().unwrap();
    assert_eq!(token.len(), 24);
    assert!(
        sender
            .url
            .as_deref()
            .unwrap()
            .contains("/api/auth/reset-password/")
    );
    let before = counts(&owner);
    for _ in 0..2 {
        let r = owner
            .password_reset_callback(&runtime, token, Some("/reset"))
            .unwrap();
        assert!(!r.retired);
        assert_eq!(
            r.outcome.unwrap().as_str(),
            format!("https://example.invalid/reset?token={token}")
        );
        assert_eq!(
            counts(&owner),
            before,
            "callback neither consumes the token nor changes sessions/audit"
        );
    }
    let complete = password_reset::Runtime {
        clock: &Fixed,
        audit_key_version: 1,
    };
    let r = owner
        .complete_password_reset(&complete, token, "synthetic-new-password")
        .unwrap();
    assert!(!r.retired);
    r.outcome.unwrap();
    assert_eq!(counts(&owner).0, 0);
    assert_eq!(counts(&owner).2, 0);
    assert!(
        owner
            .current_session(&cookies, &Fixed, &proof)
            .unwrap()
            .outcome
            .is_err()
    );
    let hash: String = owner
        .held
        .0
        .as_ref()
        .unwrap()
        .control
        .as_ref()
        .unwrap()
        .query_row(
            "SELECT password FROM account WHERE id='credential'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    rentemester_runtime::password::verify_password("synthetic-new-password", Some(&hash)).unwrap();
    assert!(
        rentemester_runtime::password::verify_password("synthetic-old-password", Some(&hash))
            .is_err()
    );
    let before = counts(&owner);
    let replay = owner
        .complete_password_reset(&complete, token, "synthetic-third-password")
        .unwrap();
    assert!(!replay.retired);
    let failure = replay.outcome.err().unwrap();
    assert_eq!(
        failure.stage,
        rentemester_application::password_login::FailureStage::NotCommitted
    );
    assert_eq!(failure.error.code, "invalid_credentials");
    assert_eq!(counts(&owner), before);
    assert!(
        owner
            .password_reset_callback(&runtime, token, Some("/reset"))
            .unwrap()
            .outcome
            .unwrap()
            .as_str()
            .contains("error=INVALID_TOKEN")
    );
}
#[test]
fn owned_request_preserves_known_committed_mail_failure_and_refuses_wrong_workspace_without_effect()
{
    let (_dir, mut owner, cookies, _proof) = setup();
    let links = RecoveryLinks::new(
        "https://example.invalid",
        &["https://example.invalid"],
        CookieProfile::Secure,
    )
    .unwrap();
    let runtime = reset_request::Runtime {
        cookies: &cookies,
        clock: &Fixed,
        links: &links,
        audit_key_version: 1,
    };
    let mut sender = Sender {
        disabled: true,
        ..Sender::default()
    };
    let before = counts(&owner);
    let r = owner
        .request_password_reset(
            &runtime,
            &mut sender,
            "synthetic@example.invalid",
            Some("/reset"),
        )
        .unwrap();
    assert!(!r.retired);
    assert_eq!(
        r.outcome.err().unwrap().stage,
        reset_request::Stage::NotCommitted
    );
    assert_eq!(counts(&owner), before);
    sender.disabled = false;
    sender.fail = true;
    let r = owner
        .request_password_reset(
            &runtime,
            &mut sender,
            "synthetic@example.invalid",
            Some("/reset"),
        )
        .unwrap();
    assert!(!r.retired);
    let failure = r.outcome.err().unwrap();
    assert_eq!(failure.stage, reset_request::Stage::Committed);
    assert_eq!(failure.error.code, "auth_delivery_unconfirmed");
    assert!(failure.connection_reusable);
    let token = sender.token.as_deref().unwrap();
    assert_eq!(counts(&owner).0, 1);
    assert!(
        owner
            .password_reset_callback(&runtime, token, Some("/reset"))
            .unwrap()
            .outcome
            .unwrap()
            .as_str()
            .contains("?token=")
    );
    let before = counts(&owner);
    let other = CookieVerifier::new(
        "other",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let wrong = reset_request::Runtime {
        cookies: &other,
        clock: &Fixed,
        links: &links,
        audit_key_version: 1,
    };
    assert!(matches!(
        owner.request_password_reset(
            &wrong,
            &mut sender,
            "synthetic@example.invalid",
            Some("/reset")
        ),
        Err(OwnerError::Context)
    ));
    assert!(matches!(
        owner.password_reset_callback(&wrong, "unused", Some("/reset")),
        Err(OwnerError::Retired)
    ));
    assert_eq!(counts(&owner), before);
    assert!(sender.token.is_some());
}
