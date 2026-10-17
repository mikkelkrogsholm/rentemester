use crate::{LeaseError, RootIdentity, bound_path};
use rustix::{
    fd::OwnedFd,
    fs::{FlockOperation, Mode, OFlags, flock, fstat, fstatfs, open},
};
use std::path::{Path, PathBuf};

/// Exclusive cooperative lease on one physical directory. Not a write/authorization capability.
/// Never clone/expose the descriptor or unlink a separate sentinel. Kernel close releases it.
#[derive(Debug)]
pub struct WorkspaceLease {
    directory: OwnedFd,
    path: PathBuf,
    identity: RootIdentity,
    filesystem: &'static str,
    invalidated: bool,
}
fn open_root(path: &Path) -> Result<OwnedFd, LeaseError> {
    open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| LeaseError::Unavailable)
}
fn identity(fd: &OwnedFd) -> Result<RootIdentity, LeaseError> {
    let stat = fstat(fd).map_err(|_| LeaseError::Io)?;
    Ok(RootIdentity {
        device: stat.st_dev as u64,
        inode: stat.st_ino as u64,
    })
}
// Candidate local families only; not proof of no synchronization or release qualification.
fn allowed_filesystem(name: &str) -> bool {
    matches!(name, "apfs" | "ext" | "xfs" | "btrfs" | "tmpfs")
}
fn filesystem(fd: &OwnedFd) -> Result<&'static str, LeaseError> {
    let info = fstatfs(fd).map_err(|_| LeaseError::Io)?;
    #[cfg(target_os = "macos")]
    let name = {
        let bytes: Vec<u8> = info
            .f_fstypename
            .iter()
            .take_while(|c| **c != 0)
            .map(|c| *c as u8)
            .collect();
        if bytes == b"apfs" {
            "apfs"
        } else {
            "unsupported"
        }
    };
    #[cfg(target_os = "linux")]
    let name = match info.f_type as u64 {
        0xef53 => "ext",
        0x58465342 => "xfs",
        0x9123683e => "btrfs",
        0x01021994 => "tmpfs",
        _ => "unsupported",
    };
    if allowed_filesystem(name) {
        Ok(name)
    } else {
        Err(LeaseError::UnsupportedFilesystem)
    }
}
impl WorkspaceLease {
    pub fn acquire(path: &Path) -> Result<Self, LeaseError> {
        let path = bound_path(path)?;
        let directory = open_root(&path)?;
        let filesystem = filesystem(&directory)?;
        let identity = identity(&directory)?;
        flock(&directory, FlockOperation::NonBlockingLockExclusive).map_err(|error| {
            if error == rustix::io::Errno::WOULDBLOCK || error == rustix::io::Errno::AGAIN {
                LeaseError::Busy
            } else {
                LeaseError::Io
            }
        })?;
        let mut lease = Self {
            directory,
            path,
            identity,
            filesystem,
            invalidated: false,
        };
        lease.ensure_current_root()?;
        Ok(lease)
    }
    /// Kernel-observed candidate family, not production storage qualification.
    pub fn filesystem(&self) -> &'static str {
        self.filesystem
    }
    /// Observation before protected work, not an atomic namespace fence. Loss is permanent.
    pub fn ensure_current_root(&mut self) -> Result<RootIdentity, LeaseError> {
        if self.invalidated {
            return Err(LeaseError::RootChanged);
        }
        let current = open_root(&self.path).and_then(|fd| identity(&fd));
        let held = identity(&self.directory);
        if current != Ok(self.identity) || held != Ok(self.identity) {
            self.invalidated = true;
            return Err(LeaseError::RootChanged);
        }
        Ok(self.identity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn same_process_exclusion_drop_and_ordinary_close() {
        let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        let mut held = WorkspaceLease::acquire(root.path()).unwrap();
        let ordinary = open_root(root.path()).unwrap();
        drop(ordinary);
        assert_eq!(
            WorkspaceLease::acquire(root.path()).unwrap_err(),
            LeaseError::Busy
        );
        assert!(held.ensure_current_root().is_ok());
        drop(held);
        assert!(WorkspaceLease::acquire(root.path()).is_ok());
    }
    #[test]
    fn known_network_and_unknown_families_refuse() {
        let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
        for name in ["nfs", "smbfs", "cifs", "fuse", "overlay", "unknown"] {
            assert!(!allowed_filesystem(name));
        }
    }
    #[test]
    fn moved_root_is_permanently_invalid_even_if_returned() {
        let _serial = crate::LEASE_TEST_LOCK.lock().unwrap();
        let parent = tempfile::tempdir().unwrap();
        let original = parent.path().join("workspace");
        let moved = parent.path().join("moved");
        std::fs::create_dir(&original).unwrap();
        let mut held = WorkspaceLease::acquire(&original).unwrap();
        std::fs::rename(&original, &moved).unwrap();
        assert_eq!(held.ensure_current_root(), Err(LeaseError::RootChanged));
        std::fs::rename(&moved, &original).unwrap();
        assert_eq!(held.ensure_current_root(), Err(LeaseError::RootChanged));
        assert_eq!(
            WorkspaceLease::acquire(&original).unwrap_err(),
            LeaseError::Busy
        );
        drop(held);
        assert!(WorkspaceLease::acquire(&original).is_ok());
    }
}

mod layout;
pub use layout::WorkspaceLayout;
