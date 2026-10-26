//! Owner-only transitions. Caller closes the selected SQLite company before switching pins.
use super::*;
use std::io::Write;
impl WorkspaceLayout {
    pub fn manifest_bytes(&mut self) -> Result<Vec<u8>, LayoutError> {
        self.ensure_current()?;
        retained_bytes(&self.bindings[0].file)
    }
    pub(crate) fn relocate_restore_root(
        &mut self,
        target: &std::path::Path,
    ) -> Result<(), LayoutError> {
        self.lease
            .relocate_restore_root(target)
            .map_err(LayoutError::Lease)?;
        self.ensure_current()
    }

    pub fn catalog(&mut self) -> Result<Vec<(String, String, bool, Purpose)>, LayoutError> {
        self.ensure_current()?;
        let bytes = retained_bytes(&self.bindings[0].file)?;
        parse_selection(&bytes, &self.slug, self.snapshot_selection)?;
        let m = workspace_manifest::parse(&bytes).map_err(|_| LayoutError::InvalidManifest)?;
        if m.companies.len() > 1000 {
            return Err(LayoutError::InvalidManifest);
        }
        Ok(m.companies
            .into_iter()
            .map(|c| { let purpose=c.purpose(); (c.slug, c.name, c.archived, purpose) })
            .collect())
    }
    pub(crate) fn select_company(&mut self, slug: &str) -> Result<(), LayoutError> {
        self.select_company_mode(slug, false)
    }
    pub(crate) fn select_company_for_snapshot(&mut self, slug: &str) -> Result<(), LayoutError> {
        self.select_company_mode(slug, true)
    }
    fn select_company_mode(&mut self, slug: &str, snapshot: bool) -> Result<(), LayoutError> {
        self.ensure_current()?;
        let was_snapshot = self.snapshot_selection;
        let result = (|| {
            parse_selection(&retained_bytes(&self.bindings[0].file)?, slug, snapshot)?;
            if slug == self.slug {
                self.snapshot_selection = snapshot;
                return self.ensure_current();
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
            self.snapshot_selection = snapshot;
            self.ensure_current()
        })();
        // A failed return from maintenance must never leave a usable Owner
        // selected on an archived company.
        if result.is_err() && was_snapshot && !snapshot {
            self.invalidated = true;
        }
        result
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
        for relative in crate::namespace::COMPANY_LAYOUT_DIRS {
            rustix::fs::mkdirat(&company, *relative, Mode::from_raw_mode(0o700))
                .map_err(|_| LayoutError::Unavailable)?;
        }
        for relative in crate::namespace::COMPANY_LAYOUT_DIRS.iter().rev() {
            let parts = std::iter::once(slug.to_owned())
                .chain(relative.split('/').map(str::to_owned))
                .collect::<Vec<_>>();
            open_relative(&self.lease, &parts, true)?
                .sync_all()
                .map_err(|_| LayoutError::Unavailable)?;
        }
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
        let mut m = workspace_manifest::parse(&retained_bytes(&self.bindings[0].file)?)
            .map_err(|_| LayoutError::InvalidManifest)?;
        if m.companies.len() >= 1000 || m.companies.iter().any(|c| c.slug == slug) {
            return Err(LayoutError::InvalidManifest);
        }
        m.version = 2;
        m.companies.push(Entry {
            slug: slug.into(),
            name: name.into(),
            created_at: created_at.into(),
            archived: false,
            purpose: Some(Purpose::Live),
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
    fn archived_fixture() -> (tempfile::TempDir, WorkspaceLayout, Connection, Connection) {
        let root = tempfile::tempdir().unwrap();
        let (control, ledger) = super::super::connection_tests::fixture(root.path());
        std::fs::create_dir_all(root.path().join("b/data")).unwrap();
        Connection::open(root.path().join("b/data/ledger.sqlite"))
            .unwrap()
            .execute_batch("CREATE TABLE synthetic(value)")
            .unwrap();
        std::fs::write(root.path().join("workspace.json"), br#"{"version":1,"companies":[{"slug":"a","name":"Synthetic live","createdAt":"2026-01-01","archived":false},{"slug":"b","name":"Synthetic archive","createdAt":"2026-01-01","archived":true}]}"#).unwrap();
        let layout =
            WorkspaceLayout::bind(WorkspaceLease::acquire(root.path()).unwrap(), "w", "a").unwrap();
        (root, layout, control, ledger)
    }
    #[test]
    fn snapshot_archived_selection_keeps_pins_and_restores_strict_normal_selection() {
        let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
        let (root, mut layout, control, old) = archived_fixture();
        assert!(layout.select_company("b").is_err());
        drop(old);
        layout.select_company_for_snapshot("b").unwrap();
        let archived = Connection::open(root.path().join("b/data/ledger.sqlite")).unwrap();
        layout.ensure_connections(&control, &archived).unwrap();
        assert!(layout.catalog().unwrap().iter().any(|c| c.0 == "b" && c.2));
        drop(archived);
        layout.select_company("a").unwrap();
        assert!(!layout.snapshot_selection);
        layout.select_company_for_snapshot("a").unwrap();
        layout.select_company("a").unwrap();
        assert!(
            !layout.snapshot_selection,
            "same-slug return must clear snapshot mode"
        );
        assert!(layout.select_company("b").is_err());
        let active = Connection::open(root.path().join("a/data/ledger.sqlite")).unwrap();
        layout.ensure_connections(&control, &active).unwrap();
    }
    #[test]
    fn snapshot_selection_still_refuses_replaced_manifest_or_ledger_and_failed_return_retires() {
        let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
        for change in ["manifest", "ledger", "return"] {
            let (root, mut layout, control, old) = archived_fixture();
            drop(old);
            layout.select_company_for_snapshot("b").unwrap();
            if change == "return" {
                assert!(layout.select_company("b").is_err());
            } else {
                let path = root.path().join(if change == "manifest" {
                    "workspace.json"
                } else {
                    "b/data/ledger.sqlite"
                });
                std::fs::rename(&path, path.with_extension("old")).unwrap();
                std::fs::write(path, b"replaced").unwrap();
            }
            assert!(layout.ensure_current().is_err());
            assert!(layout.catalog().is_err());
            drop(control);
        }
    }
    #[test]
    fn v2_nonlive_inventory_is_retained_but_never_ordinary_selection_or_promotion() {
        let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
        for purpose in ["test", "dry-run", "restore", "backup", "retest", "baseline"] {
            let (root, layout, control, old) = archived_fixture();
            drop(layout);
            let path = root.path().join("workspace.json");
            let mut manifest: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            manifest["version"] = serde_json::json!(2);
            manifest["companies"][1]["archived"] = serde_json::json!(false);
            manifest["companies"][1]["purpose"] = serde_json::json!(purpose);
            let bytes = serde_json::to_vec(&manifest).unwrap();
            std::fs::write(&path, &bytes).unwrap();
            let lease = WorkspaceLease::acquire(root.path()).unwrap();
            let mut layout = WorkspaceLayout::bind(lease, "w", "a").unwrap();
            assert_eq!(
                serde_json::to_value(layout.catalog().unwrap()[1].3).unwrap(),
                purpose
            );
            assert!(layout.select_company("b").is_err());
            assert_eq!(
                std::fs::read(&path).unwrap(),
                bytes,
                "reads never normalize metadata"
            );
            drop(old);
            layout.select_company_for_snapshot("b").unwrap();
            let db = Connection::open(root.path().join("b/data/ledger.sqlite")).unwrap();
            layout.ensure_connections(&control, &db).unwrap();
            drop(db);
            layout.select_company("a").unwrap();
            layout.create_company_files("c").unwrap();
            layout
                .publish_company("c", "Synthetic new", "2026-01-01")
                .unwrap();
            let retained = workspace_manifest::parse(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(retained.version, 2);
            assert_eq!(
                serde_json::to_value(retained.companies[1].purpose()).unwrap(),
                purpose
            );
            assert!(retained.companies[2].active());
            assert!(layout.select_company("b").is_err());
        }
    }
    #[test]
    fn held_purpose_change_remains_sticky_refusal() {
        let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
        let (root, layout, _control, _old) = archived_fixture();
        drop(layout);
        let path = root.path().join("workspace.json");
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        manifest["version"] = serde_json::json!(2);
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let mut layout =
            WorkspaceLayout::bind(WorkspaceLease::acquire(root.path()).unwrap(), "w", "a").unwrap();
        manifest["companies"][0]["purpose"] = serde_json::json!("test");
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(layout.ensure_current().is_err());
        manifest["companies"][0]
            .as_object_mut()
            .unwrap()
            .remove("purpose");
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(
            layout.ensure_current().is_err(),
            "restoring old bytes cannot revive a retired holder"
        );
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
