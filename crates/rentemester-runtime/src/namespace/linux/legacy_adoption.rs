//! Private legacy staging. No native backup signature or restore audit is fabricated.
use super::*;
use crate::legacy_adoption::{AdoptionImage, RECEIPT_PATH};
use std::collections::BTreeSet;

impl NamespaceGuard {
    /// Caller has already validated every domain byte and retained the source writer
    /// fence. This only materializes an immutable, bounded inventory in a fresh private
    /// namespace. The returned guard continuously owns its lease; it is not yet public.
    /// The host must initialize fresh control/bootstrap and close all SQLite handles
    /// before publication. A failed private stage is retained for explicit recovery.
    pub fn create_adoption_stage(
        root: &Path,
        uid: u32,
        workspace: &str,
        slug: &str,
        profile: NamespaceProfile,
        image: &AdoptionImage,
    ) -> Result<Self, Error> {
        if workspace != image.workspace() || slug != image.selected_slug() {
            return Err(Error::Invalid);
        }
        WorkspaceLayout::validate_new_manifest(workspace, slug, image.manifest_bytes())
            .map_err(|_| Error::Invalid)?;
        Self::materialize_private_stage(
            root,
            uid,
            workspace,
            slug,
            profile,
            image.manifest_bytes(),
            image.companies(),
            &std::iter::once((RECEIPT_PATH, image.receipt_bytes()))
                .chain(
                    image
                        .files()
                        .iter()
                        .map(|f| (f.path.as_str(), f.bytes.as_slice())),
                )
                .collect::<Vec<_>>(),
        )
    }

    /// Distinct source-profile proof; never accepted as an AdoptionImage/native artifact.
    pub fn create_source_restore_stage(
        root: &Path,
        uid: u32,
        workspace: &str,
        slug: &str,
        profile: NamespaceProfile,
        source: &crate::original716::transfer::TrustedArchive,
    ) -> Result<Self, Error> {
        let companies = source
            .archive()
            .manifest()
            .companies
            .iter()
            .map(|c| c.slug.clone())
            .collect::<Vec<_>>();
        let files = source
            .stage_files()
            .iter()
            .map(|(p, b)| (p.as_str(), b.as_slice()))
            .collect::<Vec<_>>();
        Self::materialize_private_stage(
            root,
            uid,
            workspace,
            slug,
            profile,
            &source.archive().files()["workspace.json"],
            &companies,
            &files,
        )
    }

    fn materialize_private_stage(
        root: &Path,
        uid: u32,
        workspace: &str,
        slug: &str,
        profile: NamespaceProfile,
        manifest: &[u8],
        companies: &[String],
        files: &[(&str, &[u8])],
    ) -> Result<Self, Error> {
        WorkspaceLayout::validate_new_manifest(workspace, slug, manifest)
            .map_err(|_| Error::Invalid)?;
        process_profile(uid, profile)?;
        if !root.is_absolute()
            || root
                .components()
                .any(|p| matches!(p, Component::ParentDir | Component::CurDir))
        {
            return Err(Error::Invalid);
        }
        let parent = root.parent().ok_or(Error::Invalid)?;
        let leaf = root.file_name().ok_or(Error::Invalid)?;
        let (prior, parent_dir) = walk(parent, uid, false)?;
        rustix::fs::mkdirat(&parent_dir, leaf, Mode::from_raw_mode(0o700)).map_err(|e| {
            if e == rustix::io::Errno::EXIST {
                Error::Exists
            } else {
                Error::Invalid
            }
        })?;
        let (ancestors, directory) = walk(root, uid, true)?;
        if ancestors[..ancestors.len() - 1] != prior {
            return Err(Error::Changed);
        }
        let lease = WorkspaceLease::acquire(root).map_err(|_| Error::Busy)?;
        let mut directories = BTreeSet::from([".rentemester".to_owned()]);
        for company in companies {
            directories.insert(company.to_owned());
            for suffix in crate::namespace::COMPANY_LAYOUT_DIRS {
                directories.insert(format!("{company}/{suffix}"));
            }
        }
        // Inventory paths are proof-validated canonical names. Create each
        // intermediate directory once, parent first, under the fresh held root.
        for (path, _) in files {
            let parts = path.split('/').collect::<Vec<_>>();
            for n in 1..parts.len() {
                directories.insert(parts[..n].join("/"));
            }
        }
        let mut dirs: Vec<_> = directories.into_iter().collect();
        dirs.sort_by_key(|d| d.split('/').count());
        for dir in &dirs {
            rustix::fs::mkdirat(&directory, dir.as_str(), Mode::from_raw_mode(0o700))
                .map_err(|_| Error::Invalid)?;
        }
        let write = |path: &str, bytes: &[u8]| -> Result<(), Error> {
            let mut file = File::from(
                openat(
                    &directory,
                    path,
                    OFlags::WRONLY
                        | OFlags::CREATE
                        | OFlags::EXCL
                        | OFlags::NOFOLLOW
                        | OFlags::CLOEXEC,
                    Mode::from_raw_mode(0o600),
                )
                .map_err(|_| Error::Invalid)?,
            );
            file.write_all(bytes).map_err(|_| Error::Invalid)?;
            file.sync_all().map_err(|_| Error::Invalid)
        };
        write(".rentemester/workspace-control.sqlite", &[])?;
        write("workspace.json", manifest)?;
        for (path, bytes) in files {
            write(path, bytes)?;
        }
        for dir in dirs.iter().rev() {
            let d = File::from(
                openat(&directory, dir.as_str(), DIRECTORY, Mode::empty())
                    .map_err(|_| Error::Invalid)?,
            );
            d.sync_all().map_err(|_| Error::Invalid)?;
        }
        directory.sync_all().map_err(|_| Error::Invalid)?;
        parent_dir.sync_all().map_err(|_| Error::Invalid)?;
        Self::bind(
            root.to_owned(),
            uid,
            workspace,
            slug,
            ancestors,
            directory,
            lease,
            profile,
        )
    }

