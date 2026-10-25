//! Original company backup byte codec. This is deliberately separate from the native workspace
//! envelope. No path is opened, no authority is inferred and no database is restored here.
//! The admitted Owner must supply the complete company/config inventory and retain the HMAC key.
use super::{Error, MAX_ENTRIES, MAX_FILE_BYTES, MAX_MANIFEST_BYTES, MAX_TOTAL_BYTES};
use base64::{Engine, engine::general_purpose::STANDARD};
use ring::{hmac, signature};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const RULE: &str = "DK-BOOKKEEPING-BACKUP-001";
pub type Files = BTreeMap<String, Vec<u8>>;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct File {
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopiedFiles {
    pub documents_originals: Vec<File>,
    pub invoices_issued: Vec<File>,
    pub config: Vec<File>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestSignature {
    pub algorithm: String,
    pub key_hint: String,
    pub signature_path: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AsymmetricSignature {
    pub algorithm: String,
    pub public_key_hint: String,
    pub public_key_path: String,
    pub signature_path: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerStats {
    pub journal_entries: u64,
    pub documents: u64,
    pub bank_transactions: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_version: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provenance: Option<serde_json::Value>,
    pub backup_id: String,
    pub created_at: String,
    pub rule_ids: Vec<String>,
    pub manifest_signature: ManifestSignature,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asymmetric_signature: Option<AsymmetricSignature>,
    pub db_snapshot: File,
    pub copied_files: CopiedFiles,
    pub ledger_stats: LedgerStats,
}
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.contains(['\\', '\0', ':'])
        && value
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
}
fn unhex(value: &str) -> Result<Vec<u8>, Error> {
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::InvalidKey);
    }
    (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).map_err(|_| Error::InvalidKey))
        .collect()
}
/// Decode the source's `.backup-manifest.key` text, not an Ed25519 key.
pub fn symmetric_key(text: &str) -> Result<Vec<u8>, Error> {
    unhex(text.trim())
}
pub fn sign(manifest_text: &[u8], key: &[u8]) -> Result<String, Error> {
    if key.len() != 32 || manifest_text.len() > MAX_MANIFEST_BYTES {
        return Err(Error::Bounds);
    }
    let tag = hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, key), manifest_text);
    Ok(tag.as_ref().iter().map(|b| format!("{b:02x}")).collect())
}
fn bounds(files: &Files) -> Result<(), Error> {
    if files.len() > MAX_ENTRIES {
        return Err(Error::Bounds);
    }
    let mut total = 0usize;
    for (name, bytes) in files {
        if !path(name) {
            return Err(Error::UnsafeEntry);
        }
        total = total.checked_add(bytes.len()).ok_or(Error::Bounds)?;
        if bytes.len() > MAX_FILE_BYTES || total > MAX_TOTAL_BYTES {
            return Err(Error::Bounds);
        }
    }
    Ok(())
}
/// Verify exact manifest text, HMAC and every source inventory member. The optional independently
/// obtained public key is raw Ed25519; a key supplied by the artifact never becomes authority.
/// Database schema/audit validation is still required inside the admitted Owner before publication.
pub fn verify(
    files: &Files,
    key: &[u8],
    independent_public_key: Option<&[u8]>,
) -> Result<VerifiedOriginal717, Error> {
    bounds(files)?;
    if key.len() != 32 {
        return Err(Error::InvalidKey);
    }
    let text = files.get("manifest.json").ok_or(Error::MissingEntry)?;
    if text.len() > MAX_MANIFEST_BYTES {
        return Err(Error::Bounds);
    }
    let tag = std::str::from_utf8(files.get("manifest.json.hmac").ok_or(Error::MissingEntry)?)
        .map_err(|_| Error::Signature)?;
    hmac::verify(
        &hmac::Key::new(hmac::HMAC_SHA256, key),
        text,
        &unhex(tag.trim())?,
    )
    .map_err(|_| Error::Signature)?;
    let manifest: Manifest = serde_json::from_slice(text).map_err(|_| Error::Malformed)?;
    match manifest.manifest_version {
        None | Some(1) if manifest.provenance.is_none() => {}
        Some(2) if manifest.provenance.as_ref().is_some_and(provenance::valid) => {}
        _ => return Err(Error::Coverage),
    }
    if manifest.backup_id.is_empty()
        || !path(&manifest.backup_id)
        || manifest.backup_id.contains('/')
        || chrono::DateTime::parse_from_rfc3339(&manifest.created_at).is_err()
        || manifest.manifest_signature.algorithm != "hmac-sha256"
        || manifest.manifest_signature.signature_path != "manifest.json.hmac"
        || manifest.manifest_signature.key_hint != sha256(key)[..16]
        || manifest.db_snapshot.path != "ledger.sqlite"
        || !manifest.rule_ids.iter().any(|r| r == RULE)
    {
        return Err(Error::Malformed);
    }
    let mut seen = BTreeSet::new();
    let mut check = |item: &File, prefix: Option<&str>| -> Result<(), Error> {
        if !path(&item.path) || prefix.is_some_and(|p| !item.path.starts_with(p)) {
            return Err(Error::UnsafeEntry);
        }
        if !seen.insert(item.path.clone()) {
            return Err(Error::Duplicate);
        }
        let bytes = files.get(&item.path).ok_or(Error::MissingEntry)?;
        if item.size_bytes != bytes.len() as u64 || item.sha256 != sha256(bytes) {
            return Err(Error::Inventory);
        }
        Ok(())
    };
    check(&manifest.db_snapshot, None)?;
    for (items, prefix) in [
        (
            &manifest.copied_files.documents_originals,
            "documents-originals/",
        ),
        (&manifest.copied_files.invoices_issued, "invoices-issued/"),
        (&manifest.copied_files.config, "config/"),
    ] {
        for item in items {
            check(item, Some(prefix))?;
        }
    }
    if let Some(asymmetric) = &manifest.asymmetric_signature {
        if asymmetric.algorithm != "ed25519"
            || asymmetric.signature_path != "manifest.json.ed25519.sig"
            || !path(&asymmetric.public_key_path)
        {
            return Err(Error::Signature);
        }
        // Require independent key custody here, rather than invent PEM/config authority.
        let public = parse_public_key(independent_public_key.ok_or(Error::InvalidKey)?)?;
        let embedded = files
            .get(&asymmetric.public_key_path)
            .ok_or(Error::MissingEntry)?;
        if parse_public_key(embedded)? != public
            || asymmetric.public_key_hint != sha256(embedded.trim_ascii())[..16]
            || !seen.contains(&asymmetric.public_key_path)
        {
            return Err(Error::Signature);
        }
        let sig = STANDARD
            .decode(
                files
                    .get("manifest.json.ed25519.sig")
                    .ok_or(Error::MissingEntry)?
                    .as_slice()
                    .trim_ascii(),
            )
            .map_err(|_| Error::Signature)?;
        signature::UnparsedPublicKey::new(&signature::ED25519, &public)
            .verify(text, &sig)
            .map_err(|_| Error::Signature)?;
    }
    seen.insert("manifest.json".into());
    seen.insert("manifest.json.hmac".into());
    if manifest.asymmetric_signature.is_some() {
        seen.insert("manifest.json.ed25519.sig".into());
    }
    if files.keys().any(|p| !seen.contains(p)) {
        return Err(Error::Inventory);
    }
    Ok(VerifiedOriginal717 {
        manifest,
        files: files.clone(),
    })
}
fn octal(out: &mut [u8], value: usize) -> Result<(), Error> {
    let digits = format!("{:0width$o}", value, width = out.len() - 1);
    if digits.len() >= out.len() {
        return Err(Error::Bounds);
    }
    out[..digits.len()].copy_from_slice(digits.as_bytes());
    Ok(())
}
/// Source-compatible deterministic USTAR (sorted, uid/gid/mtime zero, regular files only).
pub fn archive(files: &Files) -> Result<Vec<u8>, Error> {
    bounds(files)?;
    let mut out = Vec::new();
    for (name, bytes) in files {
        let (prefix, leaf) = if name.len() <= 100 {
            ("", name.as_str())
        } else {
            name.match_indices('/')
                .find_map(|(i, _)| {
                    (i <= 155 && name.len() - i - 1 <= 100).then_some((&name[..i], &name[i + 1..]))
                })
                .ok_or(Error::UnsafeEntry)?
        };
        let mut header = [0u8; 512];
        header[..leaf.len()].copy_from_slice(leaf.as_bytes());
        header[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
        for (range, value) in [
            (100..108, 0o644),
            (108..116, 0),
            (116..124, 0),
            (124..136, bytes.len()),
            (136..148, 0),
            (329..337, 0),
            (337..345, 0),
        ] {
            octal(&mut header[range], value)?;
        }
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        header[148..156].fill(b' ');
        let checksum: usize = header.iter().map(|b| *b as usize).sum();
        let sum = format!("{checksum:06o}");
        header[148..154].copy_from_slice(sum.as_bytes());
        header[154] = 0;
        out.extend_from_slice(&header);
        out.extend_from_slice(bytes);
        out.resize(out.len().div_ceil(512) * 512, 0);
    }
    out.resize(out.len() + 1024, 0);
    Ok(out)
}
fn number(bytes: &[u8]) -> Result<usize, Error> {
    let end = bytes
        .iter()
        .position(|b| *b == 0 || *b == b' ')
        .unwrap_or(bytes.len());
    if end == 0 {
        return Ok(0);
    }
    let text = std::str::from_utf8(&bytes[..end]).map_err(|_| Error::Malformed)?;
    usize::from_str_radix(text, 8).map_err(|_| Error::Malformed)
}
fn string(bytes: &[u8]) -> Result<&str, Error> {
    std::str::from_utf8(&bytes[..bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len())])
        .map_err(|_| Error::Malformed)
}
/// Parse in memory only. Links, PAX/GNU indirection, traversal, duplicate paths and truncation
/// fail closed rather than invoking a filesystem extractor or silently dropping unknown entries.
pub fn read_archive(bytes: &[u8]) -> Result<Files, Error> {
    if bytes.len() > MAX_TOTAL_BYTES + (MAX_ENTRIES + 2) * 1024 || bytes.len() % 512 != 0 {
        return Err(Error::Bounds);
    }
    let mut files = Files::new();
    let mut offset = 0;
    while offset + 512 <= bytes.len() {
        let header = &bytes[offset..offset + 512];
        if header.iter().all(|b| *b == 0) {
            if bytes.len() - offset < 1024 || bytes[offset..].iter().any(|b| *b != 0) {
                return Err(Error::Malformed);
            }
            bounds(&files)?;
            return Ok(files);
        }
        if !matches!(header[156], 0 | b'0') || &header[257..263] != b"ustar\0" {
            return Err(Error::UnsafeEntry);
        }
        let checksum: usize = header
            .iter()
            .enumerate()
            .map(|(i, b)| {
                if (148..156).contains(&i) {
                    32
                } else {
                    *b as usize
                }
            })
            .sum();
        if number(&header[148..156])? != checksum {
            return Err(Error::Malformed);
        }
        let leaf = string(&header[..100])?;
        let prefix = string(&header[345..500])?;
        let name = if prefix.is_empty() {
            leaf.to_owned()
        } else {
            format!("{prefix}/{leaf}")
        };
        if !path(&name) {
            return Err(Error::UnsafeEntry);
        }
        let size = number(&header[124..136])?;
        if size > MAX_FILE_BYTES || files.len() >= MAX_ENTRIES {
            return Err(Error::Bounds);
        }
        offset += 512;
        let end = offset.checked_add(size).ok_or(Error::Bounds)?;
        let next = end.checked_add(511).ok_or(Error::Bounds)? / 512 * 512;
        if next > bytes.len() {
            return Err(Error::Malformed);
        }
        if files.insert(name, bytes[offset..end].to_vec()).is_some() {
            return Err(Error::Duplicate);
        }
        offset = next;
    }
    Err(Error::Malformed)
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn fixture() -> (Files, Vec<u8>) {
        let key = vec![7; 32];
        let mut files = Files::from([
            (
                "ledger.sqlite".into(),
                b"synthetic snapshot codec bytes; not a restored DB".to_vec(),
            ),
            ("config/policy.yaml".into(), b"synthetic config".to_vec()),
        ]);
        let item = |name: &str| File {
            path: name.into(),
            sha256: sha256(&files[name]),
            size_bytes: files[name].len() as u64,
        };
        let manifest = Manifest {
            manifest_version: Some(1),
            provenance: None,
            backup_id: "backup-20260101T000000Z".into(),
            created_at: "2026-01-01T00:00:00.000Z".into(),
            rule_ids: vec![RULE.into()],
            manifest_signature: ManifestSignature {
                algorithm: "hmac-sha256".into(),
                key_hint: sha256(&key)[..16].into(),
                signature_path: "manifest.json.hmac".into(),
            },
            asymmetric_signature: None,
            db_snapshot: item("ledger.sqlite"),
            copied_files: CopiedFiles {
                documents_originals: vec![],
                invoices_issued: vec![],
                config: vec![item("config/policy.yaml")],
            },
            ledger_stats: LedgerStats {
                journal_entries: 0,
                documents: 0,
                bank_transactions: 0,
            },
        };
        let text = serde_json::to_vec_pretty(&manifest).unwrap();
        files.insert(
            "manifest.json.hmac".into(),
            format!("{}\n", sign(&text, &key).unwrap()).into_bytes(),
        );
        files.insert("manifest.json".into(), text);
        (files, key)
    }
    #[test]
    fn original717_v2_provenance_and_complete_inventory() {
        let (mut files, key) = fixture();
        let mut m: Manifest = serde_json::from_slice(&files["manifest.json"]).unwrap();
        m.manifest_version = Some(2);
        m.provenance = Some(
            serde_json::json!({"product":{"version":"1.2.3-beta.1+build","gitCommit":null,"builtAt":null,"bunVersion":"1.2.3","baseImageDigest":null},"schema":{"version":1,"baselineChecksum":"a".repeat(64)},"rules":{"digest":format!("sha256:{}","b".repeat(64))}}),
        );
        let update = |files: &mut Files, m: &Manifest| {
            let bytes = serde_json::to_vec_pretty(m).unwrap();
            files.insert(
                "manifest.json.hmac".into(),
                sign(&bytes, &key).unwrap().into_bytes(),
            );
            files.insert("manifest.json".into(), bytes);
        };
        update(&mut files, &m);
        assert!(verify(&files, &key, None).is_ok());
        files.insert("unlisted".into(), vec![1]);
        assert!(matches!(verify(&files, &key, None), Err(Error::Inventory)));
        files.remove("unlisted");
        m.provenance.as_mut().unwrap()["product"]["version"] = serde_json::json!("01.2.3");
        update(&mut files, &m);
        assert!(matches!(verify(&files, &key, None), Err(Error::Coverage)));
    }
    #[test]
    fn original717_independent_pem_and_raw_signature() {
        use ring::signature::{Ed25519KeyPair, KeyPair};
        let pair = Ed25519KeyPair::from_seed_unchecked(&[9; 32]).unwrap();
        let raw = pair.public_key().as_ref();
        let mut der = vec![
            0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
        ];
        der.extend_from_slice(raw);
        let pem = format!(
            "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----\n",
            STANDARD.encode(der)
        );
        assert_eq!(parse_public_key(pem.as_bytes()).unwrap().as_slice(), raw);
        let (mut files, key) = fixture();
        let mut m: Manifest = serde_json::from_slice(&files["manifest.json"]).unwrap();
        files.insert("config/backup-manifest.pub".into(), pem.as_bytes().to_vec());
        m.copied_files.config.push(File {
            path: "config/backup-manifest.pub".into(),
            sha256: sha256(pem.as_bytes()),
            size_bytes: pem.len() as u64,
        });
        m.asymmetric_signature = Some(AsymmetricSignature {
            algorithm: "ed25519".into(),
            public_key_hint: sha256(pem.trim().as_bytes())[..16].into(),
            public_key_path: "config/backup-manifest.pub".into(),
            signature_path: "manifest.json.ed25519.sig".into(),
        });
        let text = serde_json::to_vec_pretty(&m).unwrap();
        files.insert(
            "manifest.json.hmac".into(),
            sign(&text, &key).unwrap().into_bytes(),
        );
        files.insert(
            "manifest.json.ed25519.sig".into(),
            STANDARD.encode(pair.sign(&text)).into_bytes(),
        );
        files.insert("manifest.json".into(), text);
        assert!(verify(&files, &key, None).is_err());
        assert!(verify(&files, &key, Some(raw)).is_ok());
        assert!(verify(&files, &key, Some(pem.as_bytes())).is_ok());
        assert!(verify(&files, &key, Some(&[8; 32])).is_err());
    }
    #[test]
    fn original717_system_backup_inventory_hmac_codec() {
        let (mut files, key) = fixture();
        assert!(verify(&files, &key, None).is_ok());
        assert!(verify(&files, &[8; 32], None).is_err());
        files.get_mut("config/policy.yaml").unwrap()[0] ^= 1;
        assert!(matches!(verify(&files, &key, None), Err(Error::Inventory)));
    }
    #[test]
    fn original717_system_backup_archive_deterministic_codec() {
        let (files, _) = fixture();
        let wire = archive(&files).unwrap();
        assert_eq!(wire, archive(&files).unwrap());
        assert_eq!(read_archive(&wire).unwrap(), files);
        assert_eq!(&wire[257..265], b"ustar\000");
        assert!(read_archive(&wire[..wire.len() - 1024]).is_err());
    }
    #[test]
    fn original717_archive_refuses_links_traversal_duplicate_unknown() {
        let mut files = Files::from([("../escape".into(), vec![1])]);
        assert!(archive(&files).is_err());
        files = Files::from([("safe".into(), vec![1])]);
        let wire = archive(&files).unwrap();
        let mut duplicate = wire[..1024].to_vec();
        duplicate.extend_from_slice(&wire);
        assert!(matches!(read_archive(&duplicate), Err(Error::Duplicate)));
        for kind in [b'1', b'2', b'x', b'L', b'5'] {
            let mut bad = wire.clone();
            bad[156] = kind;
            assert!(matches!(read_archive(&bad), Err(Error::UnsafeEntry)));
        }
    }
}

