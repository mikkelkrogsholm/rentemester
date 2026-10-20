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
const NAME: &str = "imap.json";

impl NamespaceGuard {
    fn imap_directory(&mut self, create: bool) -> Result<Option<File>, Error> {
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
    pub fn read_imap_config(&mut self) -> Result<Option<Vec<u8>>, Error> {
        let Some(directory) = self.imap_directory(false)? else {
            return Ok(None);
        };
        let value = read(&directory, self.uid)?;
        self.ensure_current()?;
        Ok(value)
    }
    pub fn write_imap_config(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if bytes.is_empty() || bytes.len() > MAX {
            return Err(Error::Invalid);
        }
        let directory = self.imap_directory(true)?.ok_or(Error::Invalid)?;
        write(&directory, self.uid, bytes)?;
        self.ensure_current()
    }
    pub fn remove_imap_config(&mut self) -> Result<(), Error> {
        let Some(directory) = self.imap_directory(false)? else {
            return Ok(());
        };
        remove(&directory, self.uid)?;
        self.ensure_current()
    }
}
// Refuse aliases before opening: closing an alias of SQLite could release POSIX locks.
fn checked_stat(directory: &File, uid: u32) -> Result<Option<rustix::fs::Stat>, Error> {
    let st = match statat(directory, NAME, AtFlags::SYMLINK_NOFOLLOW) {
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
fn read(directory: &File, uid: u32) -> Result<Option<Vec<u8>>, Error> {
    let Some(before) = checked_stat(directory, uid)? else {
        return Ok(None);
    };
    let mut file = File::from(
        openat(
            directory,
            NAME,
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
        || checked_stat(directory, uid)?.as_ref().map(identity) != Some(identity(&before))
    {
        return Err(Error::Changed);
    }
    Ok(Some(bytes))
}
fn write(directory: &File, uid: u32, bytes: &[u8]) -> Result<(), Error> {
    if bytes.is_empty() || bytes.len() > MAX {
        return Err(Error::Invalid);
    }
    let previous = checked_stat(directory, uid)?;
    if previous.is_some() {
        read(directory, uid)?;
    }
    let token = crate::auth::new_token().map_err(|_| Error::Invalid)?;
    let name = format!(".imap-{}", token.as_str());
    let result = (|| {
        let mut file = File::from(
            openat(
                directory,
                name.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o600),
            )
            .map_err(|_| Error::Invalid)?,
        );
        file.write_all(bytes).map_err(|_| Error::Invalid)?;
        file.sync_all().map_err(|_| Error::Invalid)?;
        acl_walk_fd(&file, false)?;
        if checked_stat(directory, uid)?.as_ref().map(identity) != previous.as_ref().map(identity) {
            return Err(Error::Changed);
        }
        renameat(directory, name.as_str(), directory, NAME).map_err(|_| Error::Invalid)?;
        directory.sync_all().map_err(|_| Error::Invalid)?;
        Ok(())
    })();
    let _ = unlinkat(directory, name.as_str(), AtFlags::empty());
    result
}
fn remove(directory: &File, uid: u32) -> Result<(), Error> {
    if checked_stat(directory, uid)?.is_some() {
        read(directory, uid)?;
        unlinkat(directory, NAME, AtFlags::empty()).map_err(|_| Error::Invalid)?;
        directory.sync_all().map_err(|_| Error::Invalid)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    #[test]
    fn private_atomic_replace_remove_and_refused_aliases() {
        let temp = tempfile::tempdir().unwrap();
        let directory = File::open(temp.path()).unwrap();
        let uid = std::fs::metadata(temp.path()).unwrap().uid();
        assert!(read(&directory, uid).unwrap().is_none());
        write(&directory, uid, b"first").unwrap();
        let path = temp.path().join(NAME);
        let first = std::fs::metadata(&path).unwrap();
        assert_eq!(first.permissions().mode() & 0o7777, 0o600);
        write(&directory, uid, b"second").unwrap();
        assert_ne!(first.ino(), std::fs::metadata(&path).unwrap().ino());
        assert_eq!(read(&directory, uid).unwrap().unwrap(), b"second");
        assert!(write(&directory, uid, &vec![0; MAX + 1]).is_err());
        std::fs::hard_link(&path, temp.path().join("alias")).unwrap();
        assert!(read(&directory, uid).is_err());
        assert!(write(&directory, uid, b"bad").is_err());
        assert!(remove(&directory, uid).is_err());
        std::fs::remove_file(temp.path().join("alias")).unwrap();
        remove(&directory, uid).unwrap();
        remove(&directory, uid).unwrap();
        std::os::unix::fs::symlink("other", &path).unwrap();
        assert!(write(&directory, uid, b"bad").is_err());
        assert!(read(&directory, uid).is_err());
        assert!(remove(&directory, uid).is_err());
        assert!(!temp.path().join("other").exists());
    }
    #[test]
    fn unsafe_modes_are_not_repaired_silently() {
        let temp = tempfile::tempdir().unwrap();
        let directory = File::open(temp.path()).unwrap();
        let uid = std::fs::metadata(temp.path()).unwrap().uid();
        let path = temp.path().join(NAME);
        std::fs::write(&path, b"secret").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read(&directory, uid).is_err());
        assert!(write(&directory, uid, b"other").is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"secret");
    }
}
