use super::*;
use rentemester_runtime::auth::{AuthError, CookieProfile, CookieVerifier};
struct Fixed(i64);
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(self.0)
    }
}
fn selected() -> CompanySelection<'static> {
    CompanySelection {
        workspace: "w",
        slug: "a",
        registered: true,
        archived: false,
    }
}
fn auth() -> (tempfile::TempDir, Connection, Connection, VerifiedCookie) {
    let directory = tempfile::tempdir().unwrap();
    let mut control = Connection::open(directory.path().join("control.sqlite")).unwrap();
    let mut company = Connection::open(directory.path().join("company.sqlite")).unwrap();
    rentemester_storage::fresh::initialize_empty_pair(
        &mut control,
        &mut company,
        "Synthetic receipt authority",
    )
    .unwrap();
    control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt) VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','synthetic','u',1);INSERT INTO rm_workspace_mfa_events(user_id,event_type,actor,created_at)VALUES('u','mfa_enabled','system:better-auth','2026-01-01T00:00:00.000Z');INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId) VALUES('s','2026-01-01T12:00:00.000Z','synthetic-token','2026-01-01T00:01:00.000Z','2026-01-01T00:01:00.000Z','u');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','system:synthetic');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','system:synthetic');").unwrap();
    let vectors: Value = serde_json::from_str(include_str!(
        "../../../rentemester-runtime/fixtures/session-cookie.json"
    ))
    .unwrap();
    let cookies =
        CookieVerifier::new("w", CookieProfile::Secure, vectors["key"].as_str().unwrap()).unwrap();
    let proof = cookies
        .verify(vectors["vectors"][0]["header"].as_str(), None)
        .unwrap();
    (directory, control, company, proof)
}
#[test]
fn inventory_requires_current_owner_and_selected_admin_and_mapping_requires_fresh_session() {
    let (_directory, mut c, mut l, proof) = auth();
    let early = Fixed(1_767_225_900_000); // 00:05, inside fresh window
    assert_eq!(
        execute(
            &mut c,
            &mut l,
            &proof,
            &early,
            selected(),
            Action::Inventory
        )
        .unwrap()
        .value,
        json!({"restoredReceipts":{"rows":[]}})
    );
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'system:synthetic')",[]).unwrap();
    assert_eq!(
        execute(
            &mut c,
            &mut l,
            &proof,
            &early,
            selected(),
            Action::Inventory
        )
        .unwrap_err()
        .error
        .code,
        "access_denied"
    );
    c.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','system:synthetic')",[]).unwrap();
    let late = Fixed(1_767_226_800_000);
    let input = Reconcile {
        receipt_id: 1,
        expected_state_hash: "a".repeat(64),
        reason: "Review retained accepted source receipt".into(),
        confirm: true,
    };
    assert_eq!(
        execute(
            &mut c,
            &mut l,
            &proof,
            &late,
            selected(),
            Action::Reconcile(&input)
        )
        .unwrap_err()
        .error
        .code,
        "invalid_credentials"
    );
    assert!(execute(&mut c, &mut l, &proof, &late, selected(), Action::Inventory).is_ok());
    c.execute("INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','system:synthetic')",[]).unwrap();
    assert_eq!(
        execute(
            &mut c,
            &mut l,
            &proof,
            &early,
            selected(),
            Action::Inventory
        )
        .unwrap_err()
        .error
        .code,
        "access_denied"
    );
    assert_eq!(
        l.query_row(
            "SELECT count(*) FROM audit_log WHERE event_type='native_restore_receipt_reconciled'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}
