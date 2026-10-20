//! Real namespace/transactions; synthetic auth rows are unit fixtures only.
use super::*;
use rentemester_engine::invoice_extraction::{ProviderFailure, ProviderIdentity, ProviderOutput};
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

fn identity() -> ProviderIdentity {
    ProviderIdentity {
        id: "http-json-v1".into(),
        version: "1".into(),
    }
}
fn prepare(owner: &mut Owner, proof: &VerifiedCookie, id: i64) -> Admission {
    let attempt = owner
        .prepare_invoice_extraction(proof, &Fixed(NOW), id, Some(identity()))
        .unwrap();
    assert!(!attempt.retired);
    attempt.outcome.unwrap().value
}
fn finish(
    owner: &mut Owner,
    proof: &VerifiedCookie,
    admission: Admission,
    outcome: Option<Result<ProviderOutput, ProviderFailure>>,
) -> Result<Value, rentemester_application::control_fence::Failure> {
    owner
        .finish_invoice_extraction(
            proof,
            &Fixed(NOW),
            admission,
            outcome.map(|outcome| Completion {
                identity: identity(),
                outcome,
            }),
        )
        .unwrap()
        .outcome
        .map(|r| r.value)
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
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
fn output() -> ProviderOutput {
    ProviderOutput{fields:vec![
        ("invoiceNumber",json!("SYN-70")),("supplierName",json!("Synthetic Supplier")),
        ("supplierCountry",json!("DK")),("supplierLegalId",json!("11223344")),("supplierLegalIdKind",json!("dk_cvr")),
        ("buyerName",json!("Synthetic A")),("buyerCountry",json!("DK")),("buyerLegalId",json!("55667788")),("buyerLegalIdKind",json!("dk_cvr")),
        ("invoiceDate",json!("2026-01-01")),("currency",json!("DKK")),("netAmount",json!(100)),("vatAmount",json!(25)),("grossAmount",json!(125))
    ].into_iter().map(|(key,value)|json!({"key":key,"value":value,"confidence":0.99,"page":1,"sourceText":format!("Cited {value}")})).collect()}
}
#[test]
fn completed_cache_read_has_no_new_attempt_or_domain_effects() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let id = register(&mut owner, &proof);
    let admission = prepare(&mut owner, &proof, id);
    assert!(admission.provider_input().is_some());
    assert_eq!(count(&owner, "invoice_extraction_attempts"), 1);
    let result = finish(&mut owner, &proof, admission, Some(Ok(output()))).unwrap();
    assert_eq!(result["extraction"]["status"], "completed", "{result}");
    let attempts = count(&owner, "invoice_extraction_attempts");
    let audit = count(&owner, "audit_log");
    let cached = prepare(&mut owner, &proof, id);
    assert!(cached.provider_input().is_none());
    let duplicate = finish(&mut owner, &proof, cached, None).unwrap();
    assert_eq!(duplicate, result);
    let read = owner
        .invoice_extraction(&proof, &Fixed(NOW), id)
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(read, result);
    assert_eq!(count(&owner, "invoice_extraction_attempts"), attempts);
    assert_eq!(count(&owner, "audit_log"), audit);
    assert_eq!(count(&owner, "journal_entries"), 0);
}
#[test]
fn released_owner_and_sql_allow_revocation_while_provider_is_blocked() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let id = register(&mut owner, &proof);
    let shared = std::sync::Mutex::new(owner);
    let admission = {
        let mut owner = shared.lock().unwrap();
        prepare(&mut owner, &proof, id)
    };
    let entered = std::sync::Barrier::new(2);
    let release = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let provider = scope.spawn(|| {
            entered.wait();
            release.wait();
            output()
        });
        entered.wait();
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
            owner.held.0.as_ref().unwrap().control.as_ref().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:synthetic-revoker')").unwrap();
        }
        release.wait();
        let mut owner = shared.lock().unwrap();
        let error = finish(
            &mut owner,
            &proof,
            admission,
            Some(Ok(provider.join().unwrap())),
        )
        .unwrap_err();
        assert_eq!(error.error.code, "access_denied");
        assert_eq!(count(&owner, "invoice_extraction_attempts"), 1);
        assert_eq!(count(&owner, "invoice_extraction_results"), 0);
        assert_eq!(count(&owner, "invoice_extraction_resolutions"), 0);
    });
}
#[test]
fn off_provider_authorizes_first_and_provider_failures_are_curated_evidence() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let id = register(&mut owner, &proof);
    let disabled = owner
        .prepare_invoice_extraction(&proof, &Fixed(NOW), id, None)
        .unwrap()
        .outcome
        .err()
        .unwrap();
    assert_eq!(disabled.error.code, "EXTRACTION_PROVIDER_UNAVAILABLE");
    assert_eq!(count(&owner, "invoice_extraction_attempts"), 0);
    let admission = prepare(&mut owner, &proof, id);
    let value = finish(
        &mut owner,
        &proof,
        admission,
        Some(Err(ProviderFailure::Unavailable)),
    )
    .unwrap();
    assert_eq!(value["extraction"]["status"], "needs_resolution");
    assert_eq!(count(&owner, "invoice_extraction_results"), 0);
    owner.held.0.as_ref().unwrap().control.as_ref().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reviewer','user:synthetic-owner')").unwrap();
    let denied = owner
        .prepare_invoice_extraction(&proof, &Fixed(NOW), id, None)
        .unwrap()
        .outcome
        .err()
        .unwrap();
    assert_eq!(denied.error.code, "access_denied");
    assert_eq!(count(&owner, "invoice_extraction_attempts"), 1);
}
#[test]
fn changed_context_or_selected_ledger_cannot_finish_an_admitted_attempt() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let id = register(&mut owner, &proof);
    let admission = prepare(&mut owner, &proof, id);
    owner.select_company("b").unwrap();
    assert_eq!(
        finish(&mut owner, &proof, admission, Some(Ok(output())))
            .unwrap_err()
            .error
            .code,
        "EXTRACTION_ADMISSION_CHANGED"
    );
    owner.select_company("a").unwrap();
    assert_eq!(count(&owner, "invoice_extraction_results"), 0);
    let admission = prepare(&mut owner, &proof, id);
    owner
        .held
        .0
        .as_ref()
        .unwrap()
        .company
        .as_ref()
        .unwrap()
        .execute("UPDATE companies SET name='Changed buyer' WHERE id=1", [])
        .unwrap();
    assert!(finish(&mut owner, &proof, admission, Some(Ok(output()))).is_err());
    assert_eq!(count(&owner, "invoice_extraction_results"), 0);
    assert_eq!(count(&owner, "invoice_extraction_resolutions"), 0);
}
#[test]
fn physical_source_change_blocks_completion_and_status_without_cleanup_mutation() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let id = register(&mut owner, &proof);
    let admission = prepare(&mut owner, &proof, id);
    let db = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
    let path: String = db
        .query_row("SELECT stored_path FROM documents WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .unwrap();
    std::fs::write(
        owner.root.join("a").join(path),
        b"%PDF-1.4\nChanged physical original",
    )
    .unwrap();
    assert!(finish(&mut owner, &proof, admission, Some(Ok(output()))).is_err());
    {
        let slots = owner.held.0.as_mut().unwrap();
        assert!(validate_live_sources(slots.company.as_ref().unwrap(), &mut slots.guard).is_err());
    }
    assert!(
        owner
            .invoice_extraction(&proof, &Fixed(NOW), id)
            .unwrap()
            .outcome
            .is_err()
    );
    assert_eq!(count(&owner, "invoice_extraction_attempts"), 1);
    assert_eq!(count(&owner, "invoice_extraction_results"), 0);
    assert_eq!(count(&owner, "invoice_extraction_resolutions"), 0);
}

