//! Authenticated application packages. Never opens a workspace or migrates data.
mod manifest;
pub use manifest::{FileEntry, Manifest, Policy, VerifiedPackage, verify};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error(pub &'static str);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Error {}
#[cfg(any(target_os = "macos", target_os = "linux"))]
mod install;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub use install::{PackageLease, current_executable_lease, install, recover, status};
/// No implicit database migration accompanies a package update. A schema change
/// needs its separate stopped-server operator flow before distribution can qualify.
pub fn current_policy() -> Policy {
    Policy {
        platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        api_epoch: 1,
        control_schema: 34,
        company_schema: 36,
    }
}

/// Read only an explicitly selected protected public key, never a key supplied
/// by the candidate manifest. No secret material or workspace is involved.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn read_trusted_key(path: &std::path::Path) -> Result<Vec<u8>, Error> {
    use std::{
        fs::{self, OpenOptions},
        io::Read,
        os::unix::fs::{MetadataExt, OpenOptionsExt},
    };
    let refused = || Error("update_trust_key_refused");
    if !path.is_absolute() || fs::canonicalize(path).map_err(|_| refused())? != path {
        return Err(refused());
    }
    let before = fs::symlink_metadata(path).map_err(|_| refused())?;
    if !before.is_file()
        || before.file_type().is_symlink()
        || before.len() != 32
        || before.nlink() != 1
        || before.mode() & 0o022 != 0
        || (before.uid() != 0 && before.uid() != rustix::process::getuid().as_raw())
    {
        return Err(refused());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(path)
        .map_err(|_| refused())?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(33)
        .read_to_end(&mut bytes)
        .map_err(|_| refused())?;
    let after = file.metadata().map_err(|_| refused())?;
    let named = fs::symlink_metadata(path).map_err(|_| refused())?;
    let identity = |m: &fs::Metadata| {
        (
            m.dev(),
            m.ino(),
            m.mode(),
            m.nlink(),
            m.len(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec(),
        )
    };
    if bytes.len() != 32
        || identity(&before) != identity(&after)
        || identity(&before) != identity(&named)
    {
        return Err(refused());
    }
    Ok(bytes)
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn read_trusted_key(_: &std::path::Path) -> Result<Vec<u8>, Error> {
    Err(Error("update_platform_unqualified"))
}
