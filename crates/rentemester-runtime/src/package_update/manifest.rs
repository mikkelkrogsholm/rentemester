//! Signed package admission. The trust anchor is supplied by the installed host.
use super::Error;
use ring::signature::{ED25519, UnparsedPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, Metadata, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
};

const MANIFEST: &str = "Contents/Resources/native-update.json";
const SIGNATURE: &str = "Contents/Resources/native-update.sig";
const MAX_METADATA: u64 = 1024 * 1024;
const MAX_FILE: u64 = 512 * 1024 * 1024;
const MAX_TOTAL: u64 = 1024 * 1024 * 1024;
const MAX_FILES: usize = 20_000;
const REQUIRED: &[&str] = &[
    "Contents/MacOS/rentemester-desktop",
    "Contents/MacOS/rentemester-server",
    "Contents/MacOS/rentemester-update",
    "Contents/Resources/web/index.html",
    "Contents/Info.plist",
];
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub executable: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Manifest {
    pub format: String,
    pub bundle_id: String,
    pub version: String,
    pub sequence: u64,
    pub platform: String,
    pub api_epoch: u32,
    pub control_schema: u32,
    pub company_schema: u32,
    pub source_revision: String,
    pub toolchains: BTreeMap<String, String>,
    pub files: Vec<FileEntry>,
}
#[derive(Debug, Clone)]
pub struct Policy {
    pub platform: String,
    pub api_epoch: u32,
    pub control_schema: u32,
    pub company_schema: u32,
}
#[derive(Debug)]
pub struct VerifiedPackage {
    root: PathBuf,
    manifest: Manifest,
    manifest_sha256: String,
}
impl VerifiedPackage {
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    pub fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }
}
fn err() -> Error {
    Error("native_update_package_invalid")
}
fn hex(value: &str, size: usize) -> bool {
    value.len() == size
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn version(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|v| {
            !v.is_empty()
                && (v.len() == 1 || !v.starts_with('0'))
                && v.bytes().all(|b| b.is_ascii_digit())
                && v.parse::<u64>().is_ok()
        })
}
fn safe_path(value: &str) -> bool {
    value.starts_with("Contents/")
        && value.len() <= 1024
        && value.split('/').all(|p| {
            !p.is_empty()
                && p != "."
                && p != ".."
                && !p.ends_with('.')
                && p.len() <= 255
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                && !matches!(
                    p.split('.')
                        .next()
                        .unwrap_or("")
                        .to_ascii_uppercase()
                        .as_str(),
                    "CON"
                        | "PRN"
                        | "AUX"
                        | "NUL"
                        | "COM1"
                        | "COM2"
                        | "COM3"
                        | "COM4"
                        | "COM5"
                        | "COM6"
                        | "COM7"
                        | "COM8"
                        | "COM9"
                        | "LPT1"
                        | "LPT2"
                        | "LPT3"
                        | "LPT4"
                        | "LPT5"
                        | "LPT6"
                        | "LPT7"
                        | "LPT8"
                        | "LPT9"
                )
        })
}
fn mode(meta: &Metadata) -> Result<bool, Error> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.mode() & 0o7022 != 0 || (meta.is_file() && meta.nlink() != 1) {
            return Err(err());
        }
        Ok(meta.mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        Ok(false)
    }
}
fn same(a: &Metadata, b: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        a.dev() == b.dev()
            && a.ino() == b.ino()
            && a.mode() == b.mode()
            && a.nlink() == b.nlink()
            && a.len() == b.len()
            && a.mtime() == b.mtime()
            && a.mtime_nsec() == b.mtime_nsec()
            && a.ctime() == b.ctime()
            && a.ctime_nsec() == b.ctime_nsec()
    }
    #[cfg(not(unix))]
    {
        a.len() == b.len()
            && a.modified().ok() == b.modified().ok()
            && a.file_type() == b.file_type()
    }
}
fn open_file(path: &Path, limit: u64) -> Result<(File, Metadata), Error> {
    let before = fs::symlink_metadata(path).map_err(|_| err())?;
    if !before.is_file() || before.len() > limit {
        return Err(err());
    }
    mode(&before)?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
        );
    }
    let file = options.open(path).map_err(|_| err())?;
    if !same(&before, &file.metadata().map_err(|_| err())?) {
        return Err(err());
    }
    Ok((file, before))
}
fn stable(path: &Path, file: &File, before: &Metadata) -> Result<(), Error> {
    if !same(before, &file.metadata().map_err(|_| err())?)
        || !same(before, &fs::symlink_metadata(path).map_err(|_| err())?)
    {
        return Err(err());
    }
    Ok(())
}
fn bytes(path: &Path, limit: u64) -> Result<Vec<u8>, Error> {
    let (mut file, before) = open_file(path, limit)?;
    let mut data = Vec::new();
    (&mut file)
        .take(limit + 1)
        .read_to_end(&mut data)
        .map_err(|_| err())?;
    if data.len() as u64 != before.len() {
        return Err(err());
    }
    stable(path, &file, &before)?;
    Ok(data)
}
fn walk(
    root: &Path,
    dir: &Path,
    files: &mut BTreeMap<String, Metadata>,
    dirs: &mut Vec<(PathBuf, Metadata)>,
) -> Result<(), Error> {
    let meta = fs::symlink_metadata(dir).map_err(|_| err())?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(err());
    }
    mode(&meta)?;
    if dirs.len() + files.len() >= MAX_FILES * 2 {
        return Err(err());
    }
    dirs.push((dir.to_path_buf(), meta));
    for entry in fs::read_dir(dir).map_err(|_| err())? {
        let path = entry.map_err(|_| err())?.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| err())?
            .to_str()
            .ok_or_else(err)?
            .replace(std::path::MAIN_SEPARATOR, "/");
        if relative != "Contents" && !safe_path(&relative) {
            return Err(err());
        }
        let meta = fs::symlink_metadata(&path).map_err(|_| err())?;
        mode(&meta)?;
        if meta.is_dir() {
            walk(root, &path, files, dirs)?;
        } else if meta.is_file() && !meta.file_type().is_symlink() {
            if files.len() >= MAX_FILES + 2 {
                return Err(err());
            }
            files.insert(relative, meta);
        } else {
            return Err(err());
        }
    }
    Ok(())
}
/// Verify exact signed bytes and every package file. The publisher must reverify
/// its private staged copy immediately before publication; this is not a lease.
pub fn verify(root: &Path, trusted_key: &[u8], policy: &Policy) -> Result<VerifiedPackage, Error> {
    if !root.is_absolute()
        || fs::canonicalize(root).map_err(|_| err())? != root
        || trusted_key.len() != 32
    {
        return Err(err());
    }
    let mut actual = BTreeMap::new();
    let mut dirs = Vec::new();
    walk(root, root, &mut actual, &mut dirs)?;
    let raw = bytes(&root.join(MANIFEST), MAX_METADATA)?;
    let signature = bytes(&root.join(SIGNATURE), 64)?;
    if signature.len() != 64
        || UnparsedPublicKey::new(&ED25519, trusted_key)
            .verify(&raw, &signature)
            .is_err()
    {
        return Err(Error("native_update_signature_invalid"));
    }
    let manifest: Manifest = serde_json::from_slice(&raw).map_err(|_| err())?;
    if manifest.format != "rentemester-native-update/1"
        || manifest.bundle_id != "dk.rentemester.desktop"
        || !version(&manifest.version)
        || manifest.sequence == 0
        || manifest.sequence > 9_007_199_254_740_991
        || manifest.platform != policy.platform
        || manifest.api_epoch != policy.api_epoch
        || manifest.control_schema != policy.control_schema
        || manifest.company_schema != policy.company_schema
        || !hex(&manifest.source_revision, 40)
        || manifest.toolchains.is_empty()
        || manifest.toolchains.len() > 32
        || manifest.toolchains.iter().any(|(k, v)| {
            k.is_empty()
                || k.len() > 64
                || v.is_empty()
                || v.len() > 256
                || k.chars().chain(v.chars()).any(char::is_control)
        })
        || manifest.files.is_empty()
        || manifest.files.len() > MAX_FILES
    {
        return Err(err());
    }
    actual.remove(MANIFEST).ok_or_else(err)?;
    actual.remove(SIGNATURE).ok_or_else(err)?;
    let original_inventory = actual.clone();
    let mut last = "";
    let mut total = 0u64;
    let mut folded = BTreeSet::new();
    for entry in &manifest.files {
        if !safe_path(&entry.path)
            || entry.path.as_str() <= last
            || !folded.insert(entry.path.to_ascii_lowercase())
            || entry.size > MAX_FILE
            || !hex(&entry.sha256, 64)
        {
            return Err(err());
        }
        last = &entry.path;
        total = total.checked_add(entry.size).ok_or_else(err)?;
        if total > MAX_TOTAL {
            return Err(err());
        }
        let meta = actual.remove(&entry.path).ok_or_else(err)?;
        if meta.len() != entry.size || mode(&meta)? != entry.executable {
            return Err(err());
        }
        let path = root.join(&entry.path);
        let (mut file, before) = open_file(&path, MAX_FILE)?;
        if !same(&meta, &before) {
            return Err(err());
        }
        let mut hash = Sha256::new();
        let mut count = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            let n = file.read(&mut buffer).map_err(|_| err())?;
            if n == 0 {
                break;
            }
            count += n as u64;
            if count > entry.size {
                return Err(err());
            }
            hash.update(&buffer[..n]);
        }
        if count != entry.size || format!("{:x}", hash.finalize()) != entry.sha256 {
            return Err(err());
        }
        stable(&path, &file, &before)?;
    }
    if !actual.is_empty()
        || REQUIRED.iter().any(|path| {
            !manifest
                .files
                .iter()
                .any(|f| f.path == *path && (!path.starts_with("Contents/MacOS/") || f.executable))
        })
    {
        return Err(err());
    }
    for (relative, before) in original_inventory {
        if !same(
            &before,
            &fs::symlink_metadata(root.join(relative)).map_err(|_| err())?,
        ) {
            return Err(err());
        }
    }
    for (path, before) in dirs {
        if !same(&before, &fs::symlink_metadata(path).map_err(|_| err())?) {
            return Err(err());
        }
    }
    if bytes(&root.join(MANIFEST), MAX_METADATA)? != raw
        || bytes(&root.join(SIGNATURE), 64)? != signature
    {
        return Err(err());
    }
    Ok(VerifiedPackage {
        root: root.to_path_buf(),
        manifest,
        manifest_sha256: format!("{:x}", Sha256::digest(raw)),
    })
}

#[cfg(test)]
mod tests;
