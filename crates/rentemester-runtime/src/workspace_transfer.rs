//! Bounded destination-bound release evidence. Signature verification is not activation authority.
use crate::backup_artifact::{SigningKey, TrustedPublicKey};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
const DOMAIN: &[u8] = b"rentemester-workspace-transfer-release/1\0";
const FORMAT: &str = "rentemester-workspace-transfer-release/1";
const MAX_WIRE: usize = 16 * 1024;
const MAX_PAYLOAD: usize = 8 * 1024;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Company {
    pub slug: String,
    pub ledger_uuid: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Challenge {
    pub version: u32,
    pub transfer_id: String,
    pub source_instance_id: String,
    pub workspace_id: String,
    pub archive_sha256: String,
    pub destination_instance_id: String,
    pub destination: String,
    pub companies: Vec<Company>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Release {
    pub version: u32,
    pub challenge: Challenge,
    pub actor: String,
    pub at: String,
    pub reason: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Bounds,
    Malformed,
    Identity,
    Signature,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Bounds => "transfer_bounds_exceeded",
            Self::Malformed => "transfer_invalid_format",
            Self::Identity => "transfer_invalid_identity",
            Self::Signature => "transfer_signature_refused",
        })
    }
}
impl std::error::Error for Error {}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    format: String,
    payload_base64: String,
    signature_base64: String,
}
fn text(s: &str, min: usize, max: usize) -> bool {
    s.len() >= min && s.len() <= max && s.trim() == s && !s.chars().any(char::is_control)
}
fn hex(s: &str, n: usize) -> bool {
    s.len() == n
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl Challenge {
    pub fn validate(&self) -> Result<(), Error> {
        if self.version != 1
            || !hex(&self.transfer_id, 32)
            || !hex(&self.source_instance_id, 32)
            || !hex(&self.destination_instance_id, 32)
            || self.source_instance_id == self.destination_instance_id
            || !hex(&self.archive_sha256, 64)
            || !text(&self.workspace_id, 1, 1024)
            || !text(&self.destination, 10, 300)
            || self.companies.is_empty()
            || self.companies.len() > 64
        {
            return Err(Error::Identity);
        }
        let mut slugs = BTreeSet::new();
        let mut uuids = BTreeSet::new();
        for c in &self.companies {
            if c.slug.is_empty()
                || c.slug.len() > 100
                || !c
                    .slug
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                || !c.slug.as_bytes()[0].is_ascii_alphanumeric()
                || !hex(&c.ledger_uuid, 32)
                || !slugs.insert(&c.slug)
                || !uuids.insert(&c.ledger_uuid)
            {
                return Err(Error::Identity);
            }
        }
        Ok(())
    }
}
fn validate(r: &Release) -> Result<(), Error> {
    r.challenge.validate()?;
    let at = chrono::DateTime::parse_from_rfc3339(&r.at).map_err(|_| Error::Identity)?;
    if r.version != 1
        || !text(&r.actor, 1, 512)
        || !text(&r.reason, 10, 2000)
        || at
            .with_timezone(&chrono::Utc)
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
            != r.at
    {
        return Err(Error::Identity);
    }
    Ok(())
}
fn signed_bytes(payload: &[u8]) -> Vec<u8> {
    [DOMAIN, payload].concat()
}
pub fn sign_release(release: &Release, key: &SigningKey) -> Result<String, Error> {
    validate(release)?;
    let payload = serde_json::to_vec(release).map_err(|_| Error::Malformed)?;
    if payload.len() > MAX_PAYLOAD {
        return Err(Error::Bounds);
    }
    let wire = serde_json::to_string(&Envelope {
        format: FORMAT.into(),
        signature_base64: STANDARD.encode(key.sign_detached(&signed_bytes(&payload))),
        payload_base64: STANDARD.encode(payload),
    })
    .map_err(|_| Error::Malformed)?;
    if wire.len() > MAX_WIRE {
        return Err(Error::Bounds);
    }
    Ok(wire)
}
pub fn verify_release(wire: &str, key: &TrustedPublicKey) -> Result<Release, Error> {
    if wire.is_empty() || wire.len() > MAX_WIRE {
        return Err(Error::Bounds);
    }
    let envelope: Envelope = serde_json::from_str(wire).map_err(|_| Error::Malformed)?;
    if envelope.format != FORMAT
        || envelope.payload_base64.len() > MAX_PAYLOAD.div_ceil(3) * 4
        || envelope.signature_base64.len() != 88
    {
        return Err(Error::Malformed);
    }
    let payload = STANDARD
        .decode(&envelope.payload_base64)
        .map_err(|_| Error::Malformed)?;
    let signature = STANDARD
        .decode(&envelope.signature_base64)
        .map_err(|_| Error::Malformed)?;
    if payload.is_empty() || payload.len() > MAX_PAYLOAD || signature.len() != 64 {
        return Err(Error::Bounds);
    }
    key.verify_detached(&signed_bytes(&payload), &signature)
        .map_err(|_| Error::Signature)?;
    let release: Release = serde_json::from_slice(&payload).map_err(|_| Error::Malformed)?;
    validate(&release)?;
    Ok(release)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn release() -> Release {
        Release {
            version: 1,
            challenge: Challenge {
                version: 1,
                transfer_id: "1".repeat(32),
                source_instance_id: "2".repeat(32),
                workspace_id: "synthetic".into(),
                archive_sha256: "a".repeat(64),
                destination_instance_id: "3".repeat(32),
                destination: "https://target.invalid".into(),
                companies: vec![Company {
                    slug: "synthetic".into(),
                    ledger_uuid: "4".repeat(32),
                }],
            },
            actor: "user:synthetic".into(),
            at: "2026-10-19T00:00:00.000Z".into(),
            reason: "Reviewed destination identity".into(),
        }
    }
    #[test]
    fn exact_signed_release_tamper_key_domain_and_bounds() {
        let secret =
            ring::signature::Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new())
                .unwrap();
        let key = SigningKey::from_pkcs8(secret.as_ref()).unwrap();
        let public = TrustedPublicKey::from_bytes(&key.public_key_bytes()).unwrap();
        let r = release();
        let wire = sign_release(&r, &key).unwrap();
        assert_eq!(verify_release(&wire, &public).unwrap(), r);
        let mut e: Envelope = serde_json::from_str(&wire).unwrap();
        let mut changed = r.clone();
        changed.challenge.destination_instance_id = "5".repeat(32);
        e.payload_base64 = STANDARD.encode(serde_json::to_vec(&changed).unwrap());
        assert_eq!(
            verify_release(&serde_json::to_string(&e).unwrap(), &public),
            Err(Error::Signature)
        );
        e.payload_base64 = STANDARD.encode(serde_json::to_vec(&r).unwrap());
        e.signature_base64 = STANDARD.encode(key.sign_detached(&serde_json::to_vec(&r).unwrap()));
        assert_eq!(
            verify_release(&serde_json::to_string(&e).unwrap(), &public),
            Err(Error::Signature)
        );
        let other = TrustedPublicKey::from_bytes(&[0; 32]).unwrap();
        assert_eq!(verify_release(&wire, &other), Err(Error::Signature));
        assert_eq!(
            verify_release(&"x".repeat(MAX_WIRE + 1), &public),
            Err(Error::Bounds)
        );
        let mut invalid = r.clone();
        invalid
            .challenge
            .companies
            .push(invalid.challenge.companies[0].clone());
        assert_eq!(sign_release(&invalid, &key), Err(Error::Identity));
    }
}
