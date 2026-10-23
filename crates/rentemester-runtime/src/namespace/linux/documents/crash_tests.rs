use super::*;
use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
thread_local! { static REPLACE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
pub(super) fn after_read(directory: &File, name: &str) {
    if REPLACE.with(|v| v.replace(false)) {
        let replacement = format!("{name}.replacement");
        let mut file = File::from(
            openat(
                directory,
                replacement.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL,
                Mode::from_raw_mode(0o600),
            )
            .unwrap(),
        );
        file.write_all(input().bytes()).unwrap();
        rustix::fs::renameat(directory, replacement.as_str(), directory, name).unwrap();
    }
}
pub(super) fn checkpoint(phase: &str) {
    if std::env::var("RM_DOCUMENT_DEATH_PHASE").as_deref() == Ok(phase) {
        let marker = std::env::var("RM_DOCUMENT_DEATH_MARKER").unwrap();
        std::fs::write(marker, phase).unwrap();
        loop {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
fn input() -> crate::document::Prepared {
    crate::document::Prepared::new(
        "synthetic.txt".into(),
        b"Synthetic interrupted original".to_vec(),
    )
    .unwrap()
}
fn uid() -> u32 {
    libproc::proc_pid::pidinfo::<libproc::bsd_info::BSDInfo>(std::process::id() as i32, 0)
        .unwrap()
        .pbi_uid
}
fn acquire(root: &Path) -> NamespaceGuard {
    NamespaceGuard::acquire_with_profile(
        root,
        uid(),
        "w",
        "a",
        crate::namespace::NamespaceProfile::LocalUserMacos,
    )
    .unwrap()
}
#[test]
#[ignore = "qualified subprocess only"]
fn death_child() {
    let root = std::path::PathBuf::from(std::env::var("RM_DOCUMENT_DEATH_ROOT").unwrap());
    let action = std::env::var("RM_DOCUMENT_DEATH_ACTION").unwrap();
    let file = input();
    if action == "initialize" {
        let mut guard = NamespaceGuard::create_fresh_with_profile(
            &root,
            uid(),
            "w",
            "a",
            "Synthetic",
            "2026-01-01",
            crate::namespace::NamespaceProfile::LocalUserMacos,
        )
        .unwrap();
        let registered = crate::document::Prepared::new(
            "registered.txt".into(),
            b"Synthetic previously registered original".to_vec(),
        )
        .unwrap();
        guard.publish_document(&registered).unwrap();
        return;
    }
    if action == "busy" {
        assert!(matches!(
            NamespaceGuard::acquire_with_profile(
                &root,
                uid(),
                "w",
                "a",
                crate::namespace::NamespaceProfile::LocalUserMacos
            ),
            Err(Error::Busy)
        ));
        return;
    }
    let mut guard = acquire(&root);
    if action == "publish" {
        guard.publish_document(&file).unwrap();
        panic!("checkpoint was not reached");
    }
    let registered = crate::document::Prepared::new(
        "registered.txt".into(),
        b"Synthetic previously registered original".to_vec(),
    )
    .unwrap();
    assert_eq!(
        guard
            .read_document(&registered.relative_path(), registered.hash())
            .unwrap(),
        registered.bytes()
    );
    let snapshot = guard.snapshot_originals(false).unwrap();
    let phase = std::env::var("RM_DOCUMENT_EXPECT_PHASE").unwrap();
    let renamed = ["renamed", "directory-synced"].contains(&phase.as_str());
    assert_eq!(snapshot.len(), if renamed { 2 } else { 1 });
    let originals = root.join("a/documents/originals");
    assert!(!std::fs::read_dir(&originals).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".upload-")
    }));
    if !renamed {
        let entries: Vec<_> = std::fs::read_dir(originals.join(".recovery"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            std::fs::read(&entries[0]).unwrap(),
            if phase == "created" {
                &[][..]
            } else {
                file.bytes()
            }
        );
    }
    let (path, owned) = guard.publish_document_tracked(&file).unwrap();
    assert_eq!(owned.is_none(), renamed);
    assert!(guard.publish_document_tracked(&file).unwrap().1.is_none());
    assert_eq!(
        guard.read_document(&path, file.hash()).unwrap(),
        file.bytes()
    );
    assert_eq!(guard.snapshot_originals(false).unwrap().len(), 2);
    REPLACE.with(|v| v.set(true));
    assert_eq!(guard.read_document(&path, file.hash()), Err(Error::Changed));
    assert_eq!(
        guard.read_document(&path, file.hash()).unwrap(),
        file.bytes()
    );
    // Hostile staged objects are refused without opening/deleting their target.
    let suspicious = originals.join(format!(".upload-{}", "a".repeat(43)));
    std::os::unix::fs::symlink(
        originals.join(path.rsplit('/').next().unwrap()),
        &suspicious,
    )
    .unwrap();
    assert!(guard.snapshot_originals(false).is_err());
    assert!(
        suspicious
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink()
    );
    std::fs::remove_file(&suspicious).unwrap();
    std::fs::hard_link(
        originals.join(path.rsplit('/').next().unwrap()),
        &suspicious,
    )
    .unwrap();
    assert!(guard.snapshot_originals(false).is_err());
    assert!(suspicious.exists());
    std::fs::remove_file(&suspicious).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(&suspicious, b"Synthetic unsafe mode").unwrap();
    std::fs::set_permissions(&suspicious, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(guard.snapshot_originals(false).is_err());
    assert_eq!(
        std::fs::read(&suspicious).unwrap(),
        b"Synthetic unsafe mode"
    );
    std::fs::remove_file(&suspicious).unwrap();
    assert_eq!(guard.snapshot_originals(false).unwrap().len(), 2);
    println!("RECOVERY_EXACT_RETRY_AND_REPLACEMENT_REFUSAL");
}
fn command(root: &Path, action: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", "umask 077; exec \"$@\"", "document-death-test"])
        .arg(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "namespace::linux::documents::crash_tests::death_child",
            "--nocapture",
        ])
        .env("RM_DOCUMENT_DEATH_ROOT", root)
        .env("RM_DOCUMENT_DEATH_ACTION", action);
    command
}
#[test]
fn kill_publication_boundaries_recover_bytes_and_refuse_final_path_replacement() {
    use std::os::unix::process::ExitStatusExt;
    for phase in [
        "created",
        "written",
        "synced",
        "renamed",
        "directory-synced",
    ] {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().canonicalize().unwrap().join("workspace");
        let init = command(&root, "initialize").output().unwrap();
        assert!(
            init.status.success(),
            "{}",
            String::from_utf8_lossy(&init.stderr)
        );
        let marker = base.path().join("marker");
        let mut child = command(&root, "publish")
            .env("RM_DOCUMENT_DEATH_PHASE", phase)
            .env("RM_DOCUMENT_DEATH_MARKER", &marker)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        while !marker.exists() {
            assert!(
                child.try_wait().unwrap().is_none(),
                "child exited before {phase}"
            );
            if Instant::now() > deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("timeout at {phase}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        // A second process cannot scavenge the paused live writer's stage.
        let busy = command(&root, "busy").output().unwrap();
        assert!(
            busy.status.success(),
            "{}",
            String::from_utf8_lossy(&busy.stderr)
        );
        child.kill().unwrap();
        assert_eq!(
            child.wait().unwrap().signal(),
            Some(9),
            "must die without unwinding"
        );
        let output = command(&root, "verify")
            .env("RM_DOCUMENT_EXPECT_PHASE", phase)
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
                .contains("RECOVERY_EXACT_RETRY_AND_REPLACEMENT_REFUSAL")
        );
    }
}
