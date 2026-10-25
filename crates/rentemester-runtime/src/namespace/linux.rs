#[cfg(target_os = "macos")]
use super::macos::{acl_path, acl_walk_fd, process as process_profile};
use super::{NamespaceError as Error, NamespaceProfile};
use crate::{LeaseError, RootIdentity, WorkspaceLayout, WorkspaceLease};
use rustix::fs::{AtFlags, FileType, Mode, OFlags, Stat, fstat, open, openat, statat};
use std::{
    fs::File,
    io::Write,
    path::{Component, Path, PathBuf},
};

const DIRECTORY: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC);
#[cfg(target_os = "linux")]
const ACLS: [&str; 2] = ["system.posix_acl_access", "system.posix_acl_default"];

#[cfg(target_os = "linux")]
fn process_profile(uid: u32, profile: NamespaceProfile) -> Result<(), Error> {
    if profile != NamespaceProfile::DedicatedLinux {
        return Err(Error::Unsupported);
    }
    process(uid)
}
#[cfg(target_os = "linux")]
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
#[cfg(target_os = "linux")]
fn absent_acl(result: rustix::io::Result<usize>) -> Result<(), Error> {
    match result {
        Err(rustix::io::Errno::NODATA) => Ok(()),
        _ => Err(Error::Invalid),
    }
}
#[cfg(target_os = "linux")]
fn acl_fd(file: &File) -> Result<(), Error> {
    for name in ACLS {
        absent_acl(rustix::fs::fgetxattr(file, name, &mut [0u8; 0]))?;
    }
    Ok(())
}
#[cfg(target_os = "linux")]
fn acl_walk_fd(file: &File, _: bool) -> Result<(), Error> {
    acl_fd(file)
}
#[cfg(target_os = "linux")]
fn acl_path(path: &Path) -> Result<(), Error> {
    for name in ACLS {
        absent_acl(rustix::fs::lgetxattr(path, name, &mut [0u8; 0]))?;
    }
    Ok(())
}
fn identity(s: &Stat) -> RootIdentity {
    RootIdentity {
        #[cfg(target_os = "macos")]
        device: s.st_dev as u64,
        #[cfg(target_os = "linux")]
        device: s.st_dev,
        inode: s.st_ino,
    }
}
fn walk(root: &Path, uid: u32, private_root: bool) -> Result<(Vec<RootIdentity>, File), Error> {
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
    if private_root && parts.is_empty() {
        return Err(Error::Invalid);
    }
    for i in 0..=parts.len() {
        let stat = fstat(&file).map_err(|_| Error::Invalid)?;
        if stat.st_uid != 0 && stat.st_uid != uid || stat.st_mode & 0o7022 != 0 {
            return Err(Error::Invalid);
        }
        if private_root
            && i == parts.len()
            && (stat.st_uid != uid || stat.st_mode & 0o7777 != 0o700)
        {
            return Err(Error::Invalid);
        }
        acl_walk_fd(&file, !(private_root && i == parts.len()))?;
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
/// Requires all same-UID code trusted under the explicitly selected process profile. Owns no SQLite handles.
/// Bind before opening SQLite; the host must close every handle before dropping this guard.
pub struct NamespaceGuard {
    root: PathBuf,
    uid: u32,
    profile: NamespaceProfile,
    slug: String,
    ancestors: Vec<RootIdentity>,
    invalidated: bool,
    layout: WorkspaceLayout,
}
impl NamespaceGuard {
    pub fn acquire(root: &Path, uid: u32, workspace: &str, slug: &str) -> Result<Self, Error> {
        Self::acquire_with_profile(root, uid, workspace, slug, NamespaceProfile::DedicatedLinux)
    }
    pub fn acquire_with_profile(
        root: &Path,
        uid: u32,
        workspace: &str,
        slug: &str,
        profile: NamespaceProfile,
    ) -> Result<Self, Error> {
        process_profile(uid, profile)?;
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
        let (ancestors, directory) = walk(&root, uid, true)?;
        let lease = WorkspaceLease::acquire(&root).map_err(|e| {
            if e == LeaseError::Busy {
                Error::Busy
            } else {
                Error::Invalid
            }
        })?;
        Self::bind(
            root, uid, workspace, slug, ancestors, directory, lease, profile,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn bind(
        root: PathBuf,
        uid: u32,
        workspace: &str,
        slug: &str,
        ancestors: Vec<RootIdentity>,
        directory: File,
        mut lease: WorkspaceLease,
        profile: NamespaceProfile,
    ) -> Result<Self, Error> {
        if lease.ensure_current_root().map_err(|_| Error::Changed)?
            != identity(&fstat(&directory).map_err(|_| Error::Invalid)?)
        {
            return Err(Error::Changed);
        }
        metadata(&root, &directory, uid, slug)?;
        let layout = WorkspaceLayout::bind(lease, workspace, slug).map_err(|_| Error::Invalid)?;
        let mut result = Self {
            root,
            uid,
            profile,
            slug: slug.to_owned(),
            ancestors,
            invalidated: false,
            layout,
        };
        result.ensure_current()?;
        Ok(result)
    }
    /// Create a NEW protected namespace. Existing paths, including empty dirs and
    /// symlinks, are refused. No chmod/chown/adoption/removal or retry repair.
    /// On error a private partial directory may remain; it is never reused here.
    /// Every regular creation descriptor closes before the first SQLite open.
    pub fn create_fresh(
        root: &Path,
        uid: u32,
        workspace: &str,
        slug: &str,
        name: &str,
        created_at: &str,
    ) -> Result<Self, Error> {
        Self::create_fresh_with_profile(
            root,
            uid,
            workspace,
            slug,
            name,
            created_at,
            NamespaceProfile::DedicatedLinux,
        )
    }
    pub fn create_fresh_with_profile(
        root: &Path,
        uid: u32,
        workspace: &str,
        slug: &str,
        name: &str,
        created_at: &str,
        profile: NamespaceProfile,
    ) -> Result<Self, Error> {
        process_profile(uid, profile)?;
        let manifest = serde_json::to_vec(&serde_json::json!({"version":1,"companies":[
            {"slug":slug,"name":name,"createdAt":created_at,"archived":false}
        ]}))
        .map_err(|_| Error::Invalid)?;
        WorkspaceLayout::validate_new_manifest(workspace, slug, &manifest)
            .map_err(|_| Error::Invalid)?;
        if !root.is_absolute()
            || root
                .components()
                .any(|p| matches!(p, Component::ParentDir | Component::CurDir))
        {
            return Err(Error::Invalid);
        }
        let parent = root.parent().ok_or(Error::Invalid)?;
        let leaf = root.file_name().ok_or(Error::Invalid)?;
        let (prior_ancestors, parent_directory) = walk(parent, uid, false)?;
        rustix::fs::mkdirat(&parent_directory, leaf, Mode::from_raw_mode(0o700)).map_err(
            |error| {
                if error == rustix::io::Errno::EXIST {
                    Error::Exists
                } else {
                    Error::Invalid
                }
            },
        )?;
        let root: PathBuf = root.components().collect();
        let (ancestors, directory) = walk(&root, uid, true)?;
        if ancestors[..ancestors.len() - 1] != prior_ancestors {
            return Err(Error::Changed);
        }
        let lease = WorkspaceLease::acquire(&root).map_err(|_| Error::Invalid)?;
        for name in [".rentemester".to_owned(), slug.to_owned()]
            .into_iter()
            .chain(
                super::COMPANY_LAYOUT_DIRS
                    .iter()
                    .map(|d| format!("{slug}/{d}")),
            )
        {
            rustix::fs::mkdirat(&directory, name.as_str(), Mode::from_raw_mode(0o700))
                .map_err(|_| Error::Invalid)?;
        }
        for (name, bytes) in [
            ("workspace.json".to_owned(), manifest.as_slice()),
            (".rentemester/workspace-control.sqlite".to_owned(), &[][..]),
            (format!("{slug}/data/ledger.sqlite"), &[][..]),
        ] {
            let mut file = File::from(
                openat(
                    &directory,
                    name.as_str(),
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
            file.sync_all().map_err(|_| Error::Invalid)?;
        }
        for name in super::COMPANY_LAYOUT_DIRS
            .iter()
            .rev()
            .map(|d| format!("{slug}/{d}"))
            .chain([slug.to_owned(), ".rentemester".to_owned()])
        {
            let dir = File::from(
                openat(&directory, name.as_str(), DIRECTORY, Mode::empty())
                    .map_err(|_| Error::Invalid)?,
            );
            dir.sync_all().map_err(|_| Error::Invalid)?;
        }
        directory.sync_all().map_err(|_| Error::Invalid)?;
        parent_directory.sync_all().map_err(|_| Error::Invalid)?;
        Self::bind(
            root, uid, workspace, slug, ancestors, directory, lease, profile,
        )
    }
    fn observe(&mut self) -> Result<(), Error> {
        process_profile(self.uid, self.profile)?;
        let (ancestors, directory) = walk(&self.root, self.uid, true)?;
        if ancestors != self.ancestors {
            return Err(Error::Changed);
        }
        metadata(&self.root, &directory, self.uid, &self.slug)?;
        self.layout.ensure_current().map_err(|_| Error::Changed)
    }
    /// Retain the producer's signed artifact independently of any HTTP download.
    /// This fixed private directory is not an operator-selected offsite destination.
    pub fn retain_backup(
        &mut self,
        wire: &[u8],
        trusted: &crate::backup_artifact::TrustedPublicKey,
        workspace: &str,
    ) -> Result<serde_json::Value, Error> {
        self.ensure_current()?;
        let (_, root) = walk(&self.root, self.uid, true)?;
        match rustix::fs::mkdirat(&root, "native-backups", Mode::from_raw_mode(0o700)) {
            Ok(()) => root.sync_all().map_err(|_| Error::Invalid)?,
            Err(rustix::io::Errno::EXIST) => {}
            Err(_) => return Err(Error::Invalid),
        }
        let config = crate::backup_destination::Config {
            id: "native-local-backups".into(),
            label: "Local retained snapshots".into(),
            kind: "local-folder".into(),
            directory: self.root.join("native-backups"),
            non_related_party: false,
            in_eea_or_eu: false,
            country: "ZZ".into(),
            meets_recognised_standards: false,
            attested_by: "system:local-retention".into(),
            attested_at: "1970-01-01T00:00:00Z".into(),
        };
        let operation = crate::backup_keys::digest(wire);
        let receipt = match crate::backup_destination::place(
            &config, self.uid, &operation, wire, trusted, workspace,
        ) {
            Ok(value) => value,
            Err("backup_destination_exists_read_back") => {
                let value = crate::backup_destination::verify(
                    &config, self.uid, &operation, trusted, workspace,
                )
                .map_err(|_| Error::Invalid)?;
                if value["archiveSha256"] != operation {
                    return Err(Error::Invalid);
                }
                value
            }
            Err(_) => return Err(Error::Invalid),
        };
        self.ensure_current()?;
        Ok(
            serde_json::json!({"format":"rentemester-native-retained-backup/1",
            "archiveSha256":receipt["archiveSha256"],"archiveSizeBytes":receipt["archiveSizeBytes"],
            "backupCreatedAt":receipt["backupCreatedAt"],"signatureVerified":true,"remoteSyncVerified":false}),
        )
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

mod documents;
mod imap_config;
mod original717;

#[cfg(all(test, target_os = "linux"))]
mod tests;

mod companies;

mod portable_restore;

mod legacy_adoption;

pub(super) fn backup_directory(path: &Path, uid: u32) -> Result<File, Error> {
    walk(path, uid, true).map(|(_, directory)| directory)
}
pub(super) fn backup_file_acl(file: &File) -> Result<(), Error> {
    acl_walk_fd(file, false)
}
pub use original717::Original717RestoreStage;
