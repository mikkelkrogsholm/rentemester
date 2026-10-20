use super::*;
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use sha2::{Digest, Sha256};
use std::os::unix::fs::PermissionsExt;
fn key() -> Ed25519KeyPair {
    Ed25519KeyPair::from_pkcs8(
        Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
            .unwrap()
            .as_ref(),
    )
    .unwrap()
}
fn policy() -> Policy {
    super::super::current_policy()
}
fn package(root: &Path, version: &str, sequence: u64, key: &Ed25519KeyPair) {
    fs::create_dir(root).unwrap();
    let mut files = Vec::new();
    for (path, executable) in [
        ("Contents/MacOS/rentemester-desktop", true),
        ("Contents/MacOS/rentemester-server", true),
        ("Contents/MacOS/rentemester-update", true),
        ("Contents/Info.plist", false),
        ("Contents/Resources/web/index.html", false),
    ] {
        let target = root.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(
            &target,
            if executable {
                format!("#!/bin/sh\nprintf '%s\\n' 'synthetic {version}'\n")
            } else {
                format!("Synthetic package {version}")
            },
        )
        .unwrap();
        fs::set_permissions(
            &target,
            fs::Permissions::from_mode(if executable { 0o755 } else { 0o644 }),
        )
        .unwrap();
        files.push(json!({"path":path,"size":target.metadata().unwrap().len(),"sha256":format!("{:x}",Sha256::digest(fs::read(&target).unwrap())),"executable":executable}));
    }
    files.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    let p = policy();
    let manifest=serde_json::to_vec(&json!({"format":"rentemester-native-update/1","bundleId":"dk.rentemester.desktop","version":version,"sequence":sequence,"platform":p.platform,"apiEpoch":p.api_epoch,"controlSchema":p.control_schema,"companySchema":p.company_schema,"sourceRevision":"a".repeat(40),"toolchains":{"server":"rustc synthetic","desktop":"rustc synthetic"},"files":files})).unwrap();
    fs::write(root.join(MANIFEST), &manifest).unwrap();
    fs::write(root.join(SIGNATURE), key.sign(&manifest).as_ref()).unwrap();
}
fn run(root: &Path) -> String {
    let out = std::process::Command::new(root.join("Contents/MacOS/rentemester-desktop"))
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap()
}
fn prepared(old: &Path, new: &Path, key: &Ed25519KeyPair) -> (PathBuf, Transaction) {
    let a = verify(old, key.public_key().as_ref(), &policy()).unwrap();
    let b = verify(new, key.public_key().as_ref(), &policy()).unwrap();
    let container = sibling(old, "update-stage").unwrap();
    let tx = Transaction {
        format: "rentemester-update-transaction/1".into(),
        target: old.to_string_lossy().into_owned(),
        from_sha256: a.manifest_sha256().into(),
        to_sha256: b.manifest_sha256().into(),
    };
    stage(&b, &container, &tx).unwrap();
    (container, tx)
}
#[test]
fn authenticated_upgrade_keeps_previous_runs_and_rejects_downgrade_or_active_app() {
    let d = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let k = key();
    let target = d.path().join("Rentemester.app");
    let new = d.path().join("Candidate.app");
    package(&target, "0.1.0", 1, &k);
    package(&new, "0.1.1", 2, &k);
    let running = PackageLease::running(&target).unwrap();
    assert_eq!(
        install(&new, &target, k.public_key().as_ref(), &policy())
            .unwrap_err()
            .0,
        "application_running_or_update_busy"
    );
    drop(running);
    let result = install(&new, &target, k.public_key().as_ref(), &policy()).unwrap();
    assert_eq!(result["state"], "installed");
    assert_eq!(run(&target), "synthetic 0.1.1\n");
    let previous = sibling(&target, "update-previous").unwrap();
    assert_eq!(run(&previous), "synthetic 0.1.0\n");
    assert_eq!(
        install(&previous, &target, k.public_key().as_ref(), &policy())
            .unwrap_err()
            .0,
        "update_downgrade_refused"
    );
    assert_eq!(run(&target), "synthetic 0.1.1\n");
    let third = d.path().join("Third.app");
    package(&third, "0.1.2", 3, &k);
    install(&third, &target, k.public_key().as_ref(), &policy()).unwrap();
    assert_eq!(run(&target), "synthetic 0.1.2\n");
    assert_eq!(run(&previous), "synthetic 0.1.1\n");
}
#[test]
fn incomplete_staging_recovers_without_touching_installation_or_workspace() {
    let d = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let k = key();
    let old = d.path().join("Rentemester.app");
    let new = d.path().join("Candidate.app");
    package(&old, "0.1.0", 1, &k);
    package(&new, "0.1.1", 2, &k);
    let workspace = d.path().join("workspace-sentinel.sqlite");
    fs::write(&workspace, b"synthetic workspace remains outside updater").unwrap();
    let saved = fs::read(&workspace).unwrap();
    let (container, _) = prepared(&old, &new, &k);
    fs::remove_file(container.join("candidate.app/Contents/MacOS/rentemester-server")).unwrap();
    assert_eq!(run(&old), "synthetic 0.1.0\n");
    assert_eq!(
        status(&old, k.public_key().as_ref(), &policy()).unwrap()["pendingRecovery"],
        true
    );
    assert_eq!(
        recover(&old, k.public_key().as_ref(), &policy()).unwrap()["state"],
        "staging_discarded"
    );
    assert!(!container.exists());
    assert_eq!(run(&old), "synthetic 0.1.0\n");
    assert_eq!(fs::read(&workspace).unwrap(), saved);
    install(&new, &old, k.public_key().as_ref(), &policy()).unwrap();
    assert_eq!(fs::read(&workspace).unwrap(), saved);
}
#[test]
fn exchange_interruption_and_post_retention_interruption_are_recoverable() {
    let d = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let k = key();
    let old = d.path().join("Rentemester.app");
    let new = d.path().join("Candidate.app");
    package(&old, "0.1.0", 1, &k);
    package(&new, "0.1.1", 2, &k);
    let (container, _) = prepared(&old, &new, &k);
    rfs::renameat_with(
        rfs::CWD,
        container.join("candidate.app"),
        rfs::CWD,
        &old,
        RenameFlags::EXCHANGE,
    )
    .unwrap();
    assert_eq!(run(&old), "synthetic 0.1.1\n");
    assert_eq!(run(&container.join("candidate.app")), "synthetic 0.1.0\n");
    // Simulate termination after retaining old bytes, before metadata cleanup.
    fs::rename(
        container.join("candidate.app"),
        sibling(&old, "update-previous").unwrap(),
    )
    .unwrap();
    assert_eq!(
        recover(&old, k.public_key().as_ref(), &policy()).unwrap()["state"],
        "installed"
    );
    assert!(!container.exists());
    assert_eq!(run(&old), "synthetic 0.1.1\n");
    assert_eq!(
        run(&sibling(&old, "update-previous").unwrap()),
        "synthetic 0.1.0\n"
    );
}
#[test]
fn signature_failure_and_symlink_staging_preserve_the_runnable_old_package() {
    let d = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let k = key();
    let wrong = key();
    let old = d.path().join("Rentemester.app");
    let new = d.path().join("Candidate.app");
    package(&old, "0.1.0", 1, &k);
    package(&new, "0.1.1", 2, &wrong);
    assert!(install(&new, &old, k.public_key().as_ref(), &policy()).is_err());
    assert_eq!(run(&old), "synthetic 0.1.0\n");
    let external = d.path().join("external");
    fs::create_dir(&external).unwrap();
    fs::write(external.join("keep"), b"keep").unwrap();
    std::os::unix::fs::symlink(&external, sibling(&old, "update-stage").unwrap()).unwrap();
    assert!(recover(&old, k.public_key().as_ref(), &policy()).is_err());
    assert_eq!(fs::read(external.join("keep")).unwrap(), b"keep");
    assert_eq!(run(&old), "synthetic 0.1.0\n");
}
#[test]
fn empty_interrupted_staging_and_current_schema_contract() {
    let d = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let k = key();
    let old = d.path().join("Rentemester.app");
    package(&old, "0.1.0", 1, &k);
    let container = sibling(&old, "update-stage").unwrap();
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&container)
        .unwrap();
    assert_eq!(
        recover(&old, k.public_key().as_ref(), &policy()).unwrap()["state"],
        "staging_discarded"
    );
}

