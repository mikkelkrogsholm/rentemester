//! Canonical Better Auth password crypto only; no account/session/HTTP authority.
use scrypt::{Params, scrypt};
use subtle::ConstantTimeEq;
use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;

pub const INPUT_BYTE_LIMIT: usize = 4096;
pub const NORMALIZED_BYTE_LIMIT: usize = 16384;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordError {
    InvalidCredentials,
    CryptoUnavailable,
}
impl PasswordError {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidCredentials => "invalid_credentials",
            Self::CryptoUnavailable => "auth_crypto_unavailable",
        }
    }
}
impl std::fmt::Display for PasswordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for PasswordError {}
/// Sensitive persistence value, deliberately not Debug, Clone or serializable.
pub struct PasswordHash(Zeroizing<String>);
impl PasswordHash {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
fn normalized(password: &str) -> Result<Zeroizing<String>, PasswordError> {
    if password.len() > INPUT_BYTE_LIMIT {
        return Err(PasswordError::InvalidCredentials);
    }
    let mut result = Zeroizing::new(String::with_capacity(NORMALIZED_BYTE_LIMIT));
    for c in password.nfkc() {
        if result.len() + c.len_utf8() > NORMALIZED_BYTE_LIMIT {
            return Err(PasswordError::InvalidCredentials);
        }
        result.push(c);
    }
    Ok(result)
}
fn derive(password: &str, salt: &str) -> Result<Zeroizing<[u8; 64]>, PasswordError> {
    let password = normalized(password)?;
    let params = Params::new(14, 16, 1, 64).map_err(|_| PasswordError::CryptoUnavailable)?;
    let mut key = Zeroizing::new([0u8; 64]);
    scrypt(password.as_bytes(), salt.as_bytes(), &params, &mut key[..])
        .map_err(|_| PasswordError::CryptoUnavailable)?;
    Ok(key)
}
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        text.push(DIGITS[(b >> 4) as usize] as char);
        text.push(DIGITS[(b & 15) as usize] as char);
    }
    text
}
fn canonical(hash: &str) -> Option<(&str, Zeroizing<[u8; 64]>)> {
    let b = hash.as_bytes();
    if b.len() != 161
        || b[32] != b':'
        || !b
            .iter()
            .enumerate()
            .all(|(i, c)| i == 32 || c.is_ascii_digit() || (b'a'..=b'f').contains(c))
    {
        return None;
    }
    fn nibble(b: u8) -> u8 {
        if b.is_ascii_digit() {
            b - b'0'
        } else {
            b - b'a' + 10
        }
    }
    let mut key = Zeroizing::new([0u8; 64]);
    for (i, pair) in b[33..].as_chunks::<2>().0.iter().enumerate() {
        key[i] = (nibble(pair[0]) << 4) | nibble(pair[1]);
    }
    Some((&hash[..32], key))
}
/// Fixed-cost crypto, including one dummy derivation for absent/malformed stored credentials.
/// Caller owns request limits, bounded blocking dispatch and full identity/MFA/session checks.
pub fn verify_password(password: &str, hash: Option<&str>) -> Result<(), PasswordError> {
    let parsed = hash.and_then(canonical);
    let valid = parsed.is_some();
    let (salt, expected) = parsed.unwrap_or_else(|| {
        (
            "00000000000000000000000000000000",
            Zeroizing::new([0u8; 64]),
        )
    });
    let actual = derive(password, salt)?;
    let equal = bool::from(actual[..].ct_eq(&expected[..]));
    if valid && equal {
        Ok(())
    } else {
        Err(PasswordError::InvalidCredentials)
    }
}
/// Existing canonical format with fresh OS entropy. Enrollment policy belongs to the use case.
pub fn hash_password(password: &str) -> Result<PasswordHash, PasswordError> {
    // Refuse input budgets before requesting entropy or KDF memory.
    let checked = normalized(password)?;
    drop(checked);
    let mut entropy = [0u8; 16];
    getrandom::fill(&mut entropy).map_err(|_| PasswordError::CryptoUnavailable)?;
    let salt = hex(&entropy);
    let key = derive(password, &salt)?;
    let mut stored = Zeroizing::new(String::with_capacity(161));
    stored.push_str(&salt);
    stored.push(':');
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    for b in key.iter() {
        stored.push(DIGITS[(b >> 4) as usize] as char);
        stored.push(DIGITS[(b & 15) as usize] as char);
    }
    Ok(PasswordHash(stored))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::io::Read;
    #[test]
    fn unicode17_official_nfkc_vectors() {
        assert_eq!(unicode_normalization::UNICODE_VERSION, (17, 0, 0));
        use sha2::{Digest, Sha256};
        let provenance: Value =
            serde_json::from_str(include_str!("../fixtures/password-sources.json")).unwrap();
        assert_eq!(
            hex(&Sha256::digest(include_bytes!(
                "../fixtures/NormalizationTest-17.0.0.txt"
            ))),
            provenance["sources"][0]["sha256"].as_str().unwrap()
        );
        let mut checks = 0;
        for line in include_str!("../fixtures/NormalizationTest-17.0.0.txt").lines() {
            let line = line.split('#').next().unwrap().trim();
            if line.is_empty() || line.starts_with('@') {
                continue;
            }
            let columns: Vec<String> = line
                .split(';')
                .take(5)
                .map(|c| {
                    c.split_whitespace()
                        .map(|hex| char::from_u32(u32::from_str_radix(hex, 16).unwrap()).unwrap())
                        .collect()
                })
                .collect();
            assert_eq!(columns.len(), 5);
            for input in &columns {
                assert_eq!(normalized(input).unwrap().as_str(), columns[3]);
                checks += 1;
            }
        }
        assert_eq!(checks, 100170);
    }
    #[test]
    fn canonical_grammar_and_input_bounds_are_explicit() {
        let corpus: Value =
            serde_json::from_str(include_str!("../fixtures/password.json")).unwrap();
        let good = corpus["vectors"][0]["hash"].as_str().unwrap();
        assert!(canonical(good).is_some());
        for bad in [
            "".to_owned(),
            good.to_uppercase(),
            format!("{good}:ignored"),
            good[..160].to_owned(),
            good.replace(':', "::"),
            good.replace('0', "g"),
            format!(" {good}"),
        ] {
            assert!(canonical(&bad).is_none());
        }
        assert_eq!(
            normalized(&"a".repeat(INPUT_BYTE_LIMIT + 1)).unwrap_err(),
            PasswordError::InvalidCredentials
        );
        assert_eq!(
            normalized(&"\u{fdfa}".repeat(INPUT_BYTE_LIMIT / 3)).unwrap_err(),
            PasswordError::InvalidCredentials
        );
        assert_eq!(
            normalized(&"a".repeat(INPUT_BYTE_LIMIT)).unwrap().len(),
            INPUT_BYTE_LIMIT
        );
        assert_eq!(
            PasswordError::InvalidCredentials.to_string(),
            "invalid_credentials"
        );
    }
    #[test]
    fn fixed_python_credential_and_missing_hash_refuse_uniformly() {
        let corpus: Value =
            serde_json::from_str(include_str!("../fixtures/password.json")).unwrap();
        let v = &corpus["vectors"][1];
        let hash = v["hash"].as_str().unwrap();
        let password = v["password"].as_str().unwrap();
        assert!(verify_password(password, Some(hash)).is_ok());
        assert_eq!(
            verify_password("wrong", Some(hash)),
            Err(PasswordError::InvalidCredentials)
        );
        assert_eq!(
            verify_password(password, None),
            Err(PasswordError::InvalidCredentials)
        );
    }
    #[test]
    #[ignore = "synthetic native crypto process probe selected by independent harness"]
    fn probe() {
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input).unwrap();
        let request: Value = match serde_json::from_str(&input) {
            Ok(v) => v,
            Err(_) => {
                println!(
                    "RM_PASSWORD_RESULT={}",
                    json!({"ok":false,"code":"invalid_credentials"})
                );
                return;
            }
        };
        if request["mode"] == "normalization" {
            use sha2::{Digest, Sha256};
            let mut hash = Sha256::new();
            let mut count = 0u32;
            for scalar in 0..=0x10ffffu32 {
                if let Some(c) = char::from_u32(scalar) {
                    let input = c.to_string();
                    let text: String = input.nfkc().collect();
                    hash.update(scalar.to_le_bytes());
                    hash.update((text.len() as u32).to_le_bytes());
                    hash.update(text.as_bytes());
                    count += 1;
                }
            }
            println!(
                "RM_PASSWORD_RESULT={}",
                json!({"ok":true,"scalarCount":count,"scalarDigest":hex(&hash.finalize()),"unicodeVersion":unicode_normalization::UNICODE_VERSION})
            );
            return;
        }
        let password = request["password"].as_str().unwrap_or("");
        let output = if request["mode"] == "issue" {
            match hash_password(password) {
                Ok(hash) => json!({"ok":true,"hash":hash.as_str()}),
                Err(e) => json!({"ok":false,"code":e.code()}),
            }
        } else {
            match verify_password(password, request["hash"].as_str()) {
                Ok(()) => json!({"ok":true}),
                Err(e) => json!({"ok":false,"code":e.code()}),
            }
        };
        println!("RM_PASSWORD_RESULT={output}");
    }
}
