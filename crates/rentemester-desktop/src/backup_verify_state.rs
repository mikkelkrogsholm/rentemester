//! Offline full validation through the explicitly supplied, trusted native server.
//! Selected bytes are frozen before review; source files are never passed to the child.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub const MAX_ARCHIVE: usize = 96 * 1024 * 1024;
const MAX_OUTPUT: u64 = 64 * 1024;
pub const FORMAT: &str = "rentemester-native-backup/1";
pub const SCOPE: &str = "signed_inventory_current_schema_integrity_journal_chain_and_originals";

#[derive(Clone)]
pub struct Selection {
    pub name: String,
    pub sha256: String,
    bytes: Vec<u8>,
}
impl Selection {
    pub fn read(path: &Path, key: bool) -> Result<Self, String> {
        let refusal = "Vælg en almindelig, uændret fil uden symbolske links eller hårde links";
        let before = fs::symlink_metadata(path).map_err(|_| refusal)?;
        if !path.is_absolute() || !before.is_file() || before.nlink() != 1 {
            return Err(refusal.into());
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(target_os = "macos")]
        options.custom_flags(0x100 | 0x4); // O_NOFOLLOW | O_NONBLOCK
        #[cfg(not(target_os = "macos"))]
        options.custom_flags(0x20000 | 0x800); // O_NOFOLLOW | O_NONBLOCK
        let file = options.open(path).map_err(|_| refusal)?;
        let opened = file.metadata().map_err(|_| refusal)?;
        let limit = if key { 32 } else { MAX_ARCHIVE };
        if !opened.is_file()
            || identity(&before) != identity(&opened)
            || opened.len() == 0
            || opened.len() > limit as u64
        {
            return Err(
                "Filen er tom, ændret eller for stor (backup højst 96 MiB; nøgle præcis 32 bytes)"
                    .into(),
            );
        }
        let mut bytes = Vec::new();
        (&file)
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| refusal)?;
        let after = file.metadata().map_err(|_| refusal)?;
        let current = fs::symlink_metadata(path).map_err(|_| refusal)?;
        if identity(&opened) != identity(&after)
            || identity(&opened) != identity(&current)
            || bytes.len() != opened.len() as usize
            || (key && bytes.len() != 32)
        {
            return Err(refusal.into());
        }
        Ok(Self {
            name: path
                .file_name()
                .ok_or(refusal)?
                .to_string_lossy()
                .into_owned(),
            sha256: hash(&bytes),
            bytes,
        })
    }
    pub fn size(&self) -> usize {
        self.bytes.len()
    }
}
fn identity(m: &fs::Metadata) -> (u64, u64, u64, i64, i64, i64, i64, u64) {
    (
        m.dev(),
        m.ino(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
        m.nlink(),
    )
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Self, String> {
        use ring::rand::{SecureRandom, SystemRandom};
        let mut random = [0u8; 24];
        SystemRandom::new()
            .fill(&mut random)
            .map_err(|_| "Privat kontrolmappe kunne ikke oprettes")?;
        let name = random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let parent = std::env::temp_dir()
            .canonicalize()
            .map_err(|_| "Midlertidig mappe er utilgængelig")?;
        let path = parent.join(format!("rentemester-backup-verify-{name}"));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .map_err(|_| "Privat kontrolmappe kunne ikke oprettes")?;
        Ok(Self(path))
    }
    fn write(&self, name: &str, bytes: &[u8]) -> Result<File, String> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(self.0.join(name))
            .map_err(|_| "Privat kontrolfil kunne ikke oprettes")?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "Privat kontrolfil kunne ikke gemmes")?;
        Ok(file)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        for name in ["archive.json", "trusted.key", "stdout", "stderr"] {
            let _ = fs::remove_file(self.0.join(name));
        }
        let _ = fs::remove_dir(&self.0);
    }
}
#[derive(Debug, PartialEq)]
pub struct Verified {
    pub companies: u64,
}
fn success(bytes: &[u8]) -> Result<Verified, String> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| "Kontrolprogrammet gav ikke et gyldigt kontrolsvar")?;
    let fields = value.as_object().ok_or("Kontrolsvaret har forkert form")?;
    let companies = value["companies"]
        .as_u64()
        .filter(|v| (1..=64).contains(v))
        .ok_or("Kontrolsvaret mangler et gyldigt virksomhedsantal")?;
    if fields.len() != 4
        || value["ok"] != true
        || value["format"] != FORMAT
        || value["scope"] != SCOPE
    {
        return Err(
            "Kontrolsvaret bekræfter ikke både signatur, inventar, schema, journal og originaler"
                .into(),
        );
    }
    Ok(Verified { companies })
}
/// Runs no server, config, restore or network command. The parent selects the trusted binary.
pub fn verify(binary: &Path, archive: &Selection, key: &Selection) -> Result<Verified, String> {
    run(binary, archive, key, Duration::from_secs(120))
}
fn run(
    binary: &Path,
    archive: &Selection,
    key: &Selection,
    timeout: Duration,
) -> Result<Verified, String> {
    if key.bytes.len() != 32
        || archive.bytes.is_empty()
        || archive.bytes.len() > MAX_ARCHIVE
        || hash(&archive.bytes) != archive.sha256
        || hash(&key.bytes) != key.sha256
    {
        return Err("De gennemgåede bytes og fingeraftryk stemmer ikke".into());
    }
    let metadata =
        fs::symlink_metadata(binary).map_err(|_| "Det betroede kontrolprogram er utilgængeligt")?;
    if !binary.is_absolute() || !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return Err("Kontrollen kræver det konfigurerede eller medfølgende native serverprogram; ingen PATH-søgning".into());
    }
    let scratch = Scratch::new()?;
    scratch.write("archive.json", &archive.bytes)?;
    scratch.write("trusted.key", &key.bytes)?;
    let stdout = scratch.write("stdout", &[])?;
    let stderr = scratch.write("stderr", &[])?;
    let mut child = Command::new(binary)
        .arg("--verify-backup")
        .arg(scratch.0.join("archive.json"))
        .arg("--trusted-backup-key")
        .arg(scratch.0.join("trusted.key"))
        .current_dir(&scratch.0)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|_| "Kontrolprogrammet kunne ikke startes")?;
    let started = Instant::now();
    let status = loop {
        let poll = child.try_wait();
        match poll {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Kontrolprogrammets afslutning kunne ikke fastslås".into());
            }
        }
        let too_large = ["stdout", "stderr"]
            .iter()
            .any(|name| fs::metadata(scratch.0.join(name)).map_or(true, |m| m.len() > MAX_OUTPUT));
        if started.elapsed() >= timeout || too_large {
            let _ = child.kill();
            let _ = child.wait();
            return Err(
                "Kontrollen blev afbrudt ved tids- eller svargrænsen. Ingen backup er godkendt"
                    .into(),
            );
        }
        thread::sleep(Duration::from_millis(25));
    };
    if !status.success() {
        return Err("Backupkontrollen afviste filen eller den separate nøgle. Kontrollér nøglens oprindelse og filens integritet. Ingen gendannelse er foretaget".into());
    }
    let output = bounded(&scratch.0.join("stdout"))?;
    let errors = bounded(&scratch.0.join("stderr"))?;
    if !errors.is_empty() {
        return Err("Kontrolprogrammet gav et uventet fejlsvar; backup er ikke godkendt".into());
    }
    success(&output)
}
fn bounded(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = vec![];
    File::open(path)
        .and_then(|f| f.take(MAX_OUTPUT + 1).read_to_end(&mut bytes))
        .map_err(|_| "Kontrolsvaret kunne ikke læses")?;
    if bytes.len() as u64 > MAX_OUTPUT {
        return Err("Kontrolsvaret er for stort".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    const GOOD: &str = r#"{"ok":true,"format":"rentemester-native-backup/1","companies":2,"scope":"signed_inventory_current_schema_integrity_journal_chain_and_originals"}"#;
    fn selection(bytes: &[u8]) -> Selection {
        Selection {
            name: "synthetic".into(),
            sha256: hash(bytes),
            bytes: bytes.to_vec(),
        }
    }
    #[test]
    fn signature_only_or_arbitrary_success_never_qualifies() {
        assert_eq!(success(GOOD.as_bytes()).unwrap(), Verified { companies: 2 });
        for bad in [
            r#"{"ok":true}"#,
            &GOOD.replace(SCOPE, "signature_only"),
            &GOOD.replace("/1", "/2"),
            &GOOD.replace("\"companies\":2", "\"companies\":0"),
            &GOOD.replace("\"ok\":true", "\"ok\":true,\"extra\":true"),
        ] {
            assert!(success(bad.as_bytes()).is_err());
        }
    }
    #[test]
    fn frozen_bytes_survive_source_replacement_and_private_files_are_removed() {
        let scratch = Scratch::new().unwrap();
        let path = scratch.0.clone();
        scratch.write("archive.json", b"original").unwrap();
        let frozen = Selection::read(&path.join("archive.json"), false).unwrap();
        fs::write(path.join("archive.json"), b"changed").unwrap();
        assert_eq!(frozen.bytes, b"original");
        assert_eq!(frozen.sha256, hash(b"original"));
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(path.join("archive.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        drop(scratch);
        assert!(!path.exists());
    }
    #[test]
    fn unsafe_key_sources_and_mutated_review_are_refused_before_child() {
        let scratch = Scratch::new().unwrap();
        scratch.write("trusted.key", &[1; 31]).unwrap();
        assert!(Selection::read(&scratch.0.join("trusted.key"), true).is_err());
        std::os::unix::fs::symlink(scratch.0.join("trusted.key"), scratch.0.join("link")).unwrap();
        assert!(Selection::read(&scratch.0.join("link"), true).is_err());
        fs::remove_file(scratch.0.join("link")).unwrap();
        let mut archive = selection(b"archive");
        archive.bytes[0] = 0;
        assert!(
            run(
                Path::new("/nonexistent"),
                &archive,
                &selection(&[0; 32]),
                Duration::ZERO
            )
            .is_err()
        );
    }
    #[test]
    fn child_receives_only_frozen_verify_arguments_and_exit_status_is_required() {
        let scratch = Scratch::new().unwrap();
        let binary = scratch.0.join("fixture");
        let script = format!(
            "#!/bin/sh\n[ \"$#\" = 4 ] && [ \"$1\" = --verify-backup ] && [ \"$3\" = --trusted-backup-key ] || exit 9\n[ \"$(cat \"$2\")\" = archive ] || exit 8\n[ \"$(wc -c < \"$4\" | tr -d ' ')\" = 32 ] || exit 7\nprintf '%s' '{}'\n",
            GOOD
        );
        fs::write(&binary, script).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            run(
                &binary,
                &selection(b"archive"),
                &selection(&[1; 32]),
                Duration::from_secs(2)
            )
            .unwrap(),
            Verified { companies: 2 }
        );
        fs::write(
            &binary,
            format!("#!/bin/sh\nprintf '%s' '{}'\nexit 1\n", GOOD),
        )
        .unwrap();
        assert!(
            run(
                &binary,
                &selection(b"archive"),
                &selection(&[1; 32]),
                Duration::from_secs(2)
            )
            .is_err()
        );
        fs::write(&binary, "#!/bin/sh\nwhile :; do :; done\n").unwrap();
        assert!(
            run(
                &binary,
                &selection(b"archive"),
                &selection(&[1; 32]),
                Duration::from_millis(30)
            )
            .is_err()
        );
        fs::remove_file(binary).unwrap();
    }
}
