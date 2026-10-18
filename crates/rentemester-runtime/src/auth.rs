//! Existing signed opaque cookies; identity is resolved separately under the control lock.
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    InvalidConfig,
    InvalidCredentials,
    InvalidClock,
    CryptoUnavailable,
}
impl AuthError {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidConfig => "invalid_auth_config",
            Self::InvalidCredentials => "invalid_credentials",
            Self::InvalidClock => "invalid_auth_clock",
            Self::CryptoUnavailable => "auth_crypto_unavailable",
        }
    }
}
impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for AuthError {}
#[derive(Clone, Copy, PartialEq, Eq)]
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
    token: Zeroizing<String>,
    key_tag: [u8; 32],
    profile: CookieProfile,
    session: bool,
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
    pub fn workspace(&self) -> &str {
        &self.workspace
    }
    pub fn profile(&self) -> CookieProfile {
        self.profile
    }
    pub fn issue_browser_session(&self, token: &str) -> Result<IssuedCookie, AuthError> {
        let cookie = self.issue_session(token)?;
        Ok(IssuedCookie(Zeroizing::new(
            cookie.set_cookie().replace("; Max-Age=43200", ""),
        )))
    }
    pub fn issue_session(&self, token: &str) -> Result<IssuedCookie, AuthError> {
        self.issue(token, "session_token", 43200)
    }
    pub fn issue_challenge(&self, token: &str) -> Result<IssuedCookie, AuthError> {
        if !token.starts_with("2fa-") {
            return Err(AuthError::InvalidCredentials);
        }
        self.issue(token, "two_factor", 600)
    }
    fn issue(&self, token: &str, suffix: &str, seconds: u32) -> Result<IssuedCookie, AuthError> {
        if token.is_empty()
            || token.len() > 512
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
        {
            return Err(AuthError::InvalidCredentials);
        }
        let mut mac = Hmac::<Sha256>::new_from_slice(self.key.as_bytes())
            .map_err(|_| AuthError::InvalidConfig)?;
        mac.update(token.as_bytes());
        let tag = Zeroizing::new(STANDARD.encode(mac.finalize().into_bytes()));
        let mut signed = Zeroizing::new(String::with_capacity(token.len() + 1 + tag.len()));
        signed.push_str(token);
        signed.push('.');
        signed.push_str(&tag);
        const COOKIE_ENCODE: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
            .remove(b'-')
            .remove(b'_')
            .remove(b'.')
            .remove(b'!')
            .remove(b'~')
            .remove(b'*')
            .remove(b'\'')
            .remove(b'(')
            .remove(b')');
        let encoded = Zeroizing::new(
            percent_encoding::utf8_percent_encode(&signed, COOKIE_ENCODE).to_string(),
        );
        let (prefix, secure) = match self.profile {
            CookieProfile::Secure => ("__Secure-", "; Secure"),
            CookieProfile::Development => ("", ""),
        };
        Ok(IssuedCookie(Zeroizing::new(format!(
            "{prefix}better-auth.{suffix}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={seconds}{secure}",
            encoded.as_str()
        ))))
    }
    fn accepts(&self, proof: &VerifiedCookie, session: bool) -> bool {
        use subtle::ConstantTimeEq;
        let tag: [u8; 32] = Sha256::digest(self.key.as_bytes()).into();
        proof.workspace() == self.workspace
            && proof.profile == self.profile
            && proof.session == session
            && bool::from(tag[..].ct_eq(&proof.key_tag[..]))
    }
    /// Pure local cookie expiry; never evidence of server revocation or commit.
    pub fn clear_native_session_cookies(&self) -> Vec<String> {
        let (prefix, secure) = match self.profile {
            CookieProfile::Secure => ("__Secure-", "; Secure"),
            CookieProfile::Development => ("", ""),
        };
        [
            "session_token",
            "session_data",
            "dont_remember",
            "two_factor",
        ]
        .iter()
        .map(|name| {
            format!(
                "{prefix}better-auth.{name}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{secure}"
            )
        })
        .collect()
    }
    pub fn accepts_session(&self, proof: &VerifiedCookie) -> bool {
        self.accepts(proof, true)
    }
    pub fn accepts_challenge(&self, proof: &VerifiedChallenge) -> bool {
        self.accepts(&proof.cookie, false)
    }
    pub fn verify_challenge(
        &self,
        header: Option<&str>,
        service_header: Option<&str>,
    ) -> Result<VerifiedChallenge, AuthError> {
        let proof = self.verify_named(
            header,
            service_header,
            "__Secure-better-auth.two_factor",
            "better-auth.two_factor",
            false,
        )?;
        if !proof.token.starts_with("2fa-") {
            return Err(AuthError::InvalidCredentials);
        }
        Ok(VerifiedChallenge { cookie: proof })
    }
    /// Human-only adapter: service credentials require their own dispatcher, without fallback.
    pub fn verify(
        &self,
        header: Option<&str>,
        service_header: Option<&str>,
    ) -> Result<VerifiedCookie, AuthError> {
        self.verify_named(
            header,
            service_header,
            "__Secure-better-auth.session_token",
            "better-auth.session_token",
            true,
        )
    }
    fn verify_named(
        &self,
        header: Option<&str>,
        service_header: Option<&str>,
        secure: &str,
        plain: &str,
        session: bool,
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
            token: Zeroizing::new(selected.ok_or(denied)?),
            key_tag: Sha256::digest(self.key.as_bytes()).into(),
            profile: self.profile,
            session,
        })
    }
}
/// Bearer proof only; current native context/identity and one-time state must still be checked.
pub struct VerifiedChallenge {
    cookie: VerifiedCookie,
}
impl VerifiedChallenge {
    pub fn workspace(&self) -> &str {
        self.cookie.workspace()
    }
    pub fn token(&self) -> &str {
        self.cookie.token()
    }
}
pub struct IssuedCookie(Zeroizing<String>);
impl IssuedCookie {
    pub fn set_cookie(&self) -> &str {
        &self.0
    }
    pub fn request_header(&self) -> &str {
        self.0.split(';').next().expect("issued cookie header")
    }
}
pub struct OpaqueToken(Zeroizing<String>);
impl OpaqueToken {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
pub fn new_token() -> Result<OpaqueToken, AuthError> {
    random_token("")
}
pub fn new_challenge_token() -> Result<OpaqueToken, AuthError> {
    random_token("2fa-")
}
/// Source-compatible 24-character recovery bearer with unbiased OS randomness.
pub fn new_reset_token() -> Result<OpaqueToken, AuthError> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut value = Zeroizing::new(String::with_capacity(24));
    let mut entropy = Zeroizing::new([0u8; 32]);
    for _ in 0..8 {
        getrandom::fill(&mut *entropy).map_err(|_| AuthError::CryptoUnavailable)?;
        for b in entropy.iter().copied().filter(|b| *b < 248) {
            value.push(ALPHABET[usize::from(b % 62)] as char);
            if value.len() == 24 {
                return Ok(OpaqueToken(value));
            }
        }
    }
    Err(AuthError::CryptoUnavailable)
}
impl CookieVerifier {
    /// Canonical identity is supplied by the validated use case, never stored as plaintext audit.
    pub fn audit_identity_hash(&self, canonical_identity: &str) -> Result<String, AuthError> {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.key.as_bytes())
            .map_err(|_| AuthError::InvalidConfig)?;
        mac.update(b"rentemester-auth-audit-v1\0");
        mac.update(canonical_identity.as_bytes());
        Ok(mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect())
    }
}
fn random_token(prefix: &str) -> Result<OpaqueToken, AuthError> {
    let mut entropy = Zeroizing::new([0u8; 32]);
    getrandom::fill(&mut *entropy).map_err(|_| AuthError::CryptoUnavailable)?;
    let encoded = Zeroizing::new(URL_SAFE_NO_PAD.encode(entropy.as_slice()));
    let mut result = Zeroizing::new(String::with_capacity(prefix.len() + encoded.len()));
    result.push_str(prefix);
    result.push_str(&encoded);
    Ok(OpaqueToken(result))
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
    fn pure_local_clear_has_no_bearer_and_preserves_selected_profile() {
        for profile in [CookieProfile::Secure, CookieProfile::Development] {
            let v =
                CookieVerifier::new("w", profile, "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8")
                    .unwrap();
            let clear = v.clear_native_session_cookies();
            assert_eq!(clear.len(), 4);
            for (header, name) in clear.iter().zip([
                "session_token",
                "session_data",
                "dont_remember",
                "two_factor",
            ]) {
                assert!(header.contains(&format!(
                    "better-auth.{name}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0"
                )));
                assert_eq!(
                    header.starts_with("__Secure-"),
                    profile == CookieProfile::Secure
                );
                assert_eq!(
                    header.ends_with("; Secure"),
                    profile == CookieProfile::Secure
                );
            }
        }
    }
    #[test]
    fn current_session_proof_binds_workspace_key_profile_and_kind() {
        let key = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
        let v = CookieVerifier::new("w", CookieProfile::Secure, key).unwrap();
        let c = v.issue_session("synthetic-token").unwrap();
        let proof = v.verify(Some(c.request_header()), None).unwrap();
        assert!(v.accepts_session(&proof));
        for other in [
            CookieVerifier::new("other", CookieProfile::Secure, key).unwrap(),
            CookieVerifier::new("w", CookieProfile::Development, key).unwrap(),
            CookieVerifier::new(
                "w",
                CookieProfile::Secure,
                "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE",
            )
            .unwrap(),
        ] {
            assert!(!other.accepts_session(&proof));
        }
        let c = v.issue_challenge("2fa-synthetic-token").unwrap();
        let challenge = v.verify_challenge(Some(c.request_header()), None).unwrap();
        assert!(v.accepts_challenge(&challenge));
        assert!(!v.accepts_session(&challenge.cookie));
    }
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
