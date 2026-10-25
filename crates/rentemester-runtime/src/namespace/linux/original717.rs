//! Fixed private mutable company configuration; never a document/original or backup entry.
use super::{DIRECTORY, Error, NamespaceGuard, acl_walk_fd, identity, walk};
use rustix::fs::{
    AtFlags, FileType, Mode, OFlags, fstat, mkdirat, openat, renameat, statat, unlinkat,
};
use std::{
    fs::File,
    io::{Read, Write},
};
const MAX: usize = 16 * 1024;

impl NamespaceGuard {
    fn original717_config_directory(&mut self, create: bool) -> Result<Option<File>, Error> {
        self.ensure_current()?;
        let (_, root) = walk(&self.root, self.uid, true)?;
        let company = File::from(
            openat(&root, self.slug.as_str(), DIRECTORY, Mode::empty())
                .map_err(|_| Error::Invalid)?,
        );
        let st = fstat(&company).map_err(|_| Error::Invalid)?;
        if st.st_uid != self.uid || st.st_mode & 0o7777 != 0o700 {
            return Err(Error::Invalid);
        }
        acl_walk_fd(&company, false)?;
        if create {
            match mkdirat(&company, "config", Mode::from_raw_mode(0o700)) {
                Ok(()) => company.sync_all().map_err(|_| Error::Invalid)?,
                Err(rustix::io::Errno::EXIST) => {}
                Err(_) => return Err(Error::Invalid),
            }
        }
        let fd = match openat(&company, "config", DIRECTORY, Mode::empty()) {
            Ok(fd) => fd,
            Err(rustix::io::Errno::NOENT) if !create => return Ok(None),
            Err(_) => return Err(Error::Invalid),
        };
        let directory = File::from(fd);
        let st = fstat(&directory).map_err(|_| Error::Invalid)?;
        if st.st_uid != self.uid || st.st_mode & 0o7777 != 0o700 {
            return Err(Error::Invalid);
        }
        acl_walk_fd(&directory, false)?;
        Ok(Some(directory))
    }
    pub fn read_smtp_config(&mut self) -> Result<Option<Vec<u8>>, Error> {
        self.read_original717_config("smtp.json")
    }
    pub fn read_digisense_config(&mut self) -> Result<Option<Vec<u8>>, Error> {
        self.read_original717_config("digisense.json")
    }
    pub fn write_digisense_config(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let directory = self
            .original717_config_directory(true)?
            .ok_or(Error::Invalid)?;
        write(&directory, self.uid, "digisense.json", bytes)?;
        self.ensure_current()
    }
    fn read_original717_config(&mut self, name: &str) -> Result<Option<Vec<u8>>, Error> {
        let Some(directory) = self.original717_config_directory(false)? else {
            return Ok(None);
        };
        let value = read(&directory, self.uid, name)?;
        self.ensure_current()?;
        Ok(value)
    }
}
// Refuse aliases before opening: closing an alias of SQLite could release POSIX locks.
fn checked_stat(directory: &File, uid: u32, name: &str) -> Result<Option<rustix::fs::Stat>, Error> {
    let st = match statat(directory, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(st) => st,
        Err(rustix::io::Errno::NOENT) => return Ok(None),
        Err(_) => return Err(Error::Invalid),
    };
    if FileType::from_raw_mode(st.st_mode) != FileType::RegularFile
        || st.st_nlink != 1
        || st.st_uid != uid
        || st.st_mode & 0o7777 != 0o600
        || st.st_size <= 0
        || st.st_size > MAX as i64
    {
        return Err(Error::Invalid);
    }
    Ok(Some(st))
}
fn read(directory: &File, uid: u32, name: &str) -> Result<Option<Vec<u8>>, Error> {
    let Some(before) = checked_stat(directory, uid, name)? else {
        return Ok(None);
    };
    let mut file = File::from(
        openat(
            directory,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(|_| Error::Invalid)?,
    );
    let opened = fstat(&file).map_err(|_| Error::Invalid)?;
    if identity(&opened) != identity(&before) {
        return Err(Error::Changed);
    }
    acl_walk_fd(&file, false)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Invalid)?;
    let after = fstat(&file).map_err(|_| Error::Invalid)?;
    if identity(&after) != identity(&before)
        || before.st_size != after.st_size
        || bytes.len() as i64 != after.st_size
        || before.st_mtime != after.st_mtime
        || before.st_mtime_nsec != after.st_mtime_nsec
        || before.st_ctime != after.st_ctime
        || before.st_ctime_nsec != after.st_ctime_nsec
        || checked_stat(directory, uid, name)?.as_ref().map(identity) != Some(identity(&before))
    {
        return Err(Error::Changed);
    }
    Ok(Some(bytes))
}
fn write(directory: &File, uid: u32, name: &str, bytes: &[u8]) -> Result<(), Error> {
    if bytes.is_empty() || bytes.len() > MAX {
        return Err(Error::Invalid);
    }
    let previous = checked_stat(directory, uid, name)?;
    if previous.is_some() {
        read(directory, uid, name)?;
    }
    let token = crate::auth::new_token().map_err(|_| Error::Invalid)?;
    let temporary = format!(".original717-{}", token.as_str());
    let result = (|| {
        let mut file = File::from(
            openat(
                directory,
                temporary.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o600),
            )
            .map_err(|_| Error::Invalid)?,
        );
        file.write_all(bytes).map_err(|_| Error::Invalid)?;
        file.sync_all().map_err(|_| Error::Invalid)?;
        acl_walk_fd(&file, false)?;
        if checked_stat(directory, uid, name)?.as_ref().map(identity)
            != previous.as_ref().map(identity)
        {
            return Err(Error::Changed);
        }
        renameat(directory, temporary.as_str(), directory, name).map_err(|_| Error::Invalid)?;
        directory.sync_all().map_err(|_| Error::Invalid)?;
        Ok(())
    })();
    let _ = unlinkat(directory, temporary.as_str(), AtFlags::empty());
    result
}

