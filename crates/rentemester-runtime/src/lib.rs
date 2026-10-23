//! Runtime identity verification and cooperative process lifetime.
pub mod vies;
pub mod imap;
pub mod document_scanner;
pub mod invoice_extractor;
pub mod document_pdf;
pub mod auth;
pub mod auth_mail;
pub mod backup_artifact;
pub mod backup_destination;
pub mod legacy_adoption;
pub mod email_token;
pub mod mfa;
pub mod password;
pub mod recovery;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseError {
    InvalidRoot,
    Unavailable,
    Busy,
    UnsupportedFilesystem,
    UnsupportedPlatform,
    Io,
    RootChanged,
}
impl LeaseError {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidRoot => "invalid_root",
            Self::Unavailable => "workspace_unavailable",
            Self::Busy => "workspace_busy",
            Self::UnsupportedFilesystem => "unsupported_filesystem",
            Self::UnsupportedPlatform => "unsupported_platform",
            Self::Io => "ownership_io",
            Self::RootChanged => "workspace_changed",
        }
    }
}
impl std::fmt::Display for LeaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for LeaseError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootIdentity {
    pub device: u64,
    pub inode: u64,
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod local;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use local::{WorkspaceLayout, WorkspaceLease};

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[derive(Debug)]
pub struct WorkspaceLease;
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
impl WorkspaceLease {
    pub fn filesystem(&self) -> &'static str {
        "unsupported"
    }
    pub fn acquire(_: &Path) -> Result<Self, LeaseError> {
        Err(LeaseError::UnsupportedPlatform)
    }
    pub fn ensure_current_root(&mut self) -> Result<RootIdentity, LeaseError> {
        Err(LeaseError::UnsupportedPlatform)
    }
}

/// Reject ambiguous lexical traversal rather than guessing a workspace from an environment variable.
#[cfg(any(target_os = "linux", target_os = "macos", test))]
fn bound_path(path: &Path) -> Result<std::path::PathBuf, LeaseError> {
    use std::path::Component;
    if path.as_os_str().is_empty() || path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(LeaseError::InvalidRoot);
    }
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()
            .map_err(|_| LeaseError::Io)?
            .join(path)
    };
    Ok(absolute.components().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lexical_roots_are_explicit_and_bound_once() {
        for path in ["", "..", "a/../b"] {
            assert_eq!(
                bound_path(Path::new(path)).unwrap_err(),
                LeaseError::InvalidRoot
            );
        }
        assert!(bound_path(Path::new(".")).unwrap().is_absolute());
        assert_eq!(
            bound_path(Path::new("/tmp/a/./")).unwrap(),
            Path::new("/tmp/a")
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutError {
    InvalidManifest,
    InvalidSelection,
    Unavailable,
    Changed,
    Lease(LeaseError),
}
impl LayoutError {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidManifest => "invalid_manifest",
            Self::InvalidSelection => "invalid_company_selection",
            Self::Unavailable => "layout_unavailable",
            Self::Changed => "layout_changed",
            Self::Lease(e) => e.code(),
        }
    }
}
impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for LayoutError {}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub struct WorkspaceLayout {
    _private: (),
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
impl WorkspaceLayout {
    pub fn bind(_: WorkspaceLease, _: &str, _: &str) -> Result<Self, LayoutError> {
        Err(LayoutError::Lease(LeaseError::UnsupportedPlatform))
    }
    pub fn ensure_current(&mut self) -> Result<(), LayoutError> {
        Err(LayoutError::Lease(LeaseError::UnsupportedPlatform))
    }
    pub fn ensure_connections(
        &mut self,
        _: &rusqlite::Connection,
        _: &rusqlite::Connection,
    ) -> Result<(), LayoutError> {
        Err(LayoutError::Lease(LeaseError::UnsupportedPlatform))
    }
    pub fn workspace(&self) -> &str {
        ""
    }
    pub fn slug(&self) -> &str {
        ""
    }
}

// Tests spawning fork/exec must not inherit another test's transient lease.
#[cfg(test)]
static LEASE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub mod service_key;

pub mod posting_time;

pub mod namespace;

pub mod bootstrap;

pub mod document;

pub mod invitation;

pub mod backup_keys;

pub mod invoice_mail;

pub mod workspace_transfer;

pub mod contact_import;

pub mod public_einvoice;

pub mod package_update;

pub mod backup_remote;

pub mod gdpr_audit;
