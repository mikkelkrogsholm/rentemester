//! Explicitly profiled physical namespace admission, not application authorization.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
use std::path::Path;

/// Explicit process trust profiles. LocalUserMacos trusts all code running under
/// the local account; it is not Linux no-new-privileges/capability isolation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NamespaceProfile {
    #[default]
    DedicatedLinux,
    LocalUserMacos,
}
#[cfg(target_os = "macos")]
mod macos;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamespaceError {
    Unsupported,
    Process,
    Invalid,
    Exists,
    Busy,
    Changed,
}
impl NamespaceError {
    pub fn code(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported_broker_profile",
            Self::Process => "unsafe_broker_process",
            Self::Invalid => "unsafe_broker_namespace",
            Self::Exists => "workspace_exists",
            Self::Busy => "workspace_busy",
            Self::Changed => "broker_namespace_changed",
        }
    }
}
impl std::fmt::Display for NamespaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for NamespaceError {}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod linux;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use linux::NamespaceGuard;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub struct NamespaceGuard {
    _private: (),
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
impl NamespaceGuard {
    pub fn read_imap_config(&mut self) -> Result<Option<Vec<u8>>, NamespaceError> {
        Err(NamespaceError::Unsupported)
    }
    pub fn write_imap_config(&mut self, _: &[u8]) -> Result<(), NamespaceError> {
        Err(NamespaceError::Unsupported)
    }
    pub fn remove_imap_config(&mut self) -> Result<(), NamespaceError> {
        Err(NamespaceError::Unsupported)
    }
    pub fn workspace_manifest(&mut self) -> Result<Vec<u8>, NamespaceError> {
        Err(NamespaceError::Unsupported)
    }
    pub fn snapshot_originals(
        &mut self,
        _: bool,
    ) -> Result<Vec<(String, Vec<u8>)>, NamespaceError> {
        Err(NamespaceError::Unsupported)
    }
    pub fn create_verified_restore_stage(
        _: &Path,
        _: u32,
        _: &str,
        _: &str,
        _: NamespaceProfile,
        _: &crate::backup_artifact::VerifiedArtifact,
    ) -> Result<Self, NamespaceError> {
        Err(NamespaceError::Unsupported)
    }
    pub fn create_adoption_stage(
        _: &Path, _: u32, _: &str, _: &str, _: NamespaceProfile,
        _: &crate::legacy_adoption::AdoptionImage,
    ) -> Result<Self, NamespaceError> { Err(NamespaceError::Unsupported) }
    pub fn publish_adopted_root(&mut self, _: &Path) -> Result<(), NamespaceError> {
        Err(NamespaceError::Unsupported)
    }
    pub fn publish_restored_root(&mut self, _: &Path) -> Result<(), NamespaceError> {
        Err(NamespaceError::Unsupported)
    }

    pub fn catalog(&mut self)->Result<Vec<(String,String,bool)>,NamespaceError>{Err(NamespaceError::Unsupported)}
    pub fn select_company(&mut self,_:&str)->Result<(),NamespaceError>{Err(NamespaceError::Unsupported)}
    pub fn select_company_for_snapshot(&mut self,_:&str)->Result<(),NamespaceError>{Err(NamespaceError::Unsupported)}
    pub fn create_company_files(&mut self,_:&str)->Result<(),NamespaceError>{Err(NamespaceError::Unsupported)}
    pub fn verify_pending_company(&mut self,_:&str,_:&rusqlite::Connection)->Result<(),NamespaceError>{Err(NamespaceError::Unsupported)}
    pub fn publish_company(&mut self,_:&str,_:&str,_:i64)->Result<(),NamespaceError>{Err(NamespaceError::Unsupported)}
    pub fn acquire_with_profile(
        _: &Path,
        _: u32,
        _: &str,
        _: &str,
        _: NamespaceProfile,
    ) -> Result<Self, NamespaceError> {
        Err(NamespaceError::Unsupported)
    }
    pub fn create_fresh_with_profile(
        _: &Path,
        _: u32,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: NamespaceProfile,
    ) -> Result<Self, NamespaceError> {
        Err(NamespaceError::Unsupported)
    }
    pub fn acquire(_: &Path, _: u32, _: &str, _: &str) -> Result<Self, NamespaceError> {
        Err(NamespaceError::Unsupported)
    }
    pub fn create_fresh(
        _: &Path,
        _: u32,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<Self, NamespaceError> {
        Err(NamespaceError::Unsupported)
    }
    pub fn publish_document(
        &mut self,
        _: &crate::document::Prepared,
    ) -> Result<String, NamespaceError> {
        Err(NamespaceError::Unsupported)
    }
    pub fn publish_bank_statement(&mut self, _: &str) -> Result<String, NamespaceError> {
        Err(NamespaceError::Unsupported)
    }
    pub fn publish_bank_statement_bytes(&mut self, _: &[u8]) -> Result<String, NamespaceError> {
        Err(NamespaceError::Unsupported)
    }
    pub fn read_bank_statement(&mut self, _: &str) -> Result<Vec<u8>, NamespaceError> {
        Err(NamespaceError::Unsupported)
    }
    pub fn read_document(&mut self, _: &str, _: &str) -> Result<Vec<u8>, NamespaceError> {
        Err(NamespaceError::Unsupported)
    }
    pub fn ensure_current(&mut self) -> Result<(), NamespaceError> {
        Err(NamespaceError::Unsupported)
    }
    pub fn ensure_connections(
        &mut self,
        _: &rusqlite::Connection,
        _: &rusqlite::Connection,
    ) -> Result<(), NamespaceError> {
        Err(NamespaceError::Unsupported)
    }
}
#[cfg(all(test, not(any(target_os = "linux", target_os = "macos"))))]
mod tests {
    use super::*;
    #[test]
    fn unsupported_profile_does_not_touch_the_requested_root() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("absent");
        assert!(matches!(
            NamespaceGuard::acquire(&path, 10001, "w", "a"),
            Err(NamespaceError::Unsupported)
        ));
        assert!(!path.exists());
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn backup_directory(path: &std::path::Path, uid: u32) -> Result<std::fs::File, NamespaceError> {
    linux::backup_directory(path, uid)
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn backup_file_acl(file: &std::fs::File) -> Result<(), NamespaceError> {
    linux::backup_file_acl(file)
}
