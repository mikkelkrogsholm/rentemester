//! Synthetic unit fixtures only. Browser acceptance uses normal verification/MFA.
use super::*;
use rentemester_application::recurring_invoices::Action;
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
    (dir, owner, cookies, proof, uid)
}

const AT: &str = "2026-01-01T00:00:00Z";
fn input() -> rentemester_storage::recurring_invoices::original::Create {
    serde_json::from_value(json!({"name":"Synthetic mixed recurring","interval":"monthly","deliveryChannel":"digisense","firstIssueDate":"2025-11-30","deliveryPeriodMode":"interval_window","invoice":{"invoiceType":"full","seller":{"name":"Synthetic frozen seller","address":"Frozen1","vatOrCvr":"DK87654321"},"buyer":{"name":"Synthetic frozen buyer","address":"Frozen2"},"lines":[{"description":"Taxed","quantity":1,"unitPriceExVat":100,"lineTotalExVat":100,"taxClassification":"taxable","vatRate":0.25},{"description":"Exempt","quantity":1,"unitPriceExVat":20,"lineTotalExVat":20,"taxClassification":"exempt"}],"totals":{"netAmount":120,"vatAmount":25,"grossAmount":145}},"confirm":true})).unwrap()
}
fn call(
    owner: &mut Owner,
    proof: &VerifiedCookie,
    action: Action<'_>,
) -> Result<serde_json::Value, rentemester_application::control_fence::Failure> {
    owner
        .recurring_invoices(proof, &Fixed(NOW), action, "2026-01-01", AT)
        .unwrap()
        .outcome
        .map(|v| v.value)
}
fn prepare_profile(owner: &mut Owner) {
    owner.held.0.as_mut().unwrap().company.as_mut().unwrap().execute_batch("UPDATE companies SET address='Synthetic1',cvr='87654321',vat_period_type='quarter' WHERE id=1").unwrap();
}
#[test]
fn original_and_native_generation_share_immutable_mixed_source_and_current_authority() {
    let (_dir, mut owner, _cookies, proof, _uid) = setup();
    prepare_profile(&mut owner);
    let i = input();
    let id = call(&mut owner, &proof, Action::OriginalCreate(&i)).unwrap()["templateId"]
        .as_i64()
        .unwrap();
    let generated: rentemester_storage::recurring_invoices::original::Generate =
        serde_json::from_value(json!({"templateId":id,"asOfDate":"2025-11-30","confirm":true}))
            .unwrap();
    let first = call(&mut owner, &proof, Action::OriginalGenerate(&generated)).unwrap();
    assert_eq!(first["created"], true);
    assert_eq!(first["deliveryPeriodEnd"], "2025-12-30");
    let repeated = call(&mut owner, &proof, Action::OriginalGenerate(&generated)).unwrap();
    assert_eq!(repeated["documentId"], first["documentId"]);
    assert_eq!(repeated["created"], false);
    let native = rentemester_storage::recurring_invoices::Generate {
        as_of_date: "2025-12-31".into(),
        confirm: true,
    };
    let next = call(&mut owner, &proof, Action::Generate(id, &native)).unwrap();
    assert_eq!(next["created"], true);
    assert_eq!(next["periodIndex"], 1);
    assert_eq!(next["deliveryPeriodEnd"], "2026-01-30");
    let db = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
    let raw: String = db
        .query_row(
            "SELECT payload_json FROM documents WHERE id=?1",
            [next["documentId"].as_i64().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    let p: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(p["totals"]["grossAmount"], 145.0);
    assert_eq!(p["lines"][1]["taxClassification"], "exempt");
    assert_eq!(p["buyer"]["name"], "Synthetic frozen buyer");
    assert_eq!(
        db.query_row("SELECT count(*) FROM documents", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        4
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM journal_entries", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM peppol_submissions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    owner.held.0.as_mut().unwrap().control.as_mut().unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:u')").unwrap();
    assert!(call(&mut owner, &proof, Action::OriginalGenerate(&generated)).is_err());
}
#[test]
fn original_service_uses_typed_actor_and_current_revocation_before_even_dedup() {
    use rentemester_application::service_lifecycle::Operation;
    let (_dir, mut owner, cookies, proof, _uid) = setup();
    prepare_profile(&mut owner);
    let receipt = owner
        .service_lifecycle(
            &cookies,
            &Fixed(NOW),
            &proof,
            &Operation::Create {
                display_name: "Synthetic recurring executor".into(),
                operation_id: None,
                expires_in_seconds: None,
            },
        )
        .unwrap()
        .outcome
        .unwrap();
    let r: serde_json::Value = serde_json::from_str(receipt.as_json()).unwrap();
    let subject = r["serviceAccountId"].as_str().unwrap();
    let credential = r["credentialId"].as_str().unwrap();
    let key = ServiceKey::new("w", r["secret"].as_str().unwrap()).unwrap();
    let control = owner.held.0.as_mut().unwrap().control.as_mut().unwrap();
    control.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES(?1,'activate','member','user:u')",[subject]).unwrap();
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES(?1,'a','grant','bookkeeper','user:u')",[subject]).unwrap();
    let i = input();
    let id = owner
        .recurring_invoices_service(
            &key,
            &Fixed(NOW),
            Action::OriginalCreate(&i),
            "2026-01-01",
            AT,
        )
        .unwrap()
        .outcome
        .unwrap()
        .result["templateId"]
        .as_i64()
        .unwrap();
    let generated = rentemester_storage::recurring_invoices::original::Generate {
        template_id: id,
        as_of_date: "2025-11-30".into(),
        confirm: true,
    };
    let first = owner
        .recurring_invoices_service(
            &key,
            &Fixed(NOW),
            Action::OriginalGenerate(&generated),
            "2026-01-01",
            AT,
        )
        .unwrap()
        .outcome
        .unwrap()
        .result;
    assert_eq!(first["created"], true);
    let db = owner.held.0.as_ref().unwrap().company.as_ref().unwrap();
    assert_eq!(db.query_row("SELECT count(*) FROM audit_log WHERE event_type IN ('recurring_invoice_template_create','recurring_invoice_generate','invoice_issue','invoice_render_pdf') AND actor=?1",[format!("service_account:{subject}")],|r|r.get::<_,i64>(0)).unwrap(),4);
    owner
        .service_lifecycle(
            &cookies,
            &Fixed(NOW),
            &proof,
            &Operation::Revoke {
                subject: subject.into(),
                credential: credential.into(),
                operation_id: None,
            },
        )
        .unwrap()
        .outcome
        .unwrap();
    assert!(
        owner
            .recurring_invoices_service(
                &key,
                &Fixed(NOW),
                Action::OriginalGenerate(&generated),
                "2026-01-01",
                AT
            )
            .unwrap()
            .outcome
            .is_err()
    );
}
