//! Installed source otpauth URI profile. Sensitive output, no identity authority.
use super::MfaError;
use zeroize::Zeroizing;
/// The URI contains a usable factor secret. No Debug, Clone or Serde.
pub struct EnrollmentUri(Zeroizing<String>);
impl EnrollmentUri {
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
fn escaped(value: &str, form: bool) -> Zeroizing<String> {
    let mut out = Zeroizing::new(String::with_capacity(value.len() * 3));
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || b"*-._".contains(&b) || !form && b"!~'()".contains(&b) {
            out.push(b as char);
        } else if form && b == b' ' {
            out.push('+');
        } else {
            out.push('%');
            out.push(HEX[usize::from(b >> 4)] as char);
            out.push(HEX[usize::from(b & 15)] as char);
        }
    }
    out
}
pub fn enrollment_uri(issuer: &str, email: &str, secret: &str) -> Result<EnrollmentUri, MfaError> {
    if [issuer, email, secret]
        .iter()
        .any(|s| s.is_empty() || s.len() > 1024 || s.chars().any(char::is_control))
    {
        return Err(MfaError::InvalidConfig);
    }
    const BASE32: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut encoded = Zeroizing::new(String::with_capacity((secret.len() * 8).div_ceil(5)));
    let mut bits = 0_u32;
    let mut count = 0;
    for byte in secret.bytes() {
        bits = (bits << 8) | u32::from(byte);
        count += 8;
        while count >= 5 {
            count -= 5;
            encoded.push(BASE32[((bits >> count) & 31) as usize] as char);
        }
    }
    if count > 0 {
        encoded.push(BASE32[((bits << (5 - count)) & 31) as usize] as char);
    }
    let a = escaped(issuer, false);
    let b = escaped(email, false);
    let c = escaped(issuer, true);
    // Allocate once; never grow a buffer already containing the factor secret.
    let mut uri = Zeroizing::new(String::with_capacity(
        a.len() + b.len() + c.len() + encoded.len() + 100,
    ));
    uri.push_str("otpauth://totp/");
    uri.push_str(&a);
    uri.push(':');
    uri.push_str(&b);
    uri.push_str("?secret=");
    uri.push_str(&encoded);
    uri.push_str("&issuer=");
    uri.push_str(&c);
    uri.push_str("&digits=6&period=30");
    Ok(EnrollmentUri(uri))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_uri_encodes_raw_utf8_and_distinct_path_form_rules() {
        assert_eq!(
            enrollment_uri("Rente mester!~'() Æ", "a+b@example.invalid", "foo")
                .unwrap()
                .as_str(),
            "otpauth://totp/Rente%20mester!~'()%20%C3%86:a%2Bb%40example.invalid?secret=MZXW6&issuer=Rente+mester%21%7E%27%28%29+%C3%86&digits=6&period=30"
        );
        assert_eq!(
            enrollment_uri("R", "a@b.co", "f").unwrap().as_str(),
            "otpauth://totp/R:a%40b.co?secret=MY&issuer=R&digits=6&period=30"
        );
        assert!(enrollment_uri("", "a@b.co", "secret").is_err());
    }
}
