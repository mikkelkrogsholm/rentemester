//! Optional fixed HTTP JSON invoice provider. Network only; no field validation, SQL or accounting.
use base64::{Engine, engine::general_purpose::STANDARD};
use hickory_resolver::{TokioResolver, config::LookupIpStrategy};
use rentemester_engine::{
    Error,
    invoice_extraction::{ProviderFailure, ProviderIdentity, ProviderOutput},
};
use reqwest::{
    blocking::{Client, ClientBuilder},
    header::{ACCEPT, AUTHORIZATION, CONTENT_ENCODING, CONTENT_TYPE, HeaderValue},
};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};
use url::{Host, Url};
use zeroize::Zeroizing;
/// Narrow native bound; the legacy fetch provider did not bound its JSON response.
pub const MAX_RESPONSE: usize = 1024 * 1024;
pub const MAX_PDF_BYTES: usize = 10 * 1024 * 1024;
pub const DEFAULT_TIMEOUT_MS: u64 = 15000;
const MAX_REQUEST: usize = 15 * 1024 * 1024;
/// A borrowed registered source. This is not an HTTP request DTO or caller proof.
#[derive(Clone, Copy)]
pub struct Input<'a> {
    pub sha256: &'a str,
    pub pdf_bytes: &'a [u8],
}
/// Private operator input, deliberately not Debug/Serde. No caller-supplied extractor configuration.
pub struct GatewayConfig<'a> {
    pub endpoint: &'a str,
    pub bearer_token: &'a str,
    pub timeout_ms: u64,
}
pub struct HttpJsonExtractor {
    endpoint: Url,
    identity: ProviderIdentity,
    authorization: HeaderValue,
    timeout: Duration,
    client: Client,
}
fn invalid() -> Error {
    Error::new(
        "invoice_extractor_invalid_config",
        "Invoice extractor operator configuration is invalid",
    )
}
fn local(ip: IpAddr) -> bool {
    ip.is_loopback()
        || ip.is_unspecified()
        || matches!(ip,IpAddr::V6(v) if v.to_ipv4_mapped().is_some_and(|v|v.is_loopback()||v.is_unspecified()))
}
struct ExtractorDns(TokioResolver);
impl reqwest::dns::Resolve for ExtractorDns {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let resolver = self.0.clone();
        Box::pin(async move {
            let lookup = resolver
                .lookup_ip(name.as_str())
                .await
                .map_err(|_| std::io::Error::other("extractor DNS unavailable"))?;
            let addresses: Vec<_> = lookup.iter().collect();
            // A domain cannot turn the production non-loopback policy into a local request.
            // Explicit private-CA local extractors use literal loopback URLs instead.
            if addresses.is_empty() || addresses.iter().any(|ip| local(*ip)) {
                return Err(std::io::Error::other("extractor DNS refused").into());
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
impl HttpJsonExtractor {
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
            builder = builder.dns_resolver(Arc::new(ExtractorDns(
                resolver.build().map_err(|_| invalid())?,
            )));
        }
        Ok(Self {
            endpoint,
            identity: ProviderIdentity {
                id: "http-json-v1".into(),
                version: "1".into(),
            },
            authorization,
            timeout,
            client: builder.build().map_err(|_| invalid())?,
        })
    }
}
impl HttpJsonExtractor {
    /// Fixed source identity, independent of secrets and operator endpoint configuration.
    pub fn identity(&self) -> &ProviderIdentity {
        &self.identity
    }
    pub fn extract(&self, input: Input<'_>) -> Result<ProviderOutput, ProviderFailure> {
        let start = Instant::now();
        let unavailable = || ProviderFailure::Unavailable;
        if input.pdf_bytes.is_empty()
            || input.pdf_bytes.len() > MAX_PDF_BYTES
            || input.sha256.len() != 64
            || !input
                .sha256
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            || format!("{:x}", Sha256::digest(input.pdf_bytes)) != input.sha256
        {
            return Err(unavailable());
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire<'a> {
            sha256: &'a str,
            pdf_bytes_base64: &'a str,
        }
        let encoded = Zeroizing::new(STANDARD.encode(input.pdf_bytes));
        let wire = Wire {
            sha256: input.sha256,
            pdf_bytes_base64: encoded.as_str(),
        };
        let body = Zeroizing::new(serde_json::to_vec(&wire).map_err(|_| unavailable())?);
        if body.len() > MAX_REQUEST {
            return Err(unavailable());
        }
        let remaining = self
            .timeout
            .checked_sub(start.elapsed())
            .filter(|d| !d.is_zero())
            .ok_or_else(unavailable)?;
        let response = self
            .client
            .post(self.endpoint.clone())
            .header(AUTHORIZATION, self.authorization.clone())
            .header(ACCEPT, "application/json")
            .header(CONTENT_TYPE, "application/json")
            .timeout(remaining)
            .body(body.to_vec())
            .send()
            .map_err(|_| unavailable())?;
        if !response.status().is_success()
            || response.headers().contains_key(CONTENT_ENCODING)
            || response
                .content_length()
                .is_some_and(|n| n > MAX_RESPONSE as u64)
        {
            return Err(unavailable());
        }
        let mut bytes = Zeroizing::new(Vec::new());
        response
            .take(MAX_RESPONSE as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| unavailable())?;
        if bytes.len() > MAX_RESPONSE || start.elapsed() >= self.timeout {
            return Err(unavailable());
        }
        let mut value: Value = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
        let fields = value
            .get_mut("fields")
            .and_then(Value::as_array_mut)
            .map(std::mem::take)
            .ok_or_else(unavailable)?;
        if start.elapsed() >= self.timeout {
            return Err(unavailable());
        }
        // Keep every raw field for the shared engine's citation/value/conflict checks.
        // Extra provider diagnostics are discarded, never returned or persisted here.
        Ok(ProviderOutput { fields })
    }
}
#[cfg(test)]
mod tests;
