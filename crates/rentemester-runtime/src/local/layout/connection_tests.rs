use super::*;
use rusqlite::{Connection, OpenFlags};
use std::{os::unix::fs::MetadataExt, path::Path};

pub(super) fn fixture(root: &Path) -> (Connection, Connection) {
    std::fs::create_dir(root.join(".rentemester")).unwrap();
    std::fs::create_dir_all(root.join("a/data")).unwrap();
    std::fs::write(root.join("workspace.json"), r#"{"version":1,"companies":[{"slug":"a","name":"Synthetic","createdAt":"2026-01-01","archived":false}]}"#).unwrap();
    let open = |name| {
        let connection = Connection::open(root.join(name)).unwrap();
        connection
            .execute_batch("CREATE TABLE synthetic(value); INSERT INTO synthetic VALUES (1)")
            .unwrap();
        connection
    };
    (
        open(".rentemester/workspace-control.sqlite"),
        open("a/data/ledger.sqlite"),
    )
}
fn layout(root: &Path) -> WorkspaceLayout {
    WorkspaceLayout::bind(WorkspaceLease::acquire(root).unwrap(), "w", "a").unwrap()
}
#[test]
fn actual_handles_match_retained_files_in_wal_and_delete() {
    let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
    for mode in ["WAL", "DELETE"] {
        let root = tempfile::tempdir().unwrap();
        let (control, company) = fixture(root.path());
        let mut held = layout(root.path());
        for connection in [&control, &company] {
            connection
                .pragma_update(None, "journal_mode", mode)
                .unwrap();
            let before = connection.main_file_identity().unwrap();
            connection
                .execute_batch("BEGIN IMMEDIATE; INSERT INTO synthetic VALUES (2); COMMIT")
                .unwrap();
            assert_eq!(connection.main_file_identity().unwrap(), before);
        }
        held.ensure_connections(&control, &company).unwrap();
        // Read-only is still an identity observation, never write authorization.
        let read = Connection::open_with_flags(
            root.path().join("a/data/ledger.sqlite"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        held.ensure_connections(&control, &read).unwrap();
        drop(read);
        drop(company);
        drop(control);
        let control =
            Connection::open(root.path().join(".rentemester/workspace-control.sqlite")).unwrap();
        let company = Connection::open(root.path().join("a/data/ledger.sqlite")).unwrap();
        assert_eq!(
            company
                .query_row("SELECT count(*) FROM synthetic", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
        held.ensure_connections(&control, &company).unwrap();
    }
}
#[test]
fn restored_path_does_not_hide_wrong_open_handle() {
    let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
    for name in [
        ".rentemester/workspace-control.sqlite",
        "a/data/ledger.sqlite",
    ] {
        let root = tempfile::tempdir().unwrap();
        let (control, company) = fixture(root.path());
        let mut held = layout(root.path());
        let path = root.path().join(name);
        let original = std::fs::metadata(&path).unwrap();
        let saved = root.path().join("original");
        let other = root.path().join("other");
        std::fs::rename(&path, &saved).unwrap();
        let wrong = Connection::open(&path).unwrap();
        wrong.execute_batch("CREATE TABLE other(value)").unwrap();
        let actual = wrong.main_file_identity().unwrap();
        std::fs::rename(&path, &other).unwrap();
        std::fs::rename(&saved, &path).unwrap();
        let restored = std::fs::metadata(&path).unwrap();
        // Negative oracle: pathname checks, even retaining original descriptors,
        // pass after ABA restoration while this SQLite handle points elsewhere.
        assert_eq!(
            (original.dev(), original.ino()),
            (restored.dev(), restored.ino())
        );
        held.ensure_current().unwrap();
        assert_ne!(
            (actual.device, actual.inode),
            (restored.dev(), restored.ino())
        );
        assert_eq!(wrong.main_file_identity().unwrap(), actual);
        let result = if name.starts_with(".rentemester") {
            held.ensure_connections(&wrong, &company)
        } else {
            held.ensure_connections(&control, &wrong)
        };
        assert_eq!(result, Err(LayoutError::Changed));
        assert_eq!(
            held.ensure_connections(&control, &company),
            Err(LayoutError::Changed)
        );
    }
}
#[test]
fn memory_swapped_connections_and_link_changes_refuse_stickily() {
    let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
    for case in ["memory", "swapped", "hardlink", "unlinked"] {
        let root = tempfile::tempdir().unwrap();
        let (control, company) = fixture(root.path());
        let mut held = layout(root.path());
        held.ensure_connections(&control, &company).unwrap();
        let before = company.main_file_identity().unwrap();
        match case {
            "memory" => {
                let memory = Connection::open_in_memory().unwrap();
                assert!(memory.main_file_identity().is_err());
                assert_eq!(
                    held.ensure_connections(&control, &memory),
                    Err(LayoutError::Changed)
                );
            }
            "swapped" => assert_eq!(
                held.ensure_connections(&company, &control),
                Err(LayoutError::Changed)
            ),
            "hardlink" => {
                let alias = root.path().join("alias");
                std::fs::hard_link(root.path().join("a/data/ledger.sqlite"), &alias).unwrap();
                assert_eq!(company.main_file_identity().unwrap().links, 2);
                assert_eq!(
                    held.ensure_connections(&control, &company),
                    Err(LayoutError::Changed)
                );
                std::fs::remove_file(alias).unwrap();
            }
            _ => {
                std::fs::remove_file(root.path().join("a/data/ledger.sqlite")).unwrap();
                let actual = company.main_file_identity().unwrap();
                assert_eq!(
                    (actual.device, actual.inode, actual.links),
                    (before.device, before.inode, 0)
                );
                assert_eq!(
                    held.ensure_connections(&control, &company),
                    Err(LayoutError::Changed)
                );
            }
        }
        assert_eq!(
            held.ensure_connections(&control, &company),
            Err(LayoutError::Changed)
        );
    }
}

#[test]
#[ignore = "synthetic cross-process SQLite lock contender"]
fn lock_child() {
    let path = std::env::var("RM_IDENTITY_LOCK_PATH").unwrap();
    let connection = Connection::open(path).unwrap();
    connection.busy_timeout(std::time::Duration::ZERO).unwrap();
    match connection.execute_batch("BEGIN IMMEDIATE; ROLLBACK") {
        Ok(()) => println!("RM_IDENTITY_LOCK=acquired"),
        Err(rusqlite::Error::SqliteFailure(code, _))
            if code.code == rusqlite::ErrorCode::DatabaseBusy =>
        {
            println!("RM_IDENTITY_LOCK=busy")
        }
        other => panic!("unexpected lock result {other:?}"),
    }
}
fn contender(path: &Path, expected: &str) {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "local::layout::connection_tests::lock_child",
            "--nocapture",
        ])
        .env("RM_IDENTITY_LOCK_PATH", path)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains(&format!("RM_IDENTITY_LOCK={expected}"))
    );
}
#[test]
fn identity_observation_preserves_real_cross_process_sqlite_locks() {
    let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
    for mode in ["DELETE", "WAL"] {
        for name in [
            ".rentemester/workspace-control.sqlite",
            "a/data/ledger.sqlite",
        ] {
            let root = tempfile::tempdir().unwrap();
            let (control, company) = fixture(root.path());
            let mut held = layout(root.path());
            let writer = if name.starts_with(".rentemester") {
                &control
            } else {
                &company
            };
            writer.pragma_update(None, "journal_mode", mode).unwrap();
            writer.execute_batch("BEGIN IMMEDIATE").unwrap();
            let path = root.path().join(name);
            contender(&path, "busy");
            held.ensure_current().unwrap();
            contender(&path, "busy");
            held.ensure_connections(&control, &company).unwrap();
            assert!(!writer.is_autocommit());
            contender(&path, "busy");
            writer.execute_batch("ROLLBACK").unwrap();
            contender(&path, "acquired");
        }
    }
}

#[test]
fn refused_manifest_alias_never_releases_a_live_database_lock() {
    let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
    for mode in ["DELETE", "WAL"] {
        for name in [
            ".rentemester/workspace-control.sqlite",
            "a/data/ledger.sqlite",
        ] {
            let root = tempfile::tempdir().unwrap();
            let (control, company) = fixture(root.path());
            let mut held = layout(root.path());
            let writer = if name.starts_with(".rentemester") {
                &control
            } else {
                &company
            };
            writer.pragma_update(None, "journal_mode", mode).unwrap();
            writer.execute_batch("BEGIN IMMEDIATE").unwrap();
            let path = root.path().join(name);
            contender(&path, "busy");
            let manifest = root.path().join("workspace.json");
            std::fs::remove_file(&manifest).unwrap();
            std::fs::hard_link(&path, &manifest).unwrap();
            assert_eq!(
                held.ensure_connections(&control, &company),
                Err(LayoutError::Changed)
            );
            assert!(!writer.is_autocommit());
            contender(&path, "busy");
            writer.execute_batch("ROLLBACK").unwrap();
            contender(&path, "acquired");
        }
    }
}
