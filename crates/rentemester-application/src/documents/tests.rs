//! Synthetic in-memory authority fixtures; no runtime or HTTP bypass in product.
use super::*;
use rentemester_runtime::auth::{AuthError, CookieProfile, CookieVerifier};
use serde_json::json;
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1767225600000)
    }
}
fn setup() -> (tempfile::TempDir, Connection, Connection, VerifiedCookie) {
    let dir = tempfile::tempdir().unwrap();
    let mut control = Connection::open(dir.path().join("control.sqlite")).unwrap();
    let mut company = Connection::open(dir.path().join("company.sqlite")).unwrap();
    rentemester_storage::fresh::initialize_empty_pair(&mut control, &mut company, "Synthetic scan")
        .unwrap();
    control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','bookkeeper','user:u');").unwrap();
    let cookies = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let cookie = cookies.issue_session("synthetic-token").unwrap();
    let proof = cookies
        .verify(Some(cookie.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    (dir, control, company, proof)
}
fn file() -> Prepared {
    Prepared::new(
        "synthetic.txt".into(),
        b"Synthetic immutable receipt".to_vec(),
    )
    .unwrap()
}
fn metadata() -> Metadata {
    serde_json::from_value(
        json!({"source":"synthetic-scan","documentType":"cash_register_receipt"}),
    )
    .unwrap()
}
fn context(cookie: &VerifiedCookie) -> Context<'_> {
    Context {
        cookie,
        clock: &Fixed,
        selection: CompanySelection {
            workspace: "w",
            slug: "a",
            registered: true,
            archived: false,
        },
    }
}
#[test]
fn preflight_is_effect_free_and_returns_released_transactions_with_immutable_source() {
    let (_dir, mut control, mut company, proof) = setup();
    let before = (control.total_changes(), company.total_changes());
    let admitted = prepare_ingest(
        &mut control,
        &mut company,
        context(&proof),
        file(),
        metadata(),
        ScanPolicy::Required,
    )
    .unwrap();
    assert!(admitted.control_reusable);
    assert!(control.is_autocommit() && company.is_autocommit());
    assert_eq!((control.total_changes(), company.total_changes()), before);
    assert_eq!(admitted.value.file().bytes(), file().bytes());
    assert_eq!(admitted.value.file().hash(), file().hash());
    control.execute_batch("BEGIN IMMEDIATE;ROLLBACK").unwrap();
    company.execute_batch("BEGIN IMMEDIATE;ROLLBACK").unwrap();
}
#[test]
fn current_authority_and_metadata_are_required_before_admission_exists() {
    let (_dir, mut control, mut company, proof) = setup();
    let invalid =
        serde_json::from_value(json!({"source":"synthetic","documentType":"purchase_sale"}))
            .unwrap();
    assert!(
        prepare_ingest(
            &mut control,
            &mut company,
            context(&proof),
            file(),
            invalid,
            ScanPolicy::Required
        )
        .is_err()
    );
    control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:synthetic-revoker');").unwrap();
    assert!(
        prepare_ingest(
            &mut control,
            &mut company,
            context(&proof),
            file(),
            metadata(),
            ScanPolicy::Required
        )
        .is_err()
    );
    assert!(control.is_autocommit() && company.is_autocommit());
    assert_eq!(
        company
            .query_row("SELECT count(*) FROM documents", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn opaque_admission_binds_subject_ledger_selection_and_exact_required_evidence() {
    let (_dir, mut control, mut company, proof) = setup();
    let admission = prepare_ingest(
        &mut control,
        &mut company,
        context(&proof),
        file(),
        metadata(),
        ScanPolicy::Required,
    )
    .unwrap()
    .value;
    let ledger = ledger_identity(&company).unwrap();
    assert!(admission.validate_binding("w", "a", "u", &ledger).is_ok());
    for (workspace, slug, subject, id) in [
        ("different", "a", "u", ledger.as_str()),
        ("w", "b", "u", ledger.as_str()),
        ("w", "a", "other-user", ledger.as_str()),
        ("w", "a", "u", "another-ledger"),
    ] {
        assert!(
            admission
                .validate_binding(workspace, slug, subject, id)
                .is_err()
        );
    }
    assert!(admission.validate_scan(None).is_err());
    let wrong = ScanEvidence::new(&"a".repeat(64), "synthetic", None, None).unwrap();
    assert!(admission.validate_scan(Some(&wrong)).is_err());
    let mut evidence = ScanEvidence::new(file().hash(), "synthetic", None, None).unwrap();
    assert!(admission.validate_scan(Some(&evidence)).is_ok());
    evidence.scanner_id = "\ninvalid".into();
    assert!(admission.validate_scan(Some(&evidence)).is_err());
    let off = prepare_ingest(
        &mut control,
        &mut company,
        context(&proof),
        file(),
        metadata(),
        ScanPolicy::Off,
    )
    .unwrap()
    .value;
    assert!(off.validate_scan(None).is_ok());
    assert!(off.validate_scan(Some(&wrong)).is_err());
}

#[test]
fn upload_capability_binds_all_identity_and_original_coordinates() {
    let capability = RegisteredUpload {
        workspace: "workspace".into(),
        slug: "company".into(),
        subject: "principal".into(),
        ledger: "ledger".into(),
        document_id: 71,
        hash: "source".into(),
        eligible: true,
    };
    assert!(capability.eligible_for_extraction());
    assert!(
        capability
            .validate_binding("workspace", "company", "principal", "ledger", "source")
            .is_ok()
    );
    for binding in [
        ("other", "company", "principal", "ledger", "source"),
        ("workspace", "other", "principal", "ledger", "source"),
        ("workspace", "company", "other", "ledger", "source"),
        ("workspace", "company", "principal", "other", "source"),
        ("workspace", "company", "principal", "ledger", "other"),
    ] {
        assert!(
            capability
                .validate_binding(binding.0, binding.1, binding.2, binding.3, binding.4)
                .is_err()
        );
    }
    assert_eq!(capability.document_id(), 71);
}

#[test]
fn upload_extraction_is_only_pdf_within_its_own_bound() {
    assert!(!extraction_eligible(&file()));
    let mut bytes = b"%PDF-1.4\n".to_vec();
    bytes.resize(10 * 1024 * 1024, b' ');
    assert!(extraction_eligible(
        &Prepared::new("limit.pdf".into(), bytes.clone()).unwrap()
    ));
    bytes.push(b' ');
    assert!(!extraction_eligible(
        &Prepared::new("larger.pdf".into(), bytes).unwrap()
    ));
}
