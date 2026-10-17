//! Cooperative process lifetime, never accounting/company write authority.
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
pub use local::WorkspaceLease;

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
