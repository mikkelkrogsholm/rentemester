//! Bounded official EU VIES REST adapter. No retries, redirects, cache, SQL or ambient endpoint input.
use rentemester_engine::vies;
use reqwest::{
    blocking::Client as HttpClient,
    header::{ACCEPT, CONTENT_ENCODING},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{io::Read, time::Duration};
use url::Url;
pub const OFFICIAL_BASE_URL: &str = "https://ec.europa.eu/taxation_customs/vies/rest-api";
const MAX_RESPONSE: usize = 262144;
fn timeout() -> u64 {
    12000
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    trusted_ca_pem: Option<String>,
    #[serde(default = "timeout")]
    timeout_ms: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: None,
            trusted_ca_pem: None,
            timeout_ms: timeout(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Configuration,
    InvalidInput,
    Unavailable,
    Protocol,
    Refused { status: u16 },
}
impl Error {
    pub fn code(self) -> &'static str {
        match self {
            Self::Configuration => "vies_invalid_config",
            Self::InvalidInput => "vies_invalid_input",
            Self::Unavailable => "vies_unavailable",
            Self::Protocol => "vies_protocol_invalid",
            Self::Refused { .. } => "vies_provider_refused",
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for Error {}
pub struct Client {
    http: HttpClient,
    base: Url,
}
impl Client {
    pub fn new(config: &Config) -> Result<Self, Error> {
        let endpoint = config.base_url.as_deref().unwrap_or(OFFICIAL_BASE_URL);
        if !(100..=30000).contains(&config.timeout_ms)
            || endpoint.len() > 4096
            || endpoint.trim() != endpoint
            || endpoint.chars().any(char::is_control)
            || endpoint.contains('\\')
        {
            return Err(Error::Configuration);
        }
        let base = Url::parse(endpoint).map_err(|_| Error::Configuration)?;
        if base.scheme() != "https"
            || !base.has_host()
            || !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
            || endpoint
                .split_once("://")
                .is_some_and(|(_, p)| p.split('/').next().unwrap_or("").contains('@'))
        {
            return Err(Error::Configuration);
        }
        let local = match base.host() {
            Some(url::Host::Ipv4(ip)) => ip.is_loopback() || ip.is_unspecified(),
            Some(url::Host::Ipv6(ip)) => {
                ip.is_loopback()
                    || ip.is_unspecified()
                    || ip
                        .to_ipv4_mapped()
                        .is_some_and(|v| v.is_loopback() || v.is_unspecified())
            }
            _ => false,
        };
        if local && config.trusted_ca_pem.is_none() {
            return Err(Error::Configuration);
        }
        let mut roots = webpki_root_certs::TLS_SERVER_ROOT_CERTS
            .iter()
            .map(|der| {
                reqwest::Certificate::from_der(der.as_ref()).map_err(|_| Error::Configuration)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(pem) = &config.trusted_ca_pem {
            if pem.is_empty() || pem.len() > 65536 {
                return Err(Error::Configuration);
            }
            let extra = reqwest::Certificate::from_pem_bundle(pem.as_bytes())
                .map_err(|_| Error::Configuration)?;
            if extra.is_empty() {
                return Err(Error::Configuration);
            }
            roots.extend(extra);
        }
        let timeout = Duration::from_millis(config.timeout_ms);
        let http = HttpClient::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .referer(false)
            .connection_verbose(false)
            .tls_sslkeylogfile(false)
            .http1_only()
            .http1_max_headers(64)
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd()
            .tls_certs_only(roots)
            .timeout(timeout)
            .connect_timeout(timeout)
            .build()
            .map_err(|_| Error::Configuration)?;
        Ok(Self { http, base })
    }
    pub fn validate(&self, vat: &str) -> Result<Value, Error> {
        let vat = vies::normalize(vat).map_err(|_| Error::InvalidInput)?;
        let mut url = self.base.clone();
        {
            let mut path = url.path_segments_mut().map_err(|_| Error::Configuration)?;
            path.pop_if_empty()
                .push("ms")
                .push(&vat.country_code)
                .push("vat")
                .push(&vat.vat_number);
        }
        let mut response = self
            .http
            .get(url)
            .header(ACCEPT, "application/json")
            .header("accept-encoding", "identity")
            .send()
            .map_err(|_| Error::Unavailable)?;
        if !response.status().is_success() {
            return Err(Error::Refused {
                status: response.status().as_u16(),
            });
        }
        if response
            .headers()
            .get(CONTENT_ENCODING)
            .is_some_and(|v| v.as_bytes() != b"identity")
            || response
                .content_length()
                .is_some_and(|n| n > MAX_RESPONSE as u64)
        {
            return Err(Error::Protocol);
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut response)
            .take(MAX_RESPONSE as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Unavailable)?;
        if bytes.len() > MAX_RESPONSE {
            return Err(Error::Protocol);
        }
        let body: Value = serde_json::from_slice(&bytes).map_err(|_| Error::Protocol)?;
        observation(&vat, &body)
    }
}
fn observation(vat: &vies::Vat, body: &Value) -> Result<Value, Error> {
    if !body.is_object() {
        return Err(Error::Protocol);
    }
    let values = [body.get("isValid"), body.get("valid")];
    let mut validity = None;
    for value in values.into_iter().flatten() {
        let valid = value.as_bool().ok_or(Error::Protocol)?;
        if validity.is_some_and(|previous| previous != valid) {
            return Err(Error::Protocol);
        }
        validity = Some(valid);
    }
    let valid = validity.ok_or(Error::Protocol)?;
    if let Some(status) = body.get("userError")
        && !status.is_null()
        && status.as_str() != Some("VALID")
    {
        return Err(Error::Protocol);
    }
    for (key, expected) in [
        ("countryCode", &vat.country_code),
        ("vatNumber", &vat.vat_number),
    ] {
        if let Some(value) = body.get(key)
            && value.as_str() != Some(expected)
        {
            return Err(Error::Protocol);
        }
    }
    let text = |keys: &[&str], max: usize| -> Result<Value, Error> {
        for key in keys {
            if let Some(value) = body.get(key) {
                if value.is_null() {
                    continue;
                }
                let s = value.as_str().ok_or(Error::Protocol)?.trim();
                if s.len() > max {
                    return Err(Error::Protocol);
                }
                return Ok(if s.is_empty() || s == "---" {
                    Value::Null
                } else {
                    json!(s)
                });
            }
        }
        Ok(Value::Null)
    };
    Ok(
        json!({"countryCode":vat.country_code,"vatNumber":vat.vat_number,"normalized":vat.normalized,"valid":valid,"name":text(&["name","traderName"],8192)?,"address":text(&["address","traderAddress"],32768)?,"rawResponse":serde_json::to_string(body).map_err(|_|Error::Protocol)?}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ambiguous_outages_and_changed_identity_are_not_negative_answers() {
        let vat = vies::normalize("DE123456789").unwrap();
        for body in [
            json!({}),
            json!({"isValid":true,"valid":false}),
            json!({"valid":"false"}),
            json!({"isValid":false,"userError":"MS_UNAVAILABLE"}),
            json!({"isValid":true,"vatNumber":"000000000"}),
        ] {
            assert_eq!(observation(&vat, &body), Err(Error::Protocol));
        }
        assert_eq!(
            observation(&vat, &json!({"isValid":false,"userError":"VALID"})).unwrap()["valid"],
            false
        );
        assert_eq!(
            observation(
                &vat,
                &json!({"isValid":true,"userError":"VALID","name":"---"})
            )
            .unwrap()["name"],
            Value::Null
        );
    }
    #[test]
    fn unsafe_operator_endpoints_are_rejected() {
        for base in [
            "http://example.test/vies",
            "https://user:pass@example.test",
            "https://example.test/?q=1",
            "https://example.test/#x",
            "https://127.0.0.1/vies",
        ] {
            let config: Config = serde_json::from_value(json!({"baseUrl":base})).unwrap();
            assert!(matches!(Client::new(&config), Err(Error::Configuration)));
        }
    }
}
