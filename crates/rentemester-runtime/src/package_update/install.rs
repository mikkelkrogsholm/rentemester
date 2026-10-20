//! Same-filesystem publication with a running-package lease and atomic exchange.
use super::{Error, Policy, VerifiedPackage, verify};
use rustix::{
    fs::{self as rfs, FlockOperation, Mode, OFlags, RenameFlags},
    process::getuid,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Component, Path, PathBuf},
};
const MANIFEST: &str = "Contents/Resources/native-update.json";
const SIGNATURE: &str = "Contents/Resources/native-update.sig";
fn io<T>(result: std::io::Result<T>) -> Result<T, Error> {
    result.map_err(|_| Error("update_io_refused"))
}
fn sync(path: &Path) -> Result<(), Error> {
    io(io(File::open(path))?.sync_all())
}
fn metadata(path: &Path) -> Result<fs::Metadata, Error> {
    let m = io(fs::symlink_metadata(path))?;
    if m.file_type().is_symlink() || m.uid() != getuid().as_raw() || m.mode() & 0o022 != 0 {
        return Err(Error("update_namespace_refused"));
    }
    Ok(m)
}
fn regular(path: &Path) -> Result<fs::Metadata, Error> {
    let m = metadata(path)?;
    if !m.is_file() || m.nlink() != 1 {
        return Err(Error("update_namespace_refused"));
    }
    Ok(m)
}
fn checked_target(target: &Path) -> Result<PathBuf, Error> {
    if getuid().is_root()
        || !target.is_absolute()
        || target
            .components()
            .any(|c| !matches!(c, Component::RootDir | Component::Normal(_)))
    {
        return Err(Error("update_target_refused"));
    }
    let name = target
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or(Error("update_target_refused"))?;
    if !name.ends_with(".app") || name.starts_with('.') {
        return Err(Error("update_target_refused"));
    }
    let parent = target.parent().ok_or(Error("update_target_refused"))?;
    if io(parent.canonicalize())? != parent
        || !metadata(parent)?.is_dir()
        || !metadata(target)?.is_dir()
    {
        return Err(Error("update_namespace_refused"));
    }
    Ok(parent.to_owned())
}
fn sibling(target: &Path, suffix: &str) -> Result<PathBuf, Error> {
    let name = target
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or(Error("update_target_refused"))?;
    Ok(target.with_file_name(format!(".{name}.{suffix}")))
}
/// Shared for a running app, exclusive for publication. The same lock inode is
/// retained across launches; it is never removed or replaced by the updater.
pub struct PackageLease {
    file: File,
    path: PathBuf,
    identity: (u64, u64),
    parent: PathBuf,
    parent_identity: (u64, u64),
}
impl PackageLease {
    pub fn running(target: &Path) -> Result<Self, Error> {
        Self::acquire(target, false)
    }
    fn acquire(target: &Path, exclusive: bool) -> Result<Self, Error> {
        checked_target(target)?;
        let path = sibling(target, "update-lock")?;
        let fd = rfs::open(
            &path,
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(|_| Error("update_lock_refused"))?;
        let file: File = fd.into();
        let m = regular(&path)?;
        let fm = io(file.metadata())?;
        if (m.dev(), m.ino()) != (fm.dev(), fm.ino()) || m.mode() & 0o077 != 0 {
            return Err(Error("update_lock_refused"));
        }
        let operation = if exclusive {
            FlockOperation::NonBlockingLockExclusive
        } else {
            FlockOperation::NonBlockingLockShared
        };
        rfs::flock(&file, operation).map_err(|_| Error("application_running_or_update_busy"))?;
        let parent = target
            .parent()
            .ok_or(Error("update_target_refused"))?
            .to_owned();
        let pm = metadata(&parent)?;
        Ok(Self {
            file,
            path,
            identity: (m.dev(), m.ino()),
            parent,
            parent_identity: (pm.dev(), pm.ino()),
        })
    }
    fn ensure(&self) -> Result<(), Error> {
        let m = regular(&self.path)?;
        let f = io(self.file.metadata())?;
        let pm = metadata(&self.parent)?;
        if (pm.dev(), pm.ino()) != self.parent_identity {
            return Err(Error("update_namespace_changed"));
        }
        if self.identity != (m.dev(), m.ino()) || self.identity != (f.dev(), f.ino()) {
            return Err(Error("update_lock_changed"));
        }
        Ok(())
    }
}
/// Development executables outside a bundle do not create an installation lock.
pub fn current_executable_lease() -> Result<Option<PackageLease>, Error> {
    lease_for_executable(&io(std::env::current_exe())?)
}
fn lease_for_executable(exe: &Path) -> Result<Option<PackageLease>, Error> {
    let Some(macos) = exe
        .parent()
        .filter(|p| p.file_name().is_some_and(|v| v == "MacOS"))
    else {
        return Ok(None);
    };
    let Some(contents) = macos
        .parent()
        .filter(|p| p.file_name().is_some_and(|v| v == "Contents"))
    else {
        return Ok(None);
    };
    let target = contents.parent().ok_or(Error("update_target_refused"))?;
    let name = target
        .file_name()
        .and_then(|v| v.to_str())
        .ok_or(Error("update_target_refused"))?;
    if name.ends_with(".update-previous")
        || target
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|v| v.to_str())
            .is_some_and(|v| v.ends_with(".update-stage"))
    {
        return Err(Error("update_package_not_active"));
    }
    if target.extension().is_none_or(|v| v != "app") {
        return Ok(None);
    }
    PackageLease::running(target).map(Some)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Transaction {
    format: String,
    target: String,
    from_sha256: String,
    to_sha256: String,
}
fn read_tx(container: &Path, target: &Path) -> Result<Transaction, Error> {
    if !metadata(container)?.is_dir() || metadata(container)?.mode() & 0o077 != 0 {
        return Err(Error("update_stage_refused"));
    }
    let path = container.join("transaction.json");
    if regular(&path)?.len() > 16384 {
        return Err(Error("update_stage_refused"));
    }
    let tx: Transaction =
        serde_json::from_slice(&io(fs::read(&path))?).map_err(|_| Error("update_stage_refused"))?;
    if tx.format != "rentemester-update-transaction/1"
        || Path::new(&tx.target) != target
        || tx.from_sha256.len() != 64
        || tx.to_sha256.len() != 64
        || tx.from_sha256 == tx.to_sha256
    {
        return Err(Error("update_stage_refused"));
    }
    for entry in io(fs::read_dir(container))? {
        let name = io(entry)?.file_name();
        if name != "transaction.json" && name != "candidate.app" {
            return Err(Error("update_stage_refused"));
        }
    }
    Ok(tx)
}
fn version(version: &str) -> Result<Vec<u64>, Error> {
    version
        .split('.')
        .map(|v| v.parse().map_err(|_| Error("update_version_refused")))
        .collect()
}
fn newer(old: &VerifiedPackage, new: &VerifiedPackage) -> Result<(), Error> {
    if new.manifest().sequence <= old.manifest().sequence
        || version(&new.manifest().version)? <= version(&old.manifest().version)?
    {
        return Err(Error("update_downgrade_refused"));
    }
    Ok(())
}
fn remove_private_tree(root: &Path) -> Result<(), Error> {
    // Only called on a verified old package or our private staged transaction;
    // reject links/foreign owners/irregular entries before any recursive removal.
    fn visit(root: &Path, count: &mut u64) -> Result<(), Error> {
        *count += 1;
        if *count > 40050 {
            return Err(Error("update_stage_refused"));
        }
        let m = metadata(root)?;
        if m.is_dir() {
            for e in io(fs::read_dir(root))? {
                visit(&io(e)?.path(), count)?;
            }
        } else if !m.is_file() || m.nlink() != 1 {
            return Err(Error("update_stage_refused"));
        }
        Ok(())
    }
    visit(root, &mut 0)?;
    io(fs::remove_dir_all(root))
}
fn copy_file(source: &Path, dest: &Path, executable: bool, size: u64) -> Result<(), Error> {
    if regular(source)?.len() != size || size > 512 * 1024 * 1024 {
        return Err(Error("update_source_changed"));
    }
    let source = io(OpenOptions::new()
        .read(true)
        .custom_flags(OFlags::NOFOLLOW.bits() as i32)
        .open(source))?;
    let mut output = io(OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(if executable { 0o755 } else { 0o644 })
        .open(dest))?;
    let mut limited = source.take(size + 1);
    if io(std::io::copy(&mut limited, &mut output))? != size {
        return Err(Error("update_source_changed"));
    }
    io(output.sync_all())
}
fn stage(
    candidate: &VerifiedPackage,
    container: &Path,
    tx: &Transaction,
) -> Result<PathBuf, Error> {
    io(fs::DirBuilder::new().mode(0o700).create(container))?;
    let mut file = io(OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(container.join("transaction.json")))?;
    io(file.write_all(&serde_json::to_vec(tx).map_err(|_| Error("update_stage_refused"))?))?;
    io(file.sync_all())?;
    sync(container)?;
    sync(container.parent().ok_or(Error("update_stage_refused"))?)?;
    let target = container.join("candidate.app");
    io(fs::create_dir(&target))?;
    for (path, exec, size) in [
        (
            MANIFEST,
            false,
            regular(&candidate.root().join(MANIFEST))?.len(),
        ),
        (SIGNATURE, false, 64),
    ]
    .into_iter()
    .chain(
        candidate
            .manifest()
            .files
            .iter()
            .map(|e| (e.path.as_str(), e.executable, e.size)),
    ) {
        let dest = target.join(path);
        io(fs::create_dir_all(
            dest.parent().ok_or(Error("update_stage_refused"))?,
        ))?;
        copy_file(&candidate.root().join(path), &dest, exec, size)?;
    }
    fn dirs(root: &Path) -> Result<(), Error> {
        for entry in io(fs::read_dir(root))? {
            let p = io(entry)?.path();
            if metadata(&p)?.is_dir() {
                dirs(&p)?;
            }
        }
        sync(root)
    }
    dirs(&target)?;
    sync(container)?;
    Ok(target)
}
fn finish(
    container: &Path,
    target: &Path,
    key: &[u8],
    policy: &Policy,
    tx: &Transaction,
) -> Result<Value, Error> {
    let current = verify(target, key, policy)?;
    if current.manifest_sha256() != tx.to_sha256 {
        return Err(Error("update_outcome_unknown"));
    }
    let previous = sibling(target, "update-previous")?;
    let staged = container.join("candidate.app");
    if staged.exists() {
        let old = verify(&staged, key, policy)?;
        if old.manifest_sha256() != tx.from_sha256 {
            return Err(Error("update_outcome_unknown"));
        }
        newer(&old, &current)?;
        if previous.exists() {
            return Err(Error("update_previous_exists"));
        }
        io(fs::rename(old.root(), &previous))?;
        sync(container)?;
        sync(target.parent().ok_or(Error("update_target_refused"))?)?;
    } else {
        let old = verify(&previous, key, policy)?;
        if old.manifest_sha256() != tx.from_sha256 {
            return Err(Error("update_outcome_unknown"));
        }
        newer(&old, &current)?;
    }
    io(fs::remove_file(container.join("transaction.json")))?;
    io(fs::remove_dir(container))?;
    sync(target.parent().ok_or(Error("update_target_refused"))?)?;
    Ok(
        json!({"ok":true,"state":"installed","version":current.manifest().version,"sequence":current.manifest().sequence,"manifestSha256":current.manifest_sha256(),"previousPackage":previous,"workspaceOpened":false,"migrationPerformed":false}),
    )
}
pub fn status(target: &Path, key: &[u8], policy: &Policy) -> Result<Value, Error> {
    checked_target(target)?;
    let current = verify(target, key, policy)?;
    let stage = sibling(target, "update-stage")?;
    let previous = sibling(target, "update-previous")?;
    Ok(
        json!({"ok":true,"version":current.manifest().version,"sequence":current.manifest().sequence,"manifestSha256":current.manifest_sha256(),"pendingRecovery":stage.exists(),"previousPackageRetained":previous.exists(),"workspaceOpened":false}),
    )
}
pub fn install(
    candidate: &Path,
    target: &Path,
    key: &[u8],
    policy: &Policy,
) -> Result<Value, Error> {
    let lease = PackageLease::acquire(target, true)?;
    let parent = checked_target(target)?;
    let old = verify(target, key, policy)?;
    let new = verify(candidate, key, policy)?;
    newer(&old, &new)?;
    if new.root().starts_with(target)
        || target.starts_with(new.root())
        || new.root().starts_with(&parent) && new.root() == sibling(target, "update-previous")?
    {
        return Err(Error("update_source_refused"));
    }
    let container = sibling(target, "update-stage")?;
    if container.exists() {
        return Err(Error("update_recovery_required"));
    }
    let previous = sibling(target, "update-previous")?;
    if previous.exists() {
        let p = verify(&previous, key, policy)?;
        newer(&p, &old)?;
        remove_private_tree(&previous)?;
        sync(&parent)?;
    }
    let tx = Transaction {
        format: "rentemester-update-transaction/1".into(),
        target: target.to_string_lossy().into_owned(),
        from_sha256: old.manifest_sha256().into(),
        to_sha256: new.manifest_sha256().into(),
    };
    let staged = stage(&new, &container, &tx)?;
    let checked = verify(&staged, key, policy)?;
    if checked.manifest_sha256() != new.manifest_sha256() {
        return Err(Error("update_source_changed"));
    }
    lease.ensure()?;
    if verify(target, key, policy)?.manifest_sha256() != old.manifest_sha256() {
        return Err(Error("update_target_changed"));
    }
    // Both complete directories remain present throughout publication. There is
    // no rename-old-away gap in which the installation path is missing.
    rfs::renameat_with(rfs::CWD, &staged, rfs::CWD, target, RenameFlags::EXCHANGE)
        .map_err(|_| Error("update_exchange_refused"))?;
    sync(&container)
        .and_then(|_| sync(&parent))
        .map_err(|_| Error("update_outcome_unknown"))?;
    finish(&container, target, key, policy, &tx).map_err(|_| Error("update_outcome_unknown"))
}
pub fn recover(target: &Path, key: &[u8], policy: &Policy) -> Result<Value, Error> {
    let lease = PackageLease::acquire(target, true)?;
    let parent = checked_target(target)?;
    let container = sibling(target, "update-stage")?;
    let current = verify(target, key, policy)?;
    lease.ensure()?;
    if !metadata(&container)?.is_dir() || metadata(&container)?.mode() & 0o077 != 0 {
        return Err(Error("update_stage_refused"));
    }
    if !container.join("candidate.app").exists() {
        // A crash can leave an empty container before transaction creation, or
        // after the old package was retained but before final directory removal.
        for e in io(fs::read_dir(&container))? {
            let p = io(e)?.path();
            if p.file_name().is_none_or(|v| v != "transaction.json") || regular(&p)?.len() > 16384 {
                return Err(Error("update_stage_refused"));
            }
        }
        let previous = sibling(target, "update-previous")?;
        let published = if previous.exists() {
            let old = verify(&previous, key, policy)?;
            newer(&old, &current)?;
            true
        } else {
            false
        };
        remove_private_tree(&container)?;
        sync(&parent)?;
        return Ok(
            json!({"ok":true,"state":if published{"installed"}else{"staging_discarded"},"version":current.manifest().version,"sequence":current.manifest().sequence,"workspaceOpened":false,"migrationPerformed":false}),
        );
    }
    let tx = read_tx(&container, target)?;
    if current.manifest_sha256() == tx.to_sha256 {
        return finish(&container, target, key, policy, &tx);
    }
    if current.manifest_sha256() != tx.from_sha256 {
        return Err(Error("update_outcome_unknown"));
    }
    // Publication never happened. Discard only the owned private transaction;
    // the previous runnable installation stays exactly where it was.
    remove_private_tree(&container)?;
    sync(&parent)?;
    Ok(
        json!({"ok":true,"state":"staging_discarded","version":current.manifest().version,"sequence":current.manifest().sequence,"workspaceOpened":false,"migrationPerformed":false}),
    )
}
#[cfg(test)]
mod tests;
