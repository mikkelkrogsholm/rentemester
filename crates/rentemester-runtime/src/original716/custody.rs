//! Source company Ed25519 PEM custody primitives, never native workspace signing keys.
use crate::backup_artifact::{Error, original717 as codec};
use base64::{Engine, engine::general_purpose::STANDARD};
use ring::signature::{Ed25519KeyPair, KeyPair};
use zeroize::Zeroizing;
pub const PRIVATE: &str = ".backup-signing-key.pem";
pub const PUBLIC: &str = "config/backup-manifest.pub";
pub const SIGNATURE: &str = "manifest.json.ed25519.sig";
pub struct CompanyKey {
    pair: Ed25519KeyPair,
    private_pem: Zeroizing<String>,
    public_pem: String,
}
impl CompanyKey {
    pub fn generate() -> Result<Self, Error> {
        // Node exports RFC8410 PKCS#8 v1. ring generates v2, so encode the seed as v1.
        let document = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new())
            .map_err(|_| Error::InvalidKey)?;
        let pair = Ed25519KeyPair::from_pkcs8(document.as_ref()).map_err(|_| Error::InvalidKey)?;
        // ring's documented PKCS#8 v2 seed is nested OCTET STRING at offset 16.
        let bytes = document.as_ref();
        if bytes.len() < 48 || bytes[12..16] != [0x04, 0x22, 0x04, 0x20] {
            return Err(Error::InvalidKey);
        }
        let mut der = Zeroizing::new(vec![
            0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22,
            0x04, 0x20,
        ]);
        der.extend_from_slice(&bytes[16..48]);
        Self::from_pair(
            pair,
            Zeroizing::new(format!(
                "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
                STANDARD.encode(&*der)
            )),
        )
    }
    fn from_pair(pair: Ed25519KeyPair, private_pem: Zeroizing<String>) -> Result<Self, Error> {
        let mut der = vec![
            0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
        ];
        der.extend_from_slice(pair.public_key().as_ref());
        let public_pem = format!(
            "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----\n",
            STANDARD.encode(der)
        );
        Ok(Self {
            pair,
            private_pem,
            public_pem,
        })
    }
    pub fn load(private: &[u8], public: &[u8]) -> Result<Self, Error> {
        if private.len() > 4096 {
            return Err(Error::InvalidKey);
        }
        let text = std::str::from_utf8(private).map_err(|_| Error::InvalidKey)?;
        let body = text
            .trim()
            .strip_prefix("-----BEGIN PRIVATE KEY-----")
            .and_then(|s| s.strip_suffix("-----END PRIVATE KEY-----"))
            .ok_or(Error::InvalidKey)?;
        let der = Zeroizing::new(
            STANDARD
                .decode(body.split_whitespace().collect::<String>())
                .map_err(|_| Error::InvalidKey)?,
        );
        let pair =
            Ed25519KeyPair::from_pkcs8_maybe_unchecked(&der).map_err(|_| Error::InvalidKey)?;
        if codec::parse_public_key(public)?.as_slice() != pair.public_key().as_ref() {
            return Err(Error::InvalidKey);
        }
        let mut key = Self::from_pair(pair, Zeroizing::new(text.to_owned()))?;
        key.public_pem = std::str::from_utf8(public)
            .map_err(|_| Error::InvalidKey)?
            .to_owned();
        Ok(key)
    }
    /// Only the protected namespace writer may persist this; never serialize a CompanyKey.
    pub(crate) fn private_pem(&self) -> &[u8] {
        self.private_pem.as_bytes()
    }
    pub fn public_pem(&self) -> &str {
        &self.public_pem
    }
    pub fn hint(&self) -> String {
        codec::sha256(self.public_pem.trim().as_bytes())[..16].to_owned()
    }
    /// Invoke once before the existing source backup serializes/signs its manifest with HMAC.
    pub fn decorate(&self, manifest: &mut codec::Manifest) {
        manifest.asymmetric_signature = Some(codec::AsymmetricSignature {
            algorithm: "ed25519".into(),
            public_key_hint: self.hint(),
            public_key_path: PUBLIC.into(),
            signature_path: SIGNATURE.into(),
        });
    }
    /// Sign the SAME final manifest text as HMAC, including its trailing LF.
    pub fn sign(&self, text: &[u8]) -> Result<Vec<u8>, Error> {
        if text.len() > crate::backup_artifact::MAX_MANIFEST_BYTES {
            return Err(Error::Bounds);
        }
        Ok(format!("{}\n", STANDARD.encode(self.pair.sign(text).as_ref())).into_bytes())
    }
}
/// Public-only auditor verification. Embedded keys prove integrity, never authenticity.
pub fn verify(
    files: &codec::Files,
    public: Option<&[u8]>,
    hmac_key: Option<&[u8]>,
) -> serde_json::Value {
    let mut errors = Vec::<String>::new();
    let mut algorithms = Vec::<&str>::new();
    let mut authentic = false;
    let manifest = files.get("manifest.json").and_then(|b| {
        if b.len() <= crate::backup_artifact::MAX_MANIFEST_BYTES {
            serde_json::from_slice::<codec::Manifest>(b).ok()
        } else {
            None
        }
    });
    let Some(m) = manifest else {
        return serde_json::json!({"ok":false,"algorithms":[],"trustLevel":"none","errors":["invalid or missing backup manifest"]});
    };
    let text = &files["manifest.json"];
    if let Some(a) = &m.asymmetric_signature {
        let checked = (|| {
            if a.algorithm != "ed25519"
                || a.public_key_path != PUBLIC
                || a.signature_path != SIGNATURE
            {
                return Err(Error::Signature);
            }
            let embedded = files.get(PUBLIC).ok_or(Error::MissingEntry)?;
            let key = public.unwrap_or(embedded);
            if codec::sha256(key.trim_ascii())[..16] != a.public_key_hint {
                return Err(Error::Signature);
            }
            let raw = codec::parse_public_key(key)?;
            let sig = STANDARD
                .decode(
                    files
                        .get(SIGNATURE)
                        .ok_or(Error::MissingEntry)?
                        .trim_ascii(),
                )
                .map_err(|_| Error::Signature)?;
            ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, raw)
                .verify(text, &sig)
                .map_err(|_| Error::Signature)
        })();
        if checked.is_ok() {
            algorithms.push("ed25519");
            authentic = public.is_some();
        } else {
            errors.push("ed25519 signature or public key hint mismatch".into());
        }
    }
    if let Some(key) = hmac_key {
        let checked = (|| {
            let tag = files.get("manifest.json.hmac").ok_or(Error::MissingEntry)?;
            let tag =
                codec::symmetric_key(std::str::from_utf8(tag).map_err(|_| Error::Signature)?)?;
            if key.len() != 32 {
                return Err(Error::InvalidKey);
            }
            ring::hmac::verify(
                &ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key),
                text,
                &tag,
            )
            .map_err(|_| Error::Signature)
        })();
        if checked.is_ok() {
            algorithms.push("hmac-sha256");
        } else {
            errors.push("HMAC signature mismatch".into());
        }
    }
    for item in std::iter::once(&m.db_snapshot)
        .chain(&m.copied_files.documents_originals)
        .chain(&m.copied_files.invoices_issued)
        .chain(&m.copied_files.config)
    {
        if !files
            .get(&item.path)
            .is_some_and(|b| b.len() as u64 == item.size_bytes && codec::sha256(b) == item.sha256)
        {
            errors.push(format!("size or sha256 mismatch: {}", item.path));
        }
    }
    if algorithms.is_empty() && errors.is_empty() {
        errors.push("no verifiable signature found: manifest has no asymmetricSignature block and no HMAC key was provided or inferable".into());
    }
    let ok = errors.is_empty() && !algorithms.is_empty();
    serde_json::json!({"ok":ok,"backupId":m.backup_id,"algorithms":algorithms,"trustLevel":if !ok {"none"} else if authentic {"third-party-authenticity"} else {"integrity-only"},"publicKeyHint":m.asymmetric_signature.map(|a|a.public_key_hint),"hmacKeyHint":m.manifest_signature.key_hint,"errors":errors})
}

