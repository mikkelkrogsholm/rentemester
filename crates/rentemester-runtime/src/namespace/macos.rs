//! Explicit local-account APFS profile. All same-UID code is trusted; this does
//! not provide Linux no_new_privs, capabilities, or a separate broker principal.
//! Kernel BSD credentials and native extended ACLs are checked, never inferred
//! from environment variables. No privilege/permission repair is performed.
use super::{NamespaceError as Error, NamespaceProfile};
use std::{fs::File, os::unix::ffi::OsStrExt, path::Path};

pub(super) fn process(uid: u32, profile: NamespaceProfile) -> Result<(), Error> {
    if profile != NamespaceProfile::LocalUserMacos {
        return Err(Error::Unsupported);
    }
    let info =
        libproc::proc_pid::pidinfo::<libproc::bsd_info::BSDInfo>(std::process::id() as i32, 0)
            .map_err(|_| Error::Process)?;
    if uid == 0
        || [info.pbi_uid, info.pbi_ruid, info.pbi_svuid] != [uid; 3]
        || info.pbi_gid == 0
        || info.pbi_gid != info.pbi_rgid
        || info.pbi_gid != info.pbi_svgid
    {
        return Err(Error::Process);
    }
    // Darwin has no read-only umask syscall. An environment-free system shell
    // reports the inherited kernel mask without temporarily clearing the parent's
    // process-global mask. This opens no workspace/database descriptor.
    let output = std::process::Command::new("/bin/sh")
        .args(["-c", "umask"])
        .env_clear()
        .output()
        .map_err(|_| Error::Process)?;
    if !output.status.success() || !output.stderr.is_empty() || output.stdout != b"0077\n" {
        return Err(Error::Process);
    }
    Ok(())
}
pub(super) fn acl_path(path: &Path) -> Result<(), Error> {
    acl_path_policy(path, false)
}
fn acl_path_policy(path: &Path, ancestor: bool) -> Result<(), Error> {
    let entries = exacl::getfacl(path, None).map_err(|_| Error::Invalid)?;
    // Standard macOS home/Library ancestry has "everyone deny delete". This
    // non-inheriting restriction grants no access and cannot leak into children.
    // Every allow entry, inheritance flag, or other extended permission refuses.
    if entries.is_empty()
        || (ancestor
            && entries.iter().all(|entry| {
                !entry.allow && entry.perms == exacl::Perm::DELETE && entry.flags.is_empty()
            }))
    {
        Ok(())
    } else {
        Err(Error::Invalid)
    }
}
pub(super) fn acl_walk_fd(file: &File, ancestor: bool) -> Result<(), Error> {
    if ancestor {
        observe_acl(file, true)
    } else {
        acl_fd(file)
    }
}
pub(super) fn acl_fd(file: &File) -> Result<(), Error> {
    observe_acl(file, false)
}
fn observe_acl(file: &File, ancestor: bool) -> Result<(), Error> {
    // Darwin fdescfs exposes /dev/fd's ACL, not the target vnode ACL. Resolve
    // the pinned descriptor with F_GETPATH, and verify inode identity around
    // the native acl_get_file observation; never open a regular-file alias.
    let before = rustix::fs::fstat(file).map_err(|_| Error::Invalid)?;
    let path = rustix::fs::getpath(file).map_err(|_| Error::Invalid)?;
    let path = Path::new(std::ffi::OsStr::from_bytes(path.to_bytes()));
    let named = rustix::fs::statat(rustix::fs::CWD, path, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)
        .map_err(|_| Error::Invalid)?;
    if before.st_dev != named.st_dev || before.st_ino != named.st_ino {
        return Err(Error::Changed);
    }
    acl_path_policy(path, ancestor)?;
    let after = rustix::fs::fstat(file).map_err(|_| Error::Invalid)?;
    if before.st_dev != after.st_dev
        || before.st_ino != after.st_ino
        || before.st_ctime != after.st_ctime
        || before.st_ctime_nsec != after.st_ctime_nsec
    {
        return Err(Error::Changed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::namespace::NamespaceGuard;
    #[test]
    fn real_native_acl_is_observed_through_pinned_directory_descriptor() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().canonicalize().unwrap();
        let fd = File::open(&path).unwrap();
        acl_fd(&fd).unwrap();
        exacl::setfacl(
            &[&path],
            &[exacl::AclEntry::allow_group(
                "everyone",
                exacl::Perm::READ,
                None,
            )],
            None,
        )
        .unwrap();
        assert_eq!(acl_path(&path), Err(Error::Invalid));
        assert_eq!(acl_fd(&fd), Err(Error::Invalid));
    }
    #[test]
    fn only_non_inheriting_deny_delete_is_allowed_on_ancestors() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().canonicalize().unwrap();
        let fd = File::open(&path).unwrap();
        exacl::setfacl(
            &[&path],
            &[exacl::AclEntry::deny_group(
                "everyone",
                exacl::Perm::DELETE,
                None,
            )],
            None,
        )
        .unwrap();
        acl_walk_fd(&fd, true).unwrap();
        assert_eq!(acl_fd(&fd), Err(Error::Invalid));
        for entry in [
            exacl::AclEntry::allow_group("everyone", exacl::Perm::WRITE, None),
            exacl::AclEntry::deny_group(
                "everyone",
                exacl::Perm::DELETE,
                exacl::Flag::DIRECTORY_INHERIT,
            ),
        ] {
            exacl::setfacl(&[&path], &[entry], None).unwrap();
            assert_eq!(acl_walk_fd(&fd, true), Err(Error::Invalid));
        }
    }
    #[test]
    fn elevated_or_mismatched_identity_and_wrong_profile_refuse_before_paths() {
        assert_eq!(
            process(0, NamespaceProfile::LocalUserMacos),
            Err(Error::Process)
        );
        assert_eq!(
            process(u32::MAX, NamespaceProfile::LocalUserMacos),
            Err(Error::Process)
        );
        assert_eq!(
            process(501, NamespaceProfile::DedicatedLinux),
            Err(Error::Unsupported)
        );
    }
    #[test]
    #[ignore = "qualified local-account subprocess selected by parent"]
    fn qualified_child() {
        let root = std::path::PathBuf::from(std::env::var("RM_MACOS_ROOT").unwrap());
        let uid =
            libproc::proc_pid::pidinfo::<libproc::bsd_info::BSDInfo>(std::process::id() as i32, 0)
                .unwrap()
                .pbi_uid;
        process(uid, NamespaceProfile::LocalUserMacos).unwrap();
        let mut guard = NamespaceGuard::create_fresh_with_profile(
            &root,
            uid,
            "w",
            "a",
            "Synthetic",
            "2026-01-01",
            NamespaceProfile::LocalUserMacos,
        )
        .unwrap();
        guard.ensure_current().unwrap();
        assert!(matches!(
            NamespaceGuard::acquire_with_profile(
                &root,
                uid,
                "w",
                "a",
                NamespaceProfile::LocalUserMacos
            ),
            Err(Error::Busy)
        ));
        drop(guard);
        let mut guard = NamespaceGuard::acquire_with_profile(
            &root,
            uid,
            "w",
            "a",
            NamespaceProfile::LocalUserMacos,
        )
        .unwrap();
        let manifest = root.join("workspace.json");
        let target = root.join("a/data/ledger.sqlite");
        std::fs::remove_file(&manifest).unwrap();
        std::os::unix::fs::symlink(&target, &manifest).unwrap();
        assert_eq!(guard.ensure_current(), Err(Error::Changed));
        println!("MACOS_NAMESPACE_QUALIFIED");
    }
    #[test]
    fn real_unprivileged_profile_creates_reopens_excludes_and_rejects_symlink_changes() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().canonicalize().unwrap().join("new-workspace");
        let output = std::process::Command::new("/bin/sh")
            .args(["-c", "umask 077; exec \"$@\"", "local-test"])
            .arg(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "namespace::macos::tests::qualified_child",
                "--nocapture",
            ])
            .env("RM_MACOS_ROOT", &root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("MACOS_NAMESPACE_QUALIFIED"));
        for dir in [
            &root,
            &root.join("a"),
            &root.join("a/data"),
            &root.join(".rentemester"),
        ] {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(std::fs::metadata(dir).unwrap().mode() & 0o7777, 0o700);
        }
    }
}
