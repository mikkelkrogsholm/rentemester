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