#[test]
fn ordinary_image_status_is_null_but_extraction_refuses_without_attempt() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let file = rentemester_runtime::document::Prepared::new(
        "synthetic.png".into(),
        b"\x89PNG\r\n\x1a\nSynthetic receipt image".to_vec(),
    )
    .unwrap();
    let metadata = serde_json::from_value(
        json!({"source":"synthetic-image","documentType":"cash_register_receipt"}),
    )
    .unwrap();
    let id = owner
        .ingest_document(
            &proof,
            &Fixed(NOW),
            rentemester_application::documents::Ingest {
                file: &file,
                metadata: &metadata,
                today: "2026-01-01",
            },
        )
        .unwrap()
        .outcome
        .unwrap()
        .value["document"]["id"]
        .as_i64()
        .unwrap();
    let audit = count(&owner, "audit_log");
    let value = owner
        .invoice_extraction(&proof, &Fixed(NOW), id)
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert_eq!(value, json!({"ok":true,"extraction":null}));
    assert_eq!(count(&owner, "audit_log"), audit);
    let refusal = owner
        .prepare_invoice_extraction(&proof, &Fixed(NOW), id, Some(identity()))
        .unwrap()
        .outcome
        .err()
        .unwrap();
    assert_eq!(refusal.error.code, "EXTRACTION_SNAPSHOT_INVALID");
    assert_eq!(count(&owner, "invoice_extraction_attempts"), 0);
}

