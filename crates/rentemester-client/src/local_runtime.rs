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
    /// Original read workspace is a binding assertion, never authorization.
    /// No remote identity producer exists for this ABI; do not discover a profile
    /// or route an unverified workspace assertion over the logical TCP origin.
    pub fn assert_original_workspace89(&self, requested: &str) -> Result<(), Error> {
        let local = self.local.as_ref().ok_or(Error::InvalidInput(
            "original CFO workspace assertion requires an explicit verified local profile; authenticated remote workspace identity is unavailable",
        ))?;
        local.endpoint()?;
        if requested != local.workspace_id()
            && local.workspace_root().to_str() != Some(requested)
        {
            return Err(Error::InvalidInput("workspace differs from explicit verified local runtime binding"));
        }
        Ok(())
    }
    /// Explicit Unix constructor; existing authentication/header rules remain unchanged.
    pub fn new_local(local: &LocalRuntime, cookie: Option<String>) -> Result<Self, Error> {
        let socket = local.endpoint()?;
        let mut client = Self::new(local.origin(), true, cookie)?;
        client.http = Self::construct(client.base.clone(), None, Some(&socket))?.http;
        client.local = Some(local.clone());
        Ok(client)
    }
    pub fn with_local_service_key(local: &LocalRuntime, key: String) -> Result<Self, Error> {
        let socket = local.endpoint()?;
        let mut client = Self::with_service_key(local.origin(), true, key)?;
        client.http = Self::construct(client.base.clone(), None, Some(&socket))?.http;
        client.local = Some(local.clone());
        Ok(client)
    }
}

/// Canonical knowledge selector. Paths are transport metadata, never authority.
/// Only an explicit, freshly verified local profile may translate an absolute root.
pub fn knowledge_company_selector(local: Option<&LocalRuntime>, value: &str) -> Result<String, Error> {
    fn slug(s: &str) -> bool {
        !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    }
    if slug(value) { return Ok(value.to_owned()); }
    let refuse = || Error::InvalidInput("knowledge company requires a canonical slug or registered absolute root under explicit verified --local-profile");
    let local = local.ok_or_else(refuse)?;
    local.endpoint()?;
    let path = Path::new(value);
    if !path.is_absolute() || value.contains('\\') || value.chars().any(char::is_control)
        || path.components().any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::CurDir)) {
        return Err(refuse());
    }
    let mut checked = PathBuf::new();
    for component in path.components() {
        checked.push(component.as_os_str());
        let meta = fs::symlink_metadata(&checked).map_err(|_| refuse())?;
        if !meta.is_dir() || meta.file_type().is_symlink() { return Err(refuse()); }
    }
    if fs::canonicalize(path).map_err(|_| refuse())?.as_os_str() != path.as_os_str()
        || path.parent() != Some(local.workspace_root()) { return Err(refuse()); }
    let name = path.file_name().and_then(|s| s.to_str()).filter(|s| slug(s)).ok_or_else(refuse)?;
    // Read the existing runtime registration, bounded and without following aliases.
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        let manifest = local.workspace_root().join("workspace.json");
        let before = fs::symlink_metadata(&manifest).map_err(|_| refuse())?;
        let uid = rustix::process::getuid().as_raw();
        if !before.is_file() || before.uid() != uid || before.mode() & 0o022 != 0 || before.nlink() != 1 || before.len() > 1024 * 1024 { return Err(refuse()); }
        let mut options = OpenOptions::new(); options.read(true);
        #[cfg(target_os = "linux")]
        options.custom_flags(0x20000 | 0x800);
        #[cfg(target_os = "macos")]
        options.custom_flags(0x100 | 0x4);
        let file = options.open(&manifest).map_err(|_| refuse())?;
        let opened = file.metadata().map_err(|_| refuse())?;
        if !identity(&before, &opened) { return Err(refuse()); }
        let mut bytes = Vec::new();
        file.take(1024 * 1024 + 1).read_to_end(&mut bytes).map_err(|_| refuse())?;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Manifest { version: u32, companies: Vec<Entry> }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct Entry { slug: String, name: String, created_at: String, archived: bool }
        let m: Manifest = serde_json::from_slice(&bytes).map_err(|_| refuse())?;
        if m.version != 1 || m.companies.len() > 1000 { return Err(refuse()); }
        let mut seen = std::collections::BTreeSet::new();
        let mut found = false;
        for e in m.companies {
            if !slug(&e.slug) || !seen.insert(e.slug.clone()) || e.name.trim().is_empty() || e.name.len() > 1024 || e.created_at.trim().is_empty() || e.created_at.len() > 128
                || e.name.chars().any(char::is_control) || e.created_at.chars().any(char::is_control) { return Err(refuse()); }
            if e.slug == name && !e.archived { found = true; }
        }
        if !found { return Err(refuse()); }
        let after = fs::symlink_metadata(&manifest).map_err(|_| refuse())?;
        if !identity(&opened, &after) || after.len() != bytes.len() as u64 { return Err(refuse()); }
        local.endpoint()?;
        Ok(name.to_owned())
    }
    #[cfg(not(unix))]
    { Err(refuse()) }
}

pub(crate) fn process_local() -> Option<LocalRuntime> { PROCESS.get().cloned() }