    /// Same continuously held lease and same-parent no-replace publication as native
    /// restore; the adoption host supplies distinct provenance and fresh control state.
    pub fn publish_adopted_root(&mut self, target: &Path) -> Result<(), Error> {
        self.publish_restored_root(target)
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::legacy_adoption::{AdoptionFile, AdoptionFileKind};
    use sha2::{Digest, Sha256};
    #[test]
    #[ignore = "qualified local-account subprocess selected by parent"]
    fn qualified_child() {
        let root = PathBuf::from(std::env::var("RM_ADOPTION_STAGE_ROOT").unwrap());
        let uid =
            libproc::proc_pid::pidinfo::<libproc::bsd_info::BSDInfo>(std::process::id() as i32, 0)
                .unwrap()
                .pbi_uid;
        let manifest = br#"{"version":1,"companies":[{"slug":"a","name":"Synthetic","createdAt":"2026-01-01","archived":false}]}"#.to_vec();
        let ledger = b"synthetic domain validation belongs to host".to_vec();
        let image = AdoptionImage::validate(
            "w",
            "a",
            manifest,
            vec![AdoptionFile {
                kind: AdoptionFileKind::CompanyLedger {
                    company: "a".into(),
                },
                path: "a/data/ledger.sqlite".into(),
                sha256: format!("{:x}", Sha256::digest(&ledger)),
                bytes: ledger.clone(),
            }],
            b"{}".to_vec(),
            &format!("{:x}", Sha256::digest(b"{}")),
        )
        .unwrap();
        let profile = NamespaceProfile::LocalUserMacos;
        let wrong = root.with_file_name("wrong");
        assert!(matches!(
            NamespaceGuard::create_adoption_stage(&wrong, uid, "other", "a", profile, &image),
            Err(Error::Invalid)
        ));
        assert!(!wrong.exists());
        let mut guard =
            NamespaceGuard::create_adoption_stage(&root, uid, "w", "a", profile, &image).unwrap();
        assert_eq!(
            std::fs::read(root.join("a/data/ledger.sqlite")).unwrap(),
            ledger
        );
        assert_eq!(std::fs::read(root.join(RECEIPT_PATH)).unwrap(), b"{}");
        assert_eq!(
            std::fs::metadata(root.join(".rentemester/workspace-control.sqlite"))
                .unwrap()
                .len(),
            0
        );
        assert!(matches!(
            NamespaceGuard::acquire_with_profile(&root, uid, "w", "a", profile),
            Err(Error::Busy)
        ));
        assert!(matches!(
            NamespaceGuard::create_adoption_stage(&root, uid, "w", "a", profile, &image),
            Err(Error::Exists)
        ));
        let occupied = root.with_file_name("occupied");
        std::fs::create_dir(&occupied).unwrap();
        let linked = root.with_file_name("linked");
        std::os::unix::fs::symlink(&occupied, &linked).unwrap();
        assert!(matches!(
            NamespaceGuard::create_adoption_stage(&linked, uid, "w", "a", profile, &image),
            Err(Error::Exists)
        ));
        assert_eq!(guard.publish_adopted_root(&linked), Err(Error::Exists));
        assert!(
            std::fs::symlink_metadata(&linked)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(guard.publish_adopted_root(&occupied), Err(Error::Exists));
        guard.ensure_current().unwrap();
        let target = root.with_file_name("adopted");
        guard.publish_adopted_root(&target).unwrap();
        assert!(!root.exists());
        assert!(matches!(
            NamespaceGuard::acquire_with_profile(&target, uid, "w", "a", profile),
            Err(Error::Busy)
        ));
        drop(guard);
        NamespaceGuard::acquire_with_profile(&target, uid, "w", "a", profile).unwrap();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(target.join(RECEIPT_PATH))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    #[test]
    fn private_stage_keeps_lease_and_never_overwrites_public_destination() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().canonicalize().unwrap().join("stage");
        let output = std::process::Command::new("/bin/sh")
            .args(["-c", "umask 077; exec \"$@\"", "adoption-test"])
            .arg(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "namespace::linux::legacy_adoption::tests::qualified_child",
                "--nocapture",
            ])
            .env("RM_ADOPTION_STAGE_ROOT", &root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
