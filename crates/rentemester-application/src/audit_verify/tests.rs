#![cfg(target_os = "macos")]
use super::*;
use rentemester_runtime::{
    auth::{AuthError, Clock, CookieProfile, CookieVerifier, VerifiedCookie},
    document::Prepared,
    namespace::NamespaceProfile,
};
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1767225600000)
    }
}
#[test]
fn whole_read_preserves_state_detects_an_unposted_original_and_obeys_current_revocation() {
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
    let mut guard = NamespaceGuard::create_fresh_with_profile(
        &root,
        uid,
        "w",
        "a",
        "Synthetic audit",
        "2026-01-01T00:00:00.000Z",
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    let mut control = Connection::open(root.join(".rentemester/workspace-control.sqlite")).unwrap();
    let mut company = Connection::open(root.join("a/data/ledger.sqlite")).unwrap();
    for db in [&control, &company] {
        db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL")
            .unwrap();
    }
    rentemester_storage::fresh::initialize_empty_pair(
        &mut control,
        &mut company,
        "Synthetic audit",
    )
    .unwrap();
    control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','bookkeeper','user:u');").unwrap();
    let verifier = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let issued = verifier.issue_session("synthetic-token").unwrap();
    let cookie = verifier
        .verify(Some(issued.request_header()), None)
        .unwrap();
    fn context(cookie: &VerifiedCookie) -> Context<'_> {
        Context {
            cookie,
            clock: &Fixed,
            selection: crate::control_fence::CompanySelection {
                workspace: "w",
                slug: "a",
                registered: true,
                archived: false,
            },
        }
    }
    let before = (control.total_changes(), company.total_changes());
    let clean = read(&mut control, &mut company, &mut guard, context(&cookie))
        .unwrap()
        .value;
    assert_eq!(clean["audit"]["ok"], true);
    assert_eq!(clean["audit"]["available"], true);
    assert_eq!(clean["audit"]["components"].as_array().unwrap().len(), 5);
    assert_eq!((control.total_changes(), company.total_changes()), before);
    let file = Prepared::new(
        "synthetic-audit.txt".into(),
        b"Synthetic unposted original".to_vec(),
    )
    .unwrap();
    let metadata = serde_json::from_value(
        json!({"source":"synthetic-audit","documentType":"cash_register_receipt"}),
    )
    .unwrap();
    let result = crate::documents::ingest(
        &mut control,
        &mut company,
        &mut guard,
        context(&cookie),
        crate::documents::Ingest {
            file: &file,
            metadata: &metadata,
            today: "2026-01-01",
        },
    )
    .unwrap();
    let id = result.value["document"]["id"].as_i64().unwrap();
    let path: String = company
        .query_row(
            "SELECT stored_path FROM documents WHERE id=?1",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!std::path::Path::new(&path).is_absolute());
    let before = (control.total_changes(), company.total_changes());
    assert_eq!(
        read(&mut control, &mut company, &mut guard, context(&cookie))
            .unwrap()
            .value["audit"]["ok"],
        true
    );
    std::fs::write(root.join("a").join(path), b"Changed synthetic original").unwrap();
    let broken = read(&mut control, &mut company, &mut guard, context(&cookie))
        .unwrap()
        .value;
    assert_eq!(broken["ok"], true);
    assert_eq!(broken["audit"]["ok"], false);
    assert!(!broken["audit"]["errors"].as_array().unwrap().is_empty());
    assert_eq!((control.total_changes(), company.total_changes()), before);
    control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES('u','a','revoke','user:u')").unwrap();
    assert_eq!(
        read(&mut control, &mut company, &mut guard, context(&cookie))
            .unwrap_err()
            .error
            .code,
        "access_denied"
    );
}
