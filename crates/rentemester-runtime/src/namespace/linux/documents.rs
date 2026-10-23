//! Content-addressed originals under the protected broker root. No database inode opens.
use super::{DIRECTORY, Error, NamespaceGuard, walk};
#[cfg(all(test, target_os = "macos"))]
mod crash_tests;
mod recovery;
use crate::namespace::PublishedDocument;
use rustix::fs::{
    AtFlags, FileType, Mode, OFlags, RenameFlags, fstat, mkdirat, openat, renameat_with, statat,
    unlinkat,
};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
};
impl NamespaceGuard {
    pub fn snapshot_originals(&mut self, bank: bool) -> Result<Vec<(String, Vec<u8>)>, Error> {
        self.ensure_current()?;
        let (_, root) = walk(&self.root, self.uid, true)?;
        let mut directory = File::from(
            openat(&root, self.slug.as_str(), DIRECTORY, Mode::empty())
                .map_err(|_| Error::Invalid)?,
        );
        for name in if bank {
            ["bank", "processed"]
        } else {
            ["documents", "originals"]
        } {
            let fd = match openat(&directory, name, DIRECTORY, Mode::empty()) {
                Ok(fd) => fd,
                Err(rustix::io::Errno::NOENT) => return Ok(vec![]),
                Err(_) => return Err(Error::Invalid),
            };
            directory = File::from(fd);
            let st = fstat(&directory).map_err(|_| Error::Invalid)?;
            if st.st_uid != self.uid || st.st_mode & 0o7777 != 0o700 {
                return Err(Error::Invalid);
            }
        }
        self.reconcile_staging(&directory, bank)?;
        let mut names = vec![];
        for item in rustix::fs::Dir::read_from(&directory).map_err(|_| Error::Invalid)? {
            let item = item.map_err(|_| Error::Invalid)?;
            let name = item.file_name().to_str().map_err(|_| Error::Invalid)?;
            if name == "." || name == ".." || name == ".recovery" {
                continue;
            }
            if names.len() >= crate::backup_artifact::MAX_ENTRIES {
                return Err(Error::Invalid);
            }
            names.push(name.to_owned());
        }
        names.sort();
        let mut out = vec![];
        let mut total = 0usize;
        for name in names {
            let (hash, ext) = name.split_once('.').ok_or(Error::Invalid)?;
            if hash.len() != 64
                || !hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || if bank {
                    ext != "txt"
                } else {
                    !["pdf", "png", "jpg", "jpeg", "txt", "json", "xml", "zip"].contains(&ext)
                }
            {
                return Err(Error::Invalid);
            }
            let bytes = self.read_original(&directory, &name, hash)?;
            total = total.checked_add(bytes.len()).ok_or(Error::Invalid)?;
            if total > crate::backup_artifact::MAX_TOTAL_BYTES {
                return Err(Error::Invalid);
            }
            out.push((
                format!(
                    "{}/{}",
                    if bank {
                        "bank/processed"
                    } else {
                        "documents/originals"
                    },
                    name
                ),
                bytes,
            ));
        }
        self.ensure_current()?;
        Ok(out)
    }

