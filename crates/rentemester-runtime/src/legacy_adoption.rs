//! Bounded bytes for a legacy adoption stage. This is neither a signed native
//! backup nor proof of domain validity, source fencing, or writer authority.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const MAX_FILES: usize = 2048;
pub const MAX_FILE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_METADATA_BYTES: usize = 1024 * 1024;
pub const RECEIPT_PATH: &str = ".rentemester/legacy-adoption.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdoptionFileKind {
    CompanyLedger { company: String },
    DocumentOriginal { company: String },
    BankStatement { company: String },
}
/// No filesystem path is opened from this input. Paths are canonical workspace-relative names.
#[derive(Debug, Clone)]
pub struct AdoptionFile {
    pub kind: AdoptionFileKind,
    pub path: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Bounds,
    Manifest,
    Inventory,
    Digest,
    UnsafePath,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Bounds => "adoption_stage_bounds",
            Self::Manifest => "adoption_stage_manifest",
            Self::Inventory => "adoption_stage_inventory",
            Self::Digest => "adoption_stage_digest",
            Self::UnsafePath => "adoption_stage_unsafe_path",
        })
    }
}
impl std::error::Error for Error {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    companies: Vec<Company>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Company {
    slug: String,
    name: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    archived: bool,
}
fn text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}
fn slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && s.as_bytes()[0].is_ascii_alphanumeric()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
/// Immutable inventory checks only. The host must validate converted databases,
/// original references, provenance semantics and the source writer fence before staging.
#[derive(Debug)]
pub struct AdoptionImage {
    workspace: String,
    selected_slug: String,
    manifest: Vec<u8>,
    companies: Vec<String>,
    files: Vec<AdoptionFile>,
    receipt: Vec<u8>,
}
impl AdoptionImage {
    pub fn validate(
        workspace: &str,
        selected_slug: &str,
        manifest: Vec<u8>,
        files: Vec<AdoptionFile>,
        receipt: Vec<u8>,
        receipt_sha256: &str,
    ) -> Result<Self, Error> {
        if manifest.is_empty()
            || manifest.len() > MAX_METADATA_BYTES
            || receipt.is_empty()
            || receipt.len() > MAX_METADATA_BYTES
            || files.is_empty()
            || files.len() > MAX_FILES
        {
            return Err(Error::Bounds);
        }
        if !text(workspace, 1024) || !slug(selected_slug) {
            return Err(Error::Manifest);
        }
        if hash(&receipt) != receipt_sha256 {
            return Err(Error::Digest);
        }
        let parsed: Manifest = serde_json::from_slice(&manifest).map_err(|_| Error::Manifest)?;
        if parsed.version != 1 || parsed.companies.is_empty() || parsed.companies.len() > 64 {
            return Err(Error::Manifest);
        }
        let mut companies = BTreeSet::new();
        let mut selected = false;
        for c in parsed.companies {
            if !slug(&c.slug)
                || !text(&c.name, 1024)
                || !text(&c.created_at, 128)
                || !companies.insert(c.slug.clone())
            {
                return Err(Error::Manifest);
            }
            if c.slug == selected_slug {
                selected = !c.archived;
            }
        }
        if !selected {
            return Err(Error::Manifest);
        }
        let mut paths = BTreeSet::new();
        let mut ledgers = BTreeSet::new();
        let mut total = manifest
            .len()
            .checked_add(receipt.len())
            .ok_or(Error::Bounds)?;
        for f in &files {
            if f.bytes.is_empty() || f.bytes.len() > MAX_FILE_BYTES {
                return Err(Error::Bounds);
            }
            total = total.checked_add(f.bytes.len()).ok_or(Error::Bounds)?;
            if total > MAX_TOTAL_BYTES {
                return Err(Error::Bounds);
            }
            if hash(&f.bytes) != f.sha256 {
                return Err(Error::Digest);
            }
            if f.path.len() > 512 || !paths.insert(f.path.clone()) {
                return Err(Error::Inventory);
            }
            let (company, allowed) = match &f.kind {
                AdoptionFileKind::CompanyLedger { company } => (
                    company,
                    f.path == format!("{company}/data/ledger.sqlite")
                        && ledgers.insert(company.clone()),
                ),
                AdoptionFileKind::DocumentOriginal { company } => (
                    company,
                    ["pdf", "png", "jpg", "jpeg", "txt", "json", "xml"]
                        .iter()
                        .any(|ext| {
                            f.path == format!("{company}/documents/originals/{}.{ext}", f.sha256)
                        }),
                ),
                AdoptionFileKind::BankStatement { company } => (
                    company,
                    f.path == format!("{company}/bank/processed/{}.txt", f.sha256),
                ),
            };
            if !companies.contains(company) || !allowed {
                return Err(Error::UnsafePath);
            }
        }
        if ledgers != companies {
            return Err(Error::Inventory);
        }
        Ok(Self {
            workspace: workspace.into(),
            selected_slug: selected_slug.into(),
            manifest,
            companies: companies.into_iter().collect(),
            files,
            receipt,
        })
    }
    pub fn workspace(&self) -> &str {
        &self.workspace
    }
    pub fn selected_slug(&self) -> &str {
        &self.selected_slug
    }
    pub fn manifest_bytes(&self) -> &[u8] {
        &self.manifest
    }
    pub fn companies(&self) -> &[String] {
        &self.companies
    }
    pub fn files(&self) -> &[AdoptionFile] {
        &self.files
    }
    pub fn receipt_bytes(&self) -> &[u8] {
        &self.receipt
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> Vec<u8> {
        br#"{"version":1,"companies":[{"slug":"a","name":"Synthetic","createdAt":"2026-01-01","archived":false}]}"#.to_vec()
    }
    fn ledger() -> AdoptionFile {
        AdoptionFile {
            kind: AdoptionFileKind::CompanyLedger {
                company: "a".into(),
            },
            path: "a/data/ledger.sqlite".into(),
            bytes: b"not domain validation".to_vec(),
            sha256: hash(b"not domain validation"),
        }
    }
    fn validate(files: Vec<AdoptionFile>) -> Result<AdoptionImage, Error> {
        AdoptionImage::validate("w", "a", manifest(), files, b"{}".to_vec(), &hash(b"{}"))
    }
    #[test]
    fn exact_inventory_is_bytes_only_and_preserves_receipt() {
        let image = validate(vec![ledger()]).unwrap();
        assert_eq!(image.companies(), &["a"]);
        assert_eq!(image.receipt_bytes(), b"{}");
        // Deliberately accepts non-SQLite bytes: it confers no business authority.
        assert_eq!(image.files()[0].bytes, b"not domain validation");
    }
    #[test]
    fn refuses_duplicates_digest_changes_paths_and_missing_company_ledgers() {
        assert_eq!(
            validate(vec![ledger(), ledger()]).unwrap_err(),
            Error::Inventory
        );
        let mut bad = ledger();
        bad.bytes.push(0);
        assert_eq!(validate(vec![bad]).unwrap_err(), Error::Digest);
        for path in [
            "../a/data/ledger.sqlite",
            "/a/data/ledger.sqlite",
            "a/data/./ledger.sqlite",
            "a/data/ledger.sqlite-wal",
        ] {
            let mut bad = ledger();
            bad.path = path.into();
            assert_eq!(validate(vec![bad]).unwrap_err(), Error::UnsafePath);
        }
        let mut bad = ledger();
        bad.kind = AdoptionFileKind::CompanyLedger {
            company: "other".into(),
        };
        assert_eq!(validate(vec![bad]).unwrap_err(), Error::UnsafePath);
        assert!(
            AdoptionImage::validate(
                "w",
                "a",
                manifest(),
                vec![ledger()],
                b"{}".to_vec(),
                "wrong"
            )
            .is_err()
        );
    }
    #[test]
    fn manifest_company_scope_is_complete_and_selected_company_must_be_active() {
        let mut parsed: serde_json::Value = serde_json::from_slice(&manifest()).unwrap();
        parsed["companies"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "slug":"b", "name":"Synthetic archive", "createdAt":"2026-01-01", "archived":true
            }));
        let check = |value: &serde_json::Value, files: Vec<AdoptionFile>| {
            AdoptionImage::validate(
                "w",
                "a",
                serde_json::to_vec(value).unwrap(),
                files,
                b"{}".to_vec(),
                &hash(b"{}"),
            )
        };
        assert_eq!(
            check(&parsed, vec![ledger()]).unwrap_err(),
            Error::Inventory
        );
        let mut second = ledger();
        second.kind = AdoptionFileKind::CompanyLedger {
            company: "b".into(),
        };
        second.path = "b/data/ledger.sqlite".into();
        assert!(check(&parsed, vec![ledger(), second]).is_ok());
        parsed["companies"][0]["archived"] = serde_json::json!(true);
        assert_eq!(check(&parsed, vec![ledger()]).unwrap_err(), Error::Manifest);
        assert_eq!(
            AdoptionImage::validate("w", "a", manifest(), vec![ledger()], vec![], &hash(b""))
                .unwrap_err(),
            Error::Bounds
        );
    }
    #[test]
    fn original_names_are_content_bound_and_no_control_or_source_paths_are_allowed() {
        let bytes = b"synthetic PDF".to_vec();
        let digest = hash(&bytes);
        let original = AdoptionFile {
            kind: AdoptionFileKind::DocumentOriginal {
                company: "a".into(),
            },
            path: format!("a/documents/originals/{digest}.pdf"),
            bytes,
            sha256: digest,
        };
        assert!(validate(vec![ledger(), original.clone()]).is_ok());
        for path in [
            ".rentemester/workspace-control.sqlite",
            "a/documents/originals/source.pdf",
            "a/documents/originals/../source.pdf",
        ] {
            let mut bad = original.clone();
            bad.path = path.into();
            assert_eq!(
                validate(vec![ledger(), bad]).unwrap_err(),
                Error::UnsafePath
            );
        }
    }
}

