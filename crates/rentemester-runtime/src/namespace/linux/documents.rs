//! Content-addressed originals under the protected broker root. No database inode opens.
use super::{DIRECTORY, Error, NamespaceGuard, walk};
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
        let after = fstat(&file).map_err(|_| Error::Invalid)?;
        if before.st_size != after.st_size
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
    pub fn publish_bank_statement(&mut self, content: &str) -> Result<String, Error> {
        let input =
            crate::document::Prepared::new("bank-import.txt".into(), content.as_bytes().to_vec())
                .map_err(|_| Error::Invalid)?;
        self.publish_original(&input, true)
    }
    fn publish_original(
        &mut self,
        input: &crate::document::Prepared,
        bank: bool,
    ) -> Result<String, Error> {
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
                return Ok(path);
            }
            Err(rustix::io::Errno::NOENT) => {}
            Err(_) => return Err(Error::Invalid),
        }
        let nonce = crate::auth::new_token().map_err(|_| Error::Invalid)?;
        let temporary = format!(".upload-{}", nonce.as_str());
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
            file.write_all(input.bytes()).map_err(|_| Error::Invalid)?;
            file.sync_all().map_err(|_| Error::Invalid)?;
            drop(file);
            match renameat_with(
                &directory,
                temporary.as_str(),
                &directory,
                name,
                RenameFlags::NOREPLACE,
            ) {
                Ok(()) => {}
                Err(rustix::io::Errno::EXIST) => {
                    self.read_original(&directory, name, input.hash())?;
                }
                Err(_) => return Err(Error::Invalid),
            }
            directory.sync_all().map_err(|_| Error::Invalid)?;
            Ok(path.clone())
        })();
        let _ = unlinkat(&directory, temporary.as_str(), AtFlags::empty());
        result
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
        if base != hash || !["pdf", "png", "jpg", "jpeg", "txt", "json", "xml"].contains(&extension)
        {
            return Err(Error::Invalid);
        }
        let directory = self.original_directory(false, false)?;
        self.read_original(&directory, name, hash)
    }
}
