//! Pinned namespace observations only; never a SQLite-handle or writer capability.
use super::WorkspaceLease;
use crate::{LayoutError, RootIdentity};
use rustix::fs::{AtFlags, FileType, Mode, OFlags, openat, statat};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::os::fd::AsFd;
use std::{
    fs::File,
    io::Read,
    os::unix::fs::{FileExt, MetadataExt},
};
const LIMIT: u64 = 1_048_576;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    companies: Vec<Entry>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    slug: String,
    name: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    archived: bool,
}
fn valid_slug(s: &str) -> bool {
    !s.is_empty()
        && (s.as_bytes()[0].is_ascii_lowercase() || s.as_bytes()[0].is_ascii_digit())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
// Keep slug syntax aligned with existing product/access rules, without a new normalization.
fn metadata_text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}
fn parse(bytes: &[u8], slug: &str) -> Result<(), LayoutError> {
    let manifest: Manifest =
        serde_json::from_slice(bytes).map_err(|_| LayoutError::InvalidManifest)?;
    if manifest.version != 1 {
        return Err(LayoutError::InvalidManifest);
    }
    let mut seen = std::collections::HashSet::new();
    let mut selected = false;
    for entry in manifest.companies {
        if !valid_slug(&entry.slug)
            || !metadata_text(&entry.name, 1024)
            || !metadata_text(&entry.created_at, 128)
            || !seen.insert(entry.slug.clone())
        {
            return Err(LayoutError::InvalidManifest);
        }
        if entry.slug == slug {
            if entry.archived {
                return Err(LayoutError::InvalidSelection);
            }
            selected = true;
        }
    }
    if selected {
        Ok(())
    } else {
        Err(LayoutError::InvalidSelection)
    }
}
fn open_relative(
    lease: &WorkspaceLease,
    parts: &[String],
    directory: bool,
) -> Result<File, LayoutError> {
    let mut parent = None;
    for (i, part) in parts.iter().enumerate() {
        let is_dir = i + 1 < parts.len() || directory;
        let flags = OFlags::RDONLY
            | OFlags::CLOEXEC
            | OFlags::NOFOLLOW
            | OFlags::NONBLOCK
            | if is_dir {
                OFlags::DIRECTORY
            } else {
                OFlags::empty()
            };
        let fd = match &parent {
            None => openat(&lease.directory, part.as_str(), flags, Mode::empty()),
            Some(fd) => openat(fd, part.as_str(), flags, Mode::empty()),
        }
        .map_err(|_| LayoutError::Unavailable)?;
        parent = Some(fd);
    }
    let file = File::from(parent.ok_or(LayoutError::Unavailable)?);
    physical(&file, directory)?;
    Ok(file)
}
fn physical(file: &File, directory: bool) -> Result<RootIdentity, LayoutError> {
    let m = file.metadata().map_err(|_| LayoutError::Unavailable)?;
    if (directory && !m.is_dir()) || (!directory && (!m.is_file() || m.nlink() != 1)) {
        return Err(LayoutError::Unavailable);
    }
    Ok(RootIdentity {
        device: m.dev(),
        inode: m.ino(),
    })
}
fn bytes(file: &mut File) -> Result<Vec<u8>, LayoutError> {
    if file.metadata().map_err(|_| LayoutError::Unavailable)?.len() > LIMIT {
        return Err(LayoutError::InvalidManifest);
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| LayoutError::Unavailable)?;
    if bytes.len() as u64 > LIMIT {
        return Err(LayoutError::InvalidManifest);
    }
    Ok(bytes)
}
// Read the retained manifest without opening or cloning any namespace file.
// A substituted manifest may alias a live SQLite inode; even a refused open
// followed by close could release that process's database locks.
fn retained_bytes(file: &File) -> Result<Vec<u8>, LayoutError> {
    if file.metadata().map_err(|_| LayoutError::Unavailable)?.len() > LIMIT {
        return Err(LayoutError::InvalidManifest);
    }
    let mut bytes = vec![0; LIMIT as usize + 1];
    let mut offset = 0;
    while offset < bytes.len() {
        let read = file
            .read_at(&mut bytes[offset..], offset as u64)
            .map_err(|_| LayoutError::Unavailable)?;
        if read == 0 {
            break;
        }
        offset += read;
    }
    if offset as u64 > LIMIT {
        return Err(LayoutError::InvalidManifest);
    }
    bytes.truncate(offset);
    Ok(bytes)
}
struct Binding {
    parts: Vec<String>,
    directory: bool,
    file: File,
    identity: RootIdentity,
}
/// Owns a lease and observed physical layout, not database connections or persistent logical IDs.
pub struct WorkspaceLayout {
    workspace: String,
    slug: String,
    bindings: Vec<Binding>,
    manifest_hash: Vec<u8>,
    invalidated: bool,
    // Rust drops fields in declaration order: release namespace ownership last.
    lease: WorkspaceLease,
}
impl WorkspaceLayout {
    #[cfg(target_os = "linux")]
    pub(crate) fn validate_new_manifest(
        workspace: &str,
        slug: &str,
        bytes: &[u8],
    ) -> Result<(), LayoutError> {
        if !metadata_text(workspace, 1024) || !valid_slug(slug) {
            return Err(LayoutError::InvalidSelection);
        }
        parse(bytes, slug)
    }