mod physical;
pub use physical::Original717RestoreStage;
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    #[test]
    fn original717_fixed_secret_modes_bounds_alias_and_unsafe_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let dir = File::open(temp.path()).unwrap();
        let uid = dir.metadata().unwrap().uid();
        for name in ["smtp.json", "digisense.json"] {
            assert!(read(&dir, uid, name).unwrap().is_none());
            write(&dir, uid, name, b"secret").unwrap();
            assert_eq!(read(&dir, uid, name).unwrap().unwrap(), b"secret");
            assert_eq!(
                temp.path().join(name).metadata().unwrap().mode() & 0o7777,
                0o600
            );
            assert!(write(&dir, uid, name, &vec![0; MAX + 1]).is_err());
            std::fs::hard_link(temp.path().join(name), temp.path().join("alias")).unwrap();
            assert!(read(&dir, uid, name).is_err());
            assert!(write(&dir, uid, name, b"new").is_err());
            std::fs::remove_file(temp.path().join("alias")).unwrap();
            std::fs::set_permissions(
                temp.path().join(name),
                std::fs::Permissions::from_mode(0o644),
            )
            .unwrap();
            assert!(write(&dir, uid, name, b"new").is_err());
            std::fs::remove_file(temp.path().join(name)).unwrap();
            std::os::unix::fs::symlink("absent", temp.path().join(name)).unwrap();
            assert!(read(&dir, uid, name).is_err());
            assert!(write(&dir, uid, name, b"new").is_err());
        }
    }
}
pub(crate) use physical::observe_retained;