/// Read a selected legacy source through descriptor-relative NOFOLLOW opens.
/// This is an offline operator boundary, not a native destination namespace or
/// evidence that external producers stopped. The host must bind hashes/fences.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn read_source_file(
    root: &std::path::Path,
    relative: &str,
    limit: usize,
) -> Result<Vec<u8>, String> {
    use std::io::Read;
    if limit == 0 || limit > 512 * 1024 * 1024 {
        return Err("legacy source byte bound required".into());
    }
    let (_, file) = source_file(root, relative)?;
    let meta = file
        .metadata()
        .map_err(|_| "legacy source metadata unavailable")?;
    if meta.len() > limit as u64 {
        return Err("legacy source file exceeds bound".into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "legacy source read refused")?;
    if bytes.len() > limit {
        return Err("legacy source file exceeds bound".into());
    }
    Ok(bytes)
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn source_file(
    root: &std::path::Path,
    relative: &str,
) -> Result<(std::path::PathBuf, std::fs::File), String> {
    use rustix::fs::{Mode, OFlags, open, openat};
    use std::os::unix::fs::MetadataExt;
    use std::path::Component;
    if !root.is_absolute()
        || root
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        || std::fs::symlink_metadata(root)
            .map_err(|_| "legacy root unavailable")?
            .file_type()
            .is_symlink()
    {
        return Err("absolute unlinked legacy root required".into());
    }
    let parts = relative.split('/').collect::<Vec<_>>();
    if parts.is_empty()
        || parts.len() > 16
        || parts.iter().any(|p| {
            p.is_empty()
                || *p == "."
                || *p == ".."
                || p.contains('\\')
                || p.chars().any(char::is_control)
        })
    {
        return Err("canonical relative source path required".into());
    }
    // Canonicalize trusted ancestor aliases (e.g. macOS /var); reject every
    // symlink below the explicitly selected root using one openat per component.
    let canonical = root.canonicalize().map_err(|_| "legacy root unavailable")?;
    let mut parent = open(
        &canonical,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| "legacy root open refused")?;
    for (i, part) in parts.iter().enumerate() {
        let last = i + 1 == parts.len();
        let mut flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK;
        if !last {
            flags |= OFlags::DIRECTORY;
        }
        let next = openat(&parent, *part, flags, Mode::empty())
            .map_err(|_| "legacy source path refused")?;
        if last {
            let file = std::fs::File::from(next);
            let m = file
                .metadata()
                .map_err(|_| "legacy source metadata refused")?;
            if !m.is_file()
                || m.nlink() != 1
                || m.uid() != rustix::process::geteuid().as_raw()
                || m.mode() & 0o022 != 0
            {
                return Err(
                    "legacy source must be an owned single-link regular file without other writers"
                        .into(),
                );
            }
            return Ok((canonical.join(relative), file));
        }
        parent = next;
    }
    Err("legacy source path unavailable".into())
}
/// Trusted offline operator only. Pin the exact source inode before any SQLite
/// write and recheck path identity afterward. A failed multi-DB retirement stays
/// fenced in completed DBs; the host must refuse activation until all verify.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn with_source_database<T>(
    root: &std::path::Path,
    relative: &str,
    operation: impl FnOnce(&mut rusqlite::Connection) -> Result<T, rentemester_engine::Error>,
) -> Result<T, rentemester_engine::Error> {
    use rentemester_engine::Error;
    use std::os::unix::fs::MetadataExt;
    let refused = |s| Error::new("legacy_source_refused", s);
    let (path, pin) = source_file(root, relative).map_err(&refused)?;
    let meta = pin
        .metadata()
        .map_err(|_| refused("source metadata unavailable".into()))?;
    let mut db = rusqlite::Connection::open_with_flags(
        &path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .map_err(|_| refused("source SQLite open refused".into()))?;
    let identity = db
        .main_file_identity()
        .map_err(|_| refused("source SQLite identity unavailable".into()))?;
    if identity.device != meta.dev() || identity.inode != meta.ino() || identity.links != 1 {
        return Err(refused("source SQLite inode changed".into()));
    }
    db.busy_timeout(std::time::Duration::from_secs(2))
        .map_err(|_| refused("source SQLite timeout refused".into()))?;
    db.execute_batch("PRAGMA trusted_schema=OFF;PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL")
        .map_err(|_| refused("source SQLite session refused".into()))?;
    let result = operation(&mut db);
    db.close()
        .map_err(|_| refused("source SQLite close refused".into()))?;
    let (_, current) = source_file(root, relative).map_err(&refused)?;
    let after = current
        .metadata()
        .map_err(|_| refused("source metadata unavailable".into()))?;
    if after.dev() != meta.dev() || after.ino() != meta.ino() {
        return Err(refused("source SQLite path changed".into()));
    }
    result
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn read_source_file(_: &std::path::Path, _: &str, _: usize) -> Result<Vec<u8>, String> {
    Err("legacy source capture unsupported".into())
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn with_source_database<T>(
    _: &std::path::Path,
    _: &str,
    _: impl FnOnce(&mut rusqlite::Connection) -> Result<T, rentemester_engine::Error>,
) -> Result<T, rentemester_engine::Error> {
    Err(rentemester_engine::Error::new(
        "legacy_source_refused",
        "legacy source retirement unsupported",
    ))
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod source_tests {
    use super::*;
    #[test]
    fn source_reads_are_bounded_and_reject_links_and_traversal() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("company")).unwrap();
        std::fs::write(
            dir.path().join("company/original.pdf"),
            b"synthetic original",
        )
        .unwrap();
        assert_eq!(
            read_source_file(dir.path(), "company/original.pdf", 64).unwrap(),
            b"synthetic original"
        );
        assert!(read_source_file(dir.path(), "company/original.pdf", 2).is_err());
        assert!(read_source_file(dir.path(), "company/original.pdf", 0).is_err());
        assert!(
            read_source_file(dir.path(), "company/original.pdf", 512 * 1024 * 1024 + 1).is_err()
        );
        for path in [
            "../outside",
            "company/../company/original.pdf",
            "company//original.pdf",
            "/company/original.pdf",
        ] {
            assert!(read_source_file(dir.path(), path, 64).is_err());
        }
        std::os::unix::fs::symlink(dir.path().join("company"), dir.path().join("linked")).unwrap();
        assert!(read_source_file(dir.path(), "linked/original.pdf", 64).is_err());
        std::fs::hard_link(
            dir.path().join("company/original.pdf"),
            dir.path().join("hard.pdf"),
        )
        .unwrap();
        assert!(read_source_file(dir.path(), "company/original.pdf", 64).is_err());
    }
    #[test]
    fn source_sqlite_operation_pins_inode_and_refuses_path_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.sqlite");
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE business(value TEXT);INSERT INTO business VALUES('preserved')",
        )
        .unwrap();
        db.close().unwrap();
        let value = with_source_database(dir.path(), "legacy.sqlite", |db| {
            db.query_row("SELECT value FROM business", [], |r| r.get::<_, String>(0))
                .map_err(|_| rentemester_engine::Error::new("test", "read refused"))
        })
        .unwrap();
        assert_eq!(value, "preserved");
        let result = with_source_database(dir.path(), "legacy.sqlite", |_| {
            std::fs::rename(&path, dir.path().join("preserved.sqlite")).unwrap();
            std::fs::write(&path, b"replacement").unwrap();
            Ok(())
        });
        assert_eq!(result.unwrap_err().code, "legacy_source_refused");
        let preserved = rusqlite::Connection::open(dir.path().join("preserved.sqlite")).unwrap();
        assert_eq!(
            preserved
                .query_row("SELECT value FROM business", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "preserved"
        );
    }
}
