//! Domain-separated audit signatures. Embedded public metadata is NOT a trust anchor.
use crate::backup_artifact::{SigningKey, TrustedPublicKey};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
pub const FORMAT: &str = "rentemester-gdpr-audit/1";
fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(m) => {
            let sorted = m
                .iter()
                .map(|(k, v)| (k.clone(), canonical(v)))
                .collect::<std::collections::BTreeMap<_, _>>();
            json!(sorted)
        }
        Value::Array(a) => Value::Array(a.iter().map(canonical).collect()),
        _ => value.clone(),
    }
}
fn signed(payload: &str) -> Vec<u8> {
    [FORMAT.as_bytes(), b"\0", payload.as_bytes()].concat()
}
pub fn seal(payload: &Value, key: &SigningKey) -> Result<Value, &'static str> {
    let canonical_payload =
        serde_json::to_string(&canonical(payload)).map_err(|_| "audit serialization refused")?;
    let bytes = signed(&canonical_payload);
    let public = key.public_key_bytes();
    Ok(
        json!({"format":FORMAT,"canonicalPayload":canonical_payload,"fingerprint":format!("{:x}",Sha256::digest(&bytes)),"signature":STANDARD.encode(key.sign_detached(&bytes)),"algorithm":"Ed25519","signerPublicFingerprint":format!("{:x}",Sha256::digest(public)),"signerPublicKey":STANDARD.encode(public),"trustNotice":"Verification requires an independently trusted public key; embedded metadata does not establish trust"}),
    )
}
pub fn verify(envelope: &Value, trusted: &TrustedPublicKey) -> Result<(), &'static str> {
    if envelope["format"] != FORMAT {
        return Err("audit format refused");
    }
    let payload = envelope["canonicalPayload"]
        .as_str()
        .ok_or("audit payload missing")?;
    let value: Value = serde_json::from_str(payload).map_err(|_| "audit JSON refused")?;
    if serde_json::to_string(&canonical(&value)).map_err(|_| "audit JSON refused")? != payload {
        return Err("audit not canonical");
    }
    let signature = STANDARD
        .decode(
            envelope["signature"]
                .as_str()
                .ok_or("audit signature missing")?,
        )
        .map_err(|_| "audit signature refused")?;
    let bytes = signed(payload);
    if envelope["fingerprint"] != format!("{:x}", Sha256::digest(&bytes))
        || envelope["signerPublicFingerprint"]
            != format!("{:x}", Sha256::digest(trusted.public_key_bytes()))
        || envelope["signerPublicKey"] != STANDARD.encode(trusted.public_key_bytes())
    {
        return Err("audit metadata differs");
    }
    trusted
        .verify_detached(&bytes, &signature)
        .map_err(|_| "audit signature refused")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gdpr_seal_uses_backup_key_and_independent_trust() {
        // Fixed synthetic PKCS#8 fixture; production never generates or discovers a key.
        let hex = concat!(
            "3051020101300506032b657004220420",
            "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
            "812100",
            "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"
        );
        let der = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect::<Vec<_>>();
        let key = SigningKey::from_pkcs8(&der).unwrap();
        let trusted = TrustedPublicKey::from_bytes(&key.public_key_bytes()).unwrap();
        let payload = json!({"events":[{"event_type":"gdpr_export","actor":"user:actual"}],"slug":"synthetic"});
        let envelope = seal(&payload, &key).unwrap();
        verify(&envelope, &trusted).unwrap();
        let mut bad = envelope.clone();
        bad["canonicalPayload"] = json!("{}");
        assert!(verify(&bad, &trusted).is_err());
        let wrong = TrustedPublicKey::from_bytes(&[8; 32]).unwrap();
        assert!(verify(&envelope, &wrong).is_err());
        assert_eq!(seal(&payload, &key).unwrap(), envelope);
    }
}

pub mod original717;
