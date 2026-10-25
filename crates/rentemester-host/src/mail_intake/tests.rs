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

fn original717_input(metadata: Option<Value>) -> app::original717::Input {
    app::original717::Input {
        messages: vec![app::original717::Message {
            source: "/MCP-local-only/synthetic.eml".into(),
            base64: imap::original717::encode_eml(&batch(1).messages[0].raw),
        }],
        metadata,
        metadata_per_message: None,
        force: false,
        confirm: true,
    }
}
#[test]
fn original717_mail_intake_ingest_happy_metadata_duplicate_exception_and_refusal() {
    let (_dir, mut owner, cookies, proof, _) = setup();
    let mut input = original717_input(None);
    let run = |o: &mut Owner, i: &app::original717::Input| {
        o.original717_mail_intake_ingest(&cookies, &proof, &Fixed(NOW), i)
            .unwrap()
            .outcome
            .unwrap()
            .value
    };
    assert_eq!(run(&mut owner, &input)["exceptionsCreated"], 1);
    assert_eq!(run(&mut owner, &input)["exceptionsCreated"], 0);
    input.metadata = Some(
        json!({"documentType":"cash_register_receipt","issueDate":"2026-01-01","amountIncVat":125,"vatAmount":25,"currency":"DKK","deliveryDescription":"Synthetic purchase","sender":{"name":"Synthetic vendor","vatOrCvr":"DK12345678","countryCode":"DK","identifierKind":"dk_cvr"},"futureMetadata":"accepted"}),
    );
    let happy = run(&mut owner, &input);
    assert_eq!(happy["attachmentsIngested"], 1);
    assert_eq!(happy["messagesProcessed"], 1);
    assert!(
        happy["documents"][0]["documentNo"]
            .as_str()
            .unwrap()
            .starts_with("DOC-")
    );
    assert_eq!(run(&mut owner, &input)["attachmentsSkipped"], 1);
    input.confirm = false;
    assert_eq!(run(&mut owner, &input)["ok"], false);
    assert_eq!(
        source_count(&owner),
        0,
        "source immediate ingest must not populate configured native review queue"
    );
}
#[test]
fn original717_imap_intake_poll_fake_happy_duplicate_and_revoked_finish() {
    use rentemester_runtime::imap::Client;
    struct Fake;
    impl Client for Fake {
        fn fetch(
            &mut self,
            _: &imap::Config,
            _: Option<imap::Cursor>,
        ) -> Result<Batch, imap::Failure> {
            Ok(batch(1))
        }
    }
    let (_dir, mut owner, cookies, proof, _) = setup();
    let config = || {
        imap::original717::resolve(&Default::default(), |k| match k {
            "RENTEMESTER_IMAP_HOST" => Some("mail.example.invalid".into()),
            "RENTEMESTER_IMAP_USERNAME" => Some("synthetic".into()),
            "RENTEMESTER_IMAP_PASSWORD" => Some("synthetic-secret".into()),
            _ => None,
        })
        .ok()
        .unwrap()
    };
    for expected in [1, 0] {
        let admission = owner
            .original717_prepare_imap_intake_poll(&cookies, &proof, &Fixed(NOW), config())
            .unwrap()
            .outcome
            .unwrap()
            .value;
        let outcome = Fake.fetch(admission.config(), None).unwrap();
        let mut input = original717_input(None);
        input.messages[0].base64 = imap::original717::encode_eml(&outcome.messages[0].raw);
        let value = owner
            .original717_finish_imap_intake_poll(&cookies, &proof, &Fixed(NOW), admission, &input)
            .unwrap()
            .outcome
            .unwrap()
            .value;
        assert_eq!(value["exceptionsCreated"], expected);
    }
    let admission = owner
        .original717_prepare_imap_intake_poll(&cookies, &proof, &Fixed(NOW), config())
        .unwrap()
        .outcome
        .unwrap()
        .value;
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u')",[]).unwrap();
    assert!(
        owner
            .original717_finish_imap_intake_poll(
                &cookies,
                &proof,
                &Fixed(NOW),
                admission,
                &original717_input(None)
            )
            .unwrap()
            .outcome
            .is_err()
    );
}
#[test]
fn original717_mail_scanner_exact_admission_missing_mismatch_success_and_revocation() {
    use rentemester_application::documents::ScanPolicy;
    use rentemester_engine::document_scan::ScanEvidence;
    let (_dir, mut owner, cookies, proof, _) = setup();
    let input = original717_input(Some(
        json!({"documentType":"cash_register_receipt","issueDate":"2026-01-01","amountIncVat":125,"vatAmount":25,"currency":"DKK","deliveryDescription":"Synthetic purchase","sender":{"name":"Synthetic vendor","vatOrCvr":"DK12345678","countryCode":"DK","identifierKind":"dk_cvr"}}),
    ));
    for case in ["missing", "mismatch", "success", "revoked"] {
        let mut request = input.clone();
        let admission = owner
            .original717_prepare_mail_ingest(
                &cookies,
                &proof,
                &Fixed(NOW),
                request.clone(),
                ScanPolicy::Required,
                None,
            )
            .unwrap()
            .outcome
            .unwrap()
            .value;
        assert_eq!(admission.files().len(), 1);
        let hash = admission.files()[0].hash().to_owned();
        let scans = match case {
            "missing" => vec![],
            "mismatch" => {
                vec![ScanEvidence::new(&"a".repeat(64), "synthetic", None, None).unwrap()]
            }
            _ => vec![ScanEvidence::new(&hash, "synthetic", None, None).unwrap()],
        };
        // Changing the adapter's original struct cannot change admitted bytes/metadata.
        request.metadata = None;
        assert!(request.metadata.is_none());
        if case == "revoked" {
            owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u')",[]).unwrap();
        }
        let result = owner
            .original717_finish_mail_ingest(&cookies, &proof, &Fixed(NOW), admission, scans)
            .unwrap()
            .outcome;
        if case == "success" {
            assert_eq!(result.unwrap().value["attachmentsIngested"], 1);
            let scans:i64=owner.held.0.as_ref().unwrap().company.as_ref().unwrap().query_row("SELECT count(*) FROM document_scan_evidence WHERE scanner_id='synthetic' AND result='clean'",[],|r|r.get(0)).unwrap();
            assert_eq!(scans, 1);
        } else {
            assert!(result.is_err(), "{case}");
        }
    }
}
#[test]
fn original717_imap_oversize_exception_is_idempotent_and_current_authority_bound() {
    let (_dir, mut owner, cookies, proof, _) = setup();
    let config = imap::original717::resolve(&Default::default(), |k| match k {
        "RENTEMESTER_IMAP_HOST" => Some("mail.example.invalid".into()),
        "RENTEMESTER_IMAP_USERNAME" => Some("synthetic".into()),
        "RENTEMESTER_IMAP_PASSWORD" => Some("synthetic-secret".into()),
        _ => None,
    })
    .ok()
    .unwrap();
    let poll = owner
        .original717_prepare_imap_intake_poll(&cookies, &proof, &Fixed(NOW), config)
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let refused = vec![imap::Refusal {
        uid: 34,
        declared_size: 26 * 1024 * 1024,
        code: "imap_message_too_large".into(),
    }];
    for created in [1, 0] {
        let result = owner
            .original717_oversized_mail(&cookies, &proof, &Fixed(NOW), &poll, 11, &refused)
            .unwrap()
            .outcome
            .unwrap()
            .value;
        assert_eq!(result["ok"], true);
        assert_eq!(result["messagesProcessed"], 1);
        assert_eq!(result["exceptionsCreated"], created);
    }
    let company = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
    let evidence: String = company
        .query_row(
            "SELECT source_evidence FROM exceptions WHERE type='MAIL_INTAKE_TOO_LARGE'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let evidence: Value = serde_json::from_str(&evidence).unwrap();
    assert_eq!(evidence["uid"], 34);
    assert_eq!(evidence["rawRetained"], false);
    assert_eq!(evidence["rawByteSize"], 26 * 1024 * 1024);
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u')",[]).unwrap();
    assert!(
        owner
            .original717_oversized_mail(&cookies, &proof, &Fixed(NOW), &poll, 11, &refused)
            .unwrap()
            .outcome
            .is_err()
    );
}
