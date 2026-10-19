use super::*;
use rentemester_runtime::auth::{AuthError, Clock, CookieProfile, CookieVerifier};
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1_767_225_900_000)
    }
}
#[test]
fn current_membership_and_transfer_fence_precede_registry_or_company_effects() {
    let mut c = Connection::open_in_memory().unwrap();
    let mut l = Connection::open_in_memory().unwrap();
    rentemester_storage::fresh::initialize_empty_pair(
        &mut c,
        &mut l,
        "Synthetic linking authority",
    )
    .unwrap();
    c.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','synthetic','u',1);INSERT INTO rm_workspace_mfa_events(user_id,event_type,actor,created_at)VALUES('u','mfa_enabled','system:better-auth','2026-01-01T00:00:00.000Z');INSERT INTO session(id,expiresAt,token,createdAt,updatedAt,userId)VALUES('s','2026-01-01T12:00:00.000Z','synthetic-token','2026-01-01T00:01:00.000Z','2026-01-01T00:01:00.000Z','u');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','workspace_owner','system:synthetic');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','system:synthetic');").unwrap();
    let vectors: Value = serde_json::from_str(include_str!(
        "../../../rentemester-runtime/fixtures/session-cookie.json"
    ))
    .unwrap();
    let cookies =
        CookieVerifier::new("w", CookieProfile::Secure, vectors["key"].as_str().unwrap()).unwrap();
    let proof = cookies
        .verify(vectors["vectors"][0]["header"].as_str(), None)
        .unwrap();
    let context = Context {
        cookie: &proof,
        clock: &Fixed,
        selection: control_fence::CompanySelection {
            workspace: "w",
            slug: "a",
            registered: true,
            archived: false,
        },
    };
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    assert_eq!(authorize(&tx, &context, true).unwrap(), "u");
    rentemester_storage::workspace_transfer::append(
        &tx,
        rentemester_storage::workspace_transfer::Fact::Frozen {
            transfer_id: "1".repeat(32),
            source_instance_id: "2".repeat(32),
            workspace_id: "w".into(),
            destination: "https://synthetic.invalid".into(),
            reason: "Synthetic explicit frozen workspace".into(),
            companies: vec![rentemester_storage::workspace_transfer::Company {
                slug: "a".into(),
                ledger_uuid: "3".repeat(32),
            }],
        },
        "user:u",
        "2026-01-01T00:05:00.000Z",
    )
    .unwrap();
    assert_eq!(
        authorize(&tx, &context, true).unwrap_err().code,
        "workspace_transfer_fenced"
    );
    assert!(authorize(&tx, &context, false).is_ok());
    tx.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'system:synthetic')",[]).unwrap();
    assert_eq!(
        authorize(&tx, &context, false).unwrap_err().code,
        "access_denied"
    );
    tx.rollback().unwrap();
    assert_eq!(
        l.query_row("SELECT count(*) FROM document_party_link_events", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
