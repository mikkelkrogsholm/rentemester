//! Purpose-bound cryptographic metadata only. Current user/context consumption needs a writer.
use crate::auth::{AuthError, Clock, CookieProfile, CookieVerifier, new_token};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;
const PURPOSE: &str = "rentemester-email-verification-v1";
const LAST_SECOND: i64 = 253_402_300_799;
pub struct EmailTokenSigner {
    workspace: String,
    key: Zeroizing<String>,
}
pub struct EmailToken(Zeroizing<String>);
impl EmailToken {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
/// Signature/time/profile observation, not a user, session or single-use issuance grant.
pub struct VerifiedEmailToken {
    claims: Claims,
    key_tag: [u8; 32],
    digest: [u8; 32],
}
impl VerifiedEmailToken {
    pub fn email(&self) -> &str {
        &self.claims.email
    }
    pub fn nonce(&self) -> &str {
        &self.claims.jti
    }
    pub fn issued_at_seconds(&self) -> i64 {
        self.claims.iat
    }
    pub fn expires_at_seconds(&self) -> i64 {
        self.claims.exp
    }
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    alg: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Claims {
    email: String,
    iat: i64,
    exp: i64,
    purpose: String,
    workspace: String,
    jti: String,
}
impl Drop for Claims {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.email.zeroize();
        self.jti.zeroize();
    }
}
fn refused() -> AuthError {
    AuthError::InvalidCredentials
}
fn seconds(clock: &dyn Clock) -> Result<i64, AuthError> {
    let value = clock.now_unix_ms()?;
    if !(0..=253_402_300_799_999).contains(&value) {
        return Err(AuthError::InvalidClock);
    }
    Ok(value / 1000)
}
fn coordinate(email: &str) -> bool {
    !email.is_empty()
        && email.len() <= 4096
        && email.is_ascii()
        && !email
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control() || b.is_ascii_uppercase())
        && email
            .split_once('@')
            .is_some_and(|(a, b)| !a.is_empty() && !b.is_empty() && !b.contains('@'))
}
fn decode(segment: &str, max: usize) -> Result<Zeroizing<Vec<u8>>, AuthError> {
    if segment.is_empty() || segment.len() > max {
        return Err(refused());
    }
    let bytes = Zeroizing::new(URL_SAFE_NO_PAD.decode(segment).map_err(|_| refused())?);
    if URL_SAFE_NO_PAD.encode(&bytes) != segment {
        return Err(refused());
    }
    Ok(bytes)
}
impl EmailTokenSigner {
    pub fn new(workspace: &str, key: &str) -> Result<Self, AuthError> {
        CookieVerifier::new(workspace, CookieProfile::Development, key)?;
        if workspace.len() > 1024 {
            return Err(AuthError::InvalidConfig);
        }
        Ok(Self {
            workspace: workspace.to_owned(),
            key: Zeroizing::new(key.to_owned()),
        })
    }
    pub fn workspace(&self) -> &str {
        &self.workspace
    }
    pub fn accepts(&self, proof: &VerifiedEmailToken) -> bool {
        use subtle::ConstantTimeEq;
        let tag: [u8; 32] = Sha256::digest(self.key.as_bytes()).into();
        proof.claims.workspace == self.workspace && bool::from(tag.ct_eq(&proof.key_tag))
    }
    pub fn issue(&self, email: &str, clock: &dyn Clock) -> Result<EmailToken, AuthError> {
        if !coordinate(email) {
            return Err(refused());
        }
        let iat = seconds(clock)?;
        if iat > LAST_SECOND - 3600 {
            return Err(AuthError::InvalidClock);
        }
        let nonce = new_token()?;
        let claims = Claims {
            email: email.to_owned(),
            iat,
            exp: iat + 3600,
            purpose: PURPOSE.to_owned(),
            workspace: self.workspace.clone(),
            jti: nonce.as_str().to_owned(),
        };
        let raw =
            Zeroizing::new(serde_json::to_vec(&claims).map_err(|_| AuthError::CryptoUnavailable)?);
        // Exact source protected-header shape; sign the original compact bytes.
        let mut input = Zeroizing::new(format!(
            "eyJhbGciOiJIUzI1NiJ9.{}",
            URL_SAFE_NO_PAD.encode(&raw)
        ));
        let mut mac = Hmac::<Sha256>::new_from_slice(self.key.as_bytes())
            .map_err(|_| AuthError::InvalidConfig)?;
        mac.update(input.as_bytes());
        let tag = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        input.push('.');
        input.push_str(&tag);
        if input.len() > 8192 {
            return Err(refused());
        }
        Ok(EmailToken(input))
    }
    pub fn verify(&self, token: &str, clock: &dyn Clock) -> Result<VerifiedEmailToken, AuthError> {
        if token.len() > 8192 {
            return Err(refused());
        }
        let mut parts = token.split('.');
        let h = parts.next().ok_or_else(refused)?;
        let p = parts.next().ok_or_else(refused)?;
        let s = parts.next().ok_or_else(refused)?;
        if parts.next().is_some() {
            return Err(refused());
        }
        let header = decode(h, 256)?;
        let payload = decode(p, 8192)?;
        let signature = decode(s, 43)?;
        if signature.len() != 32 {
            return Err(refused());
        }
        let mut mac = Hmac::<Sha256>::new_from_slice(self.key.as_bytes())
            .map_err(|_| AuthError::InvalidConfig)?;
        mac.update(h.as_bytes());
        mac.update(b".");
        mac.update(p.as_bytes());
        mac.verify_slice(&signature).map_err(|_| refused())?;
        let header: Header = serde_json::from_slice(&header).map_err(|_| refused())?;
        if header.alg != "HS256" {
            return Err(refused());
        }
        let claims: Claims = serde_json::from_slice(&payload).map_err(|_| refused())?;
        let nonce = decode(&claims.jti, 43)?;
        if nonce.len() != 32
            || !coordinate(&claims.email)
            || claims.workspace != self.workspace
            || claims.purpose != PURPOSE
            || !(0..=LAST_SECOND - 3600).contains(&claims.iat)
            || claims.exp != claims.iat + 3600
        {
            return Err(refused());
        }
        let now = seconds(clock)?;
        if now < claims.iat || now >= claims.exp {
            return Err(refused());
        }
        Ok(VerifiedEmailToken {
            claims,
            key_tag: Sha256::digest(self.key.as_bytes()).into(),
            digest: Sha256::digest(token.as_bytes()).into(),
        })
    }
}

#[cfg(test)]
mod tests;