#[test]
fn bundled_server_and_shell_share_lease_and_retired_or_staged_copies_refuse() {
    let d = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
    let k = key();
    let old = d.path().join("Rentemester.app");
    let new = d.path().join("New.app");
    package(&old, "0.1.0", 1, &k);
    package(&new, "0.1.1", 2, &k);
    let desktop = lease_for_executable(&old.join("Contents/MacOS/rentemester-desktop"))
        .unwrap()
        .unwrap();
    let server = lease_for_executable(&old.join("Contents/MacOS/rentemester-server"))
        .unwrap()
        .unwrap();
    drop(desktop);
    assert_eq!(
        install(&new, &old, k.public_key().as_ref(), &policy())
            .unwrap_err()
            .0,
        "application_running_or_update_busy"
    );
    drop(server);
    let (container, _) = prepared(&old, &new, &k);
    assert!(
        lease_for_executable(&container.join("candidate.app/Contents/MacOS/rentemester-desktop"))
            .is_err()
    );
    recover(&old, k.public_key().as_ref(), &policy()).unwrap();
    install(&new, &old, k.public_key().as_ref(), &policy()).unwrap();
    assert!(
        lease_for_executable(
            &sibling(&old, "update-previous")
                .unwrap()
                .join("Contents/MacOS/rentemester-server")
        )
        .is_err()
    );
}
