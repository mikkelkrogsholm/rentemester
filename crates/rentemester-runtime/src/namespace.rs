//! Dedicated-principal namespace prerequisite, not application authorization.
#[cfg(not(target_os = "linux"))]
use std::path::Path;

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

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::NamespaceGuard;

#[cfg(not(target_os = "linux"))]
pub struct NamespaceGuard {
    _private: (),
}
#[cfg(not(target_os = "linux"))]
impl NamespaceGuard {
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
#[cfg(all(test, not(target_os = "linux")))]
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
