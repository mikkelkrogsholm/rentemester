//! Explicit local operator key custody; no generation, rotation or artifact trust discovery.
use crate::backup_artifact::{SigningKey, TrustedPublicKey};
use sha2::{Digest, Sha256};
use std::path::Path;
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn timestamp(ms: i64) -> Result<String, &'static str> {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .ok_or("invalid backup clock")
}
#[cfg(unix)]
fn read(path: &Path, private: bool, max: usize) -> Result<Vec<u8>, &'static str> {
    use rustix::fs::{AtFlags, FileType, Mode, OFlags, fstat, open, statat};
    use std::io::Read;
    let st = statat(rustix::fs::CWD, path, AtFlags::SYMLINK_NOFOLLOW)
        .map_err(|_| "backup key unavailable")?;
    if FileType::from_raw_mode(st.st_mode) != FileType::RegularFile
        || st.st_nlink != 1
        || st.st_size <= 0
        || st.st_size as usize > max
        || private && st.st_mode & 0o7777 != 0o600
    {
        return Err("private backup key mode/type/size refused");
    }
    let fd = open(
        path,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|_| "backup key unavailable")?;
    let opened = fstat(&fd).map_err(|_| "backup key unavailable")?;
    if opened.st_dev != st.st_dev || opened.st_ino != st.st_ino {
        return Err("backup key changed");
    }
    let mut bytes = Vec::new();
    std::fs::File::from(fd)
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "backup key unavailable")?;
    if bytes.len() != st.st_size as usize {
        return Err("backup key changed");
    }
    Ok(bytes)
}
#[cfg(not(unix))]
fn read(_: &Path, _: bool, _: usize) -> Result<Vec<u8>, &'static str> {
    Err("backup operator files unsupported on this platform")
}
pub fn signing_key(path: &Path, workspace: &Path) -> Result<SigningKey, &'static str> {
    let target = path
        .canonicalize()
        .map_err(|_| "backup signing key unavailable")?;
    let root = workspace
        .parent()
        .and_then(|p| p.canonicalize().ok())
        .zip(workspace.file_name())
        .map(|(p, n)| p.join(n))
        .ok_or("invalid backup workspace")?;
    if target.starts_with(root) {
        return Err("backup signing key must be outside workspace");
    }
    let bytes = zeroize::Zeroizing::new(read(path, true, 4096)?);
    SigningKey::from_pkcs8(&bytes).map_err(|_| "invalid explicit Ed25519 signing key")
}
pub fn trusted_key(path: &Path) -> Result<TrustedPublicKey, &'static str> {
    TrustedPublicKey::from_bytes(&read(path, false, 32)?)
        .map_err(|_| "invalid separate trusted Ed25519 public key")
}
