//! Bounded verifier-only Drive reads. No upload, caller URLs, SQL or credentials in output.
use crate::{auth::Clock, backup_artifact::MAX_WIRE_BYTES};
use hickory_resolver::{TokioResolver, config::LookupIpStrategy};
use rentemester_engine::Error;
use reqwest::{
    blocking::{Client, Response},
    header::{ACCEPT, ACCEPT_ENCODING, AUTHORIZATION, CONTENT_ENCODING, HeaderValue},
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
const ENDPOINT: &str = "https://www.googleapis.com/drive/v3";
const MAX_METADATA: usize = 64 * 1024;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expected {
    pub object_id: String,
    pub folder_id: String,
    pub file_name: String,
    pub archive_sha256: String,
    pub archive_size_bytes: u64,
}
/// Constructed by trusted runtime execution, never deserialized from a request.
#[derive(Clone, Debug, Serialize)]
pub struct Observation {
    pub object_id: String,
    pub folder_id: String,
    pub file_name: String,
    pub archive_sha256: String,
    pub archive_size_bytes: u64,
    pub observed_at_ms: i64,
}
pub trait Verifier: Send + Sync {
    fn verify(&self, expected: &Expected, clock: &dyn Clock) -> Result<Observation, Error>;
}
/// Private operator configuration, deliberately not Debug/Serde.
pub struct GatewayConfig<'a> {
    pub endpoint: &'a str,
    pub bearer_token: &'a str,
    pub timeout_ms: u64,
}
pub struct GoogleDriveVerifier {
    endpoint: Url,
    authorization: HeaderValue,
    timeout: Duration,
    client: Client,
}
fn invalid() -> Error {
    Error::new(
        "backup_remote_invalid_config",
        "Remote backup operator configuration is invalid",
    )
}
fn refused() -> Error {
    Error::new(
        "backup_remote_refused",
        "Remote backup identity or content could not be verified",
    )
}
fn unavailable() -> Error {
    Error::new(
        "backup_remote_unavailable",
        "Remote backup verification is unavailable",
    )
}
pub fn valid_object_id(value: &str) -> bool {
    (1..=160).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn local(ip: IpAddr) -> bool {
    ip.is_loopback()
        || ip.is_unspecified()
        || matches!(ip,IpAddr::V6(v)if v.to_ipv4_mapped().is_some_and(|v|v.is_loopback()||v.is_unspecified()))
}
struct DriveDns(TokioResolver);
impl reqwest::dns::Resolve for DriveDns {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let resolver = self.0.clone();
        Box::pin(async move {
            let lookup = resolver
                .lookup_ip(name.as_str())
                .await
                .map_err(|_| std::io::Error::other("backup DNS unavailable"))?;
            let addresses: Vec<_> = lookup.iter().collect();
            if addresses.is_empty() || addresses.iter().any(|ip| local(*ip)) {
                return Err(std::io::Error::other("backup DNS refused").into());
            }
            let addresses: Vec<SocketAddr> = addresses
                .into_iter()
                .map(|ip| SocketAddr::new(ip, 0))
                .collect();
            Ok(Box::new(addresses.into_iter()) as reqwest::dns::Addrs)
        })
    }
}
fn endpoint(raw: &str, private_ca: bool) -> Result<(Url, bool), Error> {
    if raw.len() > 4096
        || raw.trim() != raw
        || raw.contains('\\')
        || raw.chars().any(char::is_control)
    {
        return Err(invalid());
    }
    let url = Url::parse(raw).map_err(|_| invalid())?;
    if raw == ENDPOINT {
        return Ok((url, true));
    }
    let authority = raw
        .split_once("://")
        .map(|(_, s)| s.split(['/', '?', '#']).next().unwrap_or(""))
        .unwrap_or("");
    let loopback = match url.host() {
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => {
            ip.is_loopback() || ip.to_ipv4_mapped().is_some_and(|v| v.is_loopback())
        }
        _ => false,
    };
    if !private_ca
        || !loopback
        || url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || authority.contains('@')
        || url.path() != "/drive/v3"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    Ok((url, false))
}
impl GoogleDriveVerifier {
    pub fn new(config: GatewayConfig<'_>) -> Result<Self, Error> {
        let roots = webpki_root_certs::TLS_SERVER_ROOT_CERTS
            .iter()
            .map(|c| reqwest::Certificate::from_der(c.as_ref()).map_err(|_| invalid()))
            .collect::<Result<Vec<_>, _>>()?;
        Self::build(config, roots, false)
    }
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
        let (endpoint, domain) = endpoint(config.endpoint, private_ca)?;
        if config.bearer_token.is_empty()
            || config.bearer_token.len() > 4096
            || !config
                .bearer_token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._~+/=".contains(&b))
            || !(100..=120000).contains(&config.timeout_ms)
        {
            return Err(invalid());
        }
        let bearer = Zeroizing::new(format!("Bearer {}", config.bearer_token));
        let mut authorization = HeaderValue::from_str(&bearer).map_err(|_| invalid())?;
        authorization.set_sensitive(true);
        let timeout = Duration::from_millis(config.timeout_ms);
        let mut builder = Client::builder()
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
            .tls_certs_only(roots);
        if domain {
            let mut resolver = TokioResolver::builder_tokio().map_err(|_| invalid())?;
            resolver.options_mut().ip_strategy = LookupIpStrategy::Ipv4AndIpv6;
            builder =
                builder.dns_resolver(Arc::new(DriveDns(resolver.build().map_err(|_| invalid())?)));
        }
        Ok(Self {
            endpoint,
            authorization,
            timeout,
            client: builder.build().map_err(|_| invalid())?,
        })
    }
    fn get(&self, expected: &Expected, media: bool, start: Instant) -> Result<Response, Error> {
        let remaining = self
            .timeout
            .checked_sub(start.elapsed())
            .filter(|v| !v.is_zero())
            .ok_or_else(unavailable)?;
        let mut url = self.endpoint.clone();
        url.path_segments_mut()
            .map_err(|_| invalid())?
            .push("files")
            .push(&expected.object_id);
        if media {
            url.query_pairs_mut()
                .append_pair("alt", "media")
                .append_pair("supportsAllDrives", "true");
        } else {
            url.query_pairs_mut()
                .append_pair("fields", "id,name,parents,size,sha256Checksum,trashed")
                .append_pair("supportsAllDrives", "true");
        }
        let response = self
            .client
            .get(url)
            .header(AUTHORIZATION, self.authorization.clone())
            .header(
                ACCEPT,
                if media {
                    "application/octet-stream"
                } else {
                    "application/json"
                },
            )
            .header(ACCEPT_ENCODING, "identity")
            .timeout(remaining)
            .send()
            .map_err(|_| unavailable())?;
        if response.status() != reqwest::StatusCode::OK
            || response.headers().contains_key(CONTENT_ENCODING)
        {
            return Err(unavailable());
        }
        Ok(response)
    }
}
fn validate_expected(e: &Expected) -> Result<(), Error> {
    if !valid_object_id(&e.object_id)
        || !valid_object_id(&e.folder_id)
        || e.file_name.is_empty()
        || e.file_name.len() > 320
        || e.file_name.trim() != e.file_name
        || e.file_name
            .chars()
            .any(|c| c.is_control() || c == '/' || c == '\\')
        || e.archive_sha256.len() != 64
        || !e
            .archive_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || e.archive_size_bytes == 0
        || e.archive_size_bytes > MAX_WIRE_BYTES as u64
    {
        return Err(refused());
    }
    Ok(())
}
fn metadata(bytes: &[u8], expected: &Expected) -> Result<(), Error> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| refused())?;
    let size = value["size"]
        .as_str()
        .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|v| v.parse::<u64>().ok());
    let parents = value["parents"].as_array();
    if value["id"].as_str() != Some(expected.object_id.as_str())
        || value["name"].as_str() != Some(expected.file_name.as_str())
        || size != Some(expected.archive_size_bytes)
        || value["trashed"] != false
        || parents
            .is_none_or(|v| v.len() != 1 || v[0].as_str() != Some(expected.folder_id.as_str()))
        || value["sha256Checksum"]
            .as_str()
            .is_none_or(|v| !v.eq_ignore_ascii_case(&expected.archive_sha256))
    {
        return Err(refused());
    }
    Ok(())
}
fn clock_now(clock: &dyn Clock) -> Result<i64, Error> {
    let now = clock.now_unix_ms().map_err(|_| unavailable())?;
    if !(0..=8_640_000_000_000_000).contains(&now) {
        return Err(unavailable());
    }
    Ok(now)
}
fn observation(expected: &Expected, before: i64, after: i64) -> Result<Observation, Error> {
    if after < before || after - before > 300_000 {
        return Err(unavailable());
    }
    Ok(Observation {
        object_id: expected.object_id.clone(),
        folder_id: expected.folder_id.clone(),
        file_name: expected.file_name.clone(),
        archive_sha256: expected.archive_sha256.clone(),
        archive_size_bytes: expected.archive_size_bytes,
        observed_at_ms: after,
    })
}
impl Verifier for GoogleDriveVerifier {
    fn verify(&self, expected: &Expected, clock: &dyn Clock) -> Result<Observation, Error> {
        validate_expected(expected)?;
        let before = clock_now(clock)?;
        let start = Instant::now();
        let response = self.get(expected, false, start)?;
        if response
            .content_length()
            .is_some_and(|n| n > MAX_METADATA as u64)
        {
            return Err(refused());
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_METADATA as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| unavailable())?;
        if bytes.len() > MAX_METADATA || start.elapsed() >= self.timeout {
            return Err(refused());
        }
        metadata(&bytes, expected)?;
        let response = self.get(expected, true, start)?;
        if response
            .content_length()
            .is_some_and(|n| n != expected.archive_size_bytes)
        {
            return Err(refused());
        }
        let mut input = response.take(expected.archive_size_bytes + 1);
        let mut digest = Sha256::new();
        let mut size = 0u64;
        let mut buffer = [0u8; 8192];
        loop {
            let n = input.read(&mut buffer).map_err(|_| unavailable())?;
            if n == 0 {
                break;
            }
            size += n as u64;
            if size > expected.archive_size_bytes {
                return Err(refused());
            }
            digest.update(&buffer[..n]);
            if start.elapsed() >= self.timeout {
                return Err(unavailable());
            }
        }
        if size != expected.archive_size_bytes
            || format!("{:x}", digest.finalize()) != expected.archive_sha256
        {
            return Err(refused());
        }
        let after = clock_now(clock)?;
        if start.elapsed() >= self.timeout {
            return Err(unavailable());
        }
        observation(expected, before, after)
    }
}
#[cfg(test)]
mod tests;
