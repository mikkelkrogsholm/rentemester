//! Inbound listing and signed-URL byte transport only. No XML signature validation.
use super::*;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

/// Opaque provider row. Signed download capabilities never leave this module.
pub struct ReceivedRecord {
    internal_id: String,
    metadata: Value,
    download_url: Option<Url>,
}
impl ReceivedRecord {
    pub fn internal_id(&self) -> &str {
        &self.internal_id
    }
    /// Allowlisted provider metadata; never the signed URL or license/search data.
    pub fn metadata(&self) -> Value {
        self.metadata.clone()
    }
}
pub struct ReceivedPage {
    pub documents: Vec<ReceivedRecord>,
    pub has_next: bool,
}

// A process-private, salted witness, stable for equivalent admitted configuration.
// Never persist, serialize, or log it.
pub(super) fn generation(config: &Config) -> Result<[u8; 32], Error> {
    static SALT: OnceLock<Result<[u8; 32], Error>> = OnceLock::new();
    let salt = SALT.get_or_init(|| {
        let mut salt = [0; 32];
        getrandom::fill(&mut salt).map_err(|_| Error::Configuration)?;
        Ok(salt)
    });
    let salt = salt.as_ref().map_err(|e| *e)?;
    let mut hash = Sha256::new();
    hash.update(salt);
    for field in [
        config.api_license_key.as_str(),
        match config.environment {
            Environment::Test => "test",
            Environment::Production => "production",
        },
        config.base_url.as_deref().unwrap_or(""),
        config.trusted_ca_pem.as_deref().unwrap_or(""),
        &config.timeout_ms.to_string(),
    ] {
        hash.update((field.len() as u64).to_be_bytes());
        hash.update(field.as_bytes());
    }
    Ok(hash.finalize().into())
}

fn signed_url(raw: &str, allow_local: bool) -> Result<Url, Error> {
    if !text(raw, 16384) || raw.contains('\\') || !raw.starts_with("https://") {
        return Err(Error::Protocol);
    }
    let url = Url::parse(raw).map_err(|_| Error::Protocol)?;
    let authority = raw
        .split_once("://")
        .ok_or(Error::Protocol)?
        .1
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("");
    if url.scheme() != "https"
        || !url.has_host()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || authority.is_empty()
        || authority.contains('@')
    {
        return Err(Error::Protocol);
    }
    let local = match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback() || ip.is_unspecified(),
        Some(url::Host::Ipv6(ip)) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip
                    .to_ipv4_mapped()
                    .is_some_and(|v| v.is_loopback() || v.is_unspecified())
        }
        Some(url::Host::Domain(host)) => {
            host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost")
        }
        _ => true,
    };
    if local && !allow_local {
        return Err(Error::Protocol);
    }
    Ok(url)
}

const FIELDS: &[&str] = &[
    "documentId",
    "documentType",
    "documentTypeUrn",
    "profileId",
    "customizationId",
    "receivedAt",
    "senderParticipantType",
    "senderParticipantId",
    "senderName",
    "destinationParticipantType",
    "destinationParticipantId",
    "destinationName",
    "sourceNetwork",
    "internalId",
    "downloadUrlExpiresAt",
];
impl Client {
    /// Process-admission comparison only: never persist or expose this witness.
    pub fn credential_generation(&self) -> [u8; 32] {
        self.credential_generation
    }

    pub fn list_received_documents(
        &self,
        company_key: &str,
        limit: u32,
        offset: u32,
        max_timestamp: Option<&str>,
    ) -> Result<ReceivedPage, Error> {
        if !super::company_key(company_key)
            || !(1..=100).contains(&limit)
            || max_timestamp.is_some_and(|v| !text(v, 128))
        {
            return Err(Error::InvalidInput);
        }
        let mut url = self.url(&["list-received-documents"])?;
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("limit", &limit.to_string())
                .append_pair("offset", &offset.to_string());
            if let Some(timestamp) = max_timestamp {
                query.append_pair("maxTimestamp", timestamp);
            }
            query.append_pair("companyKey", company_key);
        }
        let (status, bytes) = self.request(url, None)?;
        if status != 200 {
            return Err(Error::Refused { status });
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| Error::Protocol)?;
        self.received_page(value)
    }

    fn received_page(&self, value: Value) -> Result<ReceivedPage, Error> {
        let rows = value
            .get("documents")
            .and_then(Value::as_array)
            .ok_or(Error::Protocol)?;
        if rows.len() > 100 {
            return Err(Error::Protocol);
        }
        let has_next = match value.get("nextPageUrl") {
            Some(Value::Null) | None => false,
            Some(Value::String(s)) if s.len() <= 16384 => true,
            _ => return Err(Error::Protocol),
        };
        let mut documents = Vec::with_capacity(rows.len());
        for row in rows {
            let row = row.as_object().ok_or(Error::Protocol)?;
            let internal_id = row
                .get("internalId")
                .and_then(Value::as_str)
                .filter(|v| text(v, 512) && !self.reflected_secret(v))
                .ok_or(Error::Protocol)?;
            let mut metadata = serde_json::Map::new();
            for field in FIELDS {
                if let Some(value) = row.get(*field) {
                    match value {
                        Value::Null => {}
                        Value::String(s)
                            if s.len() <= 4096
                                && !s.chars().any(char::is_control)
                                && !self.reflected_secret(s)
                                && !row
                                    .get("downloadUrl")
                                    .and_then(Value::as_str)
                                    .is_some_and(|url| !url.is_empty() && s.contains(url)) => {}
                        _ => return Err(Error::Protocol),
                    }
                    metadata.insert((*field).into(), value.clone());
                }
            }
            // Missing booking metadata (and a missing capability) remains a visible row.
            let download_url = match row.get("downloadUrl") {
                None | Some(Value::Null) => None,
                Some(Value::String(s)) => Some(signed_url(s, self.allow_local_download)?),
                _ => return Err(Error::Protocol),
            };
            documents.push(ReceivedRecord {
                internal_id: internal_id.into(),
                metadata: Value::Object(metadata),
                download_url,
            });
        }
        Ok(ReceivedPage {
            documents,
            has_next,
        })
    }
    fn reflected_secret(&self, s: &str) -> bool {
        s.contains(self.secret.as_str())
            || percent_encoding::percent_decode_str(s)
                .decode_utf8_lossy()
                .contains(self.secret.as_str())
    }

    /// Fetch exact original bytes without API authorization, redirects, or retries.
    pub fn download_received_document(&self, record: &ReceivedRecord) -> Result<Vec<u8>, Error> {
        let capability = record.download_url.as_ref().ok_or(Error::Protocol)?;
        let url = signed_url(capability.as_str(), self.allow_local_download)?;
        let mut response = self
            .http
            .get(url)
            .header(ACCEPT, "application/xml")
            .header("accept-encoding", "identity")
            .send()
            .map_err(|_| Error::Unavailable)?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(Error::Refused { status });
        }
        if response
            .headers()
            .get(CONTENT_ENCODING)
            .is_some_and(|v| v.as_bytes() != b"identity")
            || response
                .content_length()
                .is_some_and(|v| v > MAX_XML as u64)
        {
            return Err(Error::Protocol);
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut response)
            .take(MAX_XML as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| Error::Unavailable)?;
        if bytes.is_empty() || bytes.len() > MAX_XML {
            return Err(Error::Protocol);
        }
        Ok(bytes)
    }
}
#[cfg(test)]
#[path = "receive_tests.rs"]
mod tests;
