use super::*;
use rentemester_runtime::{WorkspaceLayout, WorkspaceLease};
#[cfg(target_os = "linux")]
use std::io::{BufRead, Write};
use std::process::Command;

fn fixture(root: &Path) {
    std::fs::create_dir_all(root.join(".rentemester")).unwrap();
    std::fs::create_dir_all(root.join("a/data")).unwrap();
    std::fs::write(root.join("workspace.json"), r#"{"version":1,"companies":[{"slug":"a","name":"Synthetic","createdAt":"2026-01-01","archived":false}]}"#).unwrap();
    for name in [
        ".rentemester/workspace-control.sqlite",
        "a/data/ledger.sqlite",
    ] {
        let db = Connection::open(root.join(name)).unwrap();
        db.execute_batch("CREATE TABLE synthetic(value INTEGER); INSERT INTO synthetic VALUES(1)")
            .unwrap();
        db.close().unwrap();
    }
}
fn held(root: &Path, mode: &str) -> Held<WorkspaceLayout> {
    let guard = WorkspaceLayout::bind(WorkspaceLease::acquire(root).unwrap(), "w", "a").unwrap();
    let control = Connection::open(root.join(".rentemester/workspace-control.sqlite")).unwrap();
    let company = Connection::open(root.join("a/data/ledger.sqlite")).unwrap();
    for db in [&control, &company] {
        db.pragma_update(None, "journal_mode", mode).unwrap();
        db.execute_batch("BEGIN IMMEDIATE; INSERT INTO synthetic VALUES(2)")
            .unwrap();
    }
    Held(Some(Slots {
        control: Some(control),
        company: Some(company),
        guard,
    }))
}
fn child(root: &Path, name: &str) -> String {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "tests::contender", "--nocapture"])
        .env("RM_OWNER_ROOT", root)
        .env("RM_OWNER_DATABASE", name)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
