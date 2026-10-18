use super::NamespaceError as Error;
use crate::{LeaseError, RootIdentity, WorkspaceLayout, WorkspaceLease};
use rustix::fs::{
    AtFlags, FileType, Mode, OFlags, Stat, fgetxattr, fstat, lgetxattr, open, openat, statat,
};
use std::{
    fs::File,
    path::{Component, Path, PathBuf},
};

const DIRECTORY: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);
const ACLS: [&str; 2] = ["system.posix_acl_access", "system.posix_acl_default"];

fn process(uid: u32) -> Result<(), Error> {
    if uid == 0 {
        return Err(Error::Process);
    }
    // Kernel-owned executing-thread metadata, never a protected workspace inode.
    let status = std::fs::read_to_string("/proc/thread-self/status").map_err(|_| Error::Process)?;
    let field = |name: &str| -> Result<&str, Error> {
        let mut lines = status.lines().filter_map(|line| line.strip_prefix(name));
        let value = lines.next().ok_or(Error::Process)?.trim();
        if lines.next().is_some() {
            return Err(Error::Process);
        }
        Ok(value)
    };
    let ids = field("Uid:")?
        .split_whitespace()
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| Error::Process)?;
    if ids != [uid; 4] || field("NoNewPrivs:")? != "1" || field("Umask:")? != "0077" {
        return Err(Error::Process);
    }
    for key in ["CapInh:", "CapPrm:", "CapEff:", "CapBnd:", "CapAmb:"] {
        if u64::from_str_radix(field(key)?, 16).map_err(|_| Error::Process)? != 0 {
            return Err(Error::Process);
        }
    }
    Ok(())
}
fn absent_acl(result: rustix::io::Result<usize>) -> Result<(), Error> {
    match result {
        Err(rustix::io::Errno::NODATA) => Ok(()),
        _ => Err(Error::Invalid),
    }
}
fn acl_fd(file: &File) -> Result<(), Error> {
    for name in ACLS {
        absent_acl(fgetxattr(file, name, &mut [0u8; 0]))?;
    }
    Ok(())
}
fn acl_path(path: &Path) -> Result<(), Error> {
    for name in ACLS {
        absent_acl(lgetxattr(path, name, &mut [0u8; 0]))?;
    }
    Ok(())
}
fn identity(s: &Stat) -> RootIdentity {
    RootIdentity {
        device: s.st_dev,
        inode: s.st_ino,
    }
}
fn walk(root: &Path, uid: u32) -> Result<(Vec<RootIdentity>, File), Error> {
    if !root.is_absolute()
        || root
            .components()
            .any(|p| matches!(p, Component::ParentDir | Component::CurDir))
    {
        return Err(Error::Invalid);
    }
    let mut file = File::from(open("/", DIRECTORY, Mode::empty()).map_err(|_| Error::Invalid)?);
    let mut ids = Vec::new();
    let parts: Vec<_> = root
        .components()
        .filter_map(|p| {
            if let Component::Normal(s) = p {
                Some(s)
            } else {
                None
            }
        })
        .collect();
    if parts.is_empty() {
        return Err(Error::Invalid);
    }
    for i in 0..=parts.len() {
        let stat = fstat(&file).map_err(|_| Error::Invalid)?;
        if stat.st_uid != 0 && stat.st_uid != uid || stat.st_mode & 0o7022 != 0 {
            return Err(Error::Invalid);
        }
        if i == parts.len() && (stat.st_uid != uid || stat.st_mode & 0o7777 != 0o700) {
            return Err(Error::Invalid);
        }
        acl_fd(&file)?;
        ids.push(identity(&stat));
        if let Some(part) = parts.get(i) {
            file = File::from(
                openat(&file, *part, DIRECTORY, Mode::empty()).map_err(|_| Error::Invalid)?,
            );
        }
    }
    Ok((ids, file))
}
fn metadata(root: &Path, directory: &File, uid: u32, slug: &str) -> Result<(), Error> {
    let root_stat = fstat(directory).map_err(|_| Error::Invalid)?;
    let check = |name: &str, dir: bool, optional: bool| -> Result<(), Error> {
        let s = match statat(directory, name, AtFlags::SYMLINK_NOFOLLOW) {
            Err(rustix::io::Errno::NOENT) if optional => return Ok(()),
            Ok(s) => s,
            _ => return Err(Error::Invalid),
        };
        if s.st_uid != uid
            || s.st_dev != root_stat.st_dev
            || s.st_mode & 0o7777 != if dir { 0o700 } else { 0o600 }
            || FileType::from_raw_mode(s.st_mode)
                != if dir {
                    FileType::Directory
                } else {
                    FileType::RegularFile
                }
            || (!dir && s.st_nlink != 1)
        {
            return Err(Error::Invalid);
        }
        acl_path(&root.join(name))
    };
    for dir in [
        ".rentemester".to_owned(),
        slug.to_owned(),
        format!("{slug}/data"),
    ] {
        check(&dir, true, false)?;
    }
    for name in [
        "workspace.json".to_owned(),
        ".rentemester/workspace-control.sqlite".to_owned(),
        format!("{slug}/data/ledger.sqlite"),
    ] {
        check(&name, false, false)?;
        if name != "workspace.json" {
            for suffix in ["-wal", "-shm", "-journal"] {
                check(&format!("{name}{suffix}"), false, true)?;
            }
        }
    }
    Ok(())
}
/// Requires all same-UID namespace code to be enrolled/trusted. Owns no SQLite handles.
/// Bind before opening SQLite; the host must close every handle before dropping this guard.
pub struct NamespaceGuard {
    root: PathBuf,
    uid: u32,
    slug: String,
    ancestors: Vec<RootIdentity>,
    invalidated: bool,
    layout: WorkspaceLayout,
}
impl NamespaceGuard {
    pub fn acquire(root: &Path, uid: u32, workspace: &str, slug: &str) -> Result<Self, Error> {
        process(uid)?;
        // Path safety only; canonical registration/manifest checks remain in bind.
        if slug.is_empty()
            || !slug.as_bytes()[0].is_ascii_lowercase() && !slug.as_bytes()[0].is_ascii_digit()
            || !slug
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err(Error::Invalid);
        }
        let root: PathBuf = root.components().collect();
        let (ancestors, directory) = walk(&root, uid)?;
        let lease = WorkspaceLease::acquire(&root).map_err(|e| {
            if e == LeaseError::Busy {
                Error::Busy
            } else {
                Error::Invalid
            }
        })?;
        metadata(&root, &directory, uid, slug)?;
        let layout = WorkspaceLayout::bind(lease, workspace, slug).map_err(|_| Error::Invalid)?;
        let mut result = Self {
            root,
            uid,
            slug: slug.to_owned(),
            ancestors,
            invalidated: false,
            layout,
        };
        result.ensure_current()?;
        Ok(result)
    }
    fn observe(&mut self) -> Result<(), Error> {
        process(self.uid)?;
        let (ancestors, directory) = walk(&self.root, self.uid)?;
        if ancestors != self.ancestors {
            return Err(Error::Changed);
        }
        metadata(&self.root, &directory, self.uid, &self.slug)?;
        self.layout.ensure_current().map_err(|_| Error::Changed)
    }
    pub fn ensure_current(&mut self) -> Result<(), Error> {
        if self.invalidated {
            return Err(Error::Changed);
        }
        if self.observe().is_err() {
            self.invalidated = true;
            return Err(Error::Changed);
        }
        Ok(())
    }
    pub fn ensure_connections(
        &mut self,
        control: &rusqlite::Connection,
        company: &rusqlite::Connection,
    ) -> Result<(), Error> {
        self.ensure_current()?;
        if self.layout.ensure_connections(control, company).is_err() {
            self.invalidated = true;
            return Err(Error::Changed);
        }
        self.ensure_current()
    }
}

#[cfg(test)]
mod tests;
