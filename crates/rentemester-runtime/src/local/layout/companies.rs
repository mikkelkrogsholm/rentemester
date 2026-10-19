//! Owner-only transitions. Caller closes the selected SQLite company before switching pins.
use super::*;
use std::io::Write;
impl WorkspaceLayout {
    pub fn catalog(&mut self) -> Result<Vec<(String, String, bool)>, LayoutError> {
        self.ensure_current()?;
        let bytes = retained_bytes(&self.bindings[0].file)?;
        parse(&bytes, &self.slug)?;
        let m: Manifest =
            serde_json::from_slice(&bytes).map_err(|_| LayoutError::InvalidManifest)?;
        if m.companies.len() > 1000 {
            return Err(LayoutError::InvalidManifest);
        }
        Ok(m.companies
            .into_iter()
            .map(|c| (c.slug, c.name, c.archived))
            .collect())
    }
    pub(crate) fn select_company(&mut self, slug: &str) -> Result<(), LayoutError> {
        self.ensure_current()?;
        parse(&retained_bytes(&self.bindings[0].file)?, slug)?;
        if slug == self.slug {
            return Ok(());
        }
        let mut bindings: Vec<Binding> = Vec::new();
        for (parts, directory) in [
            (vec![slug.to_owned()], true),
            (vec![slug.to_owned(), "data".into()], true),
            (
                vec![slug.to_owned(), "data".into(), "ledger.sqlite".into()],
                false,
            ),
        ] {
            // Refuse aliases of any live pinned regular file BEFORE opening a
            // candidate descriptor: closing an alias can drop SQLite POSIX locks.
            if !directory {
                let parent = &bindings.last().ok_or(LayoutError::Unavailable)?.file;
                let st = statat(parent, "ledger.sqlite", AtFlags::SYMLINK_NOFOLLOW)
                    .map_err(|_| LayoutError::Unavailable)?;
                if st.st_nlink != 1
                    || FileType::from_raw_mode(st.st_mode) != FileType::RegularFile
                    || self.bindings.iter().any(|b| {
                        b.identity.device == st.st_dev as u64
                            && b.identity.inode == st.st_ino as u64
                    })
                {
                    return Err(LayoutError::Unavailable);
                }
            }
            let file = open_relative(&self.lease, &parts, directory)?;
            let identity = physical(&file, directory)?;
            if identity.device != self.bindings[0].identity.device {
                return Err(LayoutError::Unavailable);
            }
            bindings.push(Binding {
                parts,
                directory,
                file,
                identity,
            });
        }
        self.bindings.truncate(3);
        self.bindings.extend(bindings);
        self.slug = slug.to_owned();
        self.ensure_current()
    }
    pub(super) fn ensure_pending_company(&self) -> Result<(), LayoutError> {
        if let Some(pins) = &self.pending_company {
            for (index, binding) in pins.iter().enumerate() {
                if binding.directory {
                    let current = open_relative(&self.lease, &binding.parts, true)?;
                    if physical(&current, true)? != binding.identity
                        || physical(&binding.file, true)? != binding.identity
                    {
                        return Err(LayoutError::Changed);
                    }
                } else {
                    let stat = statat(
                        &pins[index - 1].file,
                        "ledger.sqlite",
                        AtFlags::SYMLINK_NOFOLLOW,
                    )
                    .map_err(|_| LayoutError::Changed)?;
                    if stat.st_dev as u64 != binding.identity.device
                        || stat.st_ino as u64 != binding.identity.inode
                        || stat.st_nlink != 1
                        || FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile
                        || physical(&binding.file, false)? != binding.identity
                    {
                        return Err(LayoutError::Changed);
                    }
                }
            }
        }
        Ok(())
    }
    pub(crate) fn verify_pending_company(
        &mut self,
        db: &rusqlite::Connection,
    ) -> Result<(), LayoutError> {
        self.ensure_current()?;
        let expected = self
            .pending_company
            .as_ref()
            .and_then(|p| p.last())
            .ok_or(LayoutError::Unavailable)?
            .identity;
        let actual = db.main_file_identity().map_err(|_| LayoutError::Changed)?;
        if actual.device != expected.device || actual.inode != expected.inode || actual.links != 1 {
            return Err(LayoutError::Changed);
        }
        Ok(())
    }
    /// Only creates a previously absent private directory; never adopts partial state.
    pub(crate) fn create_company_files(&mut self, slug: &str) -> Result<(), LayoutError> {
        self.ensure_current()?;
        if self.pending_company.is_some()
            || !valid_slug(slug)
            || self.catalog()?.iter().any(|c| c.0 == slug)
        {
            return Err(LayoutError::InvalidSelection);
        }
        rustix::fs::mkdirat(&self.lease.directory, slug, Mode::from_raw_mode(0o700))
            .map_err(|_| LayoutError::Unavailable)?;
        let company = open_relative(&self.lease, &[slug.to_owned()], true)?;
        rustix::fs::mkdirat(&company, "data", Mode::from_raw_mode(0o700))
            .map_err(|_| LayoutError::Unavailable)?;
        let data = open_relative(&self.lease, &[slug.to_owned(), "data".into()], true)?;
        let file = File::from(
            openat(
                &data,
                "ledger.sqlite",
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o600),
            )
            .map_err(|_| LayoutError::Unavailable)?,
        );
        for f in [&file, &data, &company] {
            f.sync_all().map_err(|_| LayoutError::Unavailable)?;
        }
        rustix::fs::fsync(&self.lease.directory).map_err(|_| LayoutError::Unavailable)?;
        let created_identity = physical(&file, false)?;
        drop(file);
        let mut pins: Vec<Binding> = Vec::new();
        for (parts, directory) in [
            (vec![slug.to_owned()], true),
            (vec![slug.to_owned(), "data".into()], true),
            (
                vec![slug.to_owned(), "data".into(), "ledger.sqlite".into()],
                false,
            ),
        ] {
            if !directory {
                let parent = &pins.last().ok_or(LayoutError::Unavailable)?.file;
                let st = statat(parent, "ledger.sqlite", AtFlags::SYMLINK_NOFOLLOW)
                    .map_err(|_| LayoutError::Changed)?;
                if st.st_dev as u64 != created_identity.device
                    || st.st_ino as u64 != created_identity.inode
                    || st.st_nlink != 1
                    || FileType::from_raw_mode(st.st_mode) != FileType::RegularFile
                {
                    return Err(LayoutError::Changed);
                }
            }
            let file = open_relative(&self.lease, &parts, directory)?;
            let identity = physical(&file, directory)?;
            pins.push(Binding {
                parts,
                directory,
                file,
                identity,
            });
        }
        self.pending_company = Some(pins);
        self.ensure_current()
    }
    /// Publish after company initialization and creator grant commit. No general
    /// manifest refresh: the exact previous pinned manifest must still be current.
    pub(crate) fn publish_company(
        &mut self,
        slug: &str,
        name: &str,
        created_at: &str,
    ) -> Result<(), LayoutError> {
        self.ensure_current()?;
        if self
            .pending_company
            .as_ref()
            .is_none_or(|p| p[0].parts != [slug])
        {
            return Err(LayoutError::InvalidSelection);
        }
        let mut m: Manifest = serde_json::from_slice(&retained_bytes(&self.bindings[0].file)?)
            .map_err(|_| LayoutError::InvalidManifest)?;
        if m.companies.len() >= 1000 || m.companies.iter().any(|c| c.slug == slug) {
            return Err(LayoutError::InvalidManifest);
        }
        m.companies.push(Entry {
            slug: slug.into(),
            name: name.into(),
            created_at: created_at.into(),
            archived: false,
        });
        let content = serde_json::to_vec(&m).map_err(|_| LayoutError::InvalidManifest)?;
        if content.len() as u64 > LIMIT {
            return Err(LayoutError::InvalidManifest);
        }
        parse(&content, slug)?;
        let token = crate::auth::new_token().map_err(|_| LayoutError::Unavailable)?;
        let temp = format!(".workspace-{}.tmp", token.as_str());
        let mut file = File::from(
            openat(
                &self.lease.directory,
                temp.as_str(),
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_raw_mode(0o600),
            )
            .map_err(|_| LayoutError::Unavailable)?,
        );
        file.write_all(&content)
            .map_err(|_| LayoutError::Unavailable)?;
        file.sync_all().map_err(|_| LayoutError::Unavailable)?;
        drop(file);
        self.ensure_current()?;
        rustix::fs::renameat(
            &self.lease.directory,
            temp.as_str(),
            &self.lease.directory,
            "workspace.json",
        )
        .map_err(|_| LayoutError::Unavailable)?;
        rustix::fs::fsync(&self.lease.directory).map_err(|_| LayoutError::Unavailable)?;
        let file = open_relative(&self.lease, &["workspace.json".into()], false)?;
        let identity = physical(&file, false)?;
        self.bindings[0] = Binding {
            parts: vec!["workspace.json".into()],
            directory: false,
            file,
            identity,
        };
        self.manifest_hash = Sha256::digest(&content).to_vec();
        self.pending_company = None;
        self.ensure_current()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    fn fixture() -> (tempfile::TempDir, WorkspaceLayout, Connection, Connection) {
        let root = tempfile::tempdir().unwrap();
        let (c, l) = super::super::connection_tests::fixture(root.path());
        let layout =
            WorkspaceLayout::bind(WorkspaceLease::acquire(root.path()).unwrap(), "w", "a").unwrap();
        (root, layout, c, l)
    }
    #[test]
    fn new_company_publication_and_switch_keep_same_control_and_root_lease() {
        let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
        let (root, mut layout, control, old) = fixture();
        let identity = control.main_file_identity().unwrap();
        control
            .execute_batch("BEGIN IMMEDIATE;INSERT INTO synthetic VALUES(7)")
            .unwrap();
        layout.create_company_files("b").unwrap();
        assert_eq!(layout.catalog().unwrap().len(), 1);
        let new = Connection::open(root.path().join("b/data/ledger.sqlite")).unwrap();
        new.execute_batch("CREATE TABLE isolated(value);INSERT INTO isolated VALUES(42)")
            .unwrap();
        drop(new);
        layout
            .publish_company("b", "Synthetic second", "2026-01-01T00:00:00.000Z")
            .unwrap();
        layout.ensure_connections(&control, &old).unwrap();
        assert_eq!(layout.catalog().unwrap().len(), 2);
        assert!(WorkspaceLease::acquire(root.path()).is_err());
        drop(old);
        layout.select_company("b").unwrap();
        let new = Connection::open(root.path().join("b/data/ledger.sqlite")).unwrap();
        layout.ensure_connections(&control, &new).unwrap();
        assert_eq!(control.main_file_identity().unwrap(), identity);
        assert!(!control.is_autocommit());
        assert_eq!(
            new.query_row("SELECT value FROM isolated", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            42
        );
        control.execute_batch("ROLLBACK").unwrap();
        drop(new);
        drop(control);
        drop(layout);
        let mut reopened =
            WorkspaceLayout::bind(WorkspaceLease::acquire(root.path()).unwrap(), "w", "a").unwrap();
        assert_eq!(reopened.catalog().unwrap().len(), 2);
    }
    #[test]
    fn partial_private_directory_is_not_registered_or_reused_and_unknown_selection_refuses() {
        let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
        let (_root, mut layout, c, l) = fixture();
        assert!(layout.select_company("../a").is_err());
        assert!(layout.select_company("missing").is_err());
        layout.create_company_files("partial").unwrap();
        assert!(layout.create_company_files("partial").is_err());
        assert_eq!(layout.catalog().unwrap().len(), 1);
        layout.ensure_connections(&c, &l).unwrap();
    }
}
