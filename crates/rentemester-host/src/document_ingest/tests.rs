//! Real owned macOS namespace; synthetic identities are unit fixtures only.
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
fn file(label: &str) -> Prepared {
    Prepared::new(
        "synthetic.txt".into(),
        format!("Synthetic original {label}").into_bytes(),
    )
    .unwrap()
}
fn metadata() -> Metadata {
    serde_json::from_value(
        json!({"source":"synthetic-scan","documentType":"cash_register_receipt"}),
    )
    .unwrap()
}
fn admit(owner: &mut Owner, proof: &VerifiedCookie, policy: ScanPolicy, label: &str) -> Admission {
    let result = owner
        .prepare_document_ingest(proof, &Fixed(NOW), file(label), metadata(), policy)
        .unwrap();
    assert!(!result.retired);
    result.outcome.unwrap().value
}
fn count(owner: &Owner, table: &str) -> i64 {
    owner
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}
fn originals(owner: &Owner) -> usize {
    std::fs::read_dir(owner.root.join("a/documents/originals"))
        .map(|files| files.count())
        .unwrap_or(0)
}
fn clean(admission: &Admission) -> ScanEvidence {
    ScanEvidence::new(
        admission.file().hash(),
        "synthetic-provider",
        Some("v1"),
        Some("synthetic-clean-ref"),
    )
    .unwrap()
}
#[test]
fn split_releases_owner_mutex_and_sql_writers_and_rechecks_revoked_authority() {
    let (_dir, owner, _cookies, proof, _uid) = setup();
    let shared = std::sync::Mutex::new(owner);
    let admission = {
        let mut owner = shared.lock().unwrap();
        let admission = admit(&mut owner, &proof, ScanPolicy::Required, "revoked");
        assert_eq!(count(&owner, "documents"), 0);
        assert_eq!(originals(&owner), 0);
        admission
    };
    let evidence = clean(&admission);
    // A different operation can acquire the same Owner while provider I/O is held.
    // Actual public membership-revocation HTTP is covered by root integration.
    {
        let owner = shared.try_lock().unwrap();
        let slots = owner.held.0.as_ref().unwrap();
        assert!(slots.control.as_ref().unwrap().is_autocommit());
        assert!(slots.company.as_ref().unwrap().is_autocommit());
        for path in [
            owner.root.join(".rentemester/workspace-control.sqlite"),
            owner.root.join("a/data/ledger.sqlite"),
        ] {
            let db = Connection::open(path).unwrap();
            db.busy_timeout(std::time::Duration::ZERO).unwrap();
            db.execute_batch("BEGIN IMMEDIATE;ROLLBACK").unwrap();
        }
        slots.control.as_ref().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:synthetic-revoker');").unwrap();
    }
    let mut owner = shared.lock().unwrap();
    let failure = owner
        .finish_document_ingest(&proof, &Fixed(NOW), admission, Some(evidence))
        .unwrap()
        .outcome
        .err()
        .unwrap();
    assert_eq!(failure.error.code, "access_denied");
    assert_eq!(count(&owner, "documents"), 0);
    assert_eq!(count(&owner, "document_scan_evidence"), 0);
    assert_eq!(originals(&owner), 0);
}
#[test]
fn required_missing_wrong_hash_and_off_supplied_proof_refuse_before_publication() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    for (policy, supplied) in [
        (ScanPolicy::Required, false),
        (ScanPolicy::Required, true),
        (ScanPolicy::Off, true),
    ] {
        let admission = admit(&mut owner, &proof, policy, "refused");
        let evidence =
            supplied.then(|| ScanEvidence::new(&"f".repeat(64), "synthetic", None, None).unwrap());
        assert!(
            owner
                .finish_document_ingest(&proof, &Fixed(NOW), admission, evidence)
                .unwrap()
                .outcome
                .is_err()
        );
        assert_eq!(count(&owner, "documents"), 0);
        assert_eq!(count(&owner, "document_scan_evidence"), 0);
        assert_eq!(originals(&owner), 0);
    }
}
#[test]
fn clean_registration_is_atomic_and_original_exact_and_duplicate_semantics_unchanged() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let admission = admit(&mut owner, &proof, ScanPolicy::Required, "clean");
    let evidence = clean(&admission);
    let value = owner
        .finish_document_ingest(&proof, &Fixed(NOW), admission, Some(evidence))
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(count(&owner, "documents"), 1);
    assert_eq!(count(&owner, "document_scan_evidence"), 1);
    let id = value["document"]["id"].as_i64().unwrap();
    let original = owner
        .document_original(&proof, &Fixed(NOW), id)
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(original.bytes, file("clean").bytes());
    let admission = admit(&mut owner, &proof, ScanPolicy::Required, "clean");
    let evidence = clean(&admission);
    assert!(
        owner
            .finish_document_ingest(&proof, &Fixed(NOW), admission, Some(evidence))
            .unwrap()
            .outcome
            .is_err()
    );
    assert_eq!(count(&owner, "documents"), 1);
    assert_eq!(count(&owner, "document_scan_evidence"), 1);
    let db = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
    assert_eq!(
        db.query_row(
            "SELECT actor FROM audit_log WHERE event_type='document_ingest'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "user:u"
    );
    assert_eq!(
        db.query_row("SELECT result FROM document_scan_evidence", [], |row| {
            row.get::<_, String>(0)
        })
        .unwrap(),
        "clean"
    );
}
#[test]
fn different_company_or_authenticated_subject_cannot_consume_an_admission() {
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    let admission = admit(&mut owner, &proof, ScanPolicy::Required, "wrong-ledger");
    let evidence = clean(&admission);
    owner.select_company("b").unwrap();
    assert_eq!(
        owner
            .finish_document_ingest(&proof, &Fixed(NOW), admission, Some(evidence))
            .unwrap()
            .outcome
            .err()
            .unwrap()
            .error
            .code,
        "document_admission_changed"
    );
    owner.select_company("a").unwrap();
    let admission = admit(&mut owner, &proof, ScanPolicy::Required, "wrong-user");
    let evidence = clean(&admission);
    owner.held.0.as_ref().unwrap().control.as_ref().unwrap().execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('v','Synthetic Other','v@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('fv','synthetic','[]','v',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('sv','v','other-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('v','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('v','a','grant','owner','user:u');").unwrap();
    let cookie = cookies.issue_session("other-token").unwrap();
    let other = cookies
        .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    assert_eq!(
        owner
            .finish_document_ingest(&other, &Fixed(NOW), admission, Some(evidence))
            .unwrap()
            .outcome
            .err()
            .unwrap()
            .error
            .code,
        "document_admission_changed"
    );
    assert_eq!(count(&owner, "documents"), 0);
    assert_eq!(originals(&owner), 0);
}
#[test]
fn registration_failure_rolls_back_document_scan_audit_and_number_together() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let admission = admit(&mut owner, &proof, ScanPolicy::Required, "atomic-rollback");
    let evidence = clean(&admission);
    let before = count(&owner, "sequences");
    owner.held.0.as_ref().unwrap().company.as_ref().unwrap().execute_batch("CREATE TEMP TRIGGER synthetic_document_audit_failure BEFORE INSERT ON audit_log WHEN NEW.event_type='document_ingest' BEGIN SELECT RAISE(ABORT,'synthetic audit fault'); END;").unwrap();
    assert!(
        owner
            .finish_document_ingest(&proof, &Fixed(NOW), admission, Some(evidence))
            .unwrap()
            .outcome
            .is_err()
    );
    assert_eq!(count(&owner, "documents"), 0);
    assert_eq!(count(&owner, "document_scan_evidence"), 0);
    assert_eq!(count(&owner, "audit_log"), 0);
    assert_eq!(count(&owner, "sequences"), before);
    // Publication may leave an unregistered private content blob after rollback;
    // it is never deleted as though an uncertain competing commit were absent.
}

fn service(
    owner: &mut Owner,
    cookies: &CookieVerifier,
    proof: &VerifiedCookie,
    role: &str,
) -> (ServiceKey, String, String) {
    let control = owner.held.0.as_mut().unwrap().control.as_mut().unwrap();
    let issued = rentemester_application::service_lifecycle::execute(
        control,
        cookies,
        &Fixed(NOW),
        proof,
        &rentemester_application::service_lifecycle::Operation::Create {
            display_name: "Synthetic upload service".into(),
            operation_id: None,
            expires_in_seconds: None,
        },
    )
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(issued.as_json()).unwrap();
    let subject = value["serviceAccountId"].as_str().unwrap().to_owned();
    let credential = value["credentialId"].as_str().unwrap().to_owned();
    let key = ServiceKey::new("w", value["secret"].as_str().unwrap()).unwrap();
    control.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES(?1,'activate','member','user:u')",[&subject]).unwrap();
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant',?2,'user:u')",[&subject, role]).unwrap();
    control
        .execute("UPDATE apikey SET remaining=20 WHERE id=?1", [&credential])
        .unwrap();
    (key, subject, credential)
}
fn remaining(owner: &Owner, credential: &str) -> i64 {
    owner
        .held
        .0
        .as_ref()
        .unwrap()
        .control
        .as_ref()
        .unwrap()
        .query_row(
            "SELECT remaining FROM apikey WHERE id=?1",
            [credential],
            |r| r.get(0),
        )
        .unwrap()
}
#[test]
fn service_scan_split_charges_once_preserves_original_and_service_audit() {
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    let (key, subject, credential) = service(&mut owner, &cookies, &proof, "bookkeeper");
    let admission = owner
        .prepare_document_ingest_service(
            &key,
            &Fixed(NOW),
            file("service clean"),
            metadata(),
            ScanPolicy::Required,
        )
        .unwrap()
        .outcome
        .unwrap()
        .result;
    assert_eq!(remaining(&owner, &credential), 19);
    assert_eq!(originals(&owner), 0);
    let slots = owner.held.0.as_ref().unwrap();
    assert!(
        slots.control.as_ref().unwrap().is_autocommit()
            && slots.company.as_ref().unwrap().is_autocommit()
    );
    let evidence = clean(&admission);
    let receipt = owner
        .finish_document_ingest_service(&key, &Fixed(NOW), admission, Some(evidence))
        .unwrap()
        .outcome
        .unwrap();
    assert_eq!(remaining(&owner, &credential), 19);
    assert_eq!(count(&owner, "documents"), 1);
    assert_eq!(count(&owner, "document_scan_evidence"), 1);
    let id = receipt.result["document"]["id"].as_i64().unwrap();
    let original = owner
        .document_original_service(&key, &Fixed(NOW), id)
        .unwrap()
        .outcome
        .unwrap()
        .result;
    assert_eq!(original.bytes, file("service clean").bytes());
    let actor: String = owner
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .query_row(
            "SELECT actor FROM audit_log WHERE event_type='document_ingest'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(actor, format!("service_account:{subject}"));
}
#[test]
fn service_upload_reader_denied_before_usage_and_revoked_credential_after_scan_cannot_publish() {
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    let (key, _, credential) = service(&mut owner, &cookies, &proof, "reader");
    let failure = owner
        .prepare_document_ingest_service(
            &key,
            &Fixed(NOW),
            file("reader"),
            metadata(),
            ScanPolicy::Off,
        )
        .unwrap()
        .outcome
        .err()
        .unwrap();
    assert_eq!(failure.error.code, "access_denied");
    assert_eq!(remaining(&owner, &credential), 20);
    assert_eq!(originals(&owner), 0);
    let (key, _, credential) = service(&mut owner, &cookies, &proof, "bookkeeper");
    let admission = owner
        .prepare_document_ingest_service(
            &key,
            &Fixed(NOW),
            file("revoked service"),
            metadata(),
            ScanPolicy::Required,
        )
        .unwrap()
        .outcome
        .unwrap()
        .result;
    let evidence = clean(&admission);
    owner
        .held
        .0
        .as_ref()
        .unwrap()
        .control
        .as_ref()
        .unwrap()
        .execute("UPDATE apikey SET enabled=0 WHERE id=?1", [&credential])
        .unwrap();
    let failure = owner
        .finish_document_ingest_service(&key, &Fixed(NOW), admission, Some(evidence))
        .unwrap()
        .outcome
        .err()
        .unwrap();
    assert_eq!(failure.error.code, "invalid_credentials");
    assert_eq!(remaining(&owner, &credential), 19);
    assert_eq!(count(&owner, "documents"), 0);
    assert_eq!(originals(&owner), 0);
}

#[test]
fn documents_original_vendor_rechecked_after_io_and_current_human_permissions() {
    use rentemester_application::documents::OriginalOptions;
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    owner
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .execute("INSERT INTO vendors(name) VALUES('Synthetic vendor')", [])
        .unwrap();
    let admission = owner
        .prepare_documents_original(
            &proof,
            &Fixed(NOW),
            file("vendor changed"),
            metadata(),
            ScanPolicy::Required,
            OriginalOptions {
                vendor_id: Some(1),
                force: false,
                original_api: true,
            },
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let evidence = clean(&admission);
    owner
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .execute("UPDATE vendors SET name='Changed vendor' WHERE id=1", [])
        .unwrap();
    assert!(
        owner
            .finish_document_ingest(&proof, &Fixed(NOW), admission, Some(evidence))
            .unwrap()
            .outcome
            .is_err()
    );
    assert_eq!(count(&owner, "documents"), 0);
    assert_eq!(originals(&owner), 0);
    let admission = owner
        .prepare_documents_original(
            &proof,
            &Fixed(NOW),
            file("rights changed"),
            metadata(),
            ScanPolicy::Required,
            OriginalOptions::default(),
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let evidence = clean(&admission);
    owner.held.0.as_ref().unwrap().control.as_ref().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:synthetic-revoker');").unwrap();
    assert!(
        owner
            .finish_document_ingest(&proof, &Fixed(NOW), admission, Some(evidence))
            .unwrap()
            .outcome
            .is_err()
    );
    assert_eq!(count(&owner, "documents"), 0);
    assert_eq!(originals(&owner), 0);
    assert!(
        owner
            .documents_original(
                &proof,
                &Fixed(NOW),
                rentemester_application::documents_original::Action::List
            )
            .unwrap()
            .outcome
            .is_err()
    );
}
#[test]
fn documents_original_service_typed_audit_and_revocation_before_and_after_io() {
    use rentemester_application::documents::OriginalOptions;
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    let (key, subject, credential) = service(&mut owner, &cookies, &proof, "bookkeeper");
    let admission = owner
        .prepare_documents_original_service(
            &key,
            &Fixed(NOW),
            file("original service"),
            metadata(),
            ScanPolicy::Required,
            OriginalOptions {
                original_api: true,
                ..Default::default()
            },
        )
        .unwrap()
        .outcome
        .unwrap()
        .result;
    let evidence = clean(&admission);
    let receipt = owner
        .finish_document_ingest_service(&key, &Fixed(NOW), admission, Some(evidence))
        .unwrap()
        .outcome
        .unwrap()
        .result;
    assert_eq!(receipt["ok"], true);
    let id = receipt["document"]["id"].as_i64().unwrap();
    let enriched: Metadata = serde_json::from_value(json!({"source":"synthetic-scan","documentType":"cash_register_receipt","deliveryDescription":"Synthetic service enrichment"})).unwrap();
    let mutation = owner
        .documents_original_service(
            &key,
            &Fixed(NOW),
            rentemester_application::documents_original::Action::Enrich(id, Box::new(enriched)),
        )
        .unwrap();
    assert!(!mutation.retired);
    assert_eq!(mutation.outcome.unwrap().result["enriched"], true);
    let db = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
    let enrichment_actor: String = db
        .query_row(
            "SELECT actor FROM document_metadata_enrichments WHERE document_id=?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(enrichment_actor, format!("service_account:{subject}"));
    let audit: String = db
        .query_row(
            "SELECT actor FROM audit_log WHERE event_type='document_ingest'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(audit, format!("service_account:{subject}"));
    let admission = owner
        .prepare_documents_original_service(
            &key,
            &Fixed(NOW),
            file("revoked original service"),
            metadata(),
            ScanPolicy::Required,
            OriginalOptions::default(),
        )
        .unwrap()
        .outcome
        .unwrap()
        .result;
    let evidence = clean(&admission);
    owner
        .held
        .0
        .as_ref()
        .unwrap()
        .control
        .as_ref()
        .unwrap()
        .execute("UPDATE apikey SET enabled=0 WHERE id=?1", [&credential])
        .unwrap();
    assert!(
        owner
            .finish_document_ingest_service(&key, &Fixed(NOW), admission, Some(evidence))
            .unwrap()
            .outcome
            .is_err()
    );
    assert_eq!(count(&owner, "documents"), 1);
    assert!(
        owner
            .prepare_documents_original_service(
                &key,
                &Fixed(NOW),
                file("already revoked"),
                metadata(),
                ScanPolicy::Required,
                OriginalOptions::default()
            )
            .unwrap()
            .outcome
            .is_err()
    );
}
#[test]
fn documents_original_service_pdf_current_grants_before_after_worker_io() {
    use rentemester_application::document_pdf::{Completion, Read};
    use rentemester_engine::document_pdf::{NATIVE_CONTRACT, OperationalError, ParserIdentity};
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    let pdf = Prepared::new(
        "synthetic.pdf".into(),
        b"%PDF-1.4\nSynthetic PDF evidence\n".to_vec(),
    )
    .unwrap();
    let admission = owner
        .prepare_document_ingest(&proof, &Fixed(NOW), pdf, metadata(), ScanPolicy::Off)
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let id = owner
        .finish_document_ingest(&proof, &Fixed(NOW), admission, None)
        .unwrap()
        .outcome
        .unwrap()
        .value["document"]["id"]
        .as_i64()
        .unwrap();
    let (key, subject, _credential) = service(&mut owner, &cookies, &proof, "bookkeeper");
    let identity = ParserIdentity {
        parser_id: "pdfium".into(),
        parser_version: "synthetic-adapter-1".into(),
        contract_version: NATIVE_CONTRACT.into(),
    };
    let admission = owner
        .prepare_document_pdf_service(&key, &Fixed(NOW), id, identity.clone())
        .unwrap()
        .outcome
        .unwrap()
        .result;
    assert!(admission.worker_input().is_some());
    let value = owner
        .finish_document_pdf_service(
            &key,
            &Fixed(NOW),
            admission,
            Some(Completion {
                identity: identity.clone(),
                outcome: Err(OperationalError::ProtocolViolation),
            }),
        )
        .unwrap()
        .outcome
        .unwrap()
        .result;
    assert_eq!(value["ok"], false);
    assert!(
        owner
            .document_pdf_service(&key, &Fixed(NOW), Read::Status(id))
            .unwrap()
            .outcome
            .is_ok()
    );
    let admission = owner
        .prepare_document_pdf_service(&key, &Fixed(NOW), id, identity.clone())
        .unwrap()
        .outcome
        .unwrap()
        .result;
    owner.held.0.as_ref().unwrap().control.as_ref().unwrap().execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','revoke',NULL,'user:synthetic-revoker')",[&subject]).unwrap();
    assert!(
        owner
            .finish_document_pdf_service(
                &key,
                &Fixed(NOW),
                admission,
                Some(Completion {
                    identity: identity.clone(),
                    outcome: Err(OperationalError::ProtocolViolation)
                })
            )
            .unwrap()
            .outcome
            .is_err()
    );
    assert!(
        owner
            .document_pdf_service(&key, &Fixed(NOW), Read::Status(id))
            .unwrap()
            .outcome
            .is_err()
    );
    assert!(
        owner
            .prepare_document_pdf_service(&key, &Fixed(NOW), id, identity)
            .unwrap()
            .outcome
            .is_err()
    );
    let db = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
    let actor: String = db
        .query_row(
            "SELECT actor FROM document_pdf_parse_attempts LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(actor, format!("service_account:{subject}"));
}
#[test]
fn documents_original_duplicate_refusal_deduplicated_and_force_never_content_hash() {
    use rentemester_application::documents::OriginalOptions;
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let admission = admit(&mut owner, &proof, ScanPolicy::Off, "same bytes");
    owner
        .finish_document_ingest(&proof, &Fixed(NOW), admission, None)
        .unwrap()
        .outcome
        .unwrap();
    let admission = owner
        .prepare_documents_original(
            &proof,
            &Fixed(NOW),
            file("same bytes"),
            metadata(),
            ScanPolicy::Off,
            OriginalOptions {
                force: true,
                original_api: true,
                ..Default::default()
            },
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let refusal = owner
        .finish_document_ingest(&proof, &Fixed(NOW), admission, None)
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(refusal["ok"], false);
    assert_eq!(refusal["code"], "DOCUMENT_INGEST_BLOCKED");
    let admission = owner
        .prepare_documents_original(
            &proof,
            &Fixed(NOW),
            file("same bytes"),
            metadata(),
            ScanPolicy::Off,
            OriginalOptions {
                force: true,
                original_api: true,
                ..Default::default()
            },
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(
        owner
            .finish_document_ingest(&proof, &Fixed(NOW), admission, None)
            .unwrap()
            .outcome
            .unwrap()
            .value["ok"],
        false
    );
    assert_eq!(count(&owner, "documents"), 1);
    assert_eq!(count(&owner, "exceptions"), 1);
}

#[test]
fn service_access_audit_served_denied_and_fail_closed_with_actual_principal() {
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    let (key, subject, _credential) = service(&mut owner, &cookies, &proof, "bookkeeper");
    let receipt = owner
        .ingest_document(
            &proof,
            &Fixed(NOW),
            rentemester_application::documents::Ingest {
                file: &file("audited service"),
                metadata: &metadata(),
                today: "2026-01-01",
            },
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let id = receipt["document"]["id"].as_i64().unwrap();
    let bytes = owner
        .document_original_service(&key, &Fixed(NOW), id)
        .unwrap()
        .outcome
        .unwrap()
        .result
        .bytes;
    assert_eq!(bytes, file("audited service").bytes());
    {
        let c = owner.held.0.as_mut().unwrap().control.as_mut().unwrap();
        let actor=c.query_row("SELECT actor FROM native_service_document_access_events WHERE outcome='served' AND resource_id=?1",[id],|r|r.get::<_,String>(0)).unwrap();
        assert_eq!(actor, format!("service_account:{subject}"));
        c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','revoke',NULL,'user:u')",[&subject]).unwrap();
    }
    assert!(
        owner
            .document_original_service(&key, &Fixed(NOW), id)
            .unwrap()
            .outcome
            .is_err()
    );
    {
        let c = owner.held.0.as_mut().unwrap().control.as_mut().unwrap();
        assert_eq!(c.query_row("SELECT count(*) FROM native_service_document_access_events WHERE outcome='denied' AND reason_code='authorization_denied'",[],|r|r.get::<_,i64>(0)).unwrap(),1);
        c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','bookkeeper','user:u')",[&subject]).unwrap();
        c.execute_batch("INSERT INTO native_service_document_access_events(id,actor,company_slug,resource_type,resource_id,outcome,reason_code)VALUES(9223372036854775807,'service_account:synthetic','a','document_file',1,'served','authorized')").unwrap();
    }
    assert!(
        owner
            .document_original_service(&key, &Fixed(NOW), id)
            .unwrap()
            .outcome
            .is_err()
    );
    assert_eq!(count(&owner, "documents"), 1);
    assert_eq!(count(&owner, "journal_entries"), 0);
}
