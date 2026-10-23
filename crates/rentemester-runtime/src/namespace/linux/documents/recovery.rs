//! Only the held workspace lease may reconcile staging. Canonical names are never
//! removed or implicitly registered. Interrupted bytes remain private evidence.
use super::super::{acl_walk_fd, identity};
use super::*;

fn stage(name: &str) -> bool {
    name.strip_prefix(".upload-").is_some_and(|token| {
        token.len() == 43
            && token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    })
}
impl NamespaceGuard {
    fn original_directory_current(&self, directory: &File, bank: bool) -> Result<(), Error> {
        let (_, root) = walk(&self.root, self.uid, true)?;
        let mut current = File::from(
            openat(&root, self.slug.as_str(), DIRECTORY, Mode::empty())
                .map_err(|_| Error::Changed)?,
        );
        for name in if bank {
            ["bank", "processed"]
        } else {
            ["documents", "originals"]
        } {
            current = File::from(
                openat(&current, name, DIRECTORY, Mode::empty()).map_err(|_| Error::Changed)?,
            );
            let s = fstat(&current).map_err(|_| Error::Invalid)?;
            if s.st_uid != self.uid || s.st_mode & 0o7777 != 0o700 {
                return Err(Error::Invalid);
            }
            acl_walk_fd(&current, false)?;
        }
        if identity(&fstat(&current).map_err(|_| Error::Invalid)?)
            != identity(&fstat(directory).map_err(|_| Error::Invalid)?)
        {
            return Err(Error::Changed);
        }
        Ok(())
    }

    pub(super) fn reconcile_staging(&mut self, directory: &File, bank: bool) -> Result<(), Error> {
        // NamespaceGuard owns the exclusive WorkspaceLease. All publication uses
        // &mut self, so no active writer can overlap this bounded reconciliation.
        self.ensure_current()?;
        self.original_directory_current(directory, bank)?;
        acl_walk_fd(directory, false)?;
        let parent = fstat(directory).map_err(|_| Error::Invalid)?;
        let mut pending = Vec::new();
        let mut count = 0;
        for item in rustix::fs::Dir::read_from(directory).map_err(|_| Error::Invalid)? {
            let item = item.map_err(|_| Error::Invalid)?;
            let name = item.file_name().to_str().map_err(|_| Error::Invalid)?;
            if name == "." || name == ".." {
                continue;
            }
            count += 1;
            if count > crate::backup_artifact::MAX_ENTRIES {
                return Err(Error::Invalid);
            }
            if name.starts_with(".upload-") {
                if !stage(name) {
                    return Err(Error::Invalid);
                }
                pending.push(name.to_owned());
            }
        }
        // Validate an existing recovery directory even when no upload remains.
        if pending.is_empty()
            && matches!(
                statat(directory, ".recovery", AtFlags::SYMLINK_NOFOLLOW),
                Err(rustix::io::Errno::NOENT)
            )
        {
            return Ok(());
        }
        match mkdirat(directory, ".recovery", Mode::from_raw_mode(0o700)) {
            Ok(()) => directory.sync_all().map_err(|_| Error::Invalid)?,
            Err(rustix::io::Errno::EXIST) => {}
            Err(_) => return Err(Error::Invalid),
        }
        let recovery = File::from(
            openat(directory, ".recovery", DIRECTORY, Mode::empty()).map_err(|_| Error::Invalid)?,
        );
        let st = fstat(&recovery).map_err(|_| Error::Invalid)?;
        if st.st_uid != self.uid || st.st_mode & 0o7777 != 0o700 || st.st_dev != parent.st_dev {
            return Err(Error::Invalid);
        }
        acl_walk_fd(&recovery, false)?;
        let mut total = 0u64;
        let validate = |dir: &File, name: &str| -> Result<rustix::fs::Stat, Error> {
            let s = statat(dir, name, AtFlags::SYMLINK_NOFOLLOW).map_err(|_| Error::Invalid)?;
            if FileType::from_raw_mode(s.st_mode) != FileType::RegularFile
                || s.st_uid != self.uid
                || s.st_mode & 0o7777 != 0o600
                || s.st_nlink != 1
                || s.st_dev != parent.st_dev
                || s.st_size < 0
                || s.st_size as u64 > crate::document::MAX_BYTES as u64
            {
                return Err(Error::Invalid);
            }
            // Validate descriptor ACL and identity, never open a hardlink alias.
            let file = File::from(
                openat(
                    dir,
                    name,
                    OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
                    Mode::empty(),
                )
                .map_err(|_| Error::Invalid)?,
            );
            if identity(&fstat(&file).map_err(|_| Error::Invalid)?) != identity(&s) {
                return Err(Error::Changed);
            }
            acl_walk_fd(&file, false)?;
            file.sync_all().map_err(|_| Error::Invalid)?;
            Ok(s)
        };
        for item in rustix::fs::Dir::read_from(&recovery).map_err(|_| Error::Invalid)? {
            let item = item.map_err(|_| Error::Invalid)?;
            let name = item.file_name().to_str().map_err(|_| Error::Invalid)?;
            if name == "." || name == ".." {
                continue;
            }
            count += 1;
            if count > crate::backup_artifact::MAX_ENTRIES || !stage(name) {
                return Err(Error::Invalid);
            }
            total = total
                .checked_add(validate(&recovery, name)?.st_size as u64)
                .ok_or(Error::Invalid)?;
        }
        for name in &pending {
            total = total
                .checked_add(validate(directory, name)?.st_size as u64)
                .ok_or(Error::Invalid)?;
        }
        if total > crate::backup_artifact::MAX_TOTAL_BYTES as u64 {
            return Err(Error::Invalid);
        }
        for name in pending {
            self.ensure_current()?;
            self.original_directory_current(directory, bank)?;
            if identity(
                &statat(directory, ".recovery", AtFlags::SYMLINK_NOFOLLOW)
                    .map_err(|_| Error::Changed)?,
            ) != identity(&st)
            {
                return Err(Error::Changed);
            }
            renameat_with(
                directory,
                name.as_str(),
                &recovery,
                name.as_str(),
                RenameFlags::NOREPLACE,
            )
            .map_err(|_| Error::Invalid)?;
            recovery.sync_all().map_err(|_| Error::Invalid)?;
            directory.sync_all().map_err(|_| Error::Invalid)?;
        }
        self.ensure_current()
    }
}
