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
