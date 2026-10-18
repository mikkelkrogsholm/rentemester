//! Transient key lookup input, not authenticated authority or a transferable service grant.
use crate::auth::AuthError;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

pub struct ServiceKey {
    workspace: String,
    hash: Zeroizing<String>,
}
impl ServiceKey {
    pub fn new(workspace: &str, key: &str) -> Result<Self, AuthError> {
        if workspace.trim().is_empty()
            || workspace.len() > 1024
            || workspace.chars().any(char::is_control)
            || key.trim().is_empty()
            || key.trim() != key
            || key.len() > 4096
            || key.chars().any(char::is_control)
        {
            return Err(AuthError::InvalidCredentials);
        }
        Ok(Self {
            workspace: workspace.to_owned(),
            hash: Zeroizing::new(URL_SAFE_NO_PAD.encode(Sha256::digest(key.as_bytes()))),
        })
    }
    pub fn workspace(&self) -> &str {
        &self.workspace
    }
    /// Sensitive internal lookup value. Never serialize, log, audit or accept caller-supplied hashes.
    pub fn lookup_hash(&self) -> &str {
        &self.hash
    }
}
/// New plaintext exists only in this zeroizing wrapper and a known-success receipt.
pub struct NewServiceKey {
    secret: Zeroizing<String>,
    input: ServiceKey,
}
impl NewServiceKey {
    pub fn generate(workspace: &str) -> Result<Self, AuthError> {
        let mut secret = Zeroizing::new(String::with_capacity(68));
        secret.push_str("rms_");
        const LETTERS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
        while secret.len() < 68 {
            let mut bytes = Zeroizing::new([0u8; 64]);
            getrandom::fill(bytes.as_mut()).map_err(|_| AuthError::CryptoUnavailable)?;
            for b in bytes.iter().copied().filter(|b| *b < 208) {
                if secret.len() == 68 {
                    break;
                }
                secret.push(char::from(LETTERS[usize::from(b) % 52]));
            }
        }
        let input = ServiceKey::new(workspace, &secret)?;
        Ok(Self { secret, input })
    }
    pub fn secret(&self) -> &str {
        &self.secret
    }
    pub fn lookup_hash(&self) -> &str {
        self.input.lookup_hash()
    }
}
pub fn new_uuid() -> Result<String, AuthError> {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).map_err(|_| AuthError::CryptoUnavailable)?;
    b[6] = (b[6] & 15) | 64;
    b[8] = (b[8] & 63) | 128;
    let hex: String = b.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_keys_and_uuid_have_source_shapes_and_fresh_entropy() {
        let a = NewServiceKey::generate("w").unwrap();
        let b = NewServiceKey::generate("w").unwrap();
        assert_eq!(a.secret().len(), 68);
        assert!(a.secret().starts_with("rms_"));
        assert!(
            a.secret().as_bytes()[4..]
                .iter()
                .all(u8::is_ascii_alphabetic)
        );
        assert_ne!(a.secret(), b.secret());
        assert_eq!(
            a.lookup_hash(),
            ServiceKey::new("w", a.secret()).unwrap().lookup_hash()
        );
        let id = new_uuid().unwrap();
        assert_eq!(id.len(), 36);
        assert_eq!(&id[14..15], "4");
        assert!(matches!(id.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
    }
    #[test]
    fn exact_utf8_unpadded_sha256_and_bounded_input() {
        assert_eq!(
            ServiceKey::new("w", "abc").unwrap().lookup_hash(),
            "ungWv48Bz-pBQUDeXa4iI7ADYaOWF3qctBD_YfIAFa0"
        );
        assert_ne!(
            ServiceKey::new("w", "é").unwrap().lookup_hash(),
            ServiceKey::new("w", "e\u{301}").unwrap().lookup_hash()
        );
        for key in ["", " ", " key", "key ", "key\n"] {
            assert!(ServiceKey::new("w", key).is_err());
        }
        assert!(ServiceKey::new("w", &"X".repeat(4096)).is_ok());
        assert!(ServiceKey::new("w", &"X".repeat(4097)).is_err());
        assert!(ServiceKey::new("", "abc").is_err());
    }
}
