//! Integration contract: wire `#[cfg(all(test, target_os = "macos"))] mod crash_tests;`
//! and checkpoint("published") after publication, checkpoint("registered") after
//! register_original, checkpoint("committed") only after the fence returns Ok in
//! finish_ingest_registered. No production hooks and no environment bypass.
use super::*;
use rentemester_runtime::{
    auth::{AuthError, CookieProfile, CookieVerifier},
    namespace::NamespaceProfile,
};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
pub(super) fn checkpoint(phase: &str) {
    if std::env::var("RM_REGISTRATION_PHASE").as_deref() == Ok(phase) {
        std::fs::write(std::env::var("RM_REGISTRATION_MARKER").unwrap(), phase).unwrap();
        loop {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
struct Fixed;
impl Clock for Fixed {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(1767225600000)
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
fn file(label: &str) -> Prepared {
    Prepared::new(
        format!("{label}.txt"),
        format!("Synthetic registration death {label}").into_bytes(),
    )
    .unwrap()
}
fn metadata() -> Metadata {
    serde_json::from_value(
        serde_json::json!({"source":"synthetic-death", "documentType":"cash_register_receipt"}),
    )
    .unwrap()
}
fn intake(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    cookie: &VerifiedCookie,
    label: &str,
) -> Result<Committed<RegisteredIngest>, Failure> {
    let admission = prepare_ingest(
        control,
        company,
        context(cookie),
        file(label),
        metadata(),
        ScanPolicy::Off,
    )
    .unwrap()
    .value;
    finish_ingest_registered(control, company, guard, context(cookie), admission, None)
}
#[test]
#[ignore = "qualified registration death subprocess"]
fn registration_child() {
    let root = std::path::PathBuf::from(std::env::var("RM_REGISTRATION_ROOT").unwrap());
    let action = std::env::var("RM_REGISTRATION_ACTION").unwrap();
    let uid = String::from_utf8(Command::new("id").arg("-u").output().unwrap().stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let mut guard = if action == "initialize" {
        NamespaceGuard::create_fresh_with_profile(
            &root,
            uid,
            "w",
            "a",
            "Synthetic death",
            "2026-01-01",
            NamespaceProfile::LocalUserMacos,
        )
        .unwrap()
    } else {
        NamespaceGuard::acquire_with_profile(&root, uid, "w", "a", NamespaceProfile::LocalUserMacos)
            .unwrap()
    };
    let mut control = Connection::open(root.join(".rentemester/workspace-control.sqlite")).unwrap();
    let mut company = Connection::open(root.join("a/data/ledger.sqlite")).unwrap();
    for db in [&control, &company] {
        db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL")
            .unwrap();
    }
    if action == "initialize" {
        rentemester_storage::fresh::initialize_empty_pair(
            &mut control,
            &mut company,
            "Synthetic death",
        )
        .unwrap();
        control.execute_batch("INSERT INTO user(id,name,email,emailVerified,twoFactorEnabled,createdAt,updatedAt)VALUES('u','Synthetic','u@example.invalid',1,1,'2026-01-01','2026-01-01');INSERT INTO twoFactor(id,secret,backupCodes,userId,verified)VALUES('f','synthetic','[]','u',1);INSERT INTO session(id,userId,token,createdAt,updatedAt,expiresAt)VALUES('s','u','synthetic-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z','2026-01-01T12:00:00.000Z');INSERT INTO rm_workspace_user_access_events(user_id,event_type,workspace_role,actor)VALUES('u','activate','member','user:u');INSERT INTO rm_company_membership_events(user_id,company_slug,event_type,company_role,actor)VALUES('u','a','grant','bookkeeper','user:u');").unwrap();
    }
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
    if action == "initialize" {
        intake(
            &mut control,
            &mut company,
            &mut guard,
            &cookie,
            "registered",
        )
        .unwrap();
        return;
    }
    if action == "publish" {
        intake(
            &mut control,
            &mut company,
            &mut guard,
            &cookie,
            "interrupted",
        )
        .unwrap();
        panic!("checkpoint missing");
    }
    guard.ensure_connections(&control, &company).unwrap();
    let phase = std::env::var("RM_REGISTRATION_EXPECT").unwrap();
    let count: i64 = company
        .query_row("SELECT count(*) FROM documents", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, if phase == "committed" { 2 } else { 1 });
    {
        let mut statement = company
            .prepare("SELECT stored_path, sha256_hash FROM documents")
            .unwrap();
        let rows = statement
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .unwrap();
        for row in rows {
            let (path, hash) = row.unwrap();
            let bytes = guard.read_document(&path, &hash).unwrap();
            assert!(bytes == file("registered").bytes() || bytes == file("interrupted").bytes());
        }
    }
    // Published canonical orphan is retained, exported, and reused only through
    // explicit authorized intake. No SQL registration is invented on reopen.
    assert_eq!(guard.snapshot_originals(false).unwrap().len(), 2);
    assert_eq!(
        guard
            .read_document(
                &file("interrupted").relative_path(),
                file("interrupted").hash()
            )
            .unwrap(),
        file("interrupted").bytes()
    );
    let first = intake(
        &mut control,
        &mut company,
        &mut guard,
        &cookie,
        "interrupted",
    );
    assert_eq!(first.is_ok(), phase != "committed");
    assert!(
        intake(
            &mut control,
            &mut company,
            &mut guard,
            &cookie,
            "interrupted"
        )
        .is_err()
    );
    assert_eq!(
        company
            .query_row("SELECT count(*) FROM documents", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(guard.snapshot_originals(false).unwrap().len(), 2);
    println!("REGISTRATION_DEATH_EXACT_NO_DANGLING_DEDUP");
}
fn command(root: &Path, action: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", "umask 077; exec \"$@\"", "registration-death"])
        .arg(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "documents::crash_tests::registration_child",
            "--nocapture",
        ])
        .env("RM_REGISTRATION_ROOT", root)
        .env("RM_REGISTRATION_ACTION", action);
    command
}
#[test]
fn kill_registration_boundaries_reopen_exact_and_retry_without_duplicate_sql() {
    use std::os::unix::process::ExitStatusExt;
    for phase in ["published", "registered", "committed"] {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().canonicalize().unwrap().join("workspace");
        let output = command(&root, "initialize").output().unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let marker = base.path().join("marker");
        let mut child = command(&root, "publish")
            .env("RM_REGISTRATION_PHASE", phase)
            .env("RM_REGISTRATION_MARKER", &marker)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        while !marker.exists() {
            assert!(child.try_wait().unwrap().is_none(), "early exit at {phase}");
            if Instant::now() > deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("timeout at {phase}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        child.kill().unwrap();
        assert_eq!(child.wait().unwrap().signal(), Some(9));
        let output = command(&root, "verify")
            .env("RM_REGISTRATION_EXPECT", phase)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{phase}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .contains("REGISTRATION_DEATH_EXACT_NO_DANGLING_DEDUP")
        );
    }
}
