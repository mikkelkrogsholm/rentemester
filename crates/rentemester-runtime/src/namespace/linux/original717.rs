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