#[test]
#[ignore = "subprocess lock contender"]
fn contender() {
    let root = std::env::var("RM_OWNER_ROOT").unwrap();
    let lease = match WorkspaceLease::acquire(Path::new(&root)) {
        Ok(lease) => {
            drop(lease);
            "free"
        }
        Err(rentemester_runtime::LeaseError::Busy) => "busy",
        Err(error) => panic!("unexpected lease error {error:?}"),
    };
    let db = Connection::open_with_flags(
        Path::new(&root).join(std::env::var("RM_OWNER_DATABASE").unwrap()),
        OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    db.busy_timeout(std::time::Duration::ZERO).unwrap();
    let lock = match db.execute_batch("BEGIN IMMEDIATE") {
        Ok(()) => {
            db.execute_batch("ROLLBACK").unwrap();
            "free"
        }
        Err(rusqlite::Error::SqliteFailure(error, _))
            if error.code == rusqlite::ErrorCode::DatabaseBusy =>
        {
            "busy"
        }
        Err(error) => panic!("unexpected SQLite error {error:?}"),
    };
    db.close().unwrap();
    println!("RM_OWNER_LOCK={lease}/{lock}");
}
#[test]
fn confirmed_close_releases_persistent_wal_and_delete_files() {
    for mode in ["WAL", "DELETE"] {
        let root = tempfile::tempdir().unwrap();
        fixture(root.path());
        let mut owner = held(root.path(), mode);
        assert!(child(root.path(), "a/data/ledger.sqlite").contains("RM_OWNER_LOCK=busy/busy"));
        for db in [
            &owner.0.as_ref().unwrap().control,
            &owner.0.as_ref().unwrap().company,
        ] {
            db.as_ref().unwrap().execute_batch("COMMIT").unwrap();
        }
        assert!(owner.0.as_mut().unwrap().close());
        drop(owner);
        assert!(child(root.path(), "a/data/ledger.sqlite").contains("RM_OWNER_LOCK=free/free"));
        let db = Connection::open(root.path().join("a/data/ledger.sqlite")).unwrap();
        assert_eq!(
            db.query_row("SELECT count(*) FROM synthetic", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
        db.close().unwrap();
    }
}
#[test]
fn busy_retains_actual_connection_and_layout_until_explicit_retry() {
    for mode in ["WAL", "DELETE"] {
        for name in ["control", "company"] {
            let root = tempfile::tempdir().unwrap();
            fixture(root.path());
            let mut owner = held(root.path(), mode);
            let slots = owner.0.as_mut().unwrap();
            let db = if name == "control" {
                slots.control.as_ref().unwrap()
            } else {
                slots.company.as_ref().unwrap()
            };
            // A real outstanding prepared statement makes sqlite3_close return BUSY.
            std::mem::forget(db.prepare("SELECT value FROM synthetic").unwrap());
            assert!(!slots.close());
            let db = if name == "control" {
                slots.control.as_ref().unwrap()
            } else {
                slots.company.as_ref().unwrap()
            };
            assert!(!db.is_autocommit());
            let path = if name == "control" {
                ".rentemester/workspace-control.sqlite"
            } else {
                "a/data/ledger.sqlite"
            };
            assert!(child(root.path(), path).contains("RM_OWNER_LOCK=busy/busy"));
            // Forgotten statements cannot safely be finalized through public owner
            // APIs. Drop must keep this whole set until process exit.
            drop(owner);
            assert!(child(root.path(), path).contains("RM_OWNER_LOCK=busy/busy"));
        }
    }
}
#[cfg(not(target_os = "linux"))]
#[test]
fn unsupported_owner_cannot_create_a_workspace() {
    let root = tempfile::tempdir().unwrap();
    let absent = root.path().join("absent");
    assert!(matches!(
        Owner::open_protected(&absent, 10001, "w", "a"),
        Err(OwnerError::Namespace(NamespaceError::Unsupported))
    ));
    assert!(!absent.exists());
}

pub(super) fn constructor_fault(slots: &mut Slots<NamespaceGuard>) -> Result<(), OwnerError> {
    match std::env::var("RM_OWNER_CONSTRUCTOR_FAULT").as_deref() {
        Ok("busy") | Ok("panic") => {
            let db = slots.control.as_ref().unwrap();
            db.execute_batch("BEGIN IMMEDIATE; INSERT INTO synthetic VALUES(2)")
                .unwrap();
            std::mem::forget(db.prepare("SELECT value FROM synthetic").unwrap());
            if std::env::var("RM_OWNER_CONSTRUCTOR_FAULT").as_deref() == Ok("panic") {
                panic!("synthetic constructor panic");
            }
            Err(OwnerError::Database)
        }
        Ok("clean") => Err(OwnerError::Database),
        _ => Ok(()),
    }
}
#[cfg(target_os = "linux")]
#[test]
#[ignore = "dedicated-UID Docker probe only"]
fn broker_probe() {
    let root = std::env::var("RM_OWNER_ROOT").unwrap();
    let mode = std::env::var("RM_OWNER_MODE").unwrap_or_else(|_| "DELETE".into());
    let opening =
        std::panic::catch_unwind(|| Owner::open_protected(Path::new(&root), 10001, "w", "a"));
    let mut owner = match opening {
        Ok(Ok(owner)) => Some(owner),
        Ok(Err(error)) => {
            println!("RM_OWNER_OPEN={error}");
            None
        }
        Err(_) => {
            println!("RM_OWNER_OPEN=panic");
            None
        }
    };
    let mut pending = None;
    if let Some(owner) = &mut owner {
        let slots = owner.held.0.as_mut().unwrap();
        for db in [
            slots.control.as_ref().unwrap(),
            slots.company.as_ref().unwrap(),
        ] {
            db.pragma_update(None, "journal_mode", &mode).unwrap();
            db.execute_batch("BEGIN IMMEDIATE; INSERT INTO synthetic VALUES(2)")
                .unwrap();
        }
        match std::env::var("RM_OWNER_LEAK").as_deref() {
            Ok("control") => std::mem::forget(
                slots
                    .control
                    .as_ref()
                    .unwrap()
                    .prepare("SELECT value FROM synthetic")
                    .unwrap(),
            ),
            Ok("company") => std::mem::forget(
                slots
                    .company
                    .as_ref()
                    .unwrap()
                    .prepare("SELECT value FROM synthetic")
                    .unwrap(),
            ),
            _ => (),
        }
        println!("RM_OWNER_OPEN=held");
    }
    std::io::stdout().flush().unwrap();
    for line in std::io::stdin().lock().lines() {
        match line.unwrap().as_str() {
            "close" => {
                let result = owner.take().unwrap().try_close();
                match result {
                    Ok(()) => println!("RM_OWNER_CLOSE=closed"),
                    Err(failure) => {
                        pending = Some(failure);
                        println!("RM_OWNER_CLOSE=pending");
                    }
                }
            }
            "retry" => match pending.take().unwrap().retry() {
                Ok(()) => println!("RM_OWNER_RETRY=closed"),
                Err(failure) => {
                    pending = Some(failure);
                    println!("RM_OWNER_RETRY=pending");
                }
            },
            "drop" => {
                drop(owner.take());
                drop(pending.take());
                println!("RM_OWNER_DROP=done");
            }
            "check" => {
                let owner = owner.as_mut().unwrap();
                let result = owner.admit();
                let retired = if result.is_ok() {
                    owner.finish(true)
                } else {
                    true
                };
                println!("RM_OWNER_CHECK={retired}");
            }
            "commit" => {
                let slots = owner.as_mut().unwrap().held.0.as_ref().unwrap();
                for db in [
                    slots.control.as_ref().unwrap(),
                    slots.company.as_ref().unwrap(),
                ] {
                    db.execute_batch("COMMIT").unwrap();
                }
                println!("RM_OWNER_COMMIT=done");
            }
            "panic" => {
                let owner = owner.as_mut().unwrap();
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    owner.admit().unwrap();
                    panic!("synthetic dispatch panic");
                }));
                assert!(!owner.reusable);
                println!("RM_OWNER_PANIC=retired");
            }
            "exit" => break,
            _ => panic!("unsupported test command"),
        }
        std::io::stdout().flush().unwrap();
    }
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "synthetic dedicated-UID reopen after confirmed close only"]
fn reopen() {
    let root = std::env::var("RM_OWNER_ROOT").unwrap();
    let mut counts = Vec::new();
    for name in [
        ".rentemester/workspace-control.sqlite",
        "a/data/ledger.sqlite",
    ] {
        let db = Connection::open_with_flags(
            Path::new(&root).join(name),
            OpenFlags::SQLITE_OPEN_READ_WRITE,
        )
        .unwrap();
        counts.push(
            db.query_row("SELECT count(*) FROM synthetic", [], |r| r.get::<_, i64>(0))
                .unwrap(),
        );
        db.close().unwrap();
    }
    println!("RM_OWNER_REOPEN={}/{}", counts[0], counts[1]);
}
