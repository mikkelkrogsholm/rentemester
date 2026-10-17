//! Existing MFA crypto only. No challenge, one-time consumption, identity or session authority.
mod backup;
use crate::auth::Clock;
pub use backup::{BackupCodeList, consume_backup_code, generate_backup_codes, reveal_backup_codes};
mod enrollment;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce, aead::AeadInPlace};
pub use enrollment::{EnrollmentUri, enrollment_uri};
use hmac::{Hmac, Mac};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

pub const PLAINTEXT_LIMIT: usize = 4096;
const MAX_VERSION: u64 = 9_007_199_254_740_991;
const MAX_ENVELOPE: usize = 2 * (PLAINTEXT_LIMIT + 40) + 22;
const MAX_UNIX_MS: i64 = 253_402_300_799_999;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MfaError {
    InvalidConfig,
    InvalidCredentials,
    InvalidClock,
    CryptoUnavailable,
}
impl MfaError {
    pub fn code(self) -> &'static str {
        match self {
            Self::InvalidConfig => "invalid_auth_config",
            Self::InvalidCredentials => "invalid_credentials",
            Self::InvalidClock => "invalid_auth_clock",
            Self::CryptoUnavailable => "auth_crypto_unavailable",
        }
    }
}
impl std::fmt::Display for MfaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for MfaError {}
/// Explicitly sensitive: no Debug/Clone/Serde; trusted persistence/verification access only.
pub struct MfaPlaintext(Zeroizing<String>);
pub struct MfaCiphertext(Zeroizing<String>);
impl MfaPlaintext {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
impl MfaCiphertext {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
/// First configured version encrypts; subsequent versions and explicit legacy key decrypt only.
/// Trusted runtime must bind this configuration to the actual workspace/account context.
pub struct MfaKeyring {
    keys: Vec<(u64, Zeroizing<[u8; 32]>)>,
    legacy: Option<Zeroizing<[u8; 32]>>,
}
fn derive(secret: &str) -> Result<Zeroizing<[u8; 32]>, MfaError> {
    if secret.len() > 4096 {
        return Err(MfaError::InvalidConfig);
    }
    let bytes = Zeroizing::new(
        URL_SAFE_NO_PAD
            .decode(secret)
            .map_err(|_| MfaError::InvalidConfig)?,
    );
    let encoded = Zeroizing::new(URL_SAFE_NO_PAD.encode(&bytes));
    if bytes.len() < 32 || encoded.as_str() != secret {
        return Err(MfaError::InvalidConfig);
    }
    // The source hashes the original key STRING, not its decoded entropy.
    Ok(Zeroizing::new(Sha256::digest(secret.as_bytes()).into()))
}
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        output.push(DIGITS[(b >> 4) as usize] as char);
        output.push(DIGITS[(b & 15) as usize] as char);
    }
    output
}
fn unhex(value: &str) -> Result<Zeroizing<Vec<u8>>, MfaError> {
    let b = value.as_bytes();
    if b.len() < 80
        || b.len() > 2 * (PLAINTEXT_LIMIT + 40)
        || !b.len().is_multiple_of(2)
        || !b
            .iter()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c))
    {
        return Err(MfaError::InvalidCredentials);
    }
    let digit = |c: u8| {
        if c.is_ascii_digit() {
            c - b'0'
        } else {
            c - b'a' + 10
        }
    };
    Ok(Zeroizing::new(
        b.chunks_exact(2)
            .map(|p| digit(p[0]) * 16 + digit(p[1]))
            .collect(),
    ))
}
impl MfaKeyring {
    pub fn new(keys: &[(u64, &str)], legacy: Option<&str>) -> Result<Self, MfaError> {
        if keys.is_empty() || keys.len() > 16 {
            return Err(MfaError::InvalidConfig);
        }
        let mut configured = Vec::with_capacity(keys.len());
        for (version, secret) in keys {
            if *version > MAX_VERSION || configured.iter().any(|(v, _)| v == version) {
                return Err(MfaError::InvalidConfig);
            }
            configured.push((*version, derive(secret)?));
        }
        Ok(Self {
            keys: configured,
            legacy: legacy.map(derive).transpose()?,
        })
    }
    pub fn encrypt(&self, plaintext: &str) -> Result<MfaCiphertext, MfaError> {
        if plaintext.len() > PLAINTEXT_LIMIT {
            return Err(MfaError::InvalidCredentials);
        }
        let mut nonce = [0u8; 24];
        getrandom::fill(&mut nonce).map_err(|_| MfaError::CryptoUnavailable)?;
        self.encrypt_with_nonce(plaintext, &nonce)
    }
    fn encrypt_with_nonce(
        &self,
        plaintext: &str,
        nonce: &[u8; 24],
    ) -> Result<MfaCiphertext, MfaError> {
        if plaintext.len() > PLAINTEXT_LIMIT {
            return Err(MfaError::InvalidCredentials);
        }
        let (version, key) = &self.keys[0];
        // Reserve tag capacity before copying plaintext; no secret-buffer reallocation.
        let mut buffer = Zeroizing::new(Vec::with_capacity(plaintext.len() + 16));
        buffer.extend_from_slice(plaintext.as_bytes());
        XChaCha20Poly1305::new_from_slice(key.as_ref())
            .map_err(|_| MfaError::CryptoUnavailable)?
            .encrypt_in_place(XNonce::from_slice(nonce), b"", &mut *buffer)
            .map_err(|_| MfaError::CryptoUnavailable)?;
        let mut value = Zeroizing::new(format!("$ba${version}${}", hex(nonce)));
        value.push_str(&hex(&buffer));
        Ok(MfaCiphertext(value))
    }
    pub fn decrypt(&self, value: &str) -> Result<MfaPlaintext, MfaError> {
        let denied = MfaError::InvalidCredentials;
        if value.len() > MAX_ENVELOPE {
            return Err(denied);
        }
        let (key, ciphertext) = if let Some(envelope) = value.strip_prefix("$ba$") {
            let (version, ciphertext) = envelope.split_once('$').ok_or(denied)?;
            let parsed = version.parse::<u64>().map_err(|_| denied)?;
            if parsed > MAX_VERSION || parsed.to_string() != version {
                return Err(denied);
            }
            let (_, key) = self.keys.iter().find(|(v, _)| *v == parsed).ok_or(denied)?;
            (key, ciphertext)
        } else {
            (self.legacy.as_ref().ok_or(denied)?, value)
        };
        let mut buffer = unhex(ciphertext)?;
        let nonce: [u8; 24] = buffer[..24].try_into().map_err(|_| denied)?;
        buffer.drain(..24);
        XChaCha20Poly1305::new_from_slice(key.as_ref())
            .map_err(|_| denied)?
            .decrypt_in_place(XNonce::from_slice(&nonce), b"", &mut *buffer)
            .map_err(|_| denied)?;
        let text = std::str::from_utf8(&buffer).map_err(|_| denied)?;
        // JavaScript TextDecoder strips exactly one leading UTF8 BOM.
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        Ok(MfaPlaintext(Zeroizing::new(text.to_owned())))
    }
}
/// Existing 32-character enrollment secret: uniform64-symbol alphabet =192 random bits.
pub fn new_totp_secret() -> Result<MfaPlaintext, MfaError> {
    const ALPHABET: &[u8; 64] = b"abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ-_";
    let mut entropy = Zeroizing::new([0u8; 32]);
    getrandom::fill(&mut *entropy).map_err(|_| MfaError::CryptoUnavailable)?;
    let mut secret = Zeroizing::new(String::with_capacity(32));
    for b in entropy.iter() {
        secret.push(ALPHABET[(b & 63) as usize] as char);
    }
    Ok(MfaPlaintext(secret))
}
fn hotp(secret: &[u8], counter: u64) -> [u8; 6] {
    let mut mac = <Hmac<Sha1> as Mac>::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(&counter.to_be_bytes());
    let digest = Zeroizing::new(<[u8; 20]>::from(mac.finalize().into_bytes()));
    let offset = usize::from(digest[19] & 15);
    let truncated = u32::from_be_bytes(
        digest[offset..offset + 4]
            .try_into()
            .expect("HOTP offset within20 bytes"),
    ) & 0x7fff_ffff;
    let mut value = truncated % 1_000_000;
    let mut output = [b'0'; 6];
    for b in output.iter_mut().rev() {
        *b += (value % 10) as u8;
        value /= 10;
    }
    output
}
/// Returns highest matching step, NOT durable one-time use or session/MFA authority.
/// Caller must enforce challenge/attempt/lockout/replay policy atomically in current control state.
pub fn verify_totp(secret: &str, code: &str, clock: &dyn Clock) -> Result<u64, MfaError> {
    if secret.is_empty()
        || secret.len() > 1024
        || code.len() != 6
        || !code.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(MfaError::InvalidCredentials);
    }
    let now = clock.now_unix_ms().map_err(|_| MfaError::InvalidClock)?;
    if !(0..=MAX_UNIX_MS).contains(&now) {
        return Err(MfaError::InvalidClock);
    }
    let counter = now as u64 / 30_000;
    let mut matched = None;
    for step in [
        counter.checked_sub(1),
        Some(counter),
        counter.checked_add(1),
    ]
    .into_iter()
    .flatten()
    {
        let expected = Zeroizing::new(hotp(secret.as_bytes(), step));
        if bool::from(code.as_bytes().ct_eq(&expected[..])) {
            matched = Some(step);
        }
    }
    matched.ok_or(MfaError::InvalidCredentials)
}
#[cfg(test)]
mod tests;
