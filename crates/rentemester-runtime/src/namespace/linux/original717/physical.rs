//! Fixed company resources only. Never opens a live SQLite inode.
use super::super::{backup_directory, backup_file_acl, process_profile};
use super::*;
use crate::backup_artifact::{
    self,
    original717::{self as codec, Files, VerifiedOriginal717},
};
use crate::namespace::NamespaceProfile;
use crate::{RootIdentity, WorkspaceLease};
use std::path::{Path, PathBuf};

fn names(dir: &File) -> Result<Vec<String>, Error> {
    let mut out = Vec::new();
    for e in rustix::fs::Dir::read_from(dir).map_err(|_| Error::Invalid)? {
        let e = e.map_err(|_| Error::Invalid)?;
        let n = e.file_name().to_str().map_err(|_| Error::Invalid)?;
        if n == "." || n == ".." {
            continue;
        }
        if out.len() >= backup_artifact::MAX_ENTRIES {
            return Err(Error::Invalid);
        }
        out.push(n.to_owned());
    }
    out.sort();
    Ok(out)
}
fn read_bounded(dir: &File, uid: u32, name: &str, max: usize) -> Result<Vec<u8>, Error> {
    let before = statat(dir, name, AtFlags::SYMLINK_NOFOLLOW).map_err(|_| Error::Invalid)?;
    if before.st_uid != uid
        || before.st_nlink != 1
        || before.st_mode & 0o7777 != 0o600
        || FileType::from_raw_mode(before.st_mode) != FileType::RegularFile
        || before.st_size < 0
        || before.st_size as u64 > max as u64
    {
        return Err(Error::Invalid);
    }
    let mut f = File::from(
        openat(
            dir,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(|_| Error::Invalid)?,
    );
    if identity(&fstat(&f).map_err(|_| Error::Invalid)?) != identity(&before) {
        return Err(Error::Changed);
    }
    backup_file_acl(&f)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut f)
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Invalid)?;
    let after = fstat(&f).map_err(|_| Error::Invalid)?;
    if identity(&after) != identity(&before)
        || before.st_size != after.st_size
        || bytes.len() as i64 != after.st_size
        || before.st_mtime != after.st_mtime
        || before.st_mtime_nsec != after.st_mtime_nsec
        || before.st_ctime != after.st_ctime
        || before.st_ctime_nsec != after.st_ctime_nsec
        || identity(&statat(dir, name, AtFlags::SYMLINK_NOFOLLOW).map_err(|_| Error::Changed)?)
            != identity(&before)
    {
        return Err(Error::Changed);
    }
    Ok(bytes)
}
fn inventory(dir: &File, uid: u32, prefix: &str, files: &mut Files) -> Result<(), Error> {
    if prefix.len() > 256 || prefix.split('/').count() > 32 {
        return Err(Error::Invalid);
    }
    for n in names(dir)? {
        let st = statat(dir, n.as_str(), AtFlags::SYMLINK_NOFOLLOW).map_err(|_| Error::Invalid)?;
        let path = format!("{prefix}{n}");
        if FileType::from_raw_mode(st.st_mode) == FileType::Directory {
            if st.st_uid != uid || st.st_mode & 0o7777 != 0o700 || path.len() > 256 {
                return Err(Error::Invalid);
            }
            let child = File::from(
                openat(dir, n.as_str(), DIRECTORY, Mode::empty()).map_err(|_| Error::Invalid)?,
            );
            acl_walk_fd(&child, false)?;
            if identity(&fstat(&child).map_err(|_| Error::Invalid)?) != identity(&st) {
                return Err(Error::Changed);
            }
            inventory(&child, uid, &format!("{path}/"), files)?;
        } else {
            let b = read_bounded(dir, uid, &n, backup_artifact::MAX_FILE_BYTES)?;
            files.insert(path, b);
            if files.len() > backup_artifact::MAX_ENTRIES
                || files.values().map(Vec::len).sum::<usize>() > backup_artifact::MAX_TOTAL_BYTES
            {
                return Err(Error::Invalid);
            }
        }
    }
    Ok(())
}
fn create_file(dir: &File, name: &str, bytes: &[u8]) -> Result<(), Error> {
    let mut f = File::from(
        openat(
            dir,
            name,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )
        .map_err(|_| Error::Exists)?,
    );
    f.write_all(bytes).map_err(|_| Error::Invalid)?;
    f.sync_all().map_err(|_| Error::Invalid)?;
    backup_file_acl(&f)
}
fn materialize(dir: &File, files: &Files) -> Result<(), Error> {
    let mut directories = std::collections::BTreeSet::new();
    for path in files.keys() {
        let mut prefix = String::new();
        let parts: Vec<_> = path.split('/').collect();
        for p in &parts[..parts.len() - 1] {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(p);
            if directories.insert(prefix.clone()) {
                mkdirat(dir, prefix.as_str(), Mode::from_raw_mode(0o700))
                    .map_err(|_| Error::Invalid)?;
            }
        }
        create_file(dir, path, &files[path])?;
    }
    for d in directories.iter().rev() {
        File::from(openat(dir, d.as_str(), DIRECTORY, Mode::empty()).map_err(|_| Error::Invalid)?)
            .sync_all()
            .map_err(|_| Error::Invalid)?;
    }
    dir.sync_all().map_err(|_| Error::Invalid)
}
fn id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && s != "."
        && s != ".."
}
impl NamespaceGuard {
    /// Exact original company inventory; credentials included in config under operator custody.
    /// Host supplies the separately admitted SQLite snapshot, never a DB pathname.
    pub fn snapshot_original717_inventory(&mut self) -> Result<Files, Error> {
        self.ensure_current()?;
        let mut files = Files::new();
        for (p, b) in self.snapshot_originals(false)? {
            files.insert(
                p.replacen("documents/originals/", "documents-originals/", 1),
                b,
            );
        }
        for (source, dest) in [
            ("invoices/issued", "invoices-issued/"),
            ("config", "config/"),
        ] {
            let path = self.root.join(&self.slug).join(source);
            let dir = backup_directory(&path, self.uid)?;
            inventory(&dir, self.uid, dest, &mut files)?;
        }
        self.ensure_current()?;
        Ok(files)
    }
    pub fn original717_manifest_key(&mut self) -> Result<zeroize::Zeroizing<Vec<u8>>, Error> {
        self.ensure_current()?;
        let directory = backup_directory(&self.root.join(&self.slug), self.uid)?;
        let name = ".backup-manifest.key";
        match statat(&directory, name, AtFlags::SYMLINK_NOFOLLOW) {
            Err(rustix::io::Errno::NOENT) => {
                let mut key = zeroize::Zeroizing::new([0u8; 32]);
                getrandom::fill(key.as_mut()).map_err(|_| Error::Invalid)?;
                let text = zeroize::Zeroizing::new(format!(
                    "{}\n",
                    key.iter().map(|b| format!("{b:02x}")).collect::<String>()
                ));
                // EXCL prevents replacement; partial failures are deliberately retained and refused.
                let token = crate::auth::new_token().map_err(|_| Error::Invalid)?;
                let temporary = format!(".original717-key-{}", token.as_str());
                create_file(&directory, &temporary, text.as_bytes())?;
                rustix::fs::renameat_with(
                    &directory,
                    temporary.as_str(),
                    &directory,
                    name,
                    rustix::fs::RenameFlags::NOREPLACE,
                )
                .map_err(|_| Error::Changed)?;
                directory.sync_all().map_err(|_| Error::Invalid)?;
            }
            Ok(_) => {}
            Err(_) => return Err(Error::Invalid),
        }
        let text = zeroize::Zeroizing::new(read_bounded(&directory, self.uid, name, 65)?);
        let key = codec::symmetric_key(std::str::from_utf8(&text).map_err(|_| Error::Invalid)?)
            .map_err(|_| Error::Invalid)?;
        self.ensure_current()?;
        Ok(zeroize::Zeroizing::new(key))
    }
    /// Retain just the source directory: system_backup defaults to archive:false.
    pub fn retain_original717_directory(
        &mut self,
        proof: &VerifiedOriginal717,
    ) -> Result<PathBuf, Error> {
        self.ensure_current()?;
        let name = &proof.manifest().backup_id;
        if !id(name) {
            return Err(Error::Invalid);
        }
        let backups = backup_directory(&self.root.join(&self.slug).join("backups"), self.uid)?;
        mkdirat(&backups, name.as_str(), Mode::from_raw_mode(0o700)).map_err(|_| Error::Exists)?;
        let dir = File::from(
            openat(&backups, name.as_str(), DIRECTORY, Mode::empty())
                .map_err(|_| Error::Invalid)?,
        );
        materialize(&dir, proof.files())?;
        backups.sync_all().map_err(|_| Error::Invalid)?;
        self.ensure_current()?;
        Ok(self.root.join(&self.slug).join("backups").join(name))
    }
    /// Explicit archive publication. Custom destinations require the existing trusted operator
    /// binding AND verified local transport/current authority at the calling backend boundary.
    /// Existing custom files are never opened: closing a SQLite alias could release its locks.
    pub fn pack_original717(
        &mut self,
        proof: &VerifiedOriginal717,
        out: Option<&Path>,
        operator: Option<&crate::bootstrap::Operator>,
    ) -> Result<PathBuf, Error> {
        self.ensure_current()?;
        let name = &proof.manifest().backup_id;
        if !id(name) {
            return Err(Error::Invalid);
        }
        let default = self
            .root
            .join(&self.slug)
            .join("backups")
            .join(format!("{name}.tar"));
        let target = out.unwrap_or(&default);
        if !target.is_absolute() {
            return Err(Error::Invalid);
        }
        let custom = target != default;
        if custom {
            if operator.is_none() {
                return Err(Error::Invalid);
            }
            // Even the local operator export cannot write into a live workspace's other resources.
            if target.starts_with(&self.root)
                && !target.starts_with(self.root.join(&self.slug).join("backups"))
                && !target.starts_with(self.root.join(&self.slug).join("exports"))
            {
                return Err(Error::Invalid);
            }
        }
        let leaf = target
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or(Error::Invalid)?;
        let sidecar = format!("{leaf}.sha256");
        let directory = backup_directory(target.parent().ok_or(Error::Invalid)?, self.uid)?;
        if custom {
            for file in [leaf, sidecar.as_str()] {
                match statat(&directory, file, AtFlags::SYMLINK_NOFOLLOW) {
                    Err(rustix::io::Errno::NOENT) => {}
                    _ => return Err(Error::Exists),
                }
            }
        }
        let archive = codec::archive(proof.files()).map_err(|_| Error::Invalid)?;
        let digest = format!("{}  {leaf}\n", codec::sha256(&archive));
        for (file, bytes) in [
            (leaf, archive.as_slice()),
            (sidecar.as_str(), digest.as_bytes()),
        ] {
            match statat(&directory, file, AtFlags::SYMLINK_NOFOLLOW) {
                Err(rustix::io::Errno::NOENT) => create_file(&directory, file, bytes)?,
                Ok(_) if !custom => {
                    // Only the fixed backup compartment supports exact-byte readback/reuse.
                    if read_bounded(&directory, self.uid, file, bytes.len())? != bytes {
                        return Err(Error::Exists);
                    }
                }
                _ => return Err(Error::Exists),
            }
        }
        directory.sync_all().map_err(|_| Error::Invalid)?;
        self.ensure_current()?;
        Ok(target.to_owned())
    }
    /// Existing convenience for a deliberately requested directory+archive bundle.
    pub fn retain_original717(&mut self, proof: &VerifiedOriginal717) -> Result<(), Error> {
        self.retain_original717_directory(proof)?;
        self.pack_original717(proof, None, None)?;
        Ok(())
    }
    /// None selects latest authenticated createdAt, not filesystem enumeration order.
    pub fn load_original717(
        &mut self,
        selected: Option<&str>,
        independent_public_key: Option<&[u8]>,
    ) -> Result<VerifiedOriginal717, Error> {
        if selected.is_some_and(|s| !id(s)) {
            return Err(Error::Invalid);
        }
        let key = self.original717_manifest_key()?;
        let backups = backup_directory(&self.root.join(&self.slug).join("backups"), self.uid)?;
        let candidates = if let Some(s) = selected {
            vec![s.to_owned()]
        } else {
            names(&backups)?
        };
        let mut latest: Option<VerifiedOriginal717> = None;
        for name in candidates {
            if !id(&name) {
                return Err(Error::Invalid);
            }
            let st = statat(&backups, name.as_str(), AtFlags::SYMLINK_NOFOLLOW)
                .map_err(|_| Error::Invalid)?;
            if FileType::from_raw_mode(st.st_mode) != FileType::Directory {
                if selected.is_some() {
                    return Err(Error::Invalid);
                }
                continue;
            }
            let dir = backup_directory(
                &self.root.join(&self.slug).join("backups").join(&name),
                self.uid,
            )?;
            let mut files = Files::new();
            inventory(&dir, self.uid, "", &mut files)?;
            let proof =
                codec::verify(&files, &key, independent_public_key).map_err(|_| Error::Invalid)?;
            if proof.manifest().backup_id != name {
                return Err(Error::Invalid);
            }
            if latest
                .as_ref()
                .is_none_or(|p| proof.manifest().created_at > p.manifest().created_at)
            {
                latest = Some(proof);
            }
        }
        self.ensure_current()?;
        latest.ok_or(Error::Invalid)
    }
}
/// Original company output only. No native manifest, control DB, adoption or live registration.
pub struct Original717RestoreStage {
    root: PathBuf,
    uid: u32,
    ancestors: Vec<RootIdentity>,
    directory: File,
    _lease: WorkspaceLease,
}
impl Original717RestoreStage {
    /// Host validates SQLite/business invariants in this retained private stage before publish.
    pub fn create(
        root: &Path,
        uid: u32,
        profile: NamespaceProfile,
        proof: &VerifiedOriginal717,
    ) -> Result<Self, Error> {
        process_profile(uid, profile)?;
        if !root.is_absolute()
            || root.components().any(|p| {
                matches!(
                    p,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                )
            })
        {
            return Err(Error::Invalid);
        }
        let parent = root.parent().ok_or(Error::Invalid)?;
        let (_, p) = walk(parent, uid, false)?;
        mkdirat(
            &p,
            root.file_name().ok_or(Error::Invalid)?,
            Mode::from_raw_mode(0o700),
        )
        .map_err(|_| Error::Exists)?;
        let (ancestors, directory) = walk(root, uid, true)?;
        let lease = WorkspaceLease::acquire(root).map_err(|_| Error::Busy)?;
        let mut files = Files::new();
        for (name, bytes) in proof.files() {
            let path = if name == "ledger.sqlite" {
                "data/ledger.sqlite".into()
            } else if let Some(s) = name.strip_prefix("documents-originals/") {
                format!("documents/originals/{s}")
            } else if let Some(s) = name.strip_prefix("invoices-issued/") {
                format!("invoices/issued/{s}")
            } else if name.starts_with("config/") {
                name.clone()
            } else {
                continue;
            };
            files.insert(path, bytes.clone());
        }
        materialize(&directory, &files)?;
        p.sync_all().map_err(|_| Error::Invalid)?;
        Ok(Self {
            root: root.to_owned(),
            uid,
            ancestors,
            directory,
            _lease: lease,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// Same-parent no-replace only. Existing targets (including empty) refuse; caller must
    /// explicitly remove an admitted empty destination before attempting publication.
    pub fn publish_into_empty(&mut self, target: &Path) -> Result<(), Error> {
        self._lease
            .ensure_current_root()
            .map_err(|_| Error::Changed)?;
        if walk(&self.root, self.uid, true)?.0 != self.ancestors {
            return Err(Error::Changed);
        }
        if target.parent() != self.root.parent() || target == self.root {
            return Err(Error::Invalid);
        }
        let (ids, dir) = walk(target, self.uid, true)?;
        if !names(&dir)?.is_empty() {
            return Err(Error::Exists);
        }
        let (_, parent) = walk(target.parent().ok_or(Error::Invalid)?, self.uid, false)?;
        let named = statat(
            &parent,
            target.file_name().ok_or(Error::Invalid)?,
            AtFlags::SYMLINK_NOFOLLOW,
        )
        .map_err(|_| Error::Changed)?;
        if ids.last() != Some(&identity(&named)) {
            return Err(Error::Changed);
        }
        // RMDIR refuses a concurrently populated directory. Publication still uses NOREPLACE,
        // so a newly created destination in the gap is never overwritten.
        unlinkat(
            &parent,
            target.file_name().ok_or(Error::Invalid)?,
            AtFlags::REMOVEDIR,
        )
        .map_err(|_| Error::Changed)?;
        parent.sync_all().map_err(|_| Error::Changed)?;
        self.publish(target)
    }
    pub fn publish(&mut self, target: &Path) -> Result<(), Error> {
        self._lease
            .ensure_current_root()
            .map_err(|_| Error::Changed)?;
        let (ids, _) = walk(&self.root, self.uid, true)?;
        if ids != self.ancestors || target.parent() != self.root.parent() || target == self.root {
            return Err(Error::Changed);
        }
        let (_, p) = walk(target.parent().ok_or(Error::Invalid)?, self.uid, false)?;
        self.directory.sync_all().map_err(|_| Error::Invalid)?;
        rustix::fs::renameat_with(
            &p,
            self.root.file_name().ok_or(Error::Invalid)?,
            &p,
            target.file_name().ok_or(Error::Invalid)?,
            rustix::fs::RenameFlags::NOREPLACE,
        )
        .map_err(|_| Error::Exists)?;
        p.sync_all().map_err(|_| Error::Changed)?;
        self._lease
            .relocate_restore_root(target)
            .map_err(|_| Error::Changed)?;
        self.root = target.to_owned();
        Ok(())
    }
}
#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    #[test]
    #[ignore = "qualified subprocess"]
    fn original717_child() {
        let base = PathBuf::from(std::env::var("RM_ORIGINAL717_ROOT").unwrap());
        let uid =
            libproc::proc_pid::pidinfo::<libproc::bsd_info::BSDInfo>(std::process::id() as i32, 0)
                .unwrap()
                .pbi_uid;
        let profile = NamespaceProfile::LocalUserMacos;
        let root = base.join("workspace");
        let mut guard = NamespaceGuard::create_fresh_with_profile(
            &root,
            uid,
            "w",
            "a",
            "Synthetic",
            "2026-01-01",
            profile,
        )
        .unwrap();
        guard.write_digisense_config(b"synthetic secret").unwrap();
        assert_eq!(
            guard.read_digisense_config().unwrap().unwrap(),
            b"synthetic secret"
        );
        assert!(guard.read_smtp_config().unwrap().is_none());
        let key = guard.original717_manifest_key().unwrap();
        assert_eq!(*key, *guard.original717_manifest_key().unwrap());
        let (mut files, _) = codec::tests::fixture();
        let text = &files["manifest.json"];
        files.insert(
            "manifest.json.hmac".into(),
            codec::sign(text, &key).unwrap().into_bytes(),
        );
        // Bind fixture key hint to the admitted key, preserving exact signed text thereafter.
        let mut m: codec::Manifest = serde_json::from_slice(&files["manifest.json"]).unwrap();
        m.manifest_signature.key_hint = codec::sha256(&key)[..16].into();
        let text = serde_json::to_vec_pretty(&m).unwrap();
        files.insert(
            "manifest.json.hmac".into(),
            codec::sign(&text, &key).unwrap().into_bytes(),
        );
        files.insert("manifest.json".into(), text);
        let proof = codec::verify(&files, &key, None).unwrap();
        let retained = guard.retain_original717_directory(&proof).unwrap();
        let tar = retained.with_extension("tar");
        assert!(!tar.exists(), "archive:false must not create a TAR");
        assert!(!tar.with_extension("tar.sha256").exists());
        assert_eq!(guard.load_original717(None, None).unwrap().files(), &files);
        assert_eq!(guard.pack_original717(&proof, None, None).unwrap(), tar);
        let bytes = std::fs::read(&tar).unwrap();
        assert_eq!(codec::read_archive(&bytes).unwrap(), files);
        guard.pack_original717(&proof, None, None).unwrap();
        assert_eq!(std::fs::read(&tar).unwrap(), bytes);
        let exported = base.join("exported.tar");
        assert!(
            guard
                .pack_original717(&proof, Some(&exported), None)
                .is_err()
        );
        assert!(!exported.exists());
        let operator = crate::bootstrap::Operator::from_policy(
            "w",
            "user:operator",
            &["user:operator".into()],
        )
        .unwrap();
        guard
            .pack_original717(&proof, Some(&exported), Some(&operator))
            .unwrap();
        assert_eq!(std::fs::read(&exported).unwrap(), bytes);
        assert!(
            guard
                .pack_original717(&proof, Some(&exported), Some(&operator))
                .is_err()
        );
        let protected = root.join("a/data/forbidden.tar");
        assert!(
            guard
                .pack_original717(&proof, Some(&protected), Some(&operator))
                .is_err()
        );
        assert!(!protected.exists());
        assert!(guard.retain_original717(&proof).is_err());
        assert_eq!(guard.load_original717(None, None).unwrap().files(), &files);
        assert!(guard.load_original717(Some("../bad"), None).is_err());
        let inv = guard.snapshot_original717_inventory().unwrap();
        assert_eq!(inv["config/digisense.json"], b"synthetic secret");
        let stage = base.join("stage");
        let mut output = Original717RestoreStage::create(&stage, uid, profile, &proof).unwrap();
        assert!(!stage.join("workspace.json").exists());
        assert!(!stage.join(".rentemester/workspace-control.sqlite").exists());
        assert_eq!(
            std::fs::read(stage.join("data/ledger.sqlite")).unwrap(),
            files["ledger.sqlite"]
        );
        let occupied = base.join("occupied");
        std::fs::create_dir(&occupied).unwrap();
        std::fs::write(occupied.join("keep"), b"keep").unwrap();
        assert!(output.publish_into_empty(&occupied).is_err());
        assert!(stage.exists());
        let empty = base.join("empty");
        std::fs::create_dir(&empty).unwrap();
        assert!(output.publish(&empty).is_err());
        output.publish_into_empty(&empty).unwrap();
        assert!(!stage.exists());
        assert!(empty.join("data/ledger.sqlite").exists());
        let mut fresh =
            Original717RestoreStage::create(&base.join("stage2"), uid, profile, &proof).unwrap();
        fresh.publish(&base.join("fresh")).unwrap();
        let key_path = root.join("a/.backup-manifest.key");
        std::fs::hard_link(&key_path, base.join("alias")).unwrap();
        assert!(guard.original717_manifest_key().is_err());
    }
    #[test]
    fn original717_admitted_inventory_retention_key_and_company_restore() {
        let base = tempfile::tempdir().unwrap();
        let path = base.path().canonicalize().unwrap();
        let output = std::process::Command::new("/bin/sh")
            .args(["-c", "umask 077; exec \"$@\"", "original717-test"])
            .arg(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "namespace::linux::original717::physical::tests::original717_child",
                "--nocapture",
            ])
            .env("RM_ORIGINAL717_ROOT", path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
#[cfg(test)]
mod bounded_tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    #[test]
    fn original717_inventory_refuses_oversize_links_and_nonprivate_members() {
        let temp = tempfile::tempdir().unwrap();
        let dir = File::open(temp.path()).unwrap();
        let uid = dir.metadata().unwrap().uid();
        create_file(&dir, "member", b"synthetic").unwrap();
        assert!(read_bounded(&dir, uid, "member", 2).is_err());
        std::fs::hard_link(temp.path().join("member"), temp.path().join("alias")).unwrap();
        assert!(inventory(&dir, uid, "config/", &mut Files::new()).is_err());
        std::fs::remove_file(temp.path().join("alias")).unwrap();
        std::fs::set_permissions(
            temp.path().join("member"),
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert!(inventory(&dir, uid, "config/", &mut Files::new()).is_err());
        assert!(!id("../other"));
        assert!(!id("latest/other"));
    }
}

/// Fixed-resource read proof. Paths come exclusively from the two admitted SQLite handles.
/// Deliberately does not acquire another lease or open either live SQLite inode.
pub(crate) fn observe_retained(
    control: &Path,
    company: &Path,
    slug: &str,
) -> Result<Vec<VerifiedOriginal717>, Error> {
    if !id(slug)
        || slug.contains('.')
        || control.file_name().and_then(|s| s.to_str()) != Some("workspace-control.sqlite")
        || control
            .parent()
            .and_then(Path::file_name)
            .and_then(|s| s.to_str())
            != Some(".rentemester")
    {
        return Err(Error::Invalid);
    }
    let root = control
        .parent()
        .and_then(Path::parent)
        .ok_or(Error::Invalid)?;
    if company != root.join(slug).join("data/ledger.sqlite") {
        return Err(Error::Invalid);
    }
    let uid = rustix::process::geteuid().as_raw();
    #[cfg(target_os = "macos")]
    process_profile(uid, NamespaceProfile::LocalUserMacos)?;
    #[cfg(target_os = "linux")]
    process_profile(uid, NamespaceProfile::DedicatedLinux)?;
    let directory = backup_directory(&root.join(slug), uid)?;
    let backups = backup_directory(&root.join(slug).join("backups"), uid)?;
    let candidates = names(&backups)?;
    let mut out = Vec::new();
    // An empty backup directory requires no key and observation never creates one.
    if candidates.is_empty() {
        return Ok(out);
    }
    let text = zeroize::Zeroizing::new(read_bounded(&directory, uid, ".backup-manifest.key", 65)?);
    let key = zeroize::Zeroizing::new(
        codec::symmetric_key(std::str::from_utf8(&text).map_err(|_| Error::Invalid)?)
            .map_err(|_| Error::Invalid)?,
    );
    for name in &candidates {
        let st = statat(&backups, name.as_str(), AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|_| Error::Invalid)?;
        if FileType::from_raw_mode(st.st_mode) != FileType::Directory {
            continue;
        }
        if !id(name) {
            return Err(Error::Invalid);
        }
        let dir = backup_directory(&root.join(slug).join("backups").join(name), uid)?;
        let mut files = Files::new();
        inventory(&dir, uid, "", &mut files)?;
        let proof = codec::verify(&files, &key, None).map_err(|_| Error::Invalid)?;
        if proof.manifest().backup_id != *name {
            return Err(Error::Invalid);
        }
        out.push(proof);
    }
    if names(&backups)? != candidates {
        return Err(Error::Changed);
    }
    Ok(out)
}
