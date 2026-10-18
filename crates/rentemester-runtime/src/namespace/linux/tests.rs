use super::*;
use rusqlite::{Connection, OpenFlags};
use std::{
    io::{BufRead, Write},
    os::unix::fs::PermissionsExt,
};

#[test]
fn privileged_process_is_never_a_broker_profile() {
    assert!(matches!(process(0), Err(Error::Process)));
}

#[test]
#[ignore = "isolated Docker root fixture only"]
fn fixture() {
    let root = PathBuf::from(std::env::var("RM_BROKER_ROOT").unwrap());
    assert!(!root.exists());
    std::fs::create_dir_all(root.join(".rentemester")).unwrap();
    std::fs::create_dir_all(root.join("a/data")).unwrap();
    std::fs::write(root.join("workspace.json"),r#"{"version":1,"companies":[{"slug":"a","name":"Synthetic","createdAt":"2026-01-01","archived":false}]}"#).unwrap();
    for name in [
        ".rentemester/workspace-control.sqlite",
        "a/data/ledger.sqlite",
    ] {
        let db = Connection::open(root.join(name)).unwrap();
        db.execute_batch("CREATE TABLE synthetic(value INTEGER); INSERT INTO synthetic VALUES(1)")
            .unwrap();
        db.close().unwrap();
        std::fs::set_permissions(root.join(name), std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    for dir in ["", ".rentemester", "a", "a/data"] {
        std::fs::set_permissions(root.join(dir), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    std::fs::set_permissions(
        root.join("workspace.json"),
        std::fs::Permissions::from_mode(0o600),
    )
    .unwrap();
}
#[test]
#[ignore = "isolated Docker dedicated UID probe only"]
fn probe() {
    let root = PathBuf::from(std::env::var("RM_BROKER_ROOT").unwrap());
    let uid = std::env::var("RM_BROKER_UID").unwrap().parse().unwrap();
    let mut guard = match NamespaceGuard::acquire(&root, uid, "w", "a") {
        Ok(guard) => guard,
        Err(error) => {
            println!("RM_BROKER_RESULT={}", error.code());
            return;
        }
    };
    if std::env::var("RM_BROKER_HOLD").is_err() {
        println!("RM_BROKER_RESULT=accepted");
        return;
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW;
    let control =
        Connection::open_with_flags(root.join(".rentemester/workspace-control.sqlite"), flags)
            .unwrap();
    let company = Connection::open_with_flags(root.join("a/data/ledger.sqlite"), flags).unwrap();
    guard.ensure_connections(&control, &company).unwrap();
    let mode = std::env::var("RM_BROKER_MODE").unwrap();
    for db in [&control, &company] {
        db.pragma_update(None, "journal_mode", &mode).unwrap();
        db.execute_batch("BEGIN IMMEDIATE; INSERT INTO synthetic VALUES(2)")
            .unwrap();
    }
    guard.ensure_connections(&control, &company).unwrap();
    println!("RM_BROKER_RESULT=held");
    std::io::stdout().flush().unwrap();
    for line in std::io::stdin().lock().lines() {
        match line.unwrap().as_str() {
            "check" => {
                println!(
                    "RM_BROKER_CHECK={}",
                    guard
                        .ensure_connections(&control, &company)
                        .map(|_| "accepted")
                        .unwrap_or_else(|e| e.code())
                );
                std::io::stdout().flush().unwrap();
            }
            "commit" => {
                for db in [&control, &company] {
                    db.execute_batch("COMMIT").unwrap();
                }
                break;
            }
            "rollback" => {
                for db in [&control, &company] {
                    db.execute_batch("ROLLBACK").unwrap();
                }
                break;
            }
            _ => panic!("unknown synthetic probe command"),
        }
    }
    company.close().unwrap();
    control.close().unwrap();
    drop(guard);
}

#[test]
#[ignore = "isolated Docker actual ACL fixture only"]
fn acl_fixture() {
    let root = PathBuf::from(std::env::var("RM_BROKER_ROOT").unwrap());
    let default = std::env::var("RM_BROKER_ACL_KIND").unwrap() == "default";
    let mut bytes = 2u32.to_le_bytes().to_vec();
    for (tag, permission, id) in [
        (1u16, 6u16, u32::MAX),
        (2, 6, 10002),
        (4, 0, u32::MAX),
        (16, 0, u32::MAX),
        (32, 0, u32::MAX),
    ] {
        bytes.extend(tag.to_le_bytes());
        bytes.extend(permission.to_le_bytes());
        bytes.extend(id.to_le_bytes());
    }
    let path = if default {
        root
    } else {
        root.join("workspace.json")
    };
    let name = if default {
        "system.posix_acl_default"
    } else {
        "system.posix_acl_access"
    };
    rustix::fs::setxattr(&path, name, &bytes, rustix::fs::XattrFlags::CREATE).unwrap();
    let mut actual = vec![0u8; bytes.len()];
    assert_eq!(
        rustix::fs::lgetxattr(&path, name, &mut actual[..]).unwrap(),
        bytes.len()
    );
    assert_eq!(actual, bytes);
    println!("RM_BROKER_ACL=installed-and-read-back");
}
#[test]
#[ignore = "isolated Docker actual different-UID filesystem attacks"]
fn client() {
    let root = PathBuf::from(std::env::var("RM_BROKER_ROOT").unwrap());
    if std::env::var("RM_BROKER_CLIENT_POSITIVE").is_ok() {
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(root.join("a/data/ledger.sqlite"))
            .unwrap();
        drop(file);
        println!("RM_BROKER_CLIENT=positive");
        return;
    }
    let mut denied = 0;
    let mut check = |result: std::io::Result<()>| {
        let e = result.expect_err("synthetic client unexpectedly accessed broker namespace");
        assert!(
            matches!(e.raw_os_error(), Some(1 | 13)),
            "unexpected denial errno: {e}"
        );
        denied += 1;
    };
    check(std::fs::read_dir(&root).map(drop));
    check(std::fs::write(root.join("injected"), b"synthetic"));
    check(std::fs::rename(&root, root.with_extension("moved")));
    for name in [
        "workspace.json",
        ".rentemester/workspace-control.sqlite",
        "a/data/ledger.sqlite",
        ".rentemester/workspace-control.sqlite-wal",
        ".rentemester/workspace-control.sqlite-shm",
        ".rentemester/workspace-control.sqlite-journal",
        "a/data/ledger.sqlite-wal",
        "a/data/ledger.sqlite-shm",
        "a/data/ledger.sqlite-journal",
    ] {
        let path = root.join(name);
        check(std::fs::read(&path).map(drop));
        check(
            std::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .map(drop),
        );
        check(std::fs::remove_file(&path));
        check(std::fs::rename(&path, root.join("moved")));
        check(std::fs::hard_link(&path, root.join("alias")));
        check(std::os::unix::fs::symlink(&path, root.join("alias")));
        check(std::fs::set_permissions(
            &path,
            std::fs::Permissions::from_mode(0o666),
        ));
    }
    println!("RM_BROKER_CLIENT_DENIED={denied}");
}

#[test]
#[ignore = "isolated Docker per-thread NoNewPrivs control; launcher intentionally starts at zero"]
fn thread_profile() {
    let uid = std::env::var("RM_BROKER_UID").unwrap().parse().unwrap();
    let leader = std::fs::read_to_string("/proc/self/status").unwrap();
    assert!(leader.lines().any(|s| s == "NoNewPrivs:\t0"));
    assert!(process(uid).is_err());
    rustix::thread::set_no_new_privs(true).unwrap();
    // libtest runs this on a worker thread; leader status remains zero.
    assert!(
        std::fs::read_to_string("/proc/self/status")
            .unwrap()
            .lines()
            .any(|s| s == "NoNewPrivs:\t0")
    );
    process(uid).unwrap();
    println!("RM_BROKER_THREAD=current-thread-confirmed");
}

#[test]
#[ignore = "isolated Docker pre-opened descriptor negative control"]
fn prior_descriptor() {
    let root = PathBuf::from(std::env::var("RM_BROKER_ROOT").unwrap());
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(root.join("a/data/ledger.sqlite"))
        .unwrap();
    println!("RM_BROKER_PRIOR=open");
    std::io::stdout().flush().unwrap();
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap();
    assert_eq!(line.trim(), "write");
    file.write_all(b"synthetic-fd-control").unwrap();
    file.sync_all().unwrap();
    println!("RM_BROKER_PRIOR=write-survived-mode-change");
}

#[test]
#[ignore = "isolated Docker raw POSIX lock oracle independent of SQLite mode selection"]
fn raw_lock() {
    let path = std::env::var("RM_IDENTITY_LOCK_PATH").unwrap();
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    match rustix::fs::fcntl_lock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => println!("RM_BROKER_RAW_LOCK=acquired"),
        Err(rustix::io::Errno::AGAIN | rustix::io::Errno::ACCESS) => {
            println!("RM_BROKER_RAW_LOCK=busy")
        }
        result => panic!("unexpected POSIX lock result {result:?}"),
    }
}
