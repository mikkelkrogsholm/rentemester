use super::*;
use crate::control_fence::CompanySelection;
use rentemester_runtime::{
    auth::{AuthError, Clock, CookieProfile, CookieVerifier, VerifiedCookie},
    document::Prepared,
    namespace::NamespaceProfile,
};
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1_767_225_600_000)
    }
}
fn context<'a>(cookie: &'a VerifiedCookie, slug: &'a str) -> Context<'a> {
    Context {
        cookie,
        clock: &Fixed,
        selection: CompanySelection {
            workspace: "w",
            slug,
            registered: true,
            archived: false,
        },
    }
}
#[test]
fn json_preserves_read_effects_current_access_admin_backup_and_physical_evidence() {
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
        "Synthetic HTML <company>",
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
        "Synthetic HTML <company>",
    )
    .unwrap();
    control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','reader','user:u');").unwrap();
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
    let before = (control.total_changes(), company.total_changes());
    let first = read(
        &mut control,
        &mut company,
        &mut guard,
        context(&cookie, "a"),
        "2026-01-01",
        "2026-01-01",
    )
    .unwrap()
    .value;
    assert_eq!(first["company"]["name"], "Synthetic HTML <company>");
    assert_eq!(first["backup"]["available"], false);
    for field in [
        "backupDue",
        "backupsFound",
        "latestBackupAt",
        "daysSinceLatestBackup",
        "hasActivitySinceBackup",
    ] {
        assert!(first["backup"][field].is_null());
    }
    assert_eq!(
        first,
        read(
            &mut control,
            &mut company,
            &mut guard,
            context(&cookie, "a"),
            "2026-01-01",
            "2026-01-01"
        )
        .unwrap()
        .value
    );
    assert_eq!(before, (control.total_changes(), company.total_changes()));
    assert_eq!(
        read(
            &mut control,
            &mut company,
            &mut guard,
            context(&cookie, "other"),
            "2026-01-01",
            "2026-01-01"
        )
        .err()
        .unwrap()
        .error
        .code,
        "access_denied"
    );
    control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','owner','user:u')").unwrap();
    let before = (control.total_changes(), company.total_changes());
    let owner = read(
        &mut control,
        &mut company,
        &mut guard,
        context(&cookie, "a"),
        "2026-01-01",
        "2026-01-01",
    )
    .unwrap()
    .value;
    assert_eq!(owner["backup"]["available"], true);
    assert_eq!(owner["backup"]["backupsFound"], 0);
    assert!(owner["backup"]["backupDue"].is_boolean());
    assert!(owner["backup"]["latestBackupAt"].is_null());
    assert_eq!(owner["backup"]["scope"], "local-retained-snapshots");
    assert_eq!(before, (control.total_changes(), company.total_changes()));
    let original = Prepared::new(
        "synthetic-html.txt".into(),
        b"Synthetic HTML report source".to_vec(),
    )
    .unwrap();
    let metadata = serde_json::from_value(
        serde_json::json!({"source":"synthetic-html","documentType":"cash_register_receipt"}),
    )
    .unwrap();
    let doc = crate::documents::ingest(
        &mut control,
        &mut company,
        &mut guard,
        context(&cookie, "a"),
        crate::documents::Ingest {
            file: &original,
            metadata: &metadata,
            today: "2026-01-01",
        },
    )
    .unwrap()
    .value;
    let path: String = company
        .query_row(
            "SELECT stored_path FROM documents WHERE id=?1",
            [doc["document"]["id"].as_i64().unwrap()],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        read(
            &mut control,
            &mut company,
            &mut guard,
            context(&cookie, "a"),
            "2026-01-01",
            "2026-01-01"
        )
        .is_ok()
    );
    std::fs::write(root.join("a").join(path), b"Changed synthetic source").unwrap();
    let before = (control.total_changes(), company.total_changes());
    assert_eq!(
        read(
            &mut control,
            &mut company,
            &mut guard,
            context(&cookie, "a"),
            "2026-01-01",
            "2026-01-01"
        )
        .err()
        .unwrap()
        .error
        .code,
        "report_source_unavailable"
    );
    assert_eq!(before, (control.total_changes(), company.total_changes()));
    control.execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,actor)VALUES('u','a','revoke','user:u')").unwrap();
    assert_eq!(
        read(
            &mut control,
            &mut company,
            &mut guard,
            context(&cookie, "a"),
            "2026-01-01",
            "2026-01-01"
        )
        .err()
        .unwrap()
        .error
        .code,
        "access_denied"
    );
}
