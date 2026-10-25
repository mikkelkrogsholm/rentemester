//! Original secret-config decoding. Persistence and client-map replacement require
//! a CompanyAdmin-admitted namespace capability; this module creates neither.
use super::{Client, Config, Environment, Error};
use serde::Deserialize;
use zeroize::Zeroizing;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Secret {
    api_license_key: String,
    #[serde(default = "test")]
    environment: Environment,
}
fn test() -> Environment {
    Environment::Test
}
/// Decode only the original company secret format. Caller endpoints, company
/// identities, TLS roots and timeout overrides are not part of this format.
pub fn from_admitted_secret(bytes: &[u8]) -> Result<Config, Error> {
    if bytes.is_empty() || bytes.len() > 16384 {
        return Err(Error::Configuration);
    }
    let s: Secret = serde_json::from_slice(bytes).map_err(|_| Error::Configuration)?;
    let key = Zeroizing::new(s.api_license_key);
    if !super::text(&key, 4096) || !key.is_ascii() {
        return Err(Error::Configuration);
    }
    Ok(Config {
        api_license_key: key,
        environment: s.environment,
        base_url: None,
        trusted_ca_pem: None,
        timeout_ms: super::timeout(),
    })
}
/// Constructing a client performs no provider call. Each replacement has a fresh
/// generation so a rotate-away-and-back cannot revive pending dispatch authority.
pub fn client_from_admitted_secret(bytes: &[u8]) -> Result<Client, Error> {
    Client::new(from_admitted_secret(bytes)?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original717_efaktura_konfigurer_default_rotation_and_refusal() {
        let bytes = br#"{"apiLicenseKey":"synthetic-secret"}"#;
        assert_eq!(
            from_admitted_secret(bytes).unwrap().environment(),
            Environment::Test
        );
        let a = client_from_admitted_secret(bytes).unwrap();
        let b = client_from_admitted_secret(bytes).unwrap();
        assert_ne!(a.credential_generation(), b.credential_generation());
        assert_eq!(a.credential_generation(), a.credential_generation());
        assert_eq!(
            from_admitted_secret(
                br#"{"apiLicenseKey":"synthetic-secret","environment":"production"}"#
            )
            .unwrap()
            .environment(),
            Environment::Production
        );
        for invalid in [
            br#"{"apiLicenseKey":""}"#.as_slice(),
            br#"{"apiLicenseKey":"synthetic-secret","environment":"unknown"}"#.as_slice(),
            br#"{"apiLicenseKey":"synthetic-secret","baseUrl":"https://arbitrary.invalid"}"#
                .as_slice(),
            br#"{"apiLicenseKey":"synthetic-secret","actor":"user:admin"}"#.as_slice(),
        ] {
            assert!(from_admitted_secret(invalid).is_err());
        }
    }
}