/// Verified bytes only, never a deserializable assertion. This is integrity, not a business grant.
pub struct VerifiedOriginal717 {
    manifest: Manifest,
    files: Files,
}
impl VerifiedOriginal717 {
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    pub fn files(&self) -> &Files {
        &self.files
    }
}
/// Independent raw Ed25519 or standard SPKI PEM. Artifact keys are not trusted input.
pub fn parse_public_key(bytes: &[u8]) -> Result<[u8; 32], Error> {
    if bytes.len() == 32 {
        return bytes.try_into().map_err(|_| Error::InvalidKey);
    }
    if bytes.len() > 1024 {
        return Err(Error::InvalidKey);
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| Error::InvalidKey)?
        .trim();
    let body = text
        .strip_prefix("-----BEGIN PUBLIC KEY-----")
        .and_then(|s| s.strip_suffix("-----END PUBLIC KEY-----"))
        .ok_or(Error::InvalidKey)?;
    let der = STANDARD
        .decode(body.split_whitespace().collect::<String>())
        .map_err(|_| Error::InvalidKey)?;
    const PREFIX: &[u8] = &[
        0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
    ];
    if der.len() != 44 || !der.starts_with(PREFIX) {
        return Err(Error::InvalidKey);
    }
    der[12..].try_into().map_err(|_| Error::InvalidKey)
}
mod provenance;
