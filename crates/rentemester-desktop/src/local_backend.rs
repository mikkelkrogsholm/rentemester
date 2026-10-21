//! Owns one explicit same-user Mac server child and its private durable profile.
//! The server remains the authority for namespace/schema admission and bookkeeping.
use rentemester_client::ApiClient;
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use serde_json::{Value, json};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::SocketAddr,
    path::{Component, Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

pub struct Start {
    pub profile: PathBuf,
    pub create: bool,
    pub company_name: String,
    pub server_binary: PathBuf,
    pub web: PathBuf,
}
#[derive(Default)]
struct Observation {
    origin: Option<String>,
    setup: Option<String>,
    failure: Option<&'static str>,
}
pub struct Managed {
    child: Child,
    observation: Arc<Mutex<Observation>>,
    pub origin: String,
    pub profile: PathBuf,
    stopped: bool,
}
pub fn default_profile() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join("Library/Application Support/Rentemester")
}
pub fn bundled_paths() -> (PathBuf, PathBuf) {
    let executable = std::env::current_exe().unwrap_or_default();
    let bin = executable.parent().unwrap_or(Path::new("."));
    (
        bin.join("rentemester-server"),
        bin.parent().unwrap_or(bin).join("Resources/web"),
    )
}
fn uid() -> Result<u32, String> {
    let result = Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .map_err(|_| "Brugeridentitet kunne ikke læses")?;
    let id = String::from_utf8(result.stdout)
        .map_err(|_| "Ugyldig brugeridentitet")?
        .trim()
        .parse::<u32>()
        .map_err(|_| "Ugyldig brugeridentitet")?;
    if !result.status.success() || id == 0 {
        return Err(
            "Lokal drift kræver din almindelige Mac-bruger; root understøttes ikke.".into(),
        );
    }
    Ok(id)
}
fn random_key() -> Result<String, String> {
    let mut bytes = [0; 32];
    File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|_| "Sikre lokale nøgler kunne ikke oprettes")?;
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    let mut bits = 0u32;
    let mut count = 0;
    for byte in bytes {
        bits = (bits << 8) | u32::from(byte);
        count += 8;
        while count >= 6 {
            count -= 6;
            out.push(alphabet[((bits >> count) & 63) as usize] as char);
        }
    }
    if count > 0 {
        out.push(alphabet[((bits << (6 - count)) & 63) as usize] as char);
    }
    Ok(out)
}
fn safe_path(path: &Path) -> bool {
    path.is_absolute()
        && path
            .components()
            .all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
        && !path.to_string_lossy().chars().any(char::is_control)
}
fn private(path: &Path, directory: bool, owner: u32) -> Result<(), String> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| "Den private lokale fil eller mappe mangler")?;
    if metadata.file_type().is_symlink()
        || metadata.uid() != owner
        || metadata.mode() & 0o077 != 0
        || if directory {
            !metadata.is_dir()
        } else {
            !metadata.is_file()
        }
    {
        return Err("Lokale data skal ejes af din bruger og være private (mappe0700, filer0600), uden symbolske links.".into());
    }
    let acl = Command::new("/bin/ls")
        .arg("-lde")
        .arg(path)
        .output()
        .map_err(|_| "Lokal adgangskontrol kunne ikke undersøges")?;
    if !acl.status.success()
        || String::from_utf8_lossy(&acl.stdout)
            .lines()
            .skip(1)
            .any(|line| !line.trim().is_empty())
    {
        return Err(
            "Den lokale datamappe og konfiguration må ikke have udvidede adgangslister.".into(),
        );
    }
    Ok(())
}
/// Read only owned single-link private files; compare the opened identity before reading.
fn private_bytes(path: &Path, owner: u32, limit: usize) -> Result<Vec<u8>, String> {
    private(path, false, owner)?;
    let before = fs::symlink_metadata(path).map_err(|_| "Privat nøglefil mangler")?;
    if before.nlink() != 1
        || before.mode() & 0o7777 != 0o600
        || before.len() == 0
        || before.len() > limit as u64
    {
        return Err(
            "Nøgle/konfiguration kræver en enkelt privat0600-fil af begrænset størrelse".into(),
        );
    }
    let file = File::open(path).map_err(|_| "Privat nøglefil kunne ikke åbnes")?;
    let opened = file
        .metadata()
        .map_err(|_| "Privat filidentitet kunne ikke læses")?;
    if before.dev() != opened.dev() || before.ino() != opened.ino() {
        return Err("Privat fil ændrede sig under åbning".into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Privat nøglefil kunne ikke læses")?;
    private(path, false, owner)?;
    let after = fs::symlink_metadata(path).map_err(|_| "Privat fil ændrede sig")?;
    if before.dev() != after.dev()
        || before.ino() != after.ino()
        || after.nlink() != 1
        || after.mode() & 0o7777 != 0o600
        || bytes.len() as u64 != before.len()
        || before.len() != after.len()
    {
        return Err("Privat fil ændrede sig under læsning".into());
    }
    Ok(bytes)
}
fn write_new_key(path: &Path, bytes: &[u8], owner: u32) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|_| "Backupnøglen findes allerede eller kunne ikke oprettes; profilen bevares")?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "Backupnøglen kunne ikke gemmes; profilen bevares")?;
    private(path, false, owner)
}
/// Creation owns key custody. Opening a profile never calls generation or repairs partial files.
fn provision_backup_keys(profile: &Path, owner: u32) -> Result<PathBuf, String> {
    let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
        .map_err(|_| "En sikker backupnøgle kunne ikke oprettes")?;
    let signer = Ed25519KeyPair::from_pkcs8(document.as_ref())
        .map_err(|_| "Backupnøglen kunne ikke valideres")?;
    let path = profile.join("backup-signing.pk8");
    write_new_key(&path, document.as_ref(), owner)?;
    write_new_key(
        &profile.join("backup-public.key"),
        signer.public_key().as_ref(),
        owner,
    )?;
    File::open(profile)
        .and_then(|file| file.sync_all())
        .map_err(|_| "Backupnøglernes mappe kunne ikke synkroniseres")?;
    Ok(path)
}
fn backup_public_key(profile: &Path) -> Result<(PathBuf, [u8; 32]), String> {
    let owner = uid()?;
    private(profile, true, owner)?;
    if !safe_path(profile) || profile.canonicalize().map_err(|_| "Lokal profil mangler")? != profile
    {
        return Err("Backupnøgler kræver den kanoniske ejede lokale profil".into());
    }
    let config: Value =
        serde_json::from_slice(&private_bytes(&profile.join("server.json"), owner, 65536)?)
            .map_err(|_| "Lokal serverkonfiguration er ugyldig")?;
    if config["runtimeProfile"] != "local-user-macos"
        || config["brokerUid"] != owner
        || config["root"] != profile.join("workspace").to_string_lossy().as_ref()
        || config["workspace"].as_str().is_none_or(str::is_empty)
    {
        return Err("Backupnøgler tilhører ikke denne lokale bruger og profil".into());
    }
    let signing_path = profile.join("backup-signing.pk8");
    if config.get("backupSigningKeyFile").is_none() || config["backupSigningKeyFile"].is_null() {
        return Err("Denne eksisterende profil har ingen konfigureret backupnøgle. Automatisk opgradering tilbydes ikke.".into());
    }
    if config["backupSigningKeyFile"] != signing_path.to_string_lossy().as_ref() {
        return Err(
            "Profilen bruger ikke sin oprindelige lokale backupnøglesti; ingen nøgle overtages"
                .into(),
        );
    }
    let secret = private_bytes(&signing_path, owner, 4096)?;
    let signer = Ed25519KeyPair::from_pkcs8(&secret)
        .map_err(|_| "Den konfigurerede backupnøgle er ugyldig; ingen erstatning oprettes")?;
    let public_path = profile.join("backup-public.key");
    let bytes: [u8; 32] = private_bytes(&public_path, owner, 32)?
        .try_into()
        .map_err(|_| "Den separate offentlige backupnøgle skal indeholde præcis32bytes")?;
    if signer.public_key().as_ref() != bytes {
        return Err("Den separate offentlige backupnøgle matcher ikke den konfigurerede signer; ingen nøgle overtages".into());
    }
    Ok((public_path, bytes))
}
fn prepare(input: &Start) -> Result<(PathBuf, bool), String> {
    if !safe_path(&input.profile) {
        return Err("Vælg en absolut lokal datamappe uden .. eller kontroltegn.".into());
    }
    let owner = uid()?;
    if input.create {
        if input.company_name.trim().is_empty() || input.company_name.len() > 500 {
            return Err("Angiv virksomhedens navn.".into());
        }
        let parent = input
            .profile
            .parent()
            .ok_or("Datamappen mangler en overmappe")?;
        if parent
            .canonicalize()
            .map_err(|_| "Overmappen skal findes")?
            != parent
        {
            return Err("Overmappen skal være en kanonisk sti uden symbolske links.".into());
        }
        fs::DirBuilder::new().mode(0o700).create(&input.profile).map_err(|_|"Datamappen findes allerede eller kunne ikke oprettes. Vælg Åbn for eksisterende data.".to_owned())?;
        private(&input.profile, true, owner)?;
        let backup_key = provision_backup_keys(&input.profile, owner)?;
        let config = json!({"backupSigningKeyFile":backup_key,"runtimeProfile":"local-user-macos","root":input.profile.join("workspace"),"brokerUid":owner,"workspace":format!("local-{}",&random_key()?[..16]),"companySlug":"company","companyName":input.company_name.trim(),"cookieKey":random_key()?,"mfaKey":random_key()?,"setupActor":"system:desktop-installer","operatorActors":["system:desktop-installer"],"accounts":[]});
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(input.profile.join("server.json"))
            .map_err(|_| "Privat serverkonfiguration kunne ikke oprettes")?;
        file.write_all(config.to_string().as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|_| "Serverkonfiguration kunne ikke gemmes; datamappen er bevaret")?;
        File::open(&input.profile)
            .and_then(|file| file.sync_all())
            .map_err(|_| "Serverkonfigurationens mappe kunne ikke synkroniseres")?;
    }
    private(&input.profile, true, owner)?;
    if input
        .profile
        .canonicalize()
        .map_err(|_| "Datamappen kunne ikke åbnes")?
        != input.profile
    {
        return Err("Vælg datamappens kanoniske sti uden symbolske links.".into());
    }
    let path = input.profile.join("server.json");
    private(&path, false, owner)?;
    let mut bytes = Vec::new();
    File::open(&path)
        .and_then(|f| f.take(65537).read_to_end(&mut bytes))
        .map_err(|_| "Konfigurationen kunne ikke læses")?;
    if bytes.len() > 65536 {
        return Err("Lokal konfiguration overskrider grænsen.".into());
    }
    let config: Value =
        serde_json::from_slice(&bytes).map_err(|_| "Lokal serverkonfiguration er ugyldig")?;
    if config["runtimeProfile"] != "local-user-macos"
        || config["brokerUid"] != owner
        || config["root"] != input.profile.join("workspace").to_string_lossy().as_ref()
    {
        return Err("Konfigurationen tilhører ikke denne lokale bruger og datamappe.".into());
    }
    if config
        .get("backupSigningKeyFile")
        .is_some_and(|value| !value.is_null())
    {
        backup_public_key(&input.profile)?;
    }
    // Resume an empty profile only if the server has never created its namespace.
    // A partial/existing namespace always goes through strict server reopen admission.
    Ok((
        path,
        !input
            .profile
            .join("workspace")
            .try_exists()
            .map_err(|_| "Workspace-status kunne ikke læses")?,
    ))
}
fn observe(line: &str, out: &mut Observation) {
    if let Some(origin) = line.strip_prefix("Rentemester Rust: ")
        && let Some(address) = origin
            .strip_prefix("http://")
            .and_then(|v| v.parse::<SocketAddr>().ok())
            .filter(|a| a.ip() == std::net::Ipv4Addr::LOCALHOST && a.port() > 0)
    {
        out.origin = Some(format!("http://{address}"));
    }
    if let Some(url) = line.strip_prefix("Workspace setup: ")
        && url.len() <= 8192
        && !url.chars().any(char::is_control)
    {
        out.setup = Some(url.to_owned());
    }
    for code in [
        "unsupported_broker_profile",
        "unsafe_broker_process",
        "unsafe_broker_namespace",
        "workspace_busy",
        "workspace_exists",
        "broker_namespace_changed",
        "broker_database_refused",
    ] {
        if line.contains(code) {
            out.failure = Some(code);
        }
    }
}
fn drain(mut pipe: impl Read + Send + 'static, observation: Arc<Mutex<Observation>>) {
    thread::spawn(move || {
        let mut chunk = [0; 1024];
        let mut line = Vec::new();
        while let Ok(size) = pipe.read(&mut chunk) {
            if size == 0 {
                break;
            }
            for byte in &chunk[..size] {
                if *byte == b'\n' {
                    if let Ok(line) = std::str::from_utf8(&line)
                        && let Ok(mut out) = observation.lock()
                    {
                        observe(line.trim_end_matches('\r'), &mut out);
                    }
                    line.clear();
                } else if line.len() < 8192 {
                    line.push(*byte);
                }
            }
        }
    });
}
impl Managed {
    /// Separate public trust anchor only. Never exposes private key material or provisions on open.
    pub fn backup_public_key(&self) -> Result<(PathBuf, [u8; 32]), String> {
        backup_public_key(&self.profile)
    }
    pub fn start(input: Start) -> Result<Self, String> {
        if !cfg!(target_os = "macos") {
            return Err("Den administrerede lokale runtime kræver macOS.".into());
        }
        let server = input.server_binary.canonicalize().map_err(
            |_| "Den medfølgende Rust-server mangler. Byg app-pakken eller angiv --server-binary.",
        )?;
        if !server.is_file() {
            return Err("Rust-serveren er ikke en fil.".into());
        }
        let web = input.web.canonicalize().map_err(
            |_| "Browserens opsætningsfiler mangler. Byg app-pakken eller angiv --web-dir.",
        )?;
        if !web.join("index.html").is_file() {
            return Err("Browserens opsætningsside mangler index.html.".into());
        }
        let (config, create) = prepare(&input)?;
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "umask 077; exec \"$@\"", "rentemester-local-server"])
            .arg(server)
            .arg("--listen")
            .arg("127.0.0.1:0")
            .arg("--web-dir")
            .arg(web)
            .arg("--config")
            .arg(config)
            .arg("--local-runtime-dir")
            .arg(&input.profile)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if create {
            command.arg("--create");
        }
        let mut child = command
            .spawn()
            .map_err(|_| "Den lokale server kunne ikke startes. Datamappen er bevaret.")?;
        let observation = Arc::new(Mutex::new(Observation::default()));
        if let Some(pipe) = child.stdout.take() {
            drain(pipe, observation.clone());
        }
        if let Some(pipe) = child.stderr.take() {
            drain(pipe, observation.clone());
        }
        let mut managed = Self {
            child,
            observation,
            origin: String::new(),
            profile: input.profile,
            stopped: false,
        };
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if managed
                .child
                .try_wait()
                .map_err(|_| "Serverstatus kunne ikke læses")?
                .is_some()
            {
                let code = managed
                    .observation
                    .lock()
                    .ok()
                    .and_then(|o| o.failure)
                    .unwrap_or("local_server_startup_refused");
                return Err(format!(
                    "Lokal server afviste opstart ({code}). Datamappen er bevaret."
                ));
            }
            let origin = managed
                .observation
                .lock()
                .ok()
                .and_then(|o| o.origin.clone());
            if let Some(origin) = origin {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| "HTTP-runtime kunne ikke startes")?;
                let client = {
                    let _guard = runtime.enter();
                    ApiClient::new(&origin, true, None).map_err(crate::readable)?
                };
                let health = runtime.block_on(client.health()).map_err(crate::readable)?;
                if health["backend"] == "rust"
                    && health["workspaceReady"] == true
                    && managed
                        .child
                        .try_wait()
                        .map_err(|_| "Serverstatus kunne ikke læses")?
                        .is_none()
                {
                    managed.origin = origin;
                    return Ok(managed);
                }
                return Err("Den ejede server bekræftede ikke et åbent Rust-workspace.".into());
            }
            if Instant::now() >= deadline {
                return Err(
                    "Den lokale server blev ikke klar inden for30sekunder. Datamappen er bevaret."
                        .into(),
                );
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
    /// Non-secret identity from the exact owned, persisted profile configuration.
    pub fn journal_identity(&self) -> Result<String, String> {
        let path = self.profile.join("server.json");
        private(&path, false, uid()?)?;
        let mut bytes = Vec::new();
        File::open(path)
            .and_then(|f| f.take(65537).read_to_end(&mut bytes))
            .map_err(|_| "Lokal identitet kunne ikke læses")?;
        if bytes.len() > 65536 {
            return Err("Lokal konfiguration overskrider grænsen".into());
        }
        let config: Value =
            serde_json::from_slice(&bytes).map_err(|_| "Lokal konfiguration er ugyldig")?;
        let workspace = config["workspace"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Lokal workspace-identitet mangler")?;
        if config["root"] != self.profile.join("workspace").to_string_lossy().as_ref() {
            return Err("Den lokale datamappe har ændret sig".into());
        }
        Ok(json!([self.profile, workspace]).to_string())
    }
    pub fn browser_url(&self) -> String {
        self.observation
            .lock()
            .ok()
            .and_then(|o| o.setup.clone())
            .filter(|url| url.starts_with(&format!("{}/#setup=", self.origin)))
            .unwrap_or_else(|| format!("{}/", self.origin))
    }
    pub fn verification_url(&self) -> Result<String, String> {
        let owner = uid()?;
        let mut latest = None;
        for entry in fs::read_dir(self.profile.join("workspace/.rentemester"))
            .map_err(|_| "Den lokale bekræftelsespostkasse kunne ikke åbnes")?
        {
            let entry = entry.map_err(|_| "Lokal bekræftelsespost kunne ikke læses")?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(token) = name
                .strip_prefix("verification-")
                .and_then(|n| n.strip_suffix(".txt"))
            else {
                continue;
            };
            if token.is_empty()
                || token.len() > 128
                || !token
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            {
                continue;
            }
            private(&entry.path(), false, owner)?;
            let time = entry
                .metadata()
                .and_then(|m| m.modified())
                .map_err(|_| "Bekræftelsesdato mangler")?;
            if latest.as_ref().is_none_or(|(prior, _)| time > *prior) {
                latest = Some((time, entry.path()));
            }
        }
        let (_, path) =
            latest.ok_or("Ingen lokal bekræftelsesmail endnu. Opret først ejeren i browseren.")?;
        let mut url = String::new();
        File::open(path)
            .and_then(|f| f.take(8193).read_to_string(&mut url))
            .map_err(|_| "Lokal bekræftelse kunne ikke læses")?;
        if url.len() > 8192
            || url.chars().any(char::is_control)
            || !url.starts_with(&format!("{}/api/auth/verify-email?", self.origin))
        {
            return Err("Bekræftelseslinket tilhører ikke den kørende lokale server. Bed om en ny mail i browseren.".into());
        }
        Ok(url)
    }
    pub fn stop(&mut self) -> Result<(), String> {
        if self.stopped {
            return Ok(());
        }
        if self
            .child
            .try_wait()
            .map_err(|_| "Serverstatus kunne ikke læses")?
            .is_none()
        {
            // An unreaped owned child cannot have its PID reassigned to another process.
            let _ = Command::new("/bin/kill")
                .arg("-TERM")
                .arg(self.child.id().to_string())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            let deadline = Instant::now() + Duration::from_secs(2);
            while self
                .child
                .try_wait()
                .map_err(|_| "Serverstatus kunne ikke læses")?
                .is_none()
            {
                if Instant::now() >= deadline {
                    self.child
                        .kill()
                        .map_err(|_| "Den ejede server kunne ikke stoppes")?;
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
        }
        self.child
            .wait()
            .map_err(|_| "Den ejede server kunne ikke afsluttes")?;
        self.stopped = true;
        Ok(())
    }
}
impl Drop for Managed {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temporary() -> PathBuf {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        parent.join(format!(
            "rentemester-local-profile-{}",
            random_key().unwrap()
        ))
    }
    #[test]
    fn create_open_preserves_private_keys_and_never_overwrites_existing_profile() {
        let path = temporary();
        let input = Start {
            profile: path.clone(),
            create: true,
            company_name: "Synthetic local company".into(),
            server_binary: PathBuf::new(),
            web: PathBuf::new(),
        };
        let (config, create) = prepare(&input).unwrap();
        assert!(create);
        let original = fs::read(&config).unwrap();
        let value: Value = serde_json::from_slice(&original).unwrap();
        assert_eq!(value["runtimeProfile"], "local-user-macos");
        assert_eq!(value["cookieKey"].as_str().unwrap().len(), 43);
        assert!(prepare(&input).is_err());
        let (_, retry) = prepare(&Start {
            create: false,
            ..input
        })
        .unwrap();
        assert!(retry);
        assert!(fs::read(&config).unwrap() == original);
        fs::remove_dir_all(path).unwrap();
    }
    fn signed_profile() -> Start {
        let input = Start {
            profile: temporary(),
            create: true,
            company_name: "Synthetic key custody".into(),
            server_binary: PathBuf::new(),
            web: PathBuf::new(),
        };
        prepare(&input).unwrap();
        Start {
            create: false,
            ..input
        }
    }
    #[test]
    fn new_profile_provisions_separate_matching_signer_and_reopen_keeps_exact_keys() {
        use ring::signature::{ED25519, UnparsedPublicKey};
        let input = signed_profile();
        let config = fs::read(input.profile.join("server.json")).unwrap();
        let value: Value = serde_json::from_slice(&config).unwrap();
        let key = input.profile.join("backup-signing.pk8");
        assert_eq!(
            value["backupSigningKeyFile"],
            key.to_string_lossy().as_ref()
        );
        assert!(!key.starts_with(input.profile.join("workspace")));
        let secret = fs::read(&key).unwrap();
        let (public_path, public) = backup_public_key(&input.profile).unwrap();
        assert_eq!(public_path, input.profile.join("backup-public.key"));
        let signer = Ed25519KeyPair::from_pkcs8(&secret).unwrap();
        UnparsedPublicKey::new(&ED25519, public)
            .verify(
                b"synthetic explicit backup proof",
                signer.sign(b"synthetic explicit backup proof").as_ref(),
            )
            .unwrap();
        for path in [&key, &public_path] {
            let metadata = fs::symlink_metadata(path).unwrap();
            assert_eq!(metadata.mode() & 0o7777, 0o600);
            assert_eq!(metadata.nlink(), 1);
        }
        prepare(&input).unwrap();
        assert!(fs::read(&key).unwrap() == secret);
        assert!(fs::read(input.profile.join("server.json")).unwrap() == config);
        assert!(backup_public_key(&input.profile).unwrap().1 == public);
        fs::remove_dir_all(input.profile).unwrap();
    }
    #[test]
    fn unsafe_missing_corrupt_or_mismatched_configured_keys_refuse_without_repairs() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let input = signed_profile();
        let private_path = input.profile.join("backup-signing.pk8");
        let public_path = input.profile.join("backup-public.key");
        let original = fs::read(&private_path).unwrap();
        let public = fs::read(&public_path).unwrap();
        let config = fs::read(input.profile.join("server.json")).unwrap();
        let refused = || {
            assert!(prepare(&input).is_err());
            assert!(backup_public_key(&input.profile).is_err());
            assert!(fs::read(input.profile.join("server.json")).unwrap() == config);
        };
        fs::remove_file(&private_path).unwrap();
        refused();
        assert!(!private_path.exists());
        write_new_key(&private_path, b"invalid PKCS8", uid().unwrap()).unwrap();
        refused();
        assert!(fs::read(&private_path).unwrap() == b"invalid PKCS8");
        fs::write(&private_path, &original).unwrap();
        fs::set_permissions(&private_path, fs::Permissions::from_mode(0o644)).unwrap();
        refused();
        assert_eq!(fs::metadata(&private_path).unwrap().mode() & 0o7777, 0o644);
        fs::set_permissions(&private_path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::remove_file(&private_path).unwrap();
        symlink(&public_path, &private_path).unwrap();
        refused();
        assert!(
            fs::symlink_metadata(&private_path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        fs::remove_file(&private_path).unwrap();
        write_new_key(&private_path, &original, uid().unwrap()).unwrap();
        let alias = input.profile.join("signer-link");
        fs::hard_link(&private_path, &alias).unwrap();
        refused();
        assert_eq!(fs::metadata(&private_path).unwrap().nlink(), 2);
        fs::remove_file(alias).unwrap();
        fs::write(&public_path, [0u8; 32]).unwrap();
        refused();
        assert!(fs::read(&public_path).unwrap() == [0u8; 32]);
        fs::write(&public_path, &public[..31]).unwrap();
        refused();
        assert_eq!(fs::metadata(&public_path).unwrap().len(), 31);
        fs::remove_file(&public_path).unwrap();
        refused();
        assert!(!public_path.exists());
        write_new_key(&public_path, &public, uid().unwrap()).unwrap();
        fs::set_permissions(&public_path, fs::Permissions::from_mode(0o644)).unwrap();
        refused();
        fs::set_permissions(&public_path, fs::Permissions::from_mode(0o600)).unwrap();
        let kept = input.profile.join("kept-public.key");
        fs::rename(&public_path, &kept).unwrap();
        symlink(&kept, &public_path).unwrap();
        refused();
        assert!(
            fs::symlink_metadata(&public_path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        fs::remove_file(&public_path).unwrap();
        fs::rename(kept, &public_path).unwrap();
        assert!(fs::read(&private_path).unwrap() == original);
        assert!(fs::read(&public_path).unwrap() == public);
        prepare(&input).unwrap();
        fs::remove_dir_all(input.profile).unwrap();
    }
    #[test]
    fn unsigned_and_partial_profiles_are_never_silently_upgraded_or_adopted() {
        let input = signed_profile();
        let config_path = input.profile.join("server.json");
        let mut config: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
        config
            .as_object_mut()
            .unwrap()
            .remove("backupSigningKeyFile");
        fs::write(&config_path, config.to_string()).unwrap();
        fs::remove_file(input.profile.join("backup-signing.pk8")).unwrap();
        fs::remove_file(input.profile.join("backup-public.key")).unwrap();
        let original = fs::read(&config_path).unwrap();
        prepare(&input).unwrap();
        assert!(
            backup_public_key(&input.profile)
                .unwrap_err()
                .contains("ingen konfigureret")
        );
        assert!(fs::read(&config_path).unwrap() == original);
        assert!(!input.profile.join("backup-signing.pk8").exists());
        fs::remove_file(&config_path).unwrap();
        write_new_key(
            &input.profile.join("backup-signing.pk8"),
            b"partial untouched",
            uid().unwrap(),
        )
        .unwrap();
        assert!(prepare(&input).is_err());
        assert!(
            prepare(&Start {
                create: true,
                ..input
            })
            .is_err()
        );
        assert!(!config_path.exists());
        assert!(
            fs::read(config_path.parent().unwrap().join("backup-signing.pk8")).unwrap()
                == b"partial untouched"
        );
        fs::remove_dir_all(config_path.parent().unwrap()).unwrap();
    }
    #[test]
    fn shared_configuration_and_symlink_profile_are_refused_without_rewriting_keys() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let path = temporary();
        let input = Start {
            profile: path.clone(),
            create: true,
            company_name: "Synthetic local company".into(),
            server_binary: PathBuf::new(),
            web: PathBuf::new(),
        };
        let (config, _) = prepare(&input).unwrap();
        let original = fs::read(&config).unwrap();
        fs::set_permissions(&config, fs::Permissions::from_mode(0o644)).unwrap();
        let input = Start {
            create: false,
            ..input
        };
        assert!(prepare(&input).is_err());
        assert!(fs::read(&config).unwrap() == original);
        fs::set_permissions(&config, fs::Permissions::from_mode(0o600)).unwrap();
        let alias = temporary();
        symlink(&path, &alias).unwrap();
        assert!(
            prepare(&Start {
                profile: alias.clone(),
                ..input
            })
            .is_err()
        );
        fs::remove_file(alias).unwrap();
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn markers_require_owned_loopback_and_reject_foreign_setup_url() {
        let mut observation = Observation::default();
        observe(
            "Rentemester Rust: http://example.invalid:1234",
            &mut observation,
        );
        assert!(observation.origin.is_none());
        observe("Rentemester Rust: http://127.0.0.1:0", &mut observation);
        assert!(observation.origin.is_none());
        observe("Rentemester Rust: http://127.0.0.1:41234", &mut observation);
        assert_eq!(
            observation.origin.as_deref(),
            Some("http://127.0.0.1:41234")
        );
    }
    #[test]
    fn stopping_only_owned_child_is_idempotent() {
        let child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
        let mut unrelated = Command::new("/bin/sleep").arg("30").spawn().unwrap();
        let mut managed = Managed {
            child,
            observation: Arc::default(),
            origin: String::new(),
            profile: PathBuf::new(),
            stopped: false,
        };
        managed.stop().unwrap();
        managed.stop().unwrap();
        assert!(unrelated.try_wait().unwrap().is_none());
        unrelated.kill().unwrap();
        unrelated.wait().unwrap();
    }
    #[test]
    #[ignore = "requires the real Mac runtime server and built browser assets"]
    fn actual_owned_server_creates_excludes_competing_owner_and_reopens_private_profile() {
        let server = PathBuf::from(
            std::env::var_os("RENTEMESTER_TEST_LOCAL_SERVER").expect("set compiled server path"),
        );
        let web = PathBuf::from(
            std::env::var_os("RENTEMESTER_TEST_LOCAL_WEB").expect("set built browser path"),
        );
        let path = temporary();
        let input = |create| Start {
            profile: path.clone(),
            create,
            company_name: "Synthetic managed desktop".into(),
            server_binary: server.clone(),
            web: web.clone(),
        };
        let mut first = Managed::start(input(true)).unwrap();
        assert!(first.origin.starts_with("http://127.0.0.1:"));
        assert!(
            first
                .browser_url()
                .starts_with(&format!("{}/#setup=", first.origin))
        );
        assert!(first.verification_url().is_err());
        let original = fs::read(path.join("server.json")).unwrap();
        assert!(Managed::start(input(false)).is_err());
        first.stop().unwrap();
        assert!(first.child.try_wait().unwrap().is_some());
        let mut reopened = Managed::start(input(false)).unwrap();
        assert!(fs::read(path.join("server.json")).unwrap() == original);
        reopened.stop().unwrap();
        fs::remove_dir_all(path).unwrap();
    }
}
