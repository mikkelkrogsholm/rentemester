//! Credential-free **source TAR** structural/integrity validation. Not restore admission.
//!
//! Source: src/core/workspace-snapshot.ts and system-restore.ts portable mode.
//! Unlike original717::verify, portable mode verifies Ed25519 without transporting
//! the HMAC secret. The embedded public key proves integrity, NOT provenance or
//! operator authority. The current credential/Operator and canonical Owner fence
//! must still authorize every company before any snapshot or restore effect.
//!
//! Integration blocker: NamespaceGuard::create_verified_restore_stage and
//! Owner::restore_new accept only native backup_artifact::VerifiedArtifact.
//! Original717RestoreStage accepts only HMAC-verified VerifiedOriginal717 and
//! publishes one company, not an atomic workspace. Neither type may be forged or
//! substituted for this source archive. This module deliberately opens no paths,
//! restores no SQL, issues no credentials, and registers no HTTP endpoint.
use crate::backup_artifact::{Error, MAX_MANIFEST_BYTES, original717 as company};
use base64::{Engine, engine::general_purpose::STANDARD};
use ring::signature;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

pub const RULE: &str = "RENTEMESTER-WORKSPACE-SNAPSHOT-001";
pub const CREDENTIAL_POLICY: &str = "omit-all-auth-and-operational-credentials-v1";
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct File {
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Company {
    pub slug: String,
    pub name: String,
    pub archived: bool,
    pub backup: File,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub version: u32,
    pub created_at: String,
    pub provenance: Value,
    pub credential_policy: String,
    pub workspace_manifest: File,
    pub access_plan: File,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company_knowledge: Option<File>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ownership_graph: Option<File>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_registry: Option<File>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intercompany_dispositions: Option<File>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_inbox: Option<File>,
    pub companies: Vec<Company>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Membership {
    pub company_slug: String,
    pub role: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Identity {
    pub name: String,
    pub email: String,
    pub workspace_role: String,
    pub memberships: Vec<Membership>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessPlan {
    pub version: u32,
    pub recovery: String,
    pub users: Vec<Identity>,
}
/// Integrity-checked bytes only. This is intentionally NOT VerifiedArtifact or
/// VerifiedOriginal717: neither SQL schema nor credential absence in SQLite or
/// arbitrary business evidence is established by archive checksums/signatures.
pub struct CheckedArchive {
    manifest: Manifest,
    access_plan: AccessPlan,
    files: company::Files,
}
impl CheckedArchive {
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    pub fn access_plan(&self) -> &AccessPlan {
        &self.access_plan
    }
    pub fn files(&self) -> &company::Files {
        &self.files
    }
}
fn text(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control)
}
fn slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && s.as_bytes()[0].is_ascii_alphanumeric()
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
fn json<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, Error> {
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(Error::Bounds);
    }
    serde_json::from_slice(bytes).map_err(|_| Error::Malformed)
}
fn evidence(files: &company::Files, item: &File, seen: &mut BTreeSet<String>) -> Result<(), Error> {
    if !seen.insert(item.path.clone()) {
        return Err(Error::Duplicate);
    }
    let bytes = files.get(&item.path).ok_or(Error::MissingEntry)?;
    if item.size_bytes != bytes.len() as u64 || item.sha256 != company::sha256(bytes) {
        return Err(Error::Inventory);
    }
    Ok(())
}
fn plan(bytes: &[u8], slugs: &BTreeSet<String>) -> Result<AccessPlan, Error> {
    let plan: AccessPlan = json(bytes)?;
    if plan.version != 1 || plan.recovery != "bootstrap-owner-then-reinvite" {
        return Err(Error::Coverage);
    }
    let mut emails = BTreeSet::new();
    for user in &plan.users {
        let email = user.email.trim().to_lowercase();
        let valid_email = email.split_once('@').is_some_and(|(a, b)| {
            !a.is_empty()
                && b.contains('.')
                && !b.starts_with('.')
                && !b.ends_with('.')
                && !b.contains('@')
        });
        if !text(&user.name)
            || !valid_email
            || email.chars().any(char::is_whitespace)
            || !emails.insert(email)
            || !matches!(user.workspace_role.as_str(), "workspace_owner" | "member")
        {
            return Err(Error::Malformed);
        }
        for m in &user.memberships {
            if !slugs.contains(&m.company_slug)
                || !matches!(
                    m.role.as_str(),
                    "owner" | "bookkeeper" | "reviewer" | "reader"
                )
            {
                return Err(Error::Malformed);
            }
        }
    }
    Ok(plan)
}
/// Portable-mode company verification from exact retained manifest bytes. Never
/// calls ordinary HMAC verification with a made-up key or claims embedded key trust.
fn portable_company(bytes: &[u8]) -> Result<(), Error> {
    let files = company::read_archive(bytes)?;
    let raw = files.get("manifest.json").ok_or(Error::MissingEntry)?;
    let manifest: company::Manifest = json(raw)?;
    if !matches!(manifest.manifest_version, Some(2))
        || !provenance(&manifest.provenance.clone().ok_or(Error::Coverage)?)
        || manifest.backup_id.is_empty()
        || manifest.backup_id.contains(['/', '\\', '\0'])
        || chrono::DateTime::parse_from_rfc3339(&manifest.created_at).is_err()
        || !manifest.rule_ids.iter().any(|r| r == company::RULE)
        || manifest.db_snapshot.path != "ledger.sqlite"
    {
        return Err(Error::Malformed);
    }
    let sig = manifest
        .asymmetric_signature
        .as_ref()
        .ok_or(Error::Signature)?;
    if sig.algorithm != "ed25519"
        || sig.public_key_path != "config/backup-manifest.pub"
        || sig.signature_path != "manifest.json.ed25519.sig"
    {
        return Err(Error::Signature);
    }
    let public = files.get(&sig.public_key_path).ok_or(Error::MissingEntry)?;
    if sig.public_key_hint != company::sha256(public.trim_ascii())[..16] {
        return Err(Error::Signature);
    }
    let key = company::parse_public_key(public)?;
    let signature = STANDARD
        .decode(
            files
                .get(&sig.signature_path)
                .ok_or(Error::MissingEntry)?
                .trim_ascii(),
        )
        .map_err(|_| Error::Signature)?;
    signature::UnparsedPublicKey::new(&signature::ED25519, key)
        .verify(raw, &signature)
        .map_err(|_| Error::Signature)?;
    let mut seen = BTreeSet::from([
        "manifest.json".into(),
        "manifest.json.hmac".into(),
        sig.signature_path.clone(),
    ]);
    let mut check = |f: &company::File, prefix: &str| {
        if !f.path.starts_with(prefix) {
            return Err(Error::UnsafeEntry);
        }
        evidence(
            &files,
            &File {
                path: f.path.clone(),
                sha256: f.sha256.clone(),
                size_bytes: f.size_bytes,
            },
            &mut seen,
        )
    };
    check(&manifest.db_snapshot, "ledger.sqlite")?;
    for (items, prefix) in [
        (
            &manifest.copied_files.documents_originals,
            "documents-originals/",
        ),
        (&manifest.copied_files.invoices_issued, "invoices-issued/"),
        (&manifest.copied_files.config, "config/"),
    ] {
        for file in items {
            if prefix == "config/"
                && !matches!(
                    file.path.as_str(),
                    "config/backup-lock.json" | "config/backup-manifest.pub" | "config/policy.yaml"
                )
            {
                return Err(Error::UnsafeEntry);
            }
            check(file, prefix)?;
        }
    }
    if !seen.contains(&sig.public_key_path)
        || seen.len() != files.len()
        || files.keys().any(|k| !seen.contains(k))
    {
        return Err(Error::Inventory);
    }
    Ok(())
}
// Reuse the existing source guard rather than invent a second provenance profile.
#[path = "../backup_artifact/original717/provenance.rs"]
mod source_provenance;
fn provenance(value: &Value) -> bool {
    source_provenance::valid(value)
}
/// Checks the exact TAR inventory, metadata agreement, recovery plan and nested
/// Ed25519 integrity. No target path or caller actor can be supplied here.
pub fn check_archive(bytes: &[u8]) -> Result<CheckedArchive, Error> {
    let files = company::read_archive(bytes)?;
    let manifest: Manifest = json(files.get("manifest.json").ok_or(Error::MissingEntry)?)?;
    if manifest.version != 1
        || manifest.credential_policy != CREDENTIAL_POLICY
        || chrono::DateTime::parse_from_rfc3339(&manifest.created_at).is_err()
        || !provenance(&manifest.provenance)
        || manifest.workspace_manifest.path != "workspace.json"
        || manifest.access_plan.path != "access-plan.json"
        || manifest.companies.is_empty()
        || manifest.companies.len() > crate::backup_artifact::MAX_COMPANIES
    {
        return Err(Error::Coverage);
    }
    let mut seen = BTreeSet::from(["manifest.json".into()]);
    evidence(&files, &manifest.workspace_manifest, &mut seen)?;
    evidence(&files, &manifest.access_plan, &mut seen)?;
    for (file, path) in [
        (&manifest.company_knowledge, "company-knowledge.json"),
        (&manifest.ownership_graph, "ownership-graph.json"),
        (&manifest.workspace_registry, "workspace-registry.json"),
        (
            &manifest.intercompany_dispositions,
            "intercompany-dispositions.json",
        ),
        (&manifest.workspace_inbox, "workspace-inbox.json"),
    ] {
        if let Some(file) = file {
            if file.path != path {
                return Err(Error::UnsafeEntry);
            }
            evidence(&files, file, &mut seen)?;
        }
    }
    let mut slugs = BTreeSet::new();
    for c in &manifest.companies {
        if !slug(&c.slug)
            || !text(&c.name)
            || !slugs.insert(c.slug.clone())
            || c.backup.path != format!("companies/{}.tar", c.slug)
        {
            return Err(Error::Identity);
        }
        evidence(&files, &c.backup, &mut seen)?;
        portable_company(&files[&c.backup.path])?;
    }
    if files.len() != seen.len() || files.keys().any(|k| !seen.contains(k)) {
        return Err(Error::Inventory);
    }
    let workspace: Value = json(&files["workspace.json"])?;
    let rows = workspace["companies"].as_array().ok_or(Error::Identity)?;
    let mut registered = BTreeSet::new();
    if workspace["version"] != 1 || rows.len() != slugs.len() {
        return Err(Error::Identity);
    }
    for row in rows {
        let slug = row["slug"].as_str().ok_or(Error::Identity)?;
        if !registered.insert(slug)
            || !manifest
                .companies
                .iter()
                .any(|c| c.slug == slug && row["name"] == c.name && row["archived"] == c.archived)
        {
            return Err(Error::Identity);
        }
    }
    let access_plan = plan(&files["access-plan.json"], &slugs)?;
    Ok(CheckedArchive {
        manifest,
        access_plan,
        files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    use serde_json::json;
    fn provenance_fixture() -> Value {
        json!({"product":{"version":"1.0.0","gitCommit":null,"builtAt":null,"bunVersion":null,"baseImageDigest":null},"schema":{"version":36,"baselineChecksum":"a".repeat(64)},"rules":{"digest":format!("sha256:{}","b".repeat(64))}})
    }
    fn evidence_fixture(path: &str, bytes: &[u8]) -> Value {
        json!({"path":path,"sha256":company::sha256(bytes),"sizeBytes":bytes.len()})
    }
    // This fixture exercises byte codecs, not SQL restore. The ledger bytes are
    // deliberately opaque: check_archive must never be advertised as SQL admission.
    fn company_fixture(secret_config: bool) -> Vec<u8> {
        let key = Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).unwrap();
        let key = Ed25519KeyPair::from_pkcs8(key.as_ref()).unwrap();
        let public = key.public_key().as_ref().to_vec();
        let ledger = b"opaque synthetic SQLite payload; not a restore qualification";
        let mut configs = vec![evidence_fixture("config/backup-manifest.pub", &public)];
        let mut files = company::Files::from([
            ("config/backup-manifest.pub".into(), public.clone()),
            ("ledger.sqlite".into(), ledger.to_vec()),
            (
                "manifest.json.hmac".into(),
                format!("{}\n", "0".repeat(64)).into_bytes(),
            ),
        ]);
        if secret_config {
            files.insert("config/smtp.json".into(), b"synthetic-secret".to_vec());
            configs.push(evidence_fixture("config/smtp.json", b"synthetic-secret"));
        }
        let manifest = json!({"manifestVersion":2,"provenance":provenance_fixture(),"backupId":"backup-20260101T000000Z","createdAt":"2026-01-01T00:00:00.000Z","ruleIds":[company::RULE],"manifestSignature":{"algorithm":"hmac-sha256","keyHint":"not-trusted-portable-mode","signaturePath":"manifest.json.hmac"},"asymmetricSignature":{"algorithm":"ed25519","publicKeyHint":&company::sha256(public.trim_ascii())[..16],"publicKeyPath":"config/backup-manifest.pub","signaturePath":"manifest.json.ed25519.sig"},"dbSnapshot":evidence_fixture("ledger.sqlite",ledger),"copiedFiles":{"documentsOriginals":[],"invoicesIssued":[],"config":configs},"ledgerStats":{"journalEntries":0,"documents":0,"bankTransactions":0}});
        let raw = serde_json::to_vec_pretty(&manifest).unwrap();
        files.insert(
            "manifest.json.ed25519.sig".into(),
            STANDARD.encode(key.sign(&raw).as_ref()).into_bytes(),
        );
        files.insert("manifest.json".into(), raw);
        company::archive(&files).unwrap()
    }
    fn fixture(secret_config: bool) -> company::Files {
        let workspace = serde_json::to_vec(&json!({"version":1,"companies":[{"slug":"synthetic-a","name":"Synthetic A","archived":false,"createdAt":"2026-01-01"},{"slug":"synthetic-b","name":"Synthetic B","archived":true,"createdAt":"2026-01-01"}]})).unwrap();
        let plan = serde_json::to_vec(&json!({"version":1,"recovery":"bootstrap-owner-then-reinvite","users":[{"name":"Synthetic owner","email":"owner@example.invalid","workspaceRole":"workspace_owner","memberships":[{"companySlug":"synthetic-a","role":"owner"},{"companySlug":"synthetic-b","role":"owner"}]}]})).unwrap();
        let a = company_fixture(secret_config);
        let b = company_fixture(false);
        let manifest = json!({"version":1,"createdAt":"2026-01-01T00:00:00.000Z","provenance":provenance_fixture(),"credentialPolicy":CREDENTIAL_POLICY,"workspaceManifest":evidence_fixture("workspace.json",&workspace),"accessPlan":evidence_fixture("access-plan.json",&plan),"companies":[{"slug":"synthetic-a","name":"Synthetic A","archived":false,"backup":evidence_fixture("companies/synthetic-a.tar",&a)},{"slug":"synthetic-b","name":"Synthetic B","archived":true,"backup":evidence_fixture("companies/synthetic-b.tar",&b)}]});
        company::Files::from([
            (
                "manifest.json".into(),
                serde_json::to_vec(&manifest).unwrap(),
            ),
            ("workspace.json".into(), workspace),
            ("access-plan.json".into(), plan),
            ("companies/synthetic-a.tar".into(), a),
            ("companies/synthetic-b.tar".into(), b),
        ])
    }
    #[test]
    fn original716_source_transfer_two_company_integrity_readback_not_restore_authority() {
        let files = fixture(false);
        let archive = company::archive(&files).unwrap();
        let checked = check_archive(&archive).unwrap();
        assert_eq!(checked.files(), &files);
        assert_eq!(checked.manifest().companies.len(), 2);
        assert!(checked.manifest().companies[1].archived);
        assert_eq!(checked.access_plan().users.len(), 1);
        assert_eq!(
            checked.access_plan().recovery,
            "bootstrap-owner-then-reinvite"
        );
        for secret in [
            "synthetic-secret",
            "smtp.json",
            "imap.json",
            "digisense.json",
            "passwordHash",
            "sessionToken",
        ] {
            assert!(
                !archive
                    .windows(secret.len())
                    .any(|w| w == secret.as_bytes())
            );
        }
    }
    #[test]
    fn original716_source_transfer_tamper_unlisted_traversal_and_credential_refusals() {
        assert!(check_archive(&company::archive(&fixture(true)).unwrap()).is_err());
        let mut files = fixture(false);
        files.get_mut("companies/synthetic-a.tar").unwrap()[520] ^= 1;
        assert!(check_archive(&company::archive(&files).unwrap()).is_err());
        let mut files = fixture(false);
        files.insert("unlisted".into(), vec![]);
        assert!(check_archive(&company::archive(&files).unwrap()).is_err());
        // Existing canonical TAR writer refuses traversal before it can be emitted.
        files.insert("../escape".into(), vec![]);
        assert!(company::archive(&files).is_err());
        assert!(check_archive(b"{\"format\":\"rentemester-native-backup/1\"}").is_err());
    }
    #[test]
    fn original716_source_transfer_recovery_plan_is_not_an_authentication_database() {
        let slugs = BTreeSet::from(["synthetic-a".into()]);
        let valid = json!({"version":1,"recovery":"bootstrap-owner-then-reinvite","users":[{"name":"Synthetic owner","email":"owner@example.invalid","workspaceRole":"workspace_owner","memberships":[{"companySlug":"synthetic-a","role":"owner"}]}]});
        assert!(plan(&serde_json::to_vec(&valid).unwrap(), &slugs).is_ok());
        for field in [
            "password",
            "session",
            "mfa",
            "recoveryCodes",
            "smtp",
            "imap",
            "digisense",
        ] {
            let mut invalid = valid.clone();
            invalid["users"][0][field] = json!("synthetic-secret");
            assert!(plan(&serde_json::to_vec(&invalid).unwrap(), &slugs).is_err());
        }
        let mut invalid = valid.clone();
        invalid["users"][0]["memberships"][0]["companySlug"] = json!("unregistered");
        assert!(plan(&serde_json::to_vec(&invalid).unwrap(), &slugs).is_err());
    }
}