    pub fn bind(
        mut lease: WorkspaceLease,
        workspace: &str,
        slug: &str,
    ) -> Result<Self, LayoutError> {
        if !metadata_text(workspace, 1024) || !valid_slug(slug) {
            return Err(LayoutError::InvalidSelection);
        }
        let root_identity = lease.ensure_current_root().map_err(LayoutError::Lease)?;
        let mut identities =
            std::collections::HashSet::from([(root_identity.device, root_identity.inode)]);
        let specs = [
            (vec!["workspace.json"], false),
            (vec![".rentemester"], true),
            (vec![".rentemester", "workspace-control.sqlite"], false),
            (vec![slug], true),
            (vec![slug, "data"], true),
            (vec![slug, "data", "ledger.sqlite"], false),
        ];
        let mut bindings = Vec::new();
        let mut manifest_hash = Vec::new();
        for (parts, directory) in specs {
            let parts: Vec<String> = parts.into_iter().map(str::to_owned).collect();
            let mut file = open_relative(&lease, &parts, directory)?;
            let identity = physical(&file, directory)?;
            if identity.device != root_identity.device
                || !identities.insert((identity.device, identity.inode))
            {
                return Err(LayoutError::Unavailable);
            }
            if parts == ["workspace.json"] {
                let content = bytes(&mut file)?;
                parse(&content, slug)?;
                manifest_hash = Sha256::digest(content).to_vec();
            }
            bindings.push(Binding {
                parts,
                directory,
                file,
                identity,
            });
        }
        let mut result = Self {
            lease,
            workspace: workspace.to_owned(),
            slug: slug.to_owned(),
            bindings,
            manifest_hash,
            invalidated: false,
        };
        result.ensure_current()?;
        Ok(result)
    }
    /// Compare actual SQLite main handles with the retained control/ledger files.
    /// Sticky refusal; this observation alone does not exclude namespace changes
    /// between calls or qualify a deployment for persistent writers.
    pub fn ensure_connections(
        &mut self,
        control: &rusqlite::Connection,
        company: &rusqlite::Connection,
    ) -> Result<(), LayoutError> {
        self.ensure_current()?;
        let matches = |connection: &rusqlite::Connection, index: usize| {
            connection.main_file_identity().is_ok_and(|actual| {
                actual.device == self.bindings[index].identity.device
                    && actual.inode == self.bindings[index].identity.inode
                    && actual.links == 1
            })
        };
        // bind() fixes these slots; they never depend on request input.
        if !matches(control, 2) || !matches(company, 5) {
            self.invalidated = true;
            return Err(LayoutError::Changed);
        }
        self.ensure_current()
    }
    pub fn workspace(&self) -> &str {
        &self.workspace
    }
    pub fn slug(&self) -> &str {
        &self.slug
    }
    /// Sticky observation. Namespace races between checks require a separate writer fence.
    #[allow(clippy::unnecessary_cast)] // Unix stat field widths differ across supported targets.
    pub fn ensure_current(&mut self) -> Result<(), LayoutError> {
        if self.invalidated {
            return Err(LayoutError::Changed);
        }
        let check = (|| {
            self.lease
                .ensure_current_root()
                .map_err(LayoutError::Lease)?;
            for (index, binding) in self.bindings.iter().enumerate() {
                if index == 0 || index == 2 || index == 5 {
                    // Closing any ordinary descriptor for an SQLite inode can
                    // release this process's POSIX SQLite locks. Revalidation
                    // must only stat these names, never open/close them.
                    let parent = if index == 0 {
                        self.lease.directory.as_fd()
                    } else {
                        self.bindings[index - 1].file.as_fd()
                    };
                    let name = binding.parts.last().ok_or(LayoutError::Changed)?;
                    let current = statat(parent, name.as_str(), AtFlags::SYMLINK_NOFOLLOW)
                        .map_err(|_| LayoutError::Changed)?;
                    if physical(&binding.file, false)? != binding.identity
                        || current.st_dev as u64 != binding.identity.device
                        || current.st_ino as u64 != binding.identity.inode
                        || current.st_nlink != 1
                        || FileType::from_raw_mode(current.st_mode) != FileType::RegularFile
                    {
                        return Err(LayoutError::Changed);
                    }
                    if index == 0
                        && Sha256::digest(retained_bytes(&binding.file)?).as_slice()
                            != self.manifest_hash
                    {
                        return Err(LayoutError::Changed);
                    }
                    continue;
                }
                let current = open_relative(&self.lease, &binding.parts, binding.directory)?;
                if physical(&binding.file, binding.directory)? != binding.identity
                    || physical(&current, binding.directory)? != binding.identity
                {
                    return Err(LayoutError::Changed);
                }
            }
            self.lease
                .ensure_current_root()
                .map_err(LayoutError::Lease)?;
            Ok(())
        })();
        if check.is_err() {
            self.invalidated = true;
            return Err(LayoutError::Changed);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        path::Path,
        process::{Command, Stdio},
    };
    fn fixture(root: &Path) {
        std::fs::create_dir(root.join(".rentemester")).unwrap();
        std::fs::create_dir_all(root.join("a/data")).unwrap();
        std::fs::write(root.join("workspace.json"),r#"{"version":1,"companies":[{"slug":"a","name":"Synthetic","createdAt":"2026-01-01","archived":false}]}"#).unwrap();
        for name in [
            ".rentemester/workspace-control.sqlite",
            "a/data/ledger.sqlite",
        ] {
            std::fs::write(root.join(name), b"synthetic-not-sqlite").unwrap();
        }
    }
    fn bind(root: &Path) -> Result<WorkspaceLayout, LayoutError> {
        WorkspaceLayout::bind(WorkspaceLease::acquire(root).unwrap(), "w", "a")
    }
    #[test]
    fn canonical_manifest_profile_refuses_coercions_and_ambiguity() {
        let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
        let entry = r#"{"slug":"a","name":"Synthetic","createdAt":"2026-01-01","archived":false}"#;
        let good = format!(r#"{{"version":1,"companies":[{entry}]}}"#);
        assert!(parse(good.as_bytes(), "a").is_ok());
        let bad = [
            "{}".to_owned(),
            "null".to_owned(),
            good.replace("\"version\":1", "\"version\":2"),
            good.replace("\"version\":1", "\"version\":1,\"version\":1"),
            good.replace(
                "\"archived\":false",
                "\"archived\":false,\"archived\":false",
            ),
            good.replace("\"archived\":false", "\"archived\":\"false\""),
            good.replace(",\"archived\":false", ""),
            good.replace("\"slug\":\"a\"", "\"slug\":null"),
            good.replace("\"name\":\"Synthetic\"", "\"name\":\" \""),
            good.replace("\"createdAt\":\"2026-01-01\"", "\"createdAt\":123"),
            good.replace("\"slug\":\"a\"", "\"slug\":\"../a\""),
            good.replace("\"slug\":\"a\"", "\"slug\":\"A\""),
            good.replace("\"name\":\"Synthetic\"", "\"name\":\"x\\ny\""),
            format!(r#"{{"version":1,"companies":[{entry},{entry}]}}"#),
            good.replace("\"version\":1", "\"extra\":true,\"version\":1"),
            good.replace("\"slug\":\"a\"", "\"extra\":true,\"slug\":\"a\""),
        ];
        for bad in bad {
            assert_eq!(
                parse(bad.as_bytes(), "a"),
                Err(LayoutError::InvalidManifest)
            );
        }
        assert_eq!(
            parse(good.as_bytes(), "b"),
            Err(LayoutError::InvalidSelection)
        );
        assert_eq!(
            parse(good.replace("false", "true").as_bytes(), "a"),
            Err(LayoutError::InvalidSelection)
        );
    }
    #[test]
    fn binding_preserves_sources_and_holds_ownership_until_drop() {
        let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        fixture(root.path());
        let manifest = std::fs::read(root.path().join("workspace.json")).unwrap();
        let mut held = bind(root.path()).unwrap();
        assert_eq!(held.workspace(), "w");
        assert_eq!(held.slug(), "a");
        assert_eq!(
            WorkspaceLease::acquire(root.path()).unwrap_err(),
            crate::LeaseError::Busy
        );
        assert_eq!(
            std::fs::read(root.path().join("workspace.json")).unwrap(),
            manifest
        );
        // DB contents and lengths are not namespace identity.
        std::fs::write(
            root.path().join("a/data/ledger.sqlite"),
            b"changed legitimate transaction bytes",
        )
        .unwrap();
        std::fs::write(
            root.path().join(".rentemester/workspace-control.sqlite"),
            b"changed control bytes",
        )
        .unwrap();
        held.ensure_current().unwrap();
        drop(held);
        assert!(WorkspaceLease::acquire(root.path()).is_ok());
    }
    #[test]
    fn every_path_level_refuses_symlinks_and_missing_objects() {
        let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
        for name in [
            "workspace.json",
            ".rentemester",
            ".rentemester/workspace-control.sqlite",
            "a",
            "a/data",
            "a/data/ledger.sqlite",
        ] {
            for linked in [false, true] {
                let root = tempfile::tempdir().unwrap();
                fixture(root.path());
                let path = root.path().join(name);
                let moved = root.path().join("saved");
                std::fs::rename(&path, &moved).unwrap();
                if linked {
                    std::os::unix::fs::symlink(&moved, &path).unwrap();
                }
                assert!(bind(root.path()).is_err(), "{name} linked={linked}");
                assert!(
                    WorkspaceLease::acquire(root.path()).is_ok(),
                    "failed bind must release lease"
                );
            }
        }
    }
    #[test]
    fn files_refuse_hardlinks_directories_and_fifos_without_blocking() {
        let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
        for name in [
            "workspace.json",
            ".rentemester/workspace-control.sqlite",
            "a/data/ledger.sqlite",
        ] {
            for kind in ["hardlink", "directory", "fifo"] {
                let root = tempfile::tempdir().unwrap();
                fixture(root.path());
                let path = root.path().join(name);
                match kind {
                    "hardlink" => std::fs::hard_link(&path, root.path().join("alias")).unwrap(),
                    "directory" => {
                        std::fs::remove_file(&path).unwrap();
                        std::fs::create_dir(&path).unwrap();
                    }
                    _ => {
                        std::fs::remove_file(&path).unwrap();
                        assert!(
                            Command::new("mkfifo")
                                .arg(&path)
                                .status()
                                .unwrap()
                                .success()
                        );
                    }
                }
                assert!(bind(root.path()).is_err(), "{name}:{kind}");
            }
        }
    }
    #[test]
    fn every_component_change_is_sticky_even_after_restoration() {
        let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
        for name in [
            "workspace.json",
            ".rentemester",
            ".rentemester/workspace-control.sqlite",
            "a",
            "a/data",
            "a/data/ledger.sqlite",
        ] {
            let root = tempfile::tempdir().unwrap();
            fixture(root.path());
            let mut held = bind(root.path()).unwrap();
            let path = root.path().join(name);
            let moved = root.path().join("saved");
            std::fs::rename(&path, &moved).unwrap();
            assert_eq!(held.ensure_current(), Err(LayoutError::Changed), "{name}");
            std::fs::rename(&moved, &path).unwrap();
            assert_eq!(held.ensure_current(), Err(LayoutError::Changed));
            assert_eq!(
                WorkspaceLease::acquire(root.path()).unwrap_err(),
                crate::LeaseError::Busy
            );
        }
    }
    #[test]
    fn manifest_bytes_hardlink_and_root_changes_invalidate() {
        let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
        for change in ["bytes", "hardlink", "root"] {
            let parent = tempfile::tempdir().unwrap();
            let path = parent.path().join("workspace");
            std::fs::create_dir(&path).unwrap();
            fixture(&path);
            let mut held = bind(&path).unwrap();
            match change {
                "bytes" => {
                    let p = path.join("workspace.json");
                    let original = std::fs::read(&p).unwrap();
                    let mut modified = original.clone();
                    modified.push(b' ');
                    std::fs::write(&p, &modified).unwrap();
                    assert_eq!(held.ensure_current(), Err(LayoutError::Changed));
                    std::fs::write(p, original).unwrap();
                }
                "hardlink" => {
                    let alias = path.join("alias");
                    std::fs::hard_link(path.join("a/data/ledger.sqlite"), &alias).unwrap();
                    assert_eq!(held.ensure_current(), Err(LayoutError::Changed));
                    std::fs::remove_file(alias).unwrap();
                }
                _ => {
                    let moved = parent.path().join("moved");
                    std::fs::rename(&path, &moved).unwrap();
                    assert_eq!(held.ensure_current(), Err(LayoutError::Changed));
                    std::fs::rename(moved, &path).unwrap();
                }
            }
            assert_eq!(held.ensure_current(), Err(LayoutError::Changed));
        }
    }
    #[test]
    fn bounded_manifest_and_selection_have_no_defaults() {
        let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        fixture(root.path());
        for workspace in ["", " ", "bad\nkey"] {
            assert!(
                WorkspaceLayout::bind(
                    WorkspaceLease::acquire(root.path()).unwrap(),
                    workspace,
                    "a"
                )
                .is_err()
            );
        }
        for slug in ["", "../a", "a/b", "A", ".a"] {
            assert!(
                WorkspaceLayout::bind(WorkspaceLease::acquire(root.path()).unwrap(), "w", slug)
                    .is_err()
            );
        }
        std::fs::write(
            root.path().join("workspace.json"),
            vec![b' '; LIMIT as usize + 1],
        )
        .unwrap();
        assert!(matches!(
            bind(root.path()),
            Err(LayoutError::InvalidManifest)
        ));
        std::fs::write(root.path().join("workspace.json"), [0xff]).unwrap();
        assert!(matches!(
            bind(root.path()),
            Err(LayoutError::InvalidManifest)
        ));
    }
    #[test]
    #[ignore = "synthetic layout owner process selected by parent"]
    fn child() {
        let path = std::env::var("RM_LAYOUT_ROOT").unwrap();
        let mut held = bind(Path::new(&path)).unwrap();
        println!("layout-held");
        std::io::stdout().flush().unwrap();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).unwrap();
        held.ensure_current().unwrap();
    }
    #[test]
    fn real_process_layout_owns_lease_and_kernel_death_releases() {
        let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        fixture(root.path());
        struct Reaped(std::process::Child);
        impl Drop for Reaped {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "local::layout::tests::child",
                "--nocapture",
            ])
            .env("RM_LAYOUT_ROOT", root.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let output = child.stdout.take().unwrap();
        let mut child = Reaped(child);
        let (sender, receiver) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut output = BufReader::new(output);
            let mut line = String::new();
            while output.read_line(&mut line).unwrap() > 0 {
                if line.trim() == "layout-held" {
                    sender.send(()).unwrap();
                    return;
                }
                line.clear();
            }
        });
        receiver
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        reader.join().unwrap();
        assert_eq!(
            WorkspaceLease::acquire(root.path()).unwrap_err(),
            crate::LeaseError::Busy
        );
        child.0.kill().unwrap();
        let status = child.0.wait().unwrap();
        assert!(!status.success());
        assert!(bind(root.path()).is_ok());
    }
}

#[cfg(test)]
#[test]
#[ignore = "synthetic native probe selected by independent manifest harness"]
fn probe() {
    let request: serde_json::Value =
        serde_json::from_str(&std::env::var("RM_LAYOUT_REQUEST").unwrap()).unwrap();
    let result = WorkspaceLease::acquire(std::path::Path::new(request["root"].as_str().unwrap()))
        .map_err(LayoutError::Lease)
        .and_then(|lease| {
            WorkspaceLayout::bind(
                lease,
                request["workspace"].as_str().unwrap(),
                request["slug"].as_str().unwrap(),
            )
        });
    let output = match result {
        Ok(mut layout) => {
            layout.ensure_current().unwrap();
            serde_json::json!({"ok":true,"workspace":layout.workspace(),"slug":layout.slug(),"writerAuthority":false})
        }
        Err(error) => serde_json::json!({"ok":false,"code":error.code(),"writerAuthority":false}),
    };
    println!("RM_LAYOUT_RESULT={output}");
}

#[cfg(test)]
mod connection_tests;
