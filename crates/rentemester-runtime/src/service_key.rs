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
#[cfg(test)]
mod tests {
    use super::*;
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
