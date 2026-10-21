//! Explicit private local-runtime join. Never discovers, starts or stops a server.
use crate::{ApiClient, Error};
use serde::Deserialize;
use std::{
    fs::{self, Metadata, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    sync::OnceLock,
};
#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Descriptor {
    format: String,
    origin: String,
    socket: String,
    workspace_root: PathBuf,
    workspace_id: String,
}
/// Validated transport metadata, not an authentication or ledger capability.
#[derive(Clone)]
pub struct LocalRuntime {
    profile: PathBuf,
    descriptor: Descriptor,
}
static PROCESS: OnceLock<LocalRuntime> = OnceLock::new();
fn invalid() -> Error {
    Error::InvalidInput(
        "local runtime requires a matching canonical private owned profile, descriptor and Unix socket",
    )
}
#[cfg(unix)]
fn owned(metadata: &Metadata, uid: u32, mode: u32) -> bool {
    use std::os::unix::fs::MetadataExt;
    metadata.uid() == uid && metadata.mode() & 0o7777 == mode
}
#[cfg(unix)]
fn identity(a: &Metadata, b: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}
impl LocalRuntime {
    /// Read only exact r.json protocol after validating actual real/effective UID and paths.
    /// The private socket is the recipient; the logical HTTP origin is never a fallback.
    #[cfg(unix)]
    pub fn load(profile: &Path) -> Result<Self, Error> {
        use std::os::unix::{
            ffi::OsStrExt,
            fs::{FileTypeExt, MetadataExt, OpenOptionsExt},
        };
        let uid = rustix::process::getuid().as_raw();
        if uid == 0 || uid != rustix::process::geteuid().as_raw() || !profile.is_absolute() {
            return Err(invalid());
        }
        let canonical = fs::canonicalize(profile).map_err(|_| invalid())?;
        if canonical.as_os_str() != profile.as_os_str() {
            return Err(invalid());
        }
        let before = fs::symlink_metadata(profile).map_err(|_| invalid())?;
        if !before.is_dir() || !owned(&before, uid, 0o700) {
            return Err(invalid());
        }
        let socket = profile.join("r.sock");
        if socket.as_os_str().as_bytes().len() > 103 {
            return Err(invalid());
        }
        let path = profile.join("r.json");
        let observed = fs::symlink_metadata(&path).map_err(|_| invalid())?;
        if !observed.is_file()
            || !owned(&observed, uid, 0o600)
            || observed.nlink() != 1
            || observed.len() == 0
            || observed.len() > 4096
        {
            return Err(invalid());
        }
        let mut options = OpenOptions::new();
        options.read(true);
        // Same O_NOFOLLOW|O_NONBLOCK policy as the bounded service credential reader.
        #[cfg(target_os = "linux")]
        options.custom_flags(0x20000 | 0x800);
        #[cfg(target_os = "macos")]
        options.custom_flags(0x100 | 0x4);
        let mut file = options.open(&path).map_err(|_| invalid())?;
        let opened = file.metadata().map_err(|_| invalid())?;
        if !identity(&observed, &opened)
            || !opened.is_file()
            || !owned(&opened, uid, 0o600)
            || opened.nlink() != 1
        {
            return Err(invalid());
        }
        let mut bytes = Vec::new();
        file.by_ref()
            .take(4097)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid())?;
        if bytes.len() > 4096 || bytes.len() as u64 != opened.len() {
            return Err(invalid());
        }
        let descriptor: Descriptor = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let origin = reqwest::Url::parse(&descriptor.origin).map_err(|_| invalid())?;
        if descriptor.format != "rentemester-local-runtime/1"
            || descriptor.socket != "r.sock"
            || descriptor.workspace_id.is_empty()
            || descriptor.workspace_id.len() > 1024
            || descriptor.workspace_id.chars().any(char::is_control)
            || origin.scheme() != "http"
            || origin.origin().ascii_serialization() != descriptor.origin
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.query().is_some()
            || origin.fragment().is_some()
            || origin.path() != "/"
            || !origin
                .host_str()
                .and_then(|s| s.trim_matches(['[', ']']).parse::<std::net::IpAddr>().ok())
                .is_some_and(|ip| ip.is_loopback())
        {
            return Err(invalid());
        }
        let root = profile.join("workspace");
        if descriptor.workspace_root.as_os_str() != root.as_os_str()
            || fs::canonicalize(&root).map_err(|_| invalid())?.as_os_str() != root.as_os_str()
        {
            return Err(invalid());
        }
        let root_meta = fs::symlink_metadata(&root).map_err(|_| invalid())?;
        let socket_meta = fs::symlink_metadata(&socket).map_err(|_| invalid())?;
        let after = fs::symlink_metadata(profile).map_err(|_| invalid())?;
        let descriptor_after = fs::symlink_metadata(&path).map_err(|_| invalid())?;
        if !root_meta.is_dir()
            || !owned(&root_meta, uid, 0o700)
            || !socket_meta.file_type().is_socket()
            || !owned(&socket_meta, uid, 0o600)
            || !identity(&before, &after)
            || !after.is_dir()
            || !owned(&after, uid, 0o700)
            || !identity(&opened, &descriptor_after)
            || !descriptor_after.is_file()
            || !owned(&descriptor_after, uid, 0o600)
            || descriptor_after.nlink() != 1
            || descriptor_after.len() != opened.len()
        {
            return Err(invalid());
        }
        Ok(Self {
            profile: profile.to_owned(),
            descriptor,
        })
    }
    #[cfg(not(unix))]
    pub fn load(_: &Path) -> Result<Self, Error> {
        Err(Error::InvalidInput(
            "local Unix transport is unavailable on this platform",
        ))
    }
    pub fn origin(&self) -> &str {
        &self.descriptor.origin
    }
    pub fn workspace_id(&self) -> &str {
        &self.descriptor.workspace_id
    }
    pub fn workspace_root(&self) -> &Path {
        &self.descriptor.workspace_root
    }
    fn endpoint(&self) -> Result<PathBuf, Error> {
        let fresh = Self::load(&self.profile)?;
        if fresh.descriptor != self.descriptor {
            return Err(invalid());
        }
        Ok(self.profile.join("r.sock"))
    }
    /// Explicit immutable binding for a single CLI invocation. No default discovery.
    /// Every subsequent constructor revalidates this descriptor/socket and exact origin.
    pub fn install_process(self) -> Result<(), Error> {
        self.endpoint()?;
        PROCESS
            .set(self)
            .map_err(|_| Error::InvalidInput("local runtime process binding already installed"))
    }
}
pub(crate) fn process_endpoint(origin: &str) -> Result<Option<PathBuf>, Error> {
    PROCESS
        .get()
        .map(|local| {
            if origin != local.origin() {
                return Err(Error::InvalidInput(
                    "server origin differs from explicit local runtime binding",
                ));
            }
            local.endpoint()
        })
        .transpose()
}
impl ApiClient {
    /// Explicit Unix constructor; existing authentication/header rules remain unchanged.
    pub fn new_local(local: &LocalRuntime, cookie: Option<String>) -> Result<Self, Error> {
        let socket = local.endpoint()?;
        let mut client = Self::new(local.origin(), true, cookie)?;
        client.http = Self::construct(client.base.clone(), None, Some(&socket))?.http;
        Ok(client)
    }
    pub fn with_local_service_key(local: &LocalRuntime, key: String) -> Result<Self, Error> {
        let socket = local.endpoint()?;
        let mut client = Self::with_service_key(local.origin(), true, key)?;
        client.http = Self::construct(client.base.clone(), None, Some(&socket))?.http;
        Ok(client)
    }
}
