//! Fixed DigiSense REST transport. No SQL, retries, implicit validation, registration, or posting.
pub mod onboarding;
pub mod receive;
mod recovery;
use reqwest::{
    blocking::Client as HttpClient,
    header::{ACCEPT, AUTHORIZATION, CONTENT_ENCODING, CONTENT_TYPE, HeaderValue},
};
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};
use std::{io::Read, time::Duration};
use url::Url;
use zeroize::Zeroizing;
pub const MAX_XML: usize = 10 * 1024 * 1024;
pub const MAX_RESPONSE: usize = 1024 * 1024;
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Test,
    Production,
}
#[derive(Clone, Copy, Debug, Deserialize, serde::Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Network {
    #[default]
    Nemhandel,
    Peppol,
}
impl Network {
    pub fn name(self) -> &'static str {
        match self {
            Self::Nemhandel => "nemhandel",
            Self::Peppol => "peppol",
        }
    }
}
/// Operator-only secret input. Clone retains zeroizing secret ownership; never Debug/Serialize.
#[derive(Clone)]
pub struct Config {
    api_license_key: Zeroizing<String>,
    environment: Environment,
    base_url: Option<String>,
    trusted_ca_pem: Option<String>,
    timeout_ms: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WireConfig {
    api_license_key: String,
    environment: Environment,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    trusted_ca_pem: Option<String>,
    #[serde(default = "timeout")]
    timeout_ms: u64,
}
fn timeout() -> u64 {
    30_000
}
impl<'de> Deserialize<'de> for Config {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = WireConfig::deserialize(d)?;
        Ok(Self {
            api_license_key: Zeroizing::new(w.api_license_key),
            environment: w.environment,
            base_url: w.base_url,
            trusted_ca_pem: w.trusted_ca_pem,
            timeout_ms: w.timeout_ms,
        })
    }
}
impl Config {
    pub fn environment(&self) -> Environment {
        self.environment
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct CompanyRegistration {
    pub company_key: String,
}
#[derive(Debug, PartialEq, Eq)]
pub struct ParticipantRegistration {
    pub registered_on_network: bool,
    pub webhook_registered: bool,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Validation {
    pub valid: bool,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Observation {
    pub document_id: Option<String>,
    pub status: String,
    pub message: String,
    pub public_url: Option<String>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum DeliveryOutcome {
    Observed(Observation),
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Configuration,
    InvalidInput,
    Unavailable,
    Refused { status: u16 },
    Protocol,
}
impl Error {
    pub fn code(self) -> &'static str {
        match self {
            Self::Configuration => "digisense_invalid_config",
            Self::InvalidInput => "digisense_invalid_input",
            Self::Unavailable => "digisense_unavailable",
            Self::Refused { .. } => "digisense_refused",
            Self::Protocol => "digisense_protocol_invalid",
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
    authorization: HeaderValue,
    secret: Zeroizing<String>,
    environment: Environment,
    credential_generation: [u8; 32],
    allow_local_download: bool,
}
fn text(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && s.trim() == s && !s.chars().any(char::is_control)
}
fn cvr(s: &str) -> bool {
    s.len() == 10 && s.starts_with("DK") && s.as_bytes()[2..].iter().all(u8::is_ascii_digit)
}
fn company_key(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 2048
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn xml(s: &str) -> bool {
    !s.is_empty() && s.len() <= MAX_XML
}
impl Client {
    pub fn new(config: Config) -> Result<Self, Error> {
        if !text(&config.api_license_key, 4096)
            || !config.api_license_key.is_ascii()
            || !(100..=30000).contains(&config.timeout_ms)
        {
            return Err(Error::Configuration);
        }
        let credential_generation = receive::generation(&config)?;
        let allow_local_download = config.trusted_ca_pem.is_some();
        let default = match config.environment {
            Environment::Test => "https://test-api.digisense.dk/ap/api/rest",
            Environment::Production => "https://api.digisense.dk/ap/api/rest",
        };
        let endpoint = config.base_url.as_deref().unwrap_or(default);
        if !text(endpoint, 4096) || endpoint.contains('\\') {
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
        let literal_local = match base.host() {
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
        if literal_local && config.trusted_ca_pem.is_none() {
            return Err(Error::Configuration);
        }
        let mut roots = webpki_root_certs::TLS_SERVER_ROOT_CERTS
            .iter()
            .map(|der| {
                reqwest::Certificate::from_der(der.as_ref()).map_err(|_| Error::Configuration)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(pem) = config.trusted_ca_pem {
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
        let mut authorization =
            HeaderValue::from_str(&config.api_license_key).map_err(|_| Error::Configuration)?;
        authorization.set_sensitive(true);
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
        Ok(Self {
            http,
            base,
            authorization,
            secret: config.api_license_key,
            environment: config.environment,
            credential_generation,
            allow_local_download,
        })
    }
    pub fn environment(&self) -> Environment {
        self.environment
    }
    fn url(&self, segments: &[&str]) -> Result<Url, Error> {
        let mut url = self.base.clone();
        {
            let mut path = url.path_segments_mut().map_err(|_| Error::Configuration)?;
            path.pop_if_empty();
            for part in segments {
                path.push(part);
            }
        }
        Ok(url)
    }
    fn request(&self, url: Url, body: Option<(&str, Vec<u8>)>) -> Result<(u16, Vec<u8>), Error> {
        let mut r = if body.is_some() {
            self.http.post(url)
        } else {
            self.http.get(url)
        }
        .header(AUTHORIZATION, self.authorization.clone())
        .header(ACCEPT, "application/json")
        .header("accept-encoding", "identity");
        if let Some((mime, bytes)) = body {
            r = r.header(CONTENT_TYPE, mime).body(bytes);
        }
        let mut response = r.send().map_err(|_| Error::Unavailable)?;
        let status = response.status().as_u16();
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
        Ok((status, bytes))
    }
    pub fn register_company(&self, cvr_id: &str, name: &str) -> Result<CompanyRegistration, Error> {
        if !cvr(cvr_id) || !text(name, 4096) {
            return Err(Error::InvalidInput);
        }
        let bytes = serde_json::to_vec(
            &json!({"companyType":{"type":"DK:CVR","id":cvr_id},"companyName":name}),
        )
        .map_err(|_| Error::InvalidInput)?;
        let (status, bytes) = self.request(
            self.url(&["register-company"])?,
            Some(("application/json", bytes)),
        )?;
        if status != 200 {
            return Err(Error::Refused { status });
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| Error::Protocol)?;
        let key = value["companyKey"]
            .as_str()
            .filter(|s| company_key(s) && !s.contains(self.secret.as_str()))
            .ok_or(Error::Protocol)?;
        Ok(CompanyRegistration {
            company_key: key.into(),
        })
    }
    /// Explicit separate step: a company alone cannot send without an outbound participant.
    pub fn register_outbound(
        &self,
        key: &str,
        cvr_id: &str,
        network: Network,
    ) -> Result<ParticipantRegistration, Error> {
        if !company_key(key) || !cvr(cvr_id) {
            return Err(Error::InvalidInput);
        }
        let body=serde_json::to_vec(&json!({"direction":"outbound","participantType":"DK:CVR","participantId":cvr_id,"companyKey":key,"webhookUrl":null,"documentProfiles":format!("default-{}",network.name())})).map_err(|_|Error::InvalidInput)?;
        let (status, bytes) = self.request(
            self.url(&["v2", "register-participant", network.name()])?,
            Some(("application/json", body)),
        )?;
        if status != 200 {
            return Err(Error::Refused { status });
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| Error::Protocol)?;
        let registered = value["registeredOnNetwork"]
            .as_bool()
            .ok_or(Error::Protocol)?;
        if !registered {
            return Err(Error::Protocol);
        }
        Ok(ParticipantRegistration {
            registered_on_network: registered,
            webhook_registered: value["webhookRegistered"]
                .as_bool()
                .ok_or(Error::Protocol)?,
        })
    }
    pub fn validate(&self, source: &str) -> Result<Validation, Error> {
        if !xml(source) {
            return Err(Error::InvalidInput);
        }
        let (status, bytes) = self.request(
            self.url(&["validate-document"])?,
            Some(("application/xml", source.as_bytes().to_vec())),
        )?;
        if ![200, 400].contains(&status) {
            return Err(Error::Refused { status });
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            if status == 400 {
                Error::Refused { status }
            } else {
                Error::Protocol
            }
        })?;
        let valid = value["success"].as_bool().ok_or(Error::Protocol)?;
        let code = value["statusCode"].as_u64().ok_or(Error::Protocol)?;
        let errors = value["errors"].as_array().ok_or(Error::Protocol)?;
        if ![200, 400, 500].contains(&code)
            || valid && (status != 200 || code != 200)
            || errors
                .iter()
                .any(|e| !matches!(e["type"].as_str(), Some("error" | "warning")))
        {
            return Err(Error::Protocol);
        }
        if valid && errors.iter().any(|e| e["type"] == "error") {
            return Err(Error::Protocol);
        }
        Ok(Validation { valid })
    }
    /// One delivery POST only. Any response without trustworthy retained identity is unknown.
    pub fn deliver(&self, key: &str, source: &str) -> DeliveryOutcome {
        if !company_key(key) || !xml(source) {
            return DeliveryOutcome::Unknown;
        }
        let outcome = (|| {
            let mut url = self.url(&["deliver-document"])?;
            url.query_pairs_mut().append_pair("companyKey", key);
            let (status, bytes) =
                self.request(url, Some(("application/xml", source.as_bytes().to_vec())))?;
            let value: Value = serde_json::from_slice(&bytes).map_err(|_| Error::Protocol)?;
            if value["statusCode"].as_u64() != Some(u64::from(status)) {
                return Err(Error::Protocol);
            }
            self.observation(&value, None)
        })();
        match outcome {
            Ok(value) => DeliveryOutcome::Observed(value),
            Err(_) => DeliveryOutcome::Unknown,
        }
    }
    pub fn status(&self, key: &str, id: &str) -> Result<Observation, Error> {
        if !company_key(key) || !text(id, 512) {
            return Err(Error::InvalidInput);
        }
        let mut url = self.url(&["document-status", id])?;
        url.query_pairs_mut().append_pair("companyKey", key);
        let (status, bytes) = self.request(url, None)?;
        if status != 200 {
            return Err(Error::Refused { status });
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| Error::Protocol)?;
        self.observation(&value, Some(id))
    }
    fn observation(&self, value: &Value, expected: Option<&str>) -> Result<Observation, Error> {
        let id = value["documentId"]
            .as_str()
            .filter(|v| text(v, 512) && !v.contains(self.secret.as_str()))
            .ok_or(Error::Protocol)?;
        if expected.is_some_and(|e| id != e) {
            return Err(Error::Protocol);
        }
        let status = value["documentStatus"].as_str().ok_or(Error::Protocol)?;
        if ![
            "delivered",
            "queued-for-delivery",
            "document-not-valid",
            "unable-to-deliver",
            "unknown-server-error",
            "temporary-upstream-error",
        ]
        .contains(&status)
            || !value["statusCode"]
                .as_u64()
                .is_some_and(|n| [200, 202, 400, 401, 500, 502].contains(&n))
        {
            return Err(Error::Protocol);
        }
        // Ancillary URLs are never fetched. Invalid/missing/reflected URLs must
        // not erase a trustworthy accepted document identity.
        let public_url = value
            .get("publicUrl")
            .and_then(Value::as_str)
            .filter(|s| {
                if s.len() > 4096
                    || s.chars().any(char::is_control)
                    || s.contains(self.secret.as_str())
                    || percent_encoding::percent_decode_str(s)
                        .decode_utf8_lossy()
                        .contains(self.secret.as_str())
                {
                    return false;
                }
                Url::parse(s).is_ok_and(|u| {
                    matches!(u.scheme(), "http" | "https")
                        && u.has_host()
                        && u.username().is_empty()
                        && u.password().is_none()
                })
            })
            .map(str::to_owned);
        // Provider free text may contain reflected secrets or customer data. Expose a
        // curated state only, never echo arbitrary messages or diagnostic bodies.
        Ok(Observation {
            document_id: Some(id.into()),
            status: status.into(),
            message: format!("DigiSense: {status}"),
            public_url,
        })
    }
}
#[cfg(test)]
mod tests;
