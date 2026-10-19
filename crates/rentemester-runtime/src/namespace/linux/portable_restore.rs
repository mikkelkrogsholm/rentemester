//! Fresh private stage and no-replace publication. No source DB path opens or adoption.
use super::*;
use crate::backup_artifact::{EntryKind, VerifiedArtifact};
use std::collections::BTreeSet;
impl NamespaceGuard {
    pub fn workspace_manifest(&mut self) -> Result<Vec<u8>, Error> {
        self.ensure_current()?;
        self.layout.manifest_bytes().map_err(|_| Error::Changed)
    }
    /// Caller has validated ALL signed company/database/business bytes before construction.
    /// Every regular creation descriptor closes before SQLite is opened by the host.
    pub fn create_verified_restore_stage(
        root: &Path,
        uid: u32,
        workspace: &str,
        slug: &str,
        profile: NamespaceProfile,
        artifact: &VerifiedArtifact,
    ) -> Result<Self, Error> {
        process_profile(uid, profile)?;
        if !root.is_absolute()
            || root
                .components()
                .any(|p| matches!(p, Component::ParentDir | Component::CurDir))
        {
            return Err(Error::Invalid);
        }
        let manifest = artifact
            .entries()
            .iter()
            .find(|e| matches!(e.kind(), EntryKind::WorkspaceManifest))
            .ok_or(Error::Invalid)?
            .bytes();
        WorkspaceLayout::validate_new_manifest(workspace, slug, manifest)
            .map_err(|_| Error::Invalid)?;
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
        for c in &artifact.manifest().context.companies {
            for suffix in [
                "",
                "/data",
                "/documents",
                "/documents/originals",
                "/bank",
                "/bank/processed",
            ] {
                directories.insert(format!("{}{suffix}", c.slug));
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
        for entry in artifact.entries() {
            if !matches!(entry.kind(), EntryKind::ControlBusiness { .. }) {
                write(entry.path(), entry.bytes())?;
            }
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
    /// Host has confirmed closure of ALL SQLite handles before publication. Same
    /// parent only; exact root lease remains continuously held across no-replace rename.
    pub fn publish_restored_root(&mut self, target: &Path) -> Result<(), Error> {
        self.ensure_current()?;
        if target.parent() != self.root.parent() || target == self.root || !target.is_absolute() {
            return Err(Error::Invalid);
        }
        let parent = target.parent().ok_or(Error::Invalid)?;
        let (_, directory) = walk(parent, self.uid, false)?;
        rustix::fs::renameat_with(
            &directory,
            self.root.file_name().ok_or(Error::Invalid)?,
            &directory,
            target.file_name().ok_or(Error::Invalid)?,
            rustix::fs::RenameFlags::NOREPLACE,
        )
        .map_err(|e| {
            if e == rustix::io::Errno::EXIST {
                Error::Exists
            } else {
                Error::Invalid
            }
        })?;
        directory.sync_all().map_err(|_| Error::Changed)?;
        self.layout
            .relocate_restore_root(target)
            .map_err(|_| Error::Changed)?;
        self.root = target.to_owned();
        self.ensure_current()
    }
}
