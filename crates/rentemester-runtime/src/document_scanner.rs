//! Fixed operator-configured HTTPS scanner. No database, publication, retry, or provider diagnostics.
use crate::document::Prepared;
use base64::{Engine, engine::general_purpose::STANDARD};
use hickory_resolver::{TokioResolver, config::LookupIpStrategy};
use rentemester_engine::{Error, document_scan::ScanEvidence};
use reqwest::{
    blocking::{Client, ClientBuilder},
    header::{ACCEPT, AUTHORIZATION, CONTENT_ENCODING, CONTENT_TYPE, HeaderValue},
};
use serde::Serialize;
use serde_json::Value;
use std::{
    io::Read,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};
use url::{Host, Url};
use zeroize::Zeroizing;
const MAX_RESPONSE: u64 = 65536;
const MAX_REQUEST: usize = 35 * 1024 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanFailure {
    Rejected,
    Unavailable,
}
pub trait Scanner: Send + Sync {
    fn scan(&self, file: &Prepared) -> Result<ScanEvidence, ScanFailure>;
}
/// Private operator input, deliberately not Debug/Serde. No caller-supplied scanner configuration.
pub struct GatewayConfig<'a> {
    pub endpoint: &'a str,
    pub bearer_token: &'a str,
    pub timeout_ms: u64,
}
pub struct HttpJsonScanner {
    endpoint: Url,
    authorization: HeaderValue,
    timeout: Duration,
    client: Client,
}
fn invalid() -> Error {
    Error::new(
        "document_scanner_invalid_config",
        "Scanner operator configuration is invalid",
    )
}
fn local(ip: IpAddr) -> bool {
    ip.is_loopback()
        || ip.is_unspecified()
        || matches!(ip,IpAddr::V6(v) if v.to_ipv4_mapped().is_some_and(|v|v.is_loopback()||v.is_unspecified()))
}
struct ScannerDns(TokioResolver);
impl reqwest::dns::Resolve for ScannerDns {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let resolver = self.0.clone();
        Box::pin(async move {
            let lookup = resolver
                .lookup_ip(name.as_str())
                .await
                .map_err(|_| std::io::Error::other("scanner DNS unavailable"))?;
            let addresses: Vec<_> = lookup.iter().collect();
            // A domain cannot turn the production non-loopback policy into a local request.
            // Explicit private-CA local scanners use literal loopback URLs instead.
            if addresses.is_empty() || addresses.iter().any(|ip| local(*ip)) {
                return Err(std::io::Error::other("scanner DNS refused").into());
            }
            let addresses: Vec<SocketAddr> = addresses
                .into_iter()
                .map(|ip| SocketAddr::new(ip, 0))
                .collect();
            Ok(Box::new(addresses.into_iter()) as reqwest::dns::Addrs)
        })
    }
}
fn client_builder(timeout: Duration) -> ClientBuilder {
    Client::builder()
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
        .pool_max_idle_per_host(0)
        .timeout(timeout)
        .connect_timeout(timeout)
}
impl HttpJsonScanner {
    pub fn new(config: GatewayConfig<'_>) -> Result<Self, Error> {
        let roots = webpki_root_certs::TLS_SERVER_ROOT_CERTS
            .iter()
            .map(|c| reqwest::Certificate::from_der(c.as_ref()).map_err(|_| invalid()))
            .collect::<Result<Vec<_>, _>>()?;
        Self::build(config, roots, false)
    }
    /// Reviewed source difference: explicit private operator CA also permits literal HTTPS
    /// loopback. Only these roots are trusted; TLS hostname verification remains enabled.
    pub fn with_trusted_ca(config: GatewayConfig<'_>, pem: &[u8]) -> Result<Self, Error> {
        if pem.is_empty() || pem.len() > 65536 {
            return Err(invalid());
        }
        let roots = reqwest::Certificate::from_pem_bundle(pem).map_err(|_| invalid())?;
        if roots.is_empty() {
            return Err(invalid());
        }
        Self::build(config, roots, true)
    }
    fn build(
        config: GatewayConfig<'_>,
        roots: Vec<reqwest::Certificate>,
        private_ca: bool,
    ) -> Result<Self, Error> {
        if config.endpoint.is_empty()
            || config.endpoint.len() > 4096
            || config.endpoint.trim() != config.endpoint
            || config.endpoint.contains('\\')
            || config.endpoint.chars().any(char::is_control)
            || config.bearer_token.is_empty()
            || config.bearer_token.len() > 4096
            || config.bearer_token.trim() != config.bearer_token
            || !config.bearer_token.is_ascii()
            || config.bearer_token.chars().any(char::is_control)
            || !(100..=120000).contains(&config.timeout_ms)
        {
            return Err(invalid());
        }
        let endpoint = Url::parse(config.endpoint).map_err(|_| invalid())?;
        let authority = config
            .endpoint
            .split_once("://")
            .map(|(_, s)| s.split(['/', '?', '#']).next().unwrap_or(""))
            .unwrap_or("");
        if endpoint.scheme() != "https"
            || !endpoint.has_host()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || authority.contains('@')
            || endpoint.fragment().is_some()
        {
            return Err(invalid());
        }
        let domain = match endpoint.host().ok_or_else(invalid)? {
            Host::Domain(name) => {
                let name = name.trim_end_matches('.');
                if name == "localhost" || name.ends_with(".localhost") || name.contains('*') {
                    return Err(invalid());
                }
                true
            }
            Host::Ipv4(ip) => {
                if local(ip.into()) && (!private_ca || !ip.is_loopback()) {
                    return Err(invalid());
                }
                false
            }
            Host::Ipv6(ip) => {
                if local(ip.into())
                    && (!private_ca
                        || !(ip.is_loopback()
                            || ip.to_ipv4_mapped().is_some_and(|ip| ip.is_loopback())))
                {
                    return Err(invalid());
                }
                false
            }
        };
        let bearer = Zeroizing::new(format!("Bearer {}", config.bearer_token));
        let mut authorization = HeaderValue::from_str(&bearer).map_err(|_| invalid())?;
        authorization.set_sensitive(true);
        let timeout = Duration::from_millis(config.timeout_ms);
        let mut builder = client_builder(timeout).tls_certs_only(roots);
        if domain {
            let mut resolver = TokioResolver::builder_tokio().map_err(|_| invalid())?;
            resolver.options_mut().ip_strategy = LookupIpStrategy::Ipv4AndIpv6;
            builder = builder.dns_resolver(Arc::new(ScannerDns(
                resolver.build().map_err(|_| invalid())?,
            )));
        }
        Ok(Self {
            endpoint,
            authorization,
            timeout,
            client: builder.build().map_err(|_| invalid())?,
        })
    }
}
impl Scanner for HttpJsonScanner {
    fn scan(&self, file: &Prepared) -> Result<ScanEvidence, ScanFailure> {
        let start = Instant::now();
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire<'a> {
            sha256: &'a str,
            mime_type: &'a str,
            filename: &'a str,
            bytes_base64: String,
        }
        let wire = Wire {
            sha256: file.hash(),
            mime_type: file.mime(),
            filename: file.filename(),
            bytes_base64: STANDARD.encode(file.bytes()),
        };
        let body = Zeroizing::new(serde_json::to_vec(&wire).map_err(|_| ScanFailure::Unavailable)?);
        if body.len() > MAX_REQUEST {
            return Err(ScanFailure::Unavailable);
        }
        let remaining = self
            .timeout
            .checked_sub(start.elapsed())
            .filter(|d| !d.is_zero())
            .ok_or(ScanFailure::Unavailable)?;
        let response = self
            .client
            .post(self.endpoint.clone())
            .header(AUTHORIZATION, self.authorization.clone())
            .header(ACCEPT, "application/json")
            .header(CONTENT_TYPE, "application/json")
            .timeout(remaining)
            .body(body.to_vec())
            .send()
            .map_err(|_| ScanFailure::Unavailable)?;
        // Match the existing HTTP adapter: non-success/negative/malformed JSON is a
        // rejection, without retaining/logging any provider body or sensitive diagnostic.
        if !response.status().is_success() {
            return Err(ScanFailure::Rejected);
        }
        if response.headers().contains_key(CONTENT_ENCODING)
            || response.content_length().is_some_and(|n| n > MAX_RESPONSE)
        {
            return Err(ScanFailure::Unavailable);
        }
        let mut bytes = vec![];
        response
            .take(MAX_RESPONSE + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ScanFailure::Unavailable)?;
        if bytes.len() as u64 > MAX_RESPONSE || start.elapsed() >= self.timeout {
            return Err(ScanFailure::Unavailable);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| ScanFailure::Rejected)?;
        let Some(scanner) = value
            .get("scannerId")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
        else {
            return Err(ScanFailure::Rejected);
        };
        if value.get("ok") != Some(&Value::Bool(true)) {
            return Err(ScanFailure::Rejected);
        }
        let result = ScanEvidence::new(
            file.hash(),
            scanner,
            value.get("scannerVersion").and_then(Value::as_str),
            value.get("evidenceRef").and_then(Value::as_str),
        )
        .map_err(|_| ScanFailure::Unavailable)?;
        if start.elapsed() >= self.timeout {
            return Err(ScanFailure::Unavailable);
        }
        Ok(result)
    }
}
#[cfg(test)]
mod tests;