impl NamespaceGuard {
    fn original716_company_directory(&mut self) -> Result<File, Error> {
        self.ensure_current()?;
        let (_, root) = walk(&self.root, self.uid, true)?;
        let company = File::from(
            openat(&root, self.slug.as_str(), DIRECTORY, Mode::empty())
                .map_err(|_| Error::Invalid)?,
        );
        let st = fstat(&company).map_err(|_| Error::Invalid)?;
        if st.st_uid != self.uid || st.st_mode & 0o7777 != 0o700 {
            return Err(Error::Invalid);
        }
        acl_walk_fd(&company, false)?;
        Ok(company)
    }
    /// Fixed source PEM pair only. Partial or mismatched custody fails closed; no repair/retry.
    pub fn original716_signing_key(
        &mut self,
        generate: bool,
    ) -> Result<crate::original716::custody::CompanyKey, Error> {
        use crate::original716::custody::{CompanyKey, PRIVATE};
        let company = self.original716_company_directory()?;
        let config = self
            .original717_config_directory(true)?
            .ok_or(Error::Invalid)?;
        let private = read(&company, self.uid, PRIVATE)?.map(zeroize::Zeroizing::new);
        let public = read(&config, self.uid, "backup-manifest.pub")?;
        let key = match (private, public) {
            (Some(private), Some(public)) => {
                CompanyKey::load(&private, &public).map_err(|_| Error::Invalid)?
            }
            (None, None) if generate => {
                let key = CompanyKey::generate().map_err(|_| Error::Invalid)?;
                // Both names were checked absent under the one admitted Owner. Each write uses
                // EXCL staging and checked identity. A failed second write stays visibly partial.
                write(&company, self.uid, PRIVATE, key.private_pem())?;
                write(
                    &config,
                    self.uid,
                    "backup-manifest.pub",
                    key.public_pem().as_bytes(),
                )?;
                key
            }
            _ => return Err(Error::Invalid),
        };
        self.ensure_current()?;
        Ok(key)
    }
    pub fn original716_public_key(&mut self) -> Result<Option<Vec<u8>>, Error> {
        let value = self.read_original717_config("backup-manifest.pub")?;
        if let Some(v) = &value {
            crate::backup_artifact::original717::parse_public_key(v).map_err(|_| Error::Invalid)?;
        }
        Ok(value)
    }
    /// Old pair remains recoverable under source timestamp/fingerprint names before replacing
    /// live files. On any failure callers must expose outcome-unknown and must not retry.
    pub fn original716_rotate_key(&mut self, at: &str) -> Result<serde_json::Value, Error> {
        use crate::original716::custody::{CompanyKey, PRIVATE};
        if chrono::DateTime::parse_from_rfc3339(at).is_err() {
            return Err(Error::Invalid);
        }
        let old = self.original716_signing_key(false)?;
        let new = CompanyKey::generate().map_err(|_| Error::Invalid)?;
        let company = self.original716_company_directory()?;
        let config = self
            .original717_config_directory(false)?
            .ok_or(Error::Invalid)?;
        match mkdirat(&company, "backup-keys-archive", Mode::from_raw_mode(0o700)) {
            Ok(()) => company.sync_all().map_err(|_| Error::Invalid)?,
            Err(rustix::io::Errno::EXIST) => {}
            Err(_) => return Err(Error::Invalid),
        }
        let archive = File::from(
            openat(&company, "backup-keys-archive", DIRECTORY, Mode::empty())
                .map_err(|_| Error::Invalid)?,
        );
        let st = fstat(&archive).map_err(|_| Error::Invalid)?;
        if st.st_uid != self.uid || st.st_mode & 0o7777 != 0o700 {
            return Err(Error::Invalid);
        }
        acl_walk_fd(&archive, false)?;
        let stamp: String = at.chars().filter(char::is_ascii_alphanumeric).collect();
        let stem = format!("{stamp}-{}", old.hint());
        let priv_name = format!("{stem}.key.pem");
        let pub_name = format!("{stem}.pub.pem");
        if checked_stat(&archive, self.uid, &priv_name)?.is_some()
            || checked_stat(&archive, self.uid, &pub_name)?.is_some()
        {
            return Err(Error::Exists);
        }
        write(&archive, self.uid, &priv_name, old.private_pem())?;
        write(&archive, self.uid, &pub_name, old.public_pem().as_bytes())?;
        write(&company, self.uid, PRIVATE, new.private_pem())?;
        write(
            &config,
            self.uid,
            "backup-manifest.pub",
            new.public_pem().as_bytes(),
        )?;
        self.ensure_current()?;
        let root = self.root.join(&self.slug);
        Ok(
            serde_json::json!({"ok":true,"errors":[],"oldPublicKeyHint":old.hint(),"newPublicKeyHint":new.hint(),"archivedPrivateKeyPath":root.join("backup-keys-archive").join(priv_name),"archivedPublicKeyPath":root.join("backup-keys-archive").join(pub_name),"newPrivateKeyPath":root.join(PRIVATE),"newPublicKeyPath":root.join("config/backup-manifest.pub")}),
        )
    }
}

