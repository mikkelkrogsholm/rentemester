//! Private same-user socket publication, after the server owns the workspace lease.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::{Component, Path, PathBuf},
    time::Duration,
};
use tokio::net::{UnixListener, UnixStream};
fn refused(message: &'static str) -> io::Error {
    io::Error::other(message)
}
#[derive(Clone)]
pub(crate) struct Identity {
    pub config: PathBuf,
    pub root: PathBuf,
    pub workspace: String,
    pub uid: u32,
}
#[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Descriptor {
    format: String,
    origin: String,
    socket: String,
    workspace_root: PathBuf,
    workspace_id: String,
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct FileId {
    device: u64,
    inode: u64,
}
fn id(m: &fs::Metadata) -> FileId {
    FileId {
        device: m.dev(),
        inode: m.ino(),
    }
}
fn metadata(path: &Path) -> io::Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(m) => Ok(Some(m)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
fn origin(value: &str) -> io::Result<()> {
    let url = url::Url::parse(value).map_err(|_| refused("Invalid local runtime origin"))?;
    if url.scheme() != "http"
        || !url
            .host_str()
            .and_then(|v| v.trim_matches(['[', ']']).parse::<std::net::IpAddr>().ok())
            .is_some_and(|ip| ip.is_loopback())
        || url.port_or_known_default().is_none_or(|p| p == 0)
        || url.origin().ascii_serialization() != value
    {
        return Err(refused(
            "Local runtime requires a canonical literal-loopback HTTP origin",
        ));
    }
    Ok(())
}
/// Preflight before workspace creation or admission. Actual UID is proved by Backend admission.
pub(crate) fn preflight(profile: &Path, config: &Path) -> io::Result<()> {
    if !profile.is_absolute()
        || profile
            .components()
            .any(|c| !matches!(c, Component::RootDir | Component::Normal(_)))
        || profile.to_string_lossy().chars().any(char::is_control)
        || profile.canonicalize()? != profile
    {
        return Err(refused(
            "Local profile must be an absolute canonical directory",
        ));
    }
    if profile.join("r.sock").as_os_str().as_bytes().len() > 103 {
        return Err(refused("Local runtime socket pathname exceeds 103 bytes"));
    }
    let m = fs::symlink_metadata(profile)?;
    if !m.is_dir() || m.file_type().is_symlink() || m.mode() & 0o7777 != 0o700 {
        return Err(refused("Local profile requires private0700 directory"));
    }
    let c = fs::symlink_metadata(config)?;
    if config.parent() != Some(profile)
        || config.canonicalize()? != config
        || !c.is_file()
        || c.file_type().is_symlink()
        || c.mode() & 0o7777 != 0o600
        || c.uid() != m.uid()
        || c.nlink() != 1
    {
        return Err(refused(
            "Local config must be a private single-link0600 file in the selected profile",
        ));
    }
    Ok(())
}
fn private(m: &fs::Metadata, uid: u32, socket: bool) -> io::Result<()> {
    if m.uid() != uid
        || m.mode() & 0o7777 != 0o600
        || m.nlink() != 1
        || m.file_type().is_symlink()
        || if socket {
            !m.file_type().is_socket()
        } else {
            !m.is_file()
        }
    {
        return Err(refused(
            "Local runtime entry has unsafe type, owner or mode",
        ));
    }
    Ok(())
}
fn descriptor(path: &Path, uid: u32) -> io::Result<(Descriptor, FileId)> {
    let before = fs::symlink_metadata(path)?;
    private(&before, uid, false)?;
    if before.len() == 0 || before.len() > 4096 {
        return Err(refused("Local runtime descriptor exceeds bounds"));
    }
    let file = File::open(path)?;
    if id(&file.metadata()?) != id(&before) {
        return Err(refused("Local runtime descriptor changed"));
    }
    let mut bytes = vec![];
    file.take(4097).read_to_end(&mut bytes)?;
    let after = fs::symlink_metadata(path)?;
    private(&after, uid, false)?;
    if id(&after) != id(&before)
        || after.len() != before.len()
        || bytes.len() as u64 != before.len()
    {
        return Err(refused("Local runtime descriptor changed"));
    }
    let value =
        serde_json::from_slice(&bytes).map_err(|_| refused("Invalid local runtime descriptor"))?;
    Ok((value, id(&before)))
}
fn remove_same(path: &Path, expected: FileId, uid: u32, socket: bool) -> io::Result<()> {
    let m = fs::symlink_metadata(path)?;
    private(&m, uid, socket)?;
    if id(&m) != expected {
        return Err(refused("Local runtime entry replaced; preserving it"));
    }
    fs::remove_file(path)
}
pub(crate) struct Runtime {
    listener: Option<UnixListener>,
    profile: PathBuf,
    directory: File,
    directory_id: FileId,
    uid: u32,
    socket: Option<FileId>,
    descriptor: Option<FileId>,
}
impl Runtime {
    pub(crate) async fn bind(
        profile: &Path,
        identity: &Identity,
        current_origin: &str,
    ) -> io::Result<Self> {
        preflight(profile, &identity.config)?;
        origin(current_origin)?;
        let directory = File::open(profile)?;
        let dm = directory.metadata()?;
        if dm.uid() != identity.uid
            || identity.root != profile.join("workspace")
            || identity.root.canonicalize()? != identity.root
            || identity.workspace.is_empty()
        {
            return Err(refused(
                "Admitted local runtime identity differs from profile",
            ));
        }
        let sock = profile.join("r.sock");
        let record = profile.join("r.json");
        let old_record = metadata(&record)?;
        let old_socket = metadata(&sock)?;
        if let Some(socket) = &old_socket {
            private(socket, identity.uid, true)?;
        }
        if old_record.is_some() {
            let (old, record_id) = descriptor(&record, identity.uid)?;
            if old.format != "rentemester-local-runtime/1"
                || old.socket != "r.sock"
                || old.workspace_root != identity.root
                || old.workspace_id != identity.workspace
            {
                return Err(refused(
                    "Stale runtime descriptor does not match admitted workspace",
                ));
            }
            origin(&old.origin)?;
            if let Some(socket) = old_socket {
                match tokio::time::timeout(Duration::from_millis(250), UnixStream::connect(&sock))
                    .await
                {
                    Ok(Err(e))
                        if matches!(
                            e.kind(),
                            io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
                        ) => {}
                    _ => {
                        return Err(refused(
                            "Existing local runtime socket is active or unavailable; preserving it",
                        ));
                    }
                }
                // The exclusive workspace lease is already held by this Backend.
                if id(&fs::symlink_metadata(profile)?) != id(&dm) {
                    return Err(refused("Local runtime profile changed"));
                }
                remove_same(&sock, id(&socket), identity.uid, true)?;
            }
            remove_same(&record, record_id, identity.uid, false)?;
        } else if old_socket.is_some() {
            return Err(refused(
                "Socket without matching private descriptor; preserving it",
            ));
        }
        let mut runtime = Self {
            listener: None,
            profile: profile.into(),
            directory,
            directory_id: id(&dm),
            uid: identity.uid,
            socket: None,
            descriptor: None,
        };
        runtime.ensure_profile()?;
        let listener = UnixListener::bind(&sock)?;
        let created = fs::symlink_metadata(&sock)?;
        runtime.socket = Some(id(&created));
        runtime.listener = Some(listener);
        fs::set_permissions(&sock, fs::Permissions::from_mode(0o600))?;
        private(&fs::symlink_metadata(&sock)?, identity.uid, true)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&record)?;
        runtime.descriptor = Some(id(&file.metadata()?));
        let value = Descriptor {
            format: "rentemester-local-runtime/1".into(),
            origin: current_origin.into(),
            socket: "r.sock".into(),
            workspace_root: identity.root.clone(),
            workspace_id: identity.workspace.clone(),
        };
        let mut bytes = serde_json::to_vec(&value)
            .map_err(|_| refused("Local descriptor serialization failed"))?;
        bytes.push(b'\n');
        if bytes.len() > 4096 {
            return Err(refused("Local runtime descriptor exceeds bounds"));
        }
        file.write_all(&bytes)?;
        file.sync_all()?;
        runtime.directory.sync_all()?;
        runtime.ensure_profile()?;
        Ok(runtime)
    }
    fn ensure_profile(&self) -> io::Result<()> {
        let current = fs::symlink_metadata(&self.profile)?;
        if id(&current) != self.directory_id
            || id(&self.directory.metadata()?) != self.directory_id
            || current.uid() != self.uid
            || current.mode() & 0o7777 != 0o700
            || !current.is_dir()
            || current.file_type().is_symlink()
        {
            return Err(refused("Local runtime profile changed"));
        }
        Ok(())
    }
    pub(crate) fn take_listener(&mut self) -> io::Result<UnixListener> {
        self.ensure_profile()?;
        self.listener
            .take()
            .ok_or_else(|| refused("Local listener already consumed"))
    }
    pub(crate) fn cleanup(&mut self) -> io::Result<()> {
        self.listener.take();
        self.ensure_profile()?;
        if let Some(expected) = self.descriptor {
            remove_same(&self.profile.join("r.json"), expected, self.uid, false)?;
            self.descriptor = None;
        }
        if let Some(expected) = self.socket {
            remove_same(&self.profile.join("r.sock"), expected, self.uid, true)?;
            self.socket = None;
        }
        self.directory.sync_all()
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
pub(crate) async fn stopped(mut receiver: tokio::sync::watch::Receiver<bool>) {
    loop {
        if *receiver.borrow() {
            return;
        }
        if receiver.changed().await.is_err() {
            return;
        }
    }
}
#[cfg(test)]
mod tests;