/// Read-only trusted local auditor input. Every component is checked by the existing protected
/// namespace walker; no SQLite is opened and no live workspace path is permitted as payload.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub fn verify_local(
    backup: &std::path::Path,
    public: Option<&std::path::Path>,
    hmac: Option<&std::path::Path>,
    workspace: &std::path::Path,
    uid: u32,
) -> Result<serde_json::Value, &'static str> {
    use rustix::fs::{self, AtFlags, FileType, Mode, OFlags};
    use std::{fs::File, io::Read};
    fn read(path: &std::path::Path, uid: u32, max: usize) -> Result<Vec<u8>, &'static str> {
        let dir = crate::namespace::backup_directory(
            path.parent().ok_or("unsafe verification path")?,
            uid,
        )
        .map_err(|_| "unsafe verification path")?;
        let name = path.file_name().ok_or("unsafe verification path")?;
        let before = fs::statat(&dir, name, AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|_| "verification input unavailable")?;
        if FileType::from_raw_mode(before.st_mode) != FileType::RegularFile
            || before.st_uid != uid
            || before.st_nlink != 1
            || before.st_mode & 0o7777 != 0o600
            || before.st_size < 0
            || before.st_size as u64 > max as u64
        {
            return Err("unsafe verification file");
        }
        let mut file = File::from(
            fs::openat(
                &dir,
                name,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )
            .map_err(|_| "verification input unavailable")?,
        );
        crate::namespace::backup_file_acl(&file).map_err(|_| "unsafe verification file")?;
        let opened = fs::fstat(&file).map_err(|_| "verification input unavailable")?;
        if opened.st_dev != before.st_dev || opened.st_ino != before.st_ino {
            return Err("verification input changed");
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(max as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "verification input unavailable")?;
        let after = fs::statat(&dir, name, AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|_| "verification input changed")?;
        if after.st_dev != before.st_dev
            || after.st_ino != before.st_ino
            || after.st_nlink != 1
            || after.st_uid != uid
            || after.st_mode != before.st_mode
            || after.st_size != before.st_size
            || after.st_mtime != before.st_mtime
            || after.st_mtime_nsec != before.st_mtime_nsec
            || after.st_ctime != before.st_ctime
            || after.st_ctime_nsec != before.st_ctime_nsec
            || bytes.len() as i64 != before.st_size
        {
            return Err("verification input changed");
        }
        Ok(bytes)
    }
    fn inventory(
        path: &std::path::Path,
        prefix: &str,
        uid: u32,
        files: &mut codec::Files,
    ) -> Result<(), &'static str> {
        if prefix.len() > 256 || prefix.split('/').count() > 32 {
            return Err("verification bounds exceeded");
        }
        let dir =
            crate::namespace::backup_directory(path, uid).map_err(|_| "unsafe backup directory")?;
        for entry in fs::Dir::read_from(&dir).map_err(|_| "backup unavailable")? {
            let entry = entry.map_err(|_| "backup unavailable")?;
            let name = entry
                .file_name()
                .to_str()
                .map_err(|_| "unsafe backup entry")?;
            if matches!(name, "." | "..") {
                continue;
            }
            let st = fs::statat(&dir, name, AtFlags::SYMLINK_NOFOLLOW)
                .map_err(|_| "backup unavailable")?;
            let relative = format!("{prefix}{name}");
            if FileType::from_raw_mode(st.st_mode) == FileType::Directory {
                inventory(&path.join(name), &format!("{relative}/"), uid, files)?;
            } else {
                let bytes = read(
                    &path.join(name),
                    uid,
                    crate::backup_artifact::MAX_FILE_BYTES,
                )?;
                files.insert(relative, bytes);
                if files.len() > crate::backup_artifact::MAX_ENTRIES
                    || files.values().map(Vec::len).sum::<usize>()
                        > crate::backup_artifact::MAX_TOTAL_BYTES
                {
                    return Err("verification bounds exceeded");
                }
            }
        }
        Ok(())
    }
    for path in std::iter::once(backup).chain(public).chain(hmac) {
        if !path.is_absolute()
            || path.components().any(|p| {
                matches!(
                    p,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                )
            })
        {
            return Err("unsafe verification path");
        }
    }
    if backup.starts_with(workspace) {
        let parts: Vec<_> = backup
            .strip_prefix(workspace)
            .map_err(|_| "unsafe backup path")?
            .components()
            .collect();
        if parts.len() != 3 || parts[1].as_os_str() != "backups" {
            return Err("live workspace verification source refused");
        }
    }
    for path in public.into_iter().chain(hmac) {
        if path.starts_with(workspace) {
            let parts: Vec<_> = path
                .strip_prefix(workspace)
                .map_err(|_| "unsafe key path")?
                .components()
                .collect();
            let fixed = parts.len() == 2 && parts[1].as_os_str() == ".backup-manifest.key"
                || parts.len() == 3
                    && parts[1].as_os_str() == "config"
                    && parts[2].as_os_str() == "backup-manifest.pub"
                || parts.len() == 3
                    && parts[1].as_os_str() == "backup-keys-archive"
                    && path.extension().is_some_and(|s| s == "pem")
                    && path
                        .file_name()
                        .is_some_and(|s| s.to_string_lossy().ends_with(".pub.pem"));
            if !fixed {
                return Err("live workspace verification key refused");
            }
        }
    }
    let public = public.map(|p| read(p, uid, 1024)).transpose()?;
    let inferred = if hmac.is_none()
        && backup
            .parent()
            .and_then(std::path::Path::file_name)
            .is_some_and(|s| s == "backups")
    {
        backup
            .parent()
            .and_then(std::path::Path::parent)
            .map(|p| p.join(".backup-manifest.key"))
            .filter(|p| p.exists())
    } else {
        None
    };
    let hmac = hmac
        .or(inferred.as_deref())
        .map(|p| read(p, uid, 65))
        .transpose()?
        .map(Zeroizing::new);
    let hmac = hmac
        .as_ref()
        .map(|b| {
            codec::symmetric_key(std::str::from_utf8(b).map_err(|_| "invalid HMAC key")?)
                .map(Zeroizing::new)
                .map_err(|_| "invalid HMAC key")
        })
        .transpose()?;
    let mut files = codec::Files::new();
    inventory(backup, "", uid, &mut files)?;
    Ok(verify(
        &files,
        public.as_deref(),
        hmac.as_ref().map(|b| b.as_slice()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original716_sign_verify_public_export_rotate_old_signature_still_verifies() {
        let key = CompanyKey::generate().unwrap();
        let loaded = CompanyKey::load(key.private_pem(), key.public_pem().as_bytes()).unwrap();
        assert_eq!(loaded.hint(), key.hint());
        let (mut files, hmac) = codec::tests::fixture();
        let mut manifest: codec::Manifest =
            serde_json::from_slice(&files["manifest.json"]).unwrap();
        let public = key.public_pem().as_bytes().to_vec(); // independent synthetic public export
        files.insert(PUBLIC.into(), public.clone());
        manifest.copied_files.config.push(codec::File {
            path: PUBLIC.into(),
            sha256: codec::sha256(&public),
            size_bytes: public.len() as u64,
        });
        let proof = codec::seal_original716(files, manifest, &hmac, Some(&key)).unwrap();
        assert_eq!(
            verify(proof.files(), Some(&public), None)["trustLevel"],
            "third-party-authenticity"
        );
        assert_eq!(
            verify(proof.files(), None, None)["trustLevel"],
            "integrity-only"
        );
        assert_eq!(
            verify(proof.files(), Some(&public), Some(&hmac))["algorithms"],
            serde_json::json!(["ed25519", "hmac-sha256"])
        );
        let next = CompanyKey::generate().unwrap();
        assert_ne!(next.hint(), key.hint());
        assert!(CompanyKey::load(key.private_pem(), next.public_pem().as_bytes()).is_err());
        assert_eq!(
            verify(proof.files(), Some(next.public_pem().as_bytes()), None)["ok"],
            false
        );
        // Rotation never invalidates the archived independently held old public key.
        assert_eq!(verify(proof.files(), Some(&public), None)["ok"], true);
        assert!(!proof.files().contains_key(PRIVATE));
        let mut tampered = proof.files().clone();
        tampered.get_mut("ledger.sqlite").unwrap().push(0);
        assert_eq!(verify(&tampered, Some(&public), None)["ok"], false);
        let mut tampered = proof.files().clone();
        tampered.get_mut(SIGNATURE).unwrap()[0] ^= 1;
        assert_eq!(verify(&tampered, Some(&public), None)["ok"], false);
    }
}
