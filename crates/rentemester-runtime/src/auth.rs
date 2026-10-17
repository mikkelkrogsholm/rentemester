//! Existing signed opaque cookies; identity is resolved separately under the control lock.
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use hmac::{Hmac, Mac};
use sha2::Sha256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    InvalidConfig,
    InvalidCredentials,
    InvalidClock,
}
impl AuthError {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidConfig => "invalid_auth_config",
            Self::InvalidCredentials => "invalid_credentials",
            Self::InvalidClock => "invalid_auth_clock",
        }
    }
}
impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for AuthError {}
#[derive(Clone, Copy)]
pub enum CookieProfile {
    Secure,
    Development,
}
/// Secret-bearing configuration: deliberately neither Debug nor serializable.
pub struct CookieVerifier {
    workspace: String,
    profile: CookieProfile,
    key: String,
}
/// MAC proof only. This bearer must never be logged or accepted as current DB identity.
pub struct VerifiedCookie {
    workspace: String,
    token: String,
}
impl VerifiedCookie {
    pub fn workspace(&self) -> &str {
        &self.workspace
    }
    pub fn token(&self) -> &str {
        &self.token
    }
}
impl CookieVerifier {
    pub fn new(workspace: &str, profile: CookieProfile, key: &str) -> Result<Self, AuthError> {
        let entropy = URL_SAFE_NO_PAD
            .decode(key)
            .map_err(|_| AuthError::InvalidConfig)?;
        if workspace.trim().is_empty()
            || workspace.chars().any(char::is_control)
            || entropy.len() < 32
            || URL_SAFE_NO_PAD.encode(&entropy) != key
        {
            return Err(AuthError::InvalidConfig);
        }
        Ok(Self {
            workspace: workspace.to_owned(),
            profile,
            key: key.to_owned(),
        })
    }
    /// Human-only adapter: service credentials require their own dispatcher, without fallback.
    pub fn verify(
        &self,
        header: Option<&str>,
        service_header: Option<&str>,
    ) -> Result<VerifiedCookie, AuthError> {
        let denied = AuthError::InvalidCredentials;
        if service_header.is_some() {
            return Err(denied);
        }
        let header = header.ok_or(denied)?;
        if header.len() > 8192
            || !header.is_ascii()
            || header.bytes().any(|b| b.is_ascii_control() && b != b'\t')
        {
            return Err(denied);
        }
        let secure = "__Secure-better-auth.session_token";
        let plain = "better-auth.session_token";
        let expected = match self.profile {
            CookieProfile::Secure => secure,
            CookieProfile::Development => plain,
        };
        let mut selected = None;
        for part in header.split(';') {
            let Some((name, value)) = part.trim_matches([' ', '\t']).split_once('=') else {
                continue;
            };
            let name = name.trim_matches([' ', '\t']);
            if name != secure && name != plain {
                continue;
            }
            if name != expected || selected.is_some() {
                return Err(denied);
            }
            let value = value.trim_matches([' ', '\t']);
            let value = if value.starts_with('"') {
                value
                    .strip_prefix('"')
                    .and_then(|v| v.strip_suffix('"'))
                    .ok_or(denied)?
            } else {
                value
            };
            if value.contains('"') {
                return Err(denied);
            }
            let bytes = value.as_bytes();
            for (i, b) in bytes.iter().enumerate() {
                if *b == b'%'
                    && (i + 2 >= bytes.len()
                        || !bytes[i + 1].is_ascii_hexdigit()
                        || !bytes[i + 2].is_ascii_hexdigit())
                {
                    return Err(denied);
                }
            }
            let decoded = percent_encoding::percent_decode_str(value)
                .decode_utf8()
                .map_err(|_| denied)?;
            let (token, signature) = decoded.rsplit_once('.').ok_or(denied)?;
            if token.is_empty()
                || token.len() > 512
                || token.chars().any(char::is_control)
                || signature.len() != 44
                || !signature.ends_with('=')
            {
                return Err(denied);
            }
            let tag = STANDARD.decode(signature).map_err(|_| denied)?;
            if tag.len() != 32 || STANDARD.encode(&tag) != signature {
                return Err(denied);
            }
            let mut mac = Hmac::<Sha256>::new_from_slice(self.key.as_bytes())
                .map_err(|_| AuthError::InvalidConfig)?;
            mac.update(token.as_bytes());
            mac.verify_slice(&tag).map_err(|_| denied)?;
            selected = Some(token.to_owned());
        }
        Ok(VerifiedCookie {
            workspace: self.workspace.clone(),
            token: selected.ok_or(denied)?,
        })
    }
}
/// Trusted runtime dependency, never deserialized request time or environment configuration.
pub trait Clock {
    fn now_unix_ms(&self) -> Result<i64, AuthError>;
}
pub struct SystemClock;
impl Clock for SystemClock {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        let duration = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| AuthError::InvalidClock)?;
        i64::try_from(duration.as_millis()).map_err(|_| AuthError::InvalidClock)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_python_wire_vectors_and_fixed_errors() {
        let corpus: serde_json::Value =
            serde_json::from_str(include_str!("../fixtures/session-cookie.json")).unwrap();
        for case in corpus["vectors"].as_array().unwrap() {
            let profile = if case["profile"] == "development" {
                CookieProfile::Development
            } else {
                CookieProfile::Secure
            };
            let verifier =
                CookieVerifier::new("w", profile, corpus["key"].as_str().unwrap()).unwrap();
            let result = verifier.verify(case["header"].as_str(), case["service"].as_str());
            assert_eq!(
                result.is_ok(),
                case["ok"].as_bool().unwrap(),
                "{}",
                case["id"]
            );
            match result {
                Ok(proof) => {
                    assert_eq!(proof.workspace(), "w");
                    assert_eq!(proof.token(), case["token"].as_str().unwrap());
                }
                Err(e) => {
                    assert_eq!(e, AuthError::InvalidCredentials);
                    assert_eq!(e.to_string(), "invalid_credentials");
                }
            }
        }
        for key in [
            "",
            "not a secret",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
            "short",
        ] {
            assert!(matches!(
                CookieVerifier::new("w", CookieProfile::Secure, key),
                Err(AuthError::InvalidConfig)
            ));
        }
        let key = corpus["key"].as_str().unwrap();
        assert!(CookieVerifier::new("  ", CookieProfile::Secure, key).is_err());
        let now = SystemClock.now_unix_ms().unwrap();
        assert!((1_700_000_000_000..253_402_300_799_999).contains(&now));
    }
}