fn uploaded(
    owner: &mut Owner,
    proof: &VerifiedCookie,
    filename: &str,
    label: &str,
) -> rentemester_application::documents::RegisteredIngest {
    use rentemester_application::documents::ScanPolicy;
    let content = if filename.ends_with(".pdf") {
        format!("%PDF-1.4\nSynthetic registered upload {label}\n")
    } else {
        format!("Synthetic registered upload {label}\n")
    };
    let file = rentemester_runtime::document::Prepared::new(filename.into(), content.into_bytes())
        .unwrap();
    let metadata = serde_json::from_value(
        json!({"source":"synthetic-upload-extraction","documentType":"cash_register_receipt"}),
    )
    .unwrap();
    let admission = owner
        .prepare_document_ingest(proof, &Fixed(NOW), file, metadata, ScanPolicy::Off)
        .unwrap()
        .outcome
        .unwrap()
        .value;
    owner
        .finish_document_ingest_registered(proof, &Fixed(NOW), admission, None)
        .unwrap()
        .outcome
        .unwrap()
        .value
}
#[test]
fn automatic_upload_extraction_keeps_registration_through_provider_failure_and_no_ledger_effects() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let registered = uploaded(&mut owner, &proof, "uploaded.pdf", "provider-failure");
    let receipt = registered.receipt;
    assert_eq!(count(&owner, "documents"), 1);
    assert_eq!(count(&owner, "invoice_extraction_attempts"), 0);
    assert!(registered.upload.eligible_for_extraction());
    let admission = owner
        .prepare_uploaded_invoice_extraction(
            &proof,
            &Fixed(NOW),
            registered.upload,
            Some(identity()),
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    let failed = finish(
        &mut owner,
        &proof,
        admission,
        Some(Err(ProviderFailure::Unavailable)),
    )
    .unwrap();
    assert_eq!(failed["extraction"]["status"], "needs_resolution");
    assert!(receipt["document"]["id"].as_i64().is_some());
    assert_eq!(count(&owner, "documents"), 1);
    assert_eq!(count(&owner, "journal_entries"), 0);
    // The same ordinary upload still refuses its duplicate before any capability or provider execution.
    let file = rentemester_runtime::document::Prepared::new(
        "uploaded.pdf".into(),
        b"%PDF-1.4\nSynthetic registered upload provider-failure\n".to_vec(),
    )
    .unwrap();
    let metadata = serde_json::from_value(
        json!({"source":"synthetic-upload-extraction","documentType":"cash_register_receipt"}),
    )
    .unwrap();
    let admission = owner
        .prepare_document_ingest(
            &proof,
            &Fixed(NOW),
            file,
            metadata,
            rentemester_application::documents::ScanPolicy::Off,
        )
        .unwrap()
        .outcome
        .unwrap()
        .value;
    assert!(
        owner
            .finish_document_ingest_registered(&proof, &Fixed(NOW), admission, None)
            .unwrap()
            .outcome
            .is_err()
    );
    assert_eq!(count(&owner, "invoice_extraction_attempts"), 1);
}
#[test]
fn upload_capability_requires_current_authority_and_same_company_before_started_attempt() {
    for switch in [false, true] {
        let (_dir, mut owner, _cookies, proof, _uid) = setup();
        let registered = uploaded(&mut owner, &proof, "uploaded.pdf", "binding");
        if switch {
            owner.select_company("b").unwrap();
        } else {
            owner.held.0.as_ref().unwrap().control.as_ref().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:revoker')").unwrap();
        }
        assert!(
            owner
                .prepare_uploaded_invoice_extraction(
                    &proof,
                    &Fixed(NOW),
                    registered.upload,
                    Some(identity())
                )
                .unwrap()
                .outcome
                .is_err()
        );
        owner.select_company("a").unwrap();
        assert_eq!(count(&owner, "documents"), 1);
        assert_eq!(count(&owner, "invoice_extraction_attempts"), 0);
    }
}
#[test]
fn upload_extraction_releases_owner_then_refuses_revoked_finish_without_losing_upload() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let registered = uploaded(&mut owner, &proof, "uploaded.pdf", "held-revoke");
    let receipt = registered.receipt;
    let shared = std::sync::Mutex::new(owner);
    let admission = {
        let mut owner = shared.lock().unwrap();
        owner
            .prepare_uploaded_invoice_extraction(
                &proof,
                &Fixed(NOW),
                registered.upload,
                Some(identity()),
            )
            .unwrap()
            .outcome
            .unwrap()
            .value
    };
    {
        let owner = shared.try_lock().unwrap();
        let slots = owner.held.0.as_ref().unwrap();
        assert!(slots.control.as_ref().unwrap().is_autocommit());
        assert!(slots.company.as_ref().unwrap().is_autocommit());
        slots.control.as_ref().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:revoker')").unwrap();
    }
    let mut owner = shared.lock().unwrap();
    assert!(finish(&mut owner, &proof, admission, Some(Ok(output()))).is_err());
    assert_eq!(receipt["ok"], true);
    assert_eq!(count(&owner, "documents"), 1);
    assert_eq!(count(&owner, "invoice_extraction_attempts"), 1);
    assert_eq!(count(&owner, "invoice_extraction_results"), 0);
    assert_eq!(count(&owner, "journal_entries"), 0);
}
#[test]
fn nonpdf_and_disabled_upload_paths_do_not_start_extraction() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    let nonpdf = uploaded(&mut owner, &proof, "uploaded.txt", "unsupported");
    assert!(!nonpdf.upload.eligible_for_extraction());
    let disabled = uploaded(&mut owner, &proof, "uploaded.pdf", "disabled");
    assert!(
        owner
            .prepare_uploaded_invoice_extraction(&proof, &Fixed(NOW), disabled.upload, None)
            .unwrap()
            .outcome
            .is_err()
    );
    assert_eq!(count(&owner, "documents"), 2);
    assert_eq!(count(&owner, "invoice_extraction_attempts"), 0);
}
