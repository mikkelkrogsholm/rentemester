//! Synthetic unit worker outcomes on a real private namespace; actual worker
//! and HTTP/MFA acceptance are qualified independently by the root.
use super::*;
use rentemester_engine::document_pdf::{self as pdf, OperationalError, ParseResult};
use rentemester_runtime::auth::{AuthError, CookieProfile, CookieVerifier};
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
fn identity() -> ParserIdentity {
    ParserIdentity {
        parser_id: "pdfium".into(),
        parser_version: "synthetic-adapter-1".into(),
        contract_version: pdf::NATIVE_CONTRACT.into(),
    }
}
fn register(owner: &mut Owner, proof: &VerifiedCookie) -> i64 {
    let bytes = rentemester_runtime::document::Prepared::new(
        "synthetic.pdf".into(),
        b"%PDF-1.4\nSynthetic original PDF unit fixture\n".to_vec(),
    )
    .unwrap();
    let metadata = serde_json::from_value(
        json!({"source":"synthetic-pdf","documentType":"cash_register_receipt"}),
    )
    .unwrap();
    owner
        .ingest_document(
            proof,
            &Fixed(NOW),
            rentemester_application::documents::Ingest {
                file: &bytes,
                metadata: &metadata,
                today: "2026-01-01",
            },
        )
        .unwrap()
        .outcome
        .unwrap()
        .value["document"]["id"]
        .as_i64()
        .unwrap()
}
fn prepare(owner: &mut Owner, proof: &VerifiedCookie, id: i64) -> Admission {
    let attempt = owner
        .prepare_document_pdf(proof, &Fixed(NOW), id, identity())
        .unwrap();
    assert!(!attempt.retired);
    attempt.outcome.unwrap().value
}
fn output(admission: &Admission) -> Completion {
    let original = rentemester_runtime::document::Prepared::new(
        "synthetic.pdf".into(),
        admission.worker_input().unwrap().to_vec(),
    )
    .unwrap();
    let source = original.hash();
    let result: ParseResult = serde_json::from_value(json!({"contractVersion":pdf::NATIVE_CONTRACT,"inputSha256":source,"status":"ok","errorCode":null,"pages":[{"pageNumber":1,"width":612,"height":792,"rotation":0,"text":"Synthetic Æ Ω 12.34","layout":[{"text":"Synthetic Æ Ω 12.34","x":20,"y":700,"width":120,"height":12,"font":"Synthetic embedded font"}]}]})).unwrap();
    Completion {
        identity: identity(),
        outcome: Ok(result),
    }
}
fn finish(
    owner: &mut Owner,
    proof: &VerifiedCookie,
    admission: Admission,
    result: Option<Completion>,
) -> Result<Value, rentemester_application::control_fence::Failure> {
    owner
        .finish_document_pdf(proof, &Fixed(NOW), admission, result)
        .unwrap()
        .outcome
        .map(|value| value.value)
}
fn read(
    owner: &mut Owner,
    proof: &VerifiedCookie,
    action: Read,
) -> Result<Value, rentemester_application::control_fence::Failure> {
    owner
        .document_pdf(proof, &Fixed(NOW), action)
        .unwrap()
        .outcome
        .map(|value| value.value)
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
fn original(owner: &Owner, id: i64) -> std::path::PathBuf {
    let db = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
    let path: String = db
        .query_row(
            "SELECT stored_path FROM documents WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    owner.root.join("a").join(path)
}
#[test]
fn parsed_read_cache_and_pending_are_source_bound_and_reads_have_no_side_effects() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let id = register(&mut owner, &proof);
    let pending = read(
        &mut owner,
        &proof,
        Read::Pending {
            identity: identity(),
            cursor: 0,
            limit: 10,
        },
    )
    .unwrap();
    assert_eq!(pending["documentIds"], json!([id]));
    let admission = prepare(&mut owner, &proof, id);
    let completion = output(&admission);
    let first = finish(&mut owner, &proof, admission, Some(completion)).unwrap();
    assert_eq!(first["parse"]["cached"], false);
    assert_eq!(count(&owner, "document_pdf_parse_results"), 1);
    assert_eq!(count(&owner, "document_pdf_parse_pages"), 1);
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 1);
    let status = read(&mut owner, &proof, Read::Status(id)).unwrap();
    assert_eq!(status["parse"]["status"], "ok");
    assert_eq!(status["parse"]["parserId"], "pdfium");
    let text = read(
        &mut owner,
        &proof,
        Read::Text {
            document_id: id,
            offset: 0,
            limit: 1,
        },
    )
    .unwrap();
    assert_eq!(text["pages"][0]["text"], "Synthetic Æ Ω 12.34");
    assert!(text["pages"][0].get("layout").is_none());
    assert!(text["pages"][0]["layoutHash"].as_str().unwrap().len() == 64);
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 1);
    let cached = prepare(&mut owner, &proof, id);
    assert!(cached.worker_input().is_none());
    let result = finish(&mut owner, &proof, cached, None).unwrap();
    assert_eq!(result["parse"]["cached"], true);
    assert_eq!(result["parse"]["resultHash"], first["parse"]["resultHash"]);
    assert_eq!(count(&owner, "document_pdf_parse_results"), 1);
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 2);
    assert_eq!(
        read(
            &mut owner,
            &proof,
            Read::Pending {
                identity: identity(),
                cursor: 0,
                limit: 10
            }
        )
        .unwrap()["documentIds"],
        json!([])
    );
    assert!(
        read(
            &mut owner,
            &proof,
            Read::Text {
                document_id: id,
                offset: 0,
                limit: 11
            }
        )
        .is_err()
    );
    assert_eq!(count(&owner, "journal_entries"), 0);
}
#[test]
fn owner_mutex_and_sql_are_released_during_worker_and_current_revoke_blocks_final() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let id = register(&mut owner, &proof);
    let shared = std::sync::Mutex::new(owner);
    let admission = {
        let mut owner = shared.lock().unwrap();
        prepare(&mut owner, &proof, id)
    };
    let completion = output(&admission);
    {
        let owner = shared.try_lock().unwrap();
        for path in [
            owner.root.join(".rentemester/workspace-control.sqlite"),
            owner.root.join("a/data/ledger.sqlite"),
        ] {
            let db = Connection::open(path).unwrap();
            db.busy_timeout(std::time::Duration::ZERO).unwrap();
            db.execute_batch("BEGIN IMMEDIATE;ROLLBACK").unwrap();
        }
        owner.held.0.as_ref().unwrap().control.as_ref().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:synthetic-revoker');").unwrap();
    }
    let mut owner = shared.lock().unwrap();
    assert_eq!(
        finish(&mut owner, &proof, admission, Some(completion))
            .err()
            .unwrap()
            .error
            .code,
        "access_denied"
    );
    assert_eq!(count(&owner, "document_pdf_parse_results"), 0);
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 0);
    assert!(read(&mut owner, &proof, Read::Status(id)).is_err());
}
#[test]
fn worker_failure_and_source_failure_commit_only_attempts_and_terminal_never_duplicates() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let id = register(&mut owner, &proof);
    let admission = prepare(&mut owner, &proof, id);
    let result = finish(
        &mut owner,
        &proof,
        admission,
        Some(Completion {
            identity: identity(),
            outcome: Err(OperationalError::Timeout),
        }),
    )
    .unwrap();
    assert_eq!(result["ok"], false);
    assert_eq!(result["errorCode"], "timeout");
    assert_eq!(
        read(&mut owner, &proof, Read::Status(id)).unwrap()["parse"],
        Value::Null
    );
    assert_eq!(count(&owner, "document_pdf_parse_results"), 0);
    std::fs::remove_file(original(&owner, id)).unwrap();
    let admission = prepare(&mut owner, &proof, id);
    assert!(admission.worker_input().is_none());
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 2);
    let result = finish(&mut owner, &proof, admission, None).unwrap();
    assert_eq!(result["errorCode"], "source_unavailable");
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 2);
    assert!(read(&mut owner, &proof, Read::Status(id)).is_err());
}
#[test]
fn source_changed_after_worker_is_tamper_and_cannot_publish_or_hide_in_read() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let id = register(&mut owner, &proof);
    let admission = prepare(&mut owner, &proof, id);
    let completion = output(&admission);
    std::fs::write(original(&owner, id), b"%PDF-1.4\nChanged physical original").unwrap();
    assert_eq!(
        finish(&mut owner, &proof, admission, Some(completion))
            .err()
            .unwrap()
            .error
            .code,
        "PDF_EVIDENCE_TAMPERED"
    );
    assert_eq!(count(&owner, "document_pdf_parse_results"), 0);
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 0);
    assert_eq!(
        read(&mut owner, &proof, Read::Status(id))
            .err()
            .unwrap()
            .error
            .code,
        "PDF_EVIDENCE_TAMPERED"
    );
}
#[test]
fn malformed_new_worker_payload_or_identity_is_retained_protocol_failure_not_result() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let id = register(&mut owner, &proof);
    let admission = prepare(&mut owner, &proof, id);
    let mut completion = output(&admission);
    completion.identity.parser_version = "different-worker".into();
    assert_eq!(
        finish(&mut owner, &proof, admission, Some(completion)).unwrap()["errorCode"],
        "protocol_violation"
    );
    let admission = prepare(&mut owner, &proof, id);
    let mut completion = output(&admission);
    completion.outcome.as_mut().unwrap().input_sha256 = "f".repeat(64);
    assert_eq!(
        finish(&mut owner, &proof, admission, Some(completion)).unwrap()["errorCode"],
        "protocol_violation"
    );
    assert_eq!(count(&owner, "document_pdf_parse_results"), 0);
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 2);
}
#[test]
fn concurrent_canonical_winner_is_reused_without_duplicate_results() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let id = register(&mut owner, &proof);
    let left = prepare(&mut owner, &proof, id);
    let right = prepare(&mut owner, &proof, id);
    let l = output(&left);
    let r = output(&right);
    let first = finish(&mut owner, &proof, left, Some(l)).unwrap();
    let second = finish(&mut owner, &proof, right, Some(r)).unwrap();
    assert_eq!(second["parse"]["cached"], true);
    assert_eq!(first["parse"]["resultHash"], second["parse"]["resultHash"]);
    assert_eq!(count(&owner, "document_pdf_parse_results"), 1);
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 2);
}
#[test]
fn result_pages_attempt_and_audit_roll_back_together_on_storage_failure() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let id = register(&mut owner, &proof);
    let admission = prepare(&mut owner, &proof, id);
    let completion = output(&admission);
    let before = count(&owner, "audit_log");
    owner.held.0.as_ref().unwrap().company.as_ref().unwrap().execute_batch("CREATE TEMP TRIGGER synthetic_pdf_audit_failure BEFORE INSERT ON audit_log WHEN NEW.event_type='document_pdf_parse_result' BEGIN SELECT RAISE(ABORT,'synthetic PDF audit fault'); END;").unwrap();
    assert!(finish(&mut owner, &proof, admission, Some(completion)).is_err());
    assert_eq!(count(&owner, "document_pdf_parse_results"), 0);
    assert_eq!(count(&owner, "document_pdf_parse_pages"), 0);
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 0);
    assert_eq!(count(&owner, "audit_log"), before);
    assert_eq!(count(&owner, "documents"), 1);
}
#[test]
fn another_selected_ledger_or_authenticated_subject_cannot_adopt_worker_output() {
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    let id = register(&mut owner, &proof);
    let admission = prepare(&mut owner, &proof, id);
    let completion = output(&admission);
    owner.select_company("b").unwrap();
    assert_eq!(
        finish(&mut owner, &proof, admission, Some(completion))
            .err()
            .unwrap()
            .error
            .code,
        "document_pdf_admission_changed"
    );
    owner.select_company("a").unwrap();
    let admission = prepare(&mut owner, &proof, id);
    let completion = output(&admission);
    owner.held.0.as_ref().unwrap().control.as_ref().unwrap().execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('v','Synthetic Other','v@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('fv','synthetic','[]','v',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('sv','v','other-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('v','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('v','a','grant','owner','user:u');").unwrap();
    let cookie = cookies.issue_session("other-token").unwrap();
    let other = cookies
        .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    assert_eq!(
        finish(&mut owner, &other, admission, Some(completion))
            .err()
            .unwrap()
            .error
            .code,
        "document_pdf_admission_changed"
    );
    assert_eq!(count(&owner, "document_pdf_parse_results"), 0);
    assert_eq!(count(&owner, "document_pdf_parse_attempts"), 0);
}
