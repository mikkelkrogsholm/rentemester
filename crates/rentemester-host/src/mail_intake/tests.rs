//! Synthetic physical Owner tests; no provider connection or real credentials.
use super::*;
use rentemester_application::{
    mail_intake::Action,
    workspace_inbox::{Action as InboxAction, Destination, ReviewInput},
};
use rentemester_runtime::{
    auth::{AuthError, CookieProfile, CookieVerifier},
    imap::{Batch, Message},
};
use serde_json::{Value, json};
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

fn configure(
    owner: &mut Owner,
    cookies: &CookieVerifier,
    proof: &VerifiedCookie,
    password: &str,
) -> Value {
    let value = json!({"host":"imap.example.invalid","port":993,"username":"synthetic","password":password,"mailbox":"INBOX"});
    owner
        .mail_intake(cookies, proof, &Fixed(NOW), None, Action::Configure(&value))
        .unwrap()
        .outcome
        .unwrap()
        .value
}
fn batch(uid: u32) -> Batch {
    Batch{uid_validity:11,messages:vec![Message{uid,raw:b"From: synthetic@example.invalid\r\nMessage-ID: <synthetic-mail@example.invalid>\r\nSubject: Synthetic original\r\nMIME-Version: 1.0\r\nContent-Type: multipart/mixed; boundary=synthetic\r\n\r\n--synthetic\r\nContent-Type: application/pdf; name=synthetic.pdf\r\nContent-Disposition: attachment; filename=synthetic.pdf\r\nContent-Transfer-Encoding: base64\r\n\r\nJVBERi0xLjQKU3ludGhldGljIG9yaWdpbmFsCg==\r\n--synthetic--\r\n".to_vec()}],refusals:vec![],has_more:false}
}
fn source_count(owner: &Owner) -> i64 {
    owner
        .held
        .0
        .as_ref()
        .unwrap()
        .control
        .as_ref()
        .unwrap()
        .query_row("SELECT count(*) FROM rm_workspace_inbox_sources", [], |r| {
            r.get(0)
        })
        .unwrap()
}
#[test]
fn config_status_has_no_password_or_implicit_service_consent_and_delete_invalidates_poll() {
    let (_dir, mut owner, cookies, proof, _) = setup();
    let configured = configure(&mut owner, &cookies, &proof, "synthetic-private");
    assert_eq!(configured["bilagsmail"]["imapConfigured"], true);
    assert_eq!(configured["bilagsmail"]["polling"]["enabled"], false);
    assert!(!configured.to_string().contains("synthetic-private"));
    let admission = owner
        .prepare_mail_poll(&cookies, &proof, &Fixed(NOW))
        .unwrap()
        .outcome
        .unwrap()
        .value;
    owner
        .mail_intake(&cookies, &proof, &Fixed(NOW), None, Action::DeleteConfig)
        .unwrap()
        .outcome
        .unwrap();
    assert!(
        owner
            .finish_mail_poll(&cookies, &proof, &Fixed(NOW), admission, Ok(batch(1)))
            .unwrap()
            .outcome
            .is_err()
    );
    assert_eq!(source_count(&owner), 0);
}
#[test]
fn revocation_and_config_replacement_between_phases_publish_nothing() {
    let (_dir, mut owner, cookies, proof, _) = setup();
    configure(&mut owner, &cookies, &proof, "synthetic-first");
    let admission = owner
        .prepare_mail_poll(&cookies, &proof, &Fixed(NOW))
        .unwrap()
        .outcome
        .unwrap()
        .value;
    configure(&mut owner, &cookies, &proof, "synthetic-second");
    assert!(
        owner
            .finish_mail_poll(&cookies, &proof, &Fixed(NOW), admission, Ok(batch(1)))
            .unwrap()
            .outcome
            .is_err()
    );
    assert_eq!(source_count(&owner), 0);
    let admission = owner
        .prepare_mail_poll(&cookies, &proof, &Fixed(NOW))
        .unwrap()
        .outcome
        .unwrap()
        .value;
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u')",[]).unwrap();
    assert!(
        owner
            .finish_mail_poll(&cookies, &proof, &Fixed(NOW), admission, Ok(batch(1)))
            .unwrap()
            .outcome
            .is_err()
    );
    assert_eq!(source_count(&owner), 0);
}
#[test]
fn received_original_requires_review_and_exactly_once_company_receipt() {
    let (_dir, mut owner, cookies, proof, _) = setup();
    configure(&mut owner, &cookies, &proof, "synthetic-private");
    let admission = owner
        .prepare_mail_poll(&cookies, &proof, &Fixed(NOW))
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let received = owner
        .finish_mail_poll(&cookies, &proof, &Fixed(NOW), admission, Ok(batch(1)))
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let ids = received["poll"]["sourceIds"].as_array().unwrap();
    assert!(!ids.is_empty());
    let mut attachment = None;
    for id in ids {
        let id = id.as_str().unwrap();
        let inspected = owner
            .workspace_inbox(&cookies, &proof, &Fixed(NOW), "a", InboxAction::Inspect(id))
            .unwrap()
            .outcome
            .unwrap()
            .value;
        if inspected["source"]["mail"]["role"] == "attachment" {
            attachment = Some((
                id.to_owned(),
                inspected["source"]["sha256"].as_str().unwrap().to_owned(),
            ));
        }
    }
    let (id, hash) = attachment.expect("retained real PDF attachment");
    let d = Destination {
        company_slug: "a".into(),
    };
    owner
        .workspace_inbox(
            &cookies,
            &proof,
            &Fixed(NOW),
            "a",
            InboxAction::Assign(&id, &d),
        )
        .unwrap()
        .outcome
        .unwrap();
    assert!(
        owner
            .workspace_inbox(
                &cookies,
                &proof,
                &Fixed(NOW),
                "a",
                InboxAction::Complete(&id, &d)
            )
            .unwrap()
            .outcome
            .is_err()
    );
    let wrong = ReviewInput {
        company_slug: "a".into(),
        source_hash: "0".repeat(64),
        metadata: json!({"documentType":"cash_register_receipt"}),
    };
    assert!(
        owner
            .workspace_inbox(
                &cookies,
                &proof,
                &Fixed(NOW),
                "a",
                InboxAction::Review(&id, &wrong)
            )
            .unwrap()
            .outcome
            .is_err()
    );
    let input = ReviewInput {
        source_hash: hash,
        ..wrong
    };
    owner
        .workspace_inbox(
            &cookies,
            &proof,
            &Fixed(NOW),
            "a",
            InboxAction::Review(&id, &input),
        )
        .unwrap()
        .outcome
        .unwrap();
    let original = owner
        .workspace_inbox_original(&cookies, &proof, &Fixed(NOW), "a", &id)
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert!(original.bytes.starts_with(b"%PDF-"));
    assert_eq!(original.mime_type, "application/pdf");
    let first = owner
        .workspace_inbox(
            &cookies,
            &proof,
            &Fixed(NOW),
            "a",
            InboxAction::Complete(&id, &d),
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let again = owner
        .workspace_inbox(
            &cookies,
            &proof,
            &Fixed(NOW),
            "a",
            InboxAction::Complete(&id, &d),
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(first, again);
    let company = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
    assert_eq!(
        company
            .query_row("SELECT count(*) FROM documents", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        company
            .query_row("SELECT count(*) FROM mail_intake_messages", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        company
            .query_row("SELECT count(*) FROM journal_entries", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    // A second UID with the same message/attachment retains provenance but cannot create another document.
    let admission = owner
        .prepare_mail_poll(&cookies, &proof, &Fixed(NOW))
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let replay = owner
        .finish_mail_poll(&cookies, &proof, &Fixed(NOW), admission, Ok(batch(2)))
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(replay["poll"]["duplicates"], 1);
}

#[test]
fn bounded_unparseable_mail_retains_exact_original_before_advancing_cursor() {
    let (_dir, mut owner, cookies, proof, _) = setup();
    configure(&mut owner, &cookies, &proof, "synthetic-private");
    let raw = format!("Subject: {}\r\n\r\nSynthetic original\n", "x".repeat(9000)).into_bytes();
    assert!(rentemester_engine::mail_intake::parse(&raw).is_err());
    let admission = owner
        .prepare_mail_poll(&cookies, &proof, &Fixed(NOW))
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let received = owner
        .finish_mail_poll(
            &cookies,
            &proof,
            &Fixed(NOW),
            admission,
            Ok(Batch {
                uid_validity: 11,
                messages: vec![Message {
                    uid: 1,
                    raw: raw.clone(),
                }],
                refusals: vec![],
                has_more: false,
            }),
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let id = received["poll"]["sourceIds"].as_array().unwrap()[0]
        .as_str()
        .unwrap();
    let original = owner
        .workspace_inbox_original(&cookies, &proof, &Fixed(NOW), "a", id)
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(original.bytes, raw);
    assert_eq!(original.mime_type, "message/rfc822");
    let next = owner
        .prepare_mail_poll(&cookies, &proof, &Fixed(NOW))
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(next.cursor().unwrap().last_uid, 1);
}