#[cfg(test)]
mod custody_original716_tests {
    use super::*;
    #[test]
    fn original716_private_pem_modes_partial_symlink_and_archival_bytes() {
        use crate::original716::custody::CompanyKey;
        let temp = tempfile::tempdir().unwrap();
        let directory = File::open(temp.path()).unwrap();
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let uid = directory.metadata().unwrap().uid();
        let key = CompanyKey::generate().unwrap();
        write(
            &directory,
            uid,
            ".backup-signing-key.pem",
            key.private_pem(),
        )
        .unwrap();
        assert_eq!(directory.metadata().unwrap().uid(), uid);
        assert_eq!(
            temp.path()
                .join(".backup-signing-key.pem")
                .metadata()
                .unwrap()
                .mode()
                & 0o7777,
            0o600
        );
        let private = read(&directory, uid, ".backup-signing-key.pem")
            .unwrap()
            .unwrap();
        assert!(CompanyKey::load(&private, b"missing public key").is_err());
        let archived = format!("20260101T000000000Z-{}.key.pem", key.hint());
        write(&directory, uid, &archived, &private).unwrap();
        assert_eq!(read(&directory, uid, &archived).unwrap().unwrap(), private);
        std::fs::hard_link(temp.path().join(&archived), temp.path().join("alias")).unwrap();
        assert!(read(&directory, uid, &archived).is_err());
        std::fs::remove_file(temp.path().join("alias")).unwrap();
        std::fs::set_permissions(
            temp.path().join(&archived),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert!(read(&directory, uid, &archived).is_err());
        std::os::unix::fs::symlink("absent", temp.path().join("backup-manifest.pub")).unwrap();
        assert!(read(&directory, uid, "backup-manifest.pub").is_err());
    }
}

#[cfg(all(test, target_os = "macos"))]
mod admitted_custody_original716_tests {
    use super::*;
    #[test]
    #[ignore = "qualified subprocess"]
    fn original716_custody_child() {
        use crate::backup_artifact::original717 as codec;
        use crate::original716::custody;
        let base = std::path::PathBuf::from(std::env::var("RM_CUSTODY716_ROOT").unwrap());
        let uid =
            libproc::proc_pid::pidinfo::<libproc::bsd_info::BSDInfo>(std::process::id() as i32, 0)
                .unwrap()
                .pbi_uid;
        let mut guard = NamespaceGuard::create_fresh_with_profile(
            &base.join("workspace"),
            uid,
            "synthetic-workspace",
            "synthetic",
            "Synthetic",
            "2026-01-01",
            crate::namespace::NamespaceProfile::LocalUserMacos,
        )
        .unwrap();
        assert!(guard.original716_signing_key(false).is_err());
        let key = guard.original716_signing_key(true).unwrap();
        let public = guard.original716_public_key().unwrap().unwrap();
        assert_eq!(public, key.public_pem().as_bytes());
        assert_eq!(
            guard.original716_signing_key(true).unwrap().hint(),
            key.hint()
        );
        let hmac = guard.original717_manifest_key().unwrap();
        let (mut files, _) = codec::tests::fixture();
        let mut manifest: codec::Manifest =
            serde_json::from_slice(&files["manifest.json"]).unwrap();
        manifest.manifest_signature.key_hint = codec::sha256(&hmac)[..16].to_owned();
        files.insert(custody::PUBLIC.into(), public.clone());
        manifest.copied_files.config.push(codec::File {
            path: custody::PUBLIC.into(),
            sha256: codec::sha256(&public),
            size_bytes: public.len() as u64,
        });
        let proof = codec::seal_original716(files, manifest, &hmac, Some(&key)).unwrap();
        let path = guard.retain_original717_directory(&proof).unwrap();
        guard.pack_original717(&proof, None, None).unwrap();
        assert_eq!(
            custody::verify(proof.files(), Some(&public), None)["ok"],
            true
        );
        let result = guard
            .original716_rotate_key("2026-01-02T00:00:00.000Z")
            .unwrap();
        assert_ne!(result["oldPublicKeyHint"], result["newPublicKeyHint"]);
        assert_eq!(
            std::fs::read(result["archivedPublicKeyPath"].as_str().unwrap()).unwrap(),
            public
        );
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            std::fs::metadata(result["archivedPrivateKeyPath"].as_str().unwrap())
                .unwrap()
                .mode()
                & 0o7777,
            0o600
        );
        let old_public = std::path::Path::new(result["archivedPublicKeyPath"].as_str().unwrap());
        assert_eq!(
            custody::verify_local(&path, Some(old_public), None, &base.join("workspace"), uid)
                .unwrap()["ok"],
            true
        );
        assert!(
            guard
                .load_original717(Some(&proof.manifest().backup_id), Some(&public))
                .is_ok()
        );
        // Removing one half never bootstraps a new key over the retained identity.
        std::fs::remove_file(base.join("workspace/synthetic/.backup-signing-key.pem")).unwrap();
        assert!(guard.original716_signing_key(true).is_err());
    }
    #[test]
    fn original716_admitted_sign_verify_export_rotate_old_signature() {
        let base = tempfile::tempdir().unwrap();
        let output = std::process::Command::new("/bin/sh").args(["-c","umask 077; exec \"$@\"","original716-custody-test"])
            .arg(std::env::current_exe().unwrap()).args(["--ignored","--exact","namespace::linux::original717::admitted_custody_original716_tests::original716_custody_child","--nocapture"])
            .env("RM_CUSTODY716_ROOT",base.path().canonicalize().unwrap()).output().unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