    fn original_directory(&mut self, create: bool, bank: bool) -> Result<File, Error> {
        self.ensure_current()?;
        let (_, root) = walk(&self.root, self.uid, true)?;
        let mut directory = File::from(
            openat(&root, self.slug.as_str(), DIRECTORY, Mode::empty())
                .map_err(|_| Error::Invalid)?,
        );
        for name in if bank {
            ["bank", "processed"]
        } else {
            ["documents", "originals"]
        } {
            if create {
                match mkdirat(&directory, name, Mode::from_raw_mode(0o700)) {
                    Ok(()) => {
                        directory.sync_all().map_err(|_| Error::Invalid)?;
                    }
                    Err(rustix::io::Errno::EXIST) => {}
                    Err(_) => return Err(Error::Invalid),
                }
            }
            let next = File::from(
                openat(&directory, name, DIRECTORY, Mode::empty()).map_err(|_| Error::Invalid)?,
            );
            let stat = fstat(&next).map_err(|_| Error::Invalid)?;
            if stat.st_uid != self.uid || stat.st_mode & 0o7777 != 0o700 {
                return Err(Error::Invalid);
            }
            directory = next;
        }
        self.reconcile_staging(&directory, bank)?;
        Ok(directory)
    }
    fn read_original(&self, directory: &File, name: &str, hash: &str) -> Result<Vec<u8>, Error> {
        // Check links before opening: even closing a hardlink alias to a live
        // SQLite inode could release this process's POSIX database locks.
        let before =
            statat(directory, name, AtFlags::SYMLINK_NOFOLLOW).map_err(|_| Error::Invalid)?;
        if FileType::from_raw_mode(before.st_mode) != FileType::RegularFile
            || before.st_nlink != 1
            || before.st_uid != self.uid
            || before.st_mode & 0o7777 != 0o600
            || before.st_size <= 0
            || before.st_size as u64 > crate::document::MAX_BYTES as u64
        {
            return Err(Error::Invalid);
        }
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
        if before.st_dev != opened.st_dev || before.st_ino != opened.st_ino {
            return Err(Error::Changed);
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(crate::document::MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Invalid)?;
        #[cfg(all(test, target_os = "macos"))]
        crash_tests::after_read(directory, name);
        let final_path =
            statat(directory, name, AtFlags::SYMLINK_NOFOLLOW).map_err(|_| Error::Changed)?;
        let after = fstat(&file).map_err(|_| Error::Invalid)?;
        if final_path.st_dev != opened.st_dev
            || final_path.st_ino != opened.st_ino
            || final_path.st_mode != before.st_mode
            || final_path.st_uid != before.st_uid
            || final_path.st_size != after.st_size
            || final_path.st_mtime != after.st_mtime
            || final_path.st_mtime_nsec != after.st_mtime_nsec
            || final_path.st_ctime != after.st_ctime
            || final_path.st_ctime_nsec != after.st_ctime_nsec
            || final_path.st_nlink != 1
            || after.st_nlink != 1
            || before.st_size != after.st_size
            || before.st_mtime != after.st_mtime
            || before.st_mtime_nsec != after.st_mtime_nsec
            || before.st_ctime != after.st_ctime
            || before.st_ctime_nsec != after.st_ctime_nsec
            || bytes.len() as i64 != after.st_size
            || format!("{:x}", Sha256::digest(&bytes)) != hash
        {
            return Err(Error::Changed);
        }
        Ok(bytes)
    }
    pub fn publish_document(&mut self, input: &crate::document::Prepared) -> Result<String, Error> {
        self.publish_original(input, false)
    }
    pub fn publish_document_tracked(
        &mut self,
        input: &crate::document::Prepared,
    ) -> Result<(String, Option<PublishedDocument>), Error> {
        self.publish_original_tracked(input, false)
    }
    /// Remove only the same inode created by this attempt, after a known rollback.
    #[allow(clippy::unnecessary_cast)] // Native stat integer widths differ across supported hosts.
    pub fn rollback_document(&mut self, owned: PublishedDocument) -> Result<(), Error> {
        let directory = self.original_directory(false, false)?;
        let name = owned
            .path
            .strip_prefix("documents/originals/")
            .ok_or(Error::Invalid)?;
        if name.contains('/') || !name.starts_with(&format!("{}.", owned.hash)) {
            return Err(Error::Invalid);
        }
        let current =
            statat(&directory, name, AtFlags::SYMLINK_NOFOLLOW).map_err(|_| Error::Changed)?;
        if current.st_dev as u64 != owned.device || current.st_ino as u64 != owned.inode {
            return Err(Error::Changed);
        }
        self.read_original(&directory, name, &owned.hash)?;
        unlinkat(&directory, name, AtFlags::empty()).map_err(|_| Error::Invalid)?;
        directory.sync_all().map_err(|_| Error::Invalid)
    }
    pub fn publish_bank_statement(&mut self, content: &str) -> Result<String, Error> {
        self.publish_bank_statement_bytes(content.as_bytes())
    }
    pub fn publish_bank_statement_bytes(&mut self, bytes: &[u8]) -> Result<String, Error> {
        let input = crate::document::Prepared::new("bank-import.txt".into(), bytes.to_vec())
            .map_err(|_| Error::Invalid)?;
        self.publish_original(&input, true)
    }
    fn publish_original(
        &mut self,
        input: &crate::document::Prepared,
        bank: bool,
    ) -> Result<String, Error> {
        self.publish_original_tracked(input, bank)
            .map(|(path, _)| path)
    }
    #[allow(clippy::unnecessary_cast)] // Native stat integer widths differ across supported hosts.
    fn publish_original_tracked(
        &mut self,
        input: &crate::document::Prepared,
        bank: bool,
    ) -> Result<(String, Option<PublishedDocument>), Error> {
        let directory = self.original_directory(true, bank)?;
        let path = if bank {
            format!("bank/processed/{}.txt", input.hash())
        } else {
            input.relative_path()
        };
        let name = path.rsplit('/').next().unwrap();
        match statat(&directory, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(_) => {
                self.read_original(&directory, name, input.hash())?;
                return Ok((path, None));
            }
            Err(rustix::io::Errno::NOENT) => {}
            Err(_) => return Err(Error::Invalid),
        }
        let nonce = crate::auth::new_token().map_err(|_| Error::Invalid)?;
        let temporary = format!(".upload-{}", nonce.as_str());
        let mut owned = None;
        let result = (|| {
            let mut file = File::from(
                openat(
                    &directory,
                    temporary.as_str(),
                    OFlags::WRONLY
                        | OFlags::CREATE
                        | OFlags::EXCL
                        | OFlags::NOFOLLOW
                        | OFlags::CLOEXEC,
                    Mode::from_raw_mode(0o600),
                )
                .map_err(|_| Error::Invalid)?,
            );
            #[cfg(all(test, target_os = "macos"))]
            crash_tests::checkpoint("created");
            file.write_all(input.bytes()).map_err(|_| Error::Invalid)?;
            #[cfg(all(test, target_os = "macos"))]
            crash_tests::checkpoint("written");
            file.sync_all().map_err(|_| Error::Invalid)?;
            #[cfg(all(test, target_os = "macos"))]
            crash_tests::checkpoint("synced");
            let identity = fstat(&file).map_err(|_| Error::Invalid)?;
            drop(file);
            match renameat_with(
                &directory,
                temporary.as_str(),
                &directory,
                name,
                RenameFlags::NOREPLACE,
            ) {
                Ok(()) => {
                    #[cfg(all(test, target_os = "macos"))]
                    crash_tests::checkpoint("renamed");
                    owned = Some(PublishedDocument {
                        path: path.clone(),
                        hash: input.hash().into(),
                        device: identity.st_dev as u64,
                        inode: identity.st_ino as u64,
                    });
                }
                Err(rustix::io::Errno::EXIST) => {
                    self.read_original(&directory, name, input.hash())?;
                }
                Err(_) => return Err(Error::Invalid),
            }
            directory.sync_all().map_err(|_| Error::Invalid)?;
            #[cfg(all(test, target_os = "macos"))]
            crash_tests::checkpoint("directory-synced");
            Ok(path.clone())
        })();
        let _ = unlinkat(&directory, temporary.as_str(), AtFlags::empty());
        match result {
            Ok(path) => Ok((path, owned)),
            Err(error) => {
                // A failed directory sync still owns the successfully renamed inode.
                if let Some(created) = owned {
                    let same = statat(&directory, name, AtFlags::SYMLINK_NOFOLLOW).is_ok_and(|s| {
                        s.st_dev as u64 == created.device && s.st_ino as u64 == created.inode
                    });
                    if same {
                        let _ = unlinkat(&directory, name, AtFlags::empty());
                        let _ = directory.sync_all();
                    }
                }
                Err(error)
            }
        }
    }
    /// Read only the content-addressed archive admitted by bank import.
    pub fn read_bank_statement(&mut self, hash: &str) -> Result<Vec<u8>, Error> {
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::Invalid);
        }
        let directory = self.original_directory(false, true)?;
        self.read_original(&directory, &format!("{hash}.txt"), hash)
    }
    pub fn read_document(&mut self, path: &str, hash: &str) -> Result<Vec<u8>, Error> {
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::Invalid);
        }
        let name = path
            .strip_prefix("documents/originals/")
            .ok_or(Error::Invalid)?;
        let (base, extension) = name.split_once('.').ok_or(Error::Invalid)?;
        if base != hash
            || !["pdf", "png", "jpg", "jpeg", "txt", "json", "xml", "zip"].contains(&extension)
        {
            return Err(Error::Invalid);
        }
        let directory = self.original_directory(false, false)?;
        self.read_original(&directory, name, hash)
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    #[test]
    #[ignore = "qualified local-account subprocess selected by parent"]
    fn tracked_child() {
        let root = std::path::PathBuf::from(std::env::var("RM_TRACKED_ROOT").unwrap());
        let uid =
            libproc::proc_pid::pidinfo::<libproc::bsd_info::BSDInfo>(std::process::id() as i32, 0)
                .unwrap()
                .pbi_uid;
        let mut guard = NamespaceGuard::create_fresh_with_profile(
            &root,
            uid,
            "w",
            "a",
            "Synthetic",
            "2026-01-01",
            crate::namespace::NamespaceProfile::LocalUserMacos,
        )
        .unwrap();
        let original = crate::document::Prepared::new(
            "receipt.pdf".into(),
            b"%PDF-1.4\nSynthetic receipt125\n".to_vec(),
        )
        .unwrap();
        let (path, token) = guard.publish_document_tracked(&original).unwrap();
        let token = token.expect("new inode owned");
        let (same, reused) = guard.publish_document_tracked(&original).unwrap();
        assert_eq!(path, same);
        assert!(reused.is_none(), "reused bytes never become rollback-owned");
        assert_eq!(
            guard.read_document(&path, original.hash()).unwrap(),
            original.bytes()
        );
        guard.rollback_document(token).unwrap();
        assert!(!root.join("a").join(&path).exists());
        let (_, old_token) = guard.publish_document_tracked(&original).unwrap();
        let replacement = root.join("replacement.pdf");
        std::fs::write(&replacement, original.bytes()).unwrap();
        std::fs::rename(&replacement, root.join("a").join(&path)).unwrap();
        assert_eq!(
            guard.rollback_document(old_token.unwrap()),
            Err(Error::Changed)
        );
        assert_eq!(
            guard.read_document(&path, original.hash()).unwrap(),
            original.bytes()
        );
        assert!(
            guard
                .publish_document_tracked(&original)
                .unwrap()
                .1
                .is_none()
        );
        println!("TRACKED_ORIGINAL_ROLLBACK_QUALIFIED");
    }
    #[test]
    fn tracked_publication_cleans_only_owned_inode_and_preserves_reused_or_replaced_original() {
        let base = tempfile::tempdir().unwrap();
        let root = base
            .path()
            .canonicalize()
            .unwrap()
            .join("tracked-workspace");
        let output = std::process::Command::new("/bin/sh")
            .args(["-c", "umask 077; exec \"$@\"", "tracked-test"])
            .arg(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "namespace::linux::documents::tests::tracked_child",
                "--nocapture",
            ])
            .env("RM_TRACKED_ROOT", &root)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("TRACKED_ORIGINAL_ROLLBACK_QUALIFIED")
        );
    }
}
