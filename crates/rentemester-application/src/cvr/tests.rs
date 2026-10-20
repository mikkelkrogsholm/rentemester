#![cfg(target_os = "macos")]
use super::*;
use rentemester_runtime::{
    auth::{AuthError, Clock, CookieProfile, CookieVerifier, VerifiedCookie},
    contact_import::LookupError,
    namespace::NamespaceProfile,
};
use serde_json::json;
use std::path::PathBuf;
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1767225600000)
    }
}
struct Fixture {
    _dir: tempfile::TempDir,
    control: Connection,
    company: Connection,
    guard: NamespaceGuard,
    cookie: VerifiedCookie,
    control_path: PathBuf,
    company_path: PathBuf,
}
fn fixture(role: &str) -> Fixture {
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
    let guard = NamespaceGuard::create_fresh_with_profile(
        &root,
        uid,
        "w",
        "a",
        "Synthetic CVR",
        "2026-01-01T00:00:00.000Z",
        NamespaceProfile::LocalUserMacos,
    )
    .unwrap();
    let control_path = root.join(".rentemester/workspace-control.sqlite");
    let company_path = root.join("a/data/ledger.sqlite");
    let mut control = Connection::open(&control_path).unwrap();
    let mut company = Connection::open(&company_path).unwrap();
    for db in [&control, &company] {
        db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL")
            .unwrap();
    }
    rentemester_storage::fresh::initialize_empty_pair(
        &mut control,
        &mut company,
        "Synthetic Local",
    )
    .unwrap();
    control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','user:u');").unwrap();
    control.execute("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant',?1,'user:u')",[role]).unwrap();
    company
        .execute("UPDATE companies SET cvr='87654321' WHERE id=1", [])
        .unwrap();
    let verifier = CookieVerifier::new(
        "w",
        CookieProfile::Development,
        "QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE",
    )
    .unwrap();
    let issued = verifier.issue_session("synthetic-token").unwrap();
    let cookie = verifier
        .verify(Some(issued.set_cookie().split(';').next().unwrap()), None)
        .unwrap();
    Fixture {
        _dir: dir,
        control,
        company,
        guard,
        cookie,
        control_path,
        company_path,
    }
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
struct Port {
    calls: usize,
    control: PathBuf,
    company: PathBuf,
    change: Option<&'static str>,
}
impl Lookup for Port {
    fn lookup(&mut self, _: &str) -> Result<Value, LookupError> {
        self.calls += 1;
        for path in [&self.control, &self.company] {
            let db = Connection::open(path).unwrap();
            db.busy_timeout(std::time::Duration::from_millis(50))
                .unwrap();
            db.execute_batch("BEGIN IMMEDIATE;ROLLBACK").unwrap();
        }
        match self.change {
            Some("revoke") => {
                Connection::open(&self.control).unwrap().execute_batch("INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','revoke',NULL,'user:synthetic-revoker')").unwrap();
            }
            Some("cvr") => {
                Connection::open(&self.company)
                    .unwrap()
                    .execute_batch("UPDATE companies SET cvr='12345678' WHERE id=1")
                    .unwrap();
            }
            Some("expire") => {
                Connection::open(&self.control)
                    .unwrap()
                    .execute_batch(
                        "UPDATE session SET expiresAt='2025-01-01T00:00:00.000Z' WHERE id='s'",
                    )
                    .unwrap();
            }
            _ => {}
        }
        Ok(
            json!({"cvrNummer":87654321,"virksomhedMetadata":{"nyesteNavn":{"navn":"Synthetic Provider"}}}),
        )
    }
}
fn port(f: &Fixture, change: Option<&'static str>) -> Port {
    Port {
        calls: 0,
        control: f.control_path.clone(),
        company: f.company_path.clone(),
        change,
    }
}
#[test]
fn current_bookkeeper_lookup_and_sync_release_sql_during_provider_and_cache_hit_is_effect_free() {
    let mut f = fixture("bookkeeper");
    let mut p = port(&f, None);
    let value = execute(
        &mut f.control,
        &mut f.company,
        &mut f.guard,
        context(&f.cookie),
        Action::Lookup("dk 87654321"),
        &mut p,
    )
    .unwrap();
    assert_eq!(value.value["cached"], false);
    assert_eq!(p.calls, 1);
    let changes = (f.control.total_changes(), f.company.total_changes());
    let value = execute(
        &mut f.control,
        &mut f.company,
        &mut f.guard,
        context(&f.cookie),
        Action::Lookup("87654321"),
        &mut p,
    )
    .unwrap();
    assert_eq!(value.value["cached"], true);
    assert_eq!(p.calls, 1);
    assert_eq!(
        (f.control.total_changes(), f.company.total_changes()),
        changes
    );
    let value = execute(
        &mut f.control,
        &mut f.company,
        &mut f.guard,
        context(&f.cookie),
        Action::Sync,
        &mut p,
    )
    .unwrap();
    assert_eq!(value.value["company"]["name"], "Synthetic Provider");
    assert_eq!(p.calls, 1);
    assert_eq!(
        f.company
            .query_row::<i64, _, _>("SELECT count(*) FROM journal_entries", [], |r| r.get(0))
            .unwrap(),
        0
    );
}
#[test]
fn reviewer_is_denied_before_provider_or_cache() {
    let mut f = fixture("reviewer");
    let mut p = port(&f, None);
    for action in [Action::Lookup("87654321"), Action::Sync] {
        assert!(
            execute(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                context(&f.cookie),
                action,
                &mut p
            )
            .is_err()
        );
    }
    assert_eq!(p.calls, 0);
    assert_eq!(
        f.company
            .query_row::<i64, _, _>("SELECT count(*) FROM cvr_lookups", [], |r| r.get(0))
            .unwrap(),
        0
    );
}
#[test]
fn midflight_revocation_expiry_and_own_cvr_changes_publish_no_provider_result() {
    for change in ["revoke", "expire", "cvr"] {
        let mut f = fixture("bookkeeper");
        let mut p = port(&f, Some(change));
        assert!(
            execute(
                &mut f.control,
                &mut f.company,
                &mut f.guard,
                context(&f.cookie),
                Action::Sync,
                &mut p
            )
            .is_err(),
            "{change}"
        );
        assert_eq!(p.calls, 1);
        assert_eq!(
            f.company
                .query_row::<i64, _, _>("SELECT count(*) FROM cvr_lookups", [], |r| r.get(0))
                .unwrap(),
            0
        );
        assert_eq!(
            f.company
                .query_row::<i64, _, _>(
                    "SELECT count(*) FROM audit_log WHERE event_type='company_cvr_sync'",
                    [],
                    |r| r.get(0)
                )
                .unwrap(),
            0
        );
        assert_eq!(
            f.company
                .query_row::<String, _, _>("SELECT name FROM companies WHERE id=1", [], |r| r
                    .get(0))
                .unwrap(),
            "Synthetic Local"
        );
    }
}
