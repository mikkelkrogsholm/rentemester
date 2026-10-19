//! Signed, bounded native data container; NOT SQLite validation, restore admission or filesystem authority.
//!
//! The producer supplies exact owned snapshot bytes and truthful section counts. In particular it must
//! count unsupported business sections under its ownership barrier, and refuse nonempty sections.
//! Raw control databases, config, credentials and grants have no entry kind. Historical actor labels
//! in WorkspaceAudit are evidence, never restored authority. Company database credential filtering and
//! semantic/schema/identity checks belong to the admitted snapshot producer and future restore path.
//!
//! Wire: strict JSON {format, manifestBase64, signatureBase64, entries:[{path, bytesBase64}]}.
//! Ed25519 signs the exact decoded manifest bytes (retained by VerifiedArtifact). Each payload's kind,
//! canonical relative name, length and SHA-256 is authenticated by that manifest. There is no embedded
//! verification key: verify ALWAYS requires an independently supplied trusted 32-byte Ed25519 key.
//! JSON/base64 is a bounded portable representation, not encrypted storage. No paths are opened here.
use base64::{Engine, engine::general_purpose::STANDARD};
use ring::signature::{self, Ed25519KeyPair, KeyPair};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const FORMAT: &str = "rentemester-native-backup/1";
pub const MAX_ENTRIES: usize = 2048;
pub const MAX_FILE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
pub const MAX_WIRE_BYTES: usize = 96 * 1024 * 1024;
pub const MAX_COMPANIES: usize = 64;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Bounds,
    Malformed,
    Identity,
    Coverage,
    UnsafeEntry,
    Duplicate,
    MissingEntry,
    Signature,
    InvalidKey,
    Inventory,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Bounds => "backup_bounds_exceeded",
            Self::Malformed => "backup_invalid_format",
            Self::Identity => "backup_invalid_identity",
            Self::Coverage => "backup_unsupported_coverage",
            Self::UnsafeEntry => "backup_unsafe_entry",
            Self::Duplicate => "backup_duplicate_entry",
            Self::MissingEntry => "backup_missing_entry",
            Self::Signature => "backup_signature_refused",
            Self::InvalidKey => "backup_invalid_key",
            Self::Inventory => "backup_inventory_mismatch",
        })
    }
}
impl std::error::Error for Error {}
/// No key generation, fallback, replacement or file custody is performed by this module.
/// Callers own the separately persisted PKCS#8 and must never put it into a snapshot.
pub struct SigningKey(Ed25519KeyPair);
impl SigningKey {
    pub fn from_pkcs8(bytes: &[u8]) -> Result<Self, Error> {
        Ed25519KeyPair::from_pkcs8(bytes)
            .map(Self)
            .map_err(|_| Error::InvalidKey)
    }
    pub(crate) fn sign_detached(&self, bytes: &[u8]) -> [u8; 64] {
        self.0
            .sign(bytes)
            .as_ref()
            .try_into()
            .expect("Ed25519 signature length")
    }
    /// Export separately. The verifier's caller must establish this key's trust out of band.
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.0
            .public_key()
            .as_ref()
            .try_into()
            .expect("ring Ed25519 public key length")
    }
}
/// Explicit verifier input, never resolved from an artifact or from a manifest hint.
pub struct TrustedPublicKey([u8; 32]);
impl TrustedPublicKey {
    pub fn public_key_bytes(&self) -> [u8; 32] {
        self.0
    }
    pub(crate) fn verify_detached(&self, bytes: &[u8], signature: &[u8]) -> Result<(), Error> {
        signature::UnparsedPublicKey::new(&signature::ED25519, &self.0)
            .verify(bytes, signature)
            .map_err(|_| Error::Signature)
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        Ok(Self(bytes.try_into().map_err(|_| Error::InvalidKey)?))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Company {
    pub slug: String,
    pub ledger_uuid: String,
    pub schema_version: u32,
}
/// Closed source-derived control domains. Naming a section does not implement its export codec.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum BusinessSection {
    WorkspaceAudit,
    Parties,
    CorporateRecords,
    CompanyKnowledge,
    Ownership,
    WorkspaceInbox,
    Intercompany,
    Consolidation,
}
impl BusinessSection {
    pub const ALL: [Self; 8] = [
        Self::WorkspaceAudit,
        Self::Parties,
        Self::CorporateRecords,
        Self::CompanyKnowledge,
        Self::Ownership,
        Self::WorkspaceInbox,
        Self::Intercompany,
        Self::Consolidation,
    ];
    fn name(self) -> &'static str {
        match self {
            Self::WorkspaceAudit => "workspace-audit",
            Self::Parties => "parties",
            Self::CorporateRecords => "corporate-records",
            Self::CompanyKnowledge => "company-knowledge",
            Self::Ownership => "ownership",
            Self::WorkspaceInbox => "workspace-inbox",
            Self::Intercompany => "intercompany",
            Self::Consolidation => "consolidation",
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum Exclusion {
    AuthenticationCredentials,
    LiveGrants,
    OperationalKeys,
    SigningPrivateKey,
    RuntimeState,
}
impl Exclusion {
    pub const ALL: [Self; 5] = [
        Self::AuthenticationCredentials,
        Self::LiveGrants,
        Self::OperationalKeys,
        Self::SigningPrivateKey,
        Self::RuntimeState,
    ];
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct UnsupportedSection {
    pub section: BusinessSection,
    pub record_count: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Coverage {
    pub included: Vec<BusinessSection>,
    pub unsupported: Vec<UnsupportedSection>,
    pub excluded: Vec<Exclusion>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Context {
    /// Existing stable logical workspace identity supplied by the admitted owner, not a directory.
    pub workspace_id: String,
    /// Canonical UTC RFC3339 with milliseconds, e.g. 2026-01-02T03:04:05.000Z.
    pub created_at: String,
    pub control_schema_version: u32,
    pub companies: Vec<Company>,
    pub coverage: Coverage,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum EntryKind {
    WorkspaceManifest,
    CompanyLedger { company: String },
    DocumentOriginal { company: String },
    BankStatement { company: String },
    ControlBusiness { section: BusinessSection },
}
/// Intentionally bytes only: no file, link target, directory recursion or extraction callback.
pub struct Snapshot {
    pub kind: EntryKind,
    pub path: String,
    pub bytes: Vec<u8>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct InventoryEntry {
    pub kind: EntryKind,
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Manifest {
    pub format: String,
    pub signing_public_key_sha256: String,
    pub context: Context,
    pub inventory: Vec<InventoryEntry>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Envelope {
    format: String,
    manifest_base64: String,
    signature_base64: String,
    entries: Vec<WireEntry>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct WireEntry {
    path: String,
    bytes_base64: String,
}
/// Successful signature and inventory verification only. No authority to publish/restore/open a DB.
/// Semantic database, migration and source/replay identity validation still MUST precede restore.
pub struct VerifiedArtifact {
    manifest: Manifest,
    manifest_bytes: Vec<u8>,
    entries: Vec<VerifiedEntry>,
}
pub struct VerifiedEntry {
    kind: EntryKind,
    path: String,
    bytes: Vec<u8>,
}
impl VerifiedArtifact {
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    pub fn manifest_bytes(&self) -> &[u8] {
        &self.manifest_bytes
    }
    pub fn entries(&self) -> &[VerifiedEntry] {
        &self.entries
    }
}
impl VerifiedEntry {
    pub fn kind(&self) -> &EntryKind {
        &self.kind
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && s.bytes()
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn context(value: &Context) -> Result<(), Error> {
    if value.workspace_id.trim().is_empty()
        || value.workspace_id.len() > 1024
        || value.workspace_id.chars().any(char::is_control)
        || value.control_schema_version == 0
        || value.companies.is_empty()
        || value.companies.len() > MAX_COMPANIES
    {
        return Err(Error::Identity);
    }
    let date =
        chrono::DateTime::parse_from_rfc3339(&value.created_at).map_err(|_| Error::Identity)?;
    if date
        .with_timezone(&chrono::Utc)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        != value.created_at
    {
        return Err(Error::Identity);
    }
    let mut slugs = BTreeSet::new();
    let mut ledgers = BTreeSet::new();
    for company in &value.companies {
        if !slug(&company.slug)
            || !hex(&company.ledger_uuid, 32)
            || company.schema_version == 0
            || !slugs.insert(&company.slug)
            || !ledgers.insert(&company.ledger_uuid)
        {
            return Err(Error::Identity);
        }
    }
    let mut sections = BTreeSet::new();
    for section in &value.coverage.included {
        if !sections.insert(*section) {
            return Err(Error::Coverage);
        }
    }
    if !sections.contains(&BusinessSection::WorkspaceAudit) {
        return Err(Error::Coverage);
    }
    for section in &value.coverage.unsupported {
        if section.record_count != 0 || !sections.insert(section.section) {
            return Err(Error::Coverage);
        }
    }
    if sections != BusinessSection::ALL.into_iter().collect()
        || value.coverage.excluded.len() != Exclusion::ALL.len()
        || value
            .coverage
            .excluded
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            != Exclusion::ALL.into_iter().collect()
    {
        return Err(Error::Coverage);
    }
    Ok(())
}
fn canonical_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 512
        && path.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && !part.starts_with('.')
                && part.bytes().all(|b| {
                    b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.')
                })
        })
}
fn entry(entry: &InventoryEntry, ctx: &Context) -> Result<(), Error> {
    if !canonical_path(&entry.path) || !hex(&entry.sha256, 64) {
        return Err(Error::UnsafeEntry);
    }
    if entry.size_bytes > MAX_FILE_BYTES as u64 {
        return Err(Error::Bounds);
    }
    let member = |company: &str| ctx.companies.iter().any(|c| c.slug == company);
    let original = |company: &str, prefix: &str, extensions: &[&str]| {
        if !member(company) {
            return false;
        }
        extensions
            .iter()
            .any(|ext| entry.path == format!("{company}/{prefix}/{}.{ext}", entry.sha256))
    };
    let allowed = match &entry.kind {
        EntryKind::WorkspaceManifest => entry.path == "workspace.json",
        EntryKind::CompanyLedger { company } => {
            member(company) && entry.path == format!("{company}/data/ledger.sqlite")
        }
        EntryKind::DocumentOriginal { company } => original(
            company,
            "documents/originals",
            &["pdf", "png", "jpg", "jpeg", "txt", "json", "xml"],
        ),
        EntryKind::BankStatement { company } => original(company, "bank/processed", &["txt"]),
        EntryKind::ControlBusiness { section } => {
            ctx.coverage.included.contains(section)
                && entry.path == format!("control/{}.json", section.name())
        }
    };
    if !allowed || entry.size_bytes == 0 {
        return Err(Error::UnsafeEntry);
    }
    Ok(())
}
fn inventory(manifest: &Manifest) -> Result<(), Error> {
    if manifest.format != FORMAT || !hex(&manifest.signing_public_key_sha256, 64) {
        return Err(Error::Malformed);
    }
    context(&manifest.context)?;
    if manifest.inventory.is_empty() || manifest.inventory.len() > MAX_ENTRIES {
        return Err(Error::Bounds);
    }
    let mut paths = BTreeSet::new();
    let mut total = 0u64;
    for item in &manifest.inventory {
        entry(item, &manifest.context)?;
        if !paths.insert(&item.path) {
            return Err(Error::Duplicate);
        }
        total = total.checked_add(item.size_bytes).ok_or(Error::Bounds)?;
        if total > MAX_TOTAL_BYTES as u64 {
            return Err(Error::Bounds);
        }
    }
    if !paths.contains(&"workspace.json".to_owned()) {
        return Err(Error::MissingEntry);
    }
    for company in &manifest.context.companies {
        if !paths.contains(&format!("{}/data/ledger.sqlite", company.slug)) {
            return Err(Error::MissingEntry);
        }
    }
    for section in &manifest.context.coverage.included {
        if !paths.contains(&format!("control/{}.json", section.name())) {
            return Err(Error::MissingEntry);
        }
    }
    Ok(())
}
fn decode(text: &str, max: usize) -> Result<Vec<u8>, Error> {
    let encoded_max = max
        .checked_add(2)
        .and_then(|n| n.checked_div(3))
        .and_then(|n| n.checked_mul(4))
        .ok_or(Error::Bounds)?;
    if text.len() > encoded_max {
        return Err(Error::Bounds);
    }
    let bytes = STANDARD.decode(text).map_err(|_| Error::Malformed)?;
    if bytes.len() > max {
        return Err(Error::Bounds);
    }
    Ok(bytes)
}
/// Exact supplied bytes only. No discovery/copying of an unknown directory or control database.
/// Stable ordering makes repeated creation deterministic for the same context, snapshots and key.
pub fn create(
    mut ctx: Context,
    mut snapshots: Vec<Snapshot>,
    key: &SigningKey,
) -> Result<Vec<u8>, Error> {
    context(&ctx)?;
    if snapshots.is_empty() || snapshots.len() > MAX_ENTRIES {
        return Err(Error::Bounds);
    }
    let total = snapshots.iter().try_fold(0usize, |total, s| {
        total.checked_add(s.bytes.len()).ok_or(Error::Bounds)
    })?;
    if total > MAX_TOTAL_BYTES || snapshots.iter().any(|s| s.bytes.len() > MAX_FILE_BYTES) {
        return Err(Error::Bounds);
    }
    ctx.companies.sort_by(|a, b| a.slug.cmp(&b.slug));
    ctx.coverage.included.sort();
    ctx.coverage.unsupported.sort_by_key(|s| s.section);
    ctx.coverage.excluded.sort();
    snapshots.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest = Manifest {
        format: FORMAT.into(),
        signing_public_key_sha256: hash(&key.public_key_bytes()),
        context: ctx,
        inventory: snapshots
            .iter()
            .map(|s| InventoryEntry {
                kind: s.kind.clone(),
                path: s.path.clone(),
                size_bytes: s.bytes.len() as u64,
                sha256: hash(&s.bytes),
            })
            .collect(),
    };
    inventory(&manifest)?;
    let manifest_bytes = serde_json::to_vec(&manifest).map_err(|_| Error::Malformed)?;
    if manifest_bytes.len() > MAX_MANIFEST_BYTES {
        return Err(Error::Bounds);
    }
    let signature = key.0.sign(&manifest_bytes);
    let envelope = Envelope {
        format: FORMAT.into(),
        manifest_base64: STANDARD.encode(&manifest_bytes),
        signature_base64: STANDARD.encode(signature.as_ref()),
        entries: snapshots
            .into_iter()
            .map(|s| WireEntry {
                path: s.path,
                bytes_base64: STANDARD.encode(s.bytes),
            })
            .collect(),
    };
    let wire = serde_json::to_vec(&envelope).map_err(|_| Error::Malformed)?;
    if wire.len() > MAX_WIRE_BYTES {
        return Err(Error::Bounds);
    }
    Ok(wire)
}
/// Trust cannot be bootstrapped from this artifact. Caller must separately obtain the genuine key.
pub fn verify(wire: &[u8], trusted: &TrustedPublicKey) -> Result<VerifiedArtifact, Error> {
    if wire.is_empty() || wire.len() > MAX_WIRE_BYTES {
        return Err(Error::Bounds);
    }
    let envelope: Envelope = serde_json::from_slice(wire).map_err(|_| Error::Malformed)?;
    if envelope.format != FORMAT {
        return Err(Error::Malformed);
    }
    if envelope.entries.len() > MAX_ENTRIES {
        return Err(Error::Bounds);
    }
    let manifest_bytes = decode(&envelope.manifest_base64, MAX_MANIFEST_BYTES)?;
    let sig = decode(&envelope.signature_base64, 64)?;
    if sig.len() != 64 {
        return Err(Error::Signature);
    }
    signature::UnparsedPublicKey::new(&signature::ED25519, &trusted.0)
        .verify(&manifest_bytes, &sig)
        .map_err(|_| Error::Signature)?;
    let manifest: Manifest =
        serde_json::from_slice(&manifest_bytes).map_err(|_| Error::Malformed)?;
    if manifest.signing_public_key_sha256 != hash(&trusted.0) {
        return Err(Error::Signature);
    }
    inventory(&manifest)?;
    let mut payloads = BTreeMap::new();
    for item in envelope.entries {
        if !canonical_path(&item.path) {
            return Err(Error::UnsafeEntry);
        }
        if payloads.insert(item.path, item.bytes_base64).is_some() {
            return Err(Error::Duplicate);
        }
    }
    if payloads.len() != manifest.inventory.len() {
        return Err(Error::Inventory);
    }
    let mut entries = Vec::with_capacity(manifest.inventory.len());
    for item in &manifest.inventory {
        let encoded = payloads.remove(&item.path).ok_or(Error::MissingEntry)?;
        let bytes = decode(
            &encoded,
            usize::try_from(item.size_bytes).map_err(|_| Error::Bounds)?,
        )?;
        if bytes.len() as u64 != item.size_bytes || hash(&bytes) != item.sha256 {
            return Err(Error::Inventory);
        }
        entries.push(VerifiedEntry {
            kind: item.kind.clone(),
            path: item.path.clone(),
            bytes,
        });
    }
    Ok(VerifiedArtifact {
        manifest,
        manifest_bytes,
        entries,
    })
}
#[cfg(test)]
mod tests;
