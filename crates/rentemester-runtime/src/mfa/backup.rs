//! Configured default encrypted backup-code pool. No identity/session authority.
use super::{MfaCiphertext, MfaError, MfaKeyring};
use serde::de::{Deserializer, SeqAccess, Visitor};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;
fn canonical(code: &str) -> bool {
    code.len() == 11
        && code.as_bytes()[5] == b'-'
        && code
            .bytes()
            .enumerate()
            .all(|(i, b)| i == 5 || b.is_ascii_alphanumeric())
}
struct Codes;
impl<'de> Visitor<'de> for Codes {
    type Value = Vec<Zeroizing<String>>;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("bounded default backup-code pool")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut codes: Self::Value = Vec::new();
        while let Some(value) = seq.next_element::<String>()? {
            let value = Zeroizing::new(value);
            if codes.len() >= 10 || !canonical(&value) || codes.iter().any(|c| **c == *value) {
                return Err(serde::de::Error::custom("unsupported backup-code pool"));
            }
            codes.push(value);
        }
        Ok(codes)
    }
}
/// None means a valid pool but wrong/absent code; errors mean unsupported state/crypto.
/// The returned encrypted replacement must be atomically persisted with authentication effects.
pub fn consume_backup_code(
    keys: &MfaKeyring,
    encrypted: &str,
    code: &str,
) -> Result<Option<MfaCiphertext>, MfaError> {
    let plaintext = keys.decrypt(encrypted)?;
    let mut decoder = serde_json::Deserializer::from_str(plaintext.as_str());
    let codes = decoder
        .deserialize_seq(Codes)
        .map_err(|_| MfaError::InvalidCredentials)?;
    decoder.end().map_err(|_| MfaError::InvalidCredentials)?;
    if !canonical(code) {
        return Ok(None);
    }
    let mut found = None;
    for (i, candidate) in codes.iter().enumerate() {
        if bool::from(candidate.as_bytes().ct_eq(code.as_bytes())) {
            found = Some(i);
        }
    }
    let Some(found) = found else {
        return Ok(None);
    };
    let remaining: Vec<&str> = codes
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != found)
        .map(|(_, c)| c.as_str())
        .collect();
    // Ten validated ASCII codes serialize to at most 141 bytes. Reserve once
    // so no previously filled plaintext allocation is freed during growth.
    let mut serialized = Zeroizing::new(Vec::with_capacity(141));
    serde_json::to_writer(&mut *serialized, &remaining)
        .map_err(|_| MfaError::InvalidCredentials)?;
    let text = std::str::from_utf8(&serialized).map_err(|_| MfaError::InvalidCredentials)?;
    keys.encrypt(text).map(Some)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_pool_consumption_preserves_order_and_case() {
        let keys =
            MfaKeyring::new(&[(1, "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8")], None).unwrap();
        let encrypted = keys.encrypt(r#" ["ABCDE-12345", "abcde-67890"] "#).unwrap();
        let replacement = consume_backup_code(&keys, encrypted.as_str(), "ABCDE-12345")
            .unwrap()
            .unwrap();
        assert_eq!(
            keys.decrypt(replacement.as_str()).unwrap().as_str(),
            r#"["abcde-67890"]"#
        );
        assert!(
            consume_backup_code(&keys, replacement.as_str(), "ABCDE-12345")
                .unwrap()
                .is_none()
        );
        for code in [
            "abcde-12345",
            " ABCDE-12345",
            "ABCDE-12345\n",
            "",
            "ABCDE12345",
            "ＡBCDE-12345",
        ] {
            assert!(
                consume_backup_code(&keys, encrypted.as_str(), code)
                    .unwrap()
                    .is_none()
            );
        }
        for pool in [
            "null",
            "{}",
            "1",
            "[1]",
            r#"["short"]"#,
            r#"["ABCDE-12345","ABCDE-12345"]"#,
            "[] trailing",
        ] {
            let encrypted = keys.encrypt(pool).unwrap();
            assert!(consume_backup_code(&keys, encrypted.as_str(), "ABCDE-12345").is_err());
        }
        let historical =
            MfaKeyring::new(&[(0, "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8")], None).unwrap();
        let old = historical
            .encrypt(r#"["ABCDE-12345","abcde-67890"]"#)
            .unwrap();
        let rotated = MfaKeyring::new(
            &[
                (1, "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE"),
                (0, "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8"),
            ],
            None,
        )
        .unwrap();
        let replacement = consume_backup_code(&rotated, old.as_str(), "ABCDE-12345")
            .unwrap()
            .unwrap();
        assert!(replacement.as_str().starts_with("$ba$1$"));
        assert_eq!(
            rotated.decrypt(replacement.as_str()).unwrap().as_str(),
            r#"["abcde-67890"]"#
        );
        assert!(consume_backup_code(&keys, old.as_str(), "ABCDE-12345").is_err());
        let empty = keys.encrypt("[]").unwrap();
        assert!(
            consume_backup_code(&keys, empty.as_str(), "ABCDE-12345")
                .unwrap()
                .is_none()
        );
    }
}
