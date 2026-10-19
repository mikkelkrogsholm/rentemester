//! Fixed read-only CVR HTTPS port. Operator secrets never enter requests from users or errors.
use reqwest::{
    blocking::Client,
    header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue},
};
use serde_json::{Value, json};
use std::{io::Read, time::Duration};
use url::Url;
use zeroize::Zeroizing;
const MAX_RESPONSE: u64 = 4 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(12);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookupError {
    MissingConfiguration,
    InvalidConfiguration,
    Unavailable,
    InvalidResponse,
    NotFound,
}
impl LookupError {
    pub fn message(self) -> &'static str {
        match self {
            Self::MissingConfiguration => {
                "CVR-opslag kræver miljøvariablerne CVR_USERNAME og CVR_PASSWORD"
            }
            Self::InvalidConfiguration => {
                "CVR-operatørkonfiguration eller privat HTTPS-certifikat er ugyldigt"
            }
            Self::Unavailable => "CVR-opslag fejlede eller overskred tidsgrænsen; CSV-data bevares",
            Self::InvalidResponse => "CVR returnerede et ugyldigt eller for stort svar",
            Self::NotFound => "Ingen virksomhed fundet for CVR-nummeret",
        }
    }
}
/// Narrow trusted test/runtime port; no SQL, caller endpoint, credentials, retries or callbacks.
pub trait Lookup {
    fn lookup(&mut self, cvr: &str) -> Result<Value, LookupError>;
}
pub struct Provider {
    client: Client,
    endpoint: Url,
    authorization: HeaderValue,
}
impl Provider {
    /// Trusted operator-only constructor. Never deserialized from an HTTP request.
    pub fn new(
        base: &str,
        username: &str,
        password: &str,
        trusted_ca: Option<&[u8]>,
    ) -> Result<Self, LookupError> {
        let invalid = LookupError::InvalidConfiguration;
        if base.trim() != base
            || base.len() > 4096
            || base.contains('\\')
            || base.chars().any(char::is_control)
            || username.is_empty()
            || username.len() > 4096
            || username.contains(':')
            || username.chars().any(char::is_control)
            || password.is_empty()
            || password.len() > 4096
            || password.chars().any(char::is_control)
        {
            return Err(invalid);
        }
        let mut endpoint = Url::parse(base.trim_end_matches('/')).map_err(|_| invalid)?;
        if endpoint.scheme() != "https"
            || !endpoint.has_host()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(invalid);
        }
        {
            let mut segments = endpoint.path_segments_mut().map_err(|_| invalid)?;
            segments.pop_if_empty().push("virksomhed").push("_search");
        }
        use base64::Engine;
        let credentials = Zeroizing::new(format!("{username}:{password}"));
        let encoded = Zeroizing::new(
            base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes()),
        );
        let mut authorization =
            HeaderValue::from_str(&format!("Basic {}", encoded.as_str())).map_err(|_| invalid)?;
        authorization.set_sensitive(true);
        let roots = if let Some(pem) = trusted_ca {
            if pem.is_empty() || pem.len() > 65536 {
                return Err(invalid);
            }
            reqwest::Certificate::from_pem_bundle(pem).map_err(|_| invalid)?
        } else {
            webpki_root_certs::TLS_SERVER_ROOT_CERTS
                .iter()
                .map(|c| reqwest::Certificate::from_der(c.as_ref()).map_err(|_| invalid))
                .collect::<Result<Vec<_>, _>>()?
        };
        if roots.is_empty() {
            return Err(invalid);
        }
        let client = Client::builder()
            .https_only(true)
            .hickory_dns(true)
            .tls_certs_only(roots)
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
            .timeout(TIMEOUT)
            .connect_timeout(TIMEOUT)
            .build()
            .map_err(|_| invalid)?;
        Ok(Self {
            client,
            endpoint,
            authorization,
        })
    }
    fn from_env() -> Result<Self, LookupError> {
        let username = Zeroizing::new(
            std::env::var("CVR_USERNAME").map_err(|_| LookupError::MissingConfiguration)?,
        );
        let password = Zeroizing::new(
            std::env::var("CVR_PASSWORD").map_err(|_| LookupError::MissingConfiguration)?,
        );
        if username.is_empty() || password.is_empty() {
            return Err(LookupError::MissingConfiguration);
        }
        let base = match std::env::var("RENTEMESTER_CVR_ENDPOINT") {
            Ok(v) => v,
            Err(std::env::VarError::NotPresent) => {
                "https://distribution.virk.dk/cvr-permanent".into()
            }
            Err(_) => return Err(LookupError::InvalidConfiguration),
        };
        let ca = std::env::var_os("RENTEMESTER_CVR_TRUSTED_CA_FILE")
            .map(|p| private_ca(std::path::Path::new(&p)))
            .transpose()?;
        Self::new(&base, &username, &password, ca.as_deref())
    }
}
fn request(cvr: &str) -> Result<Value, LookupError> {
    if cvr.len() != 8 || !cvr.bytes().all(|b| b.is_ascii_digit()) {
        return Err(LookupError::InvalidResponse);
    }
    let number: u32 = cvr.parse().map_err(|_| LookupError::InvalidResponse)?;
    Ok(
        json!({"from":0,"size":1,"_source":["Vrvirksomhed.cvrNummer","Vrvirksomhed.navne","Vrvirksomhed.virksomhedsform","Vrvirksomhed.virksomhedsstatus","Vrvirksomhed.beliggenhedsadresse","Vrvirksomhed.hovedbranche","Vrvirksomhed.virksomhedMetadata","Vrvirksomhed.telefonNummer","Vrvirksomhed.elektroniskPost","Vrvirksomhed.hjemmeside","Vrvirksomhed.attributter","Vrvirksomhed.deltagerRelation","Vrvirksomhed.reklamebeskyttet"],"query":{"term":{"Vrvirksomhed.cvrNummer":number}}}),
    )
}
impl Lookup for Provider {
    fn lookup(&mut self, cvr: &str) -> Result<Value, LookupError> {
        let body = serde_json::to_vec(&request(cvr)?).map_err(|_| LookupError::InvalidResponse)?;
        let response = self
            .client
            .post(self.endpoint.clone())
            .header(AUTHORIZATION, self.authorization.clone())
            .header(CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .map_err(|_| LookupError::Unavailable)?;
        if !response.status().is_success() {
            return Err(LookupError::Unavailable);
        }
        if response.content_length().is_some_and(|n| n > MAX_RESPONSE) {
            return Err(LookupError::InvalidResponse);
        }
        let mut bytes = vec![];
        response
            .take(MAX_RESPONSE + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| LookupError::Unavailable)?;
        if bytes.len() as u64 > MAX_RESPONSE {
            return Err(LookupError::InvalidResponse);
        }
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| LookupError::InvalidResponse)?;
        let entity = &value["hits"]["hits"][0]["_source"]["Vrvirksomhed"];
        if !entity.is_object() {
            return Err(LookupError::NotFound);
        }
        Ok(entity.clone())
    }
}
/// Construction is inert. ENV/CA/client initialization occurs only after authorized lookup needs.
#[derive(Default)]
pub struct OperatorLookup {
    provider: Option<Result<Provider, LookupError>>,
}
impl Lookup for OperatorLookup {
    fn lookup(&mut self, cvr: &str) -> Result<Value, LookupError> {
        match self.provider.get_or_insert_with(Provider::from_env) {
            Ok(p) => p.lookup(cvr),
            Err(e) => Err(*e),
        }
    }
}
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn private_ca(path: &std::path::Path) -> Result<Vec<u8>, LookupError> {
    use rustix::fs::{AtFlags, FileType, Mode, OFlags, fstat, open, statat};
    let invalid = LookupError::InvalidConfiguration;
    if !path.is_absolute() {
        return Err(invalid);
    }
    let before = statat(rustix::fs::CWD, path, AtFlags::SYMLINK_NOFOLLOW).map_err(|_| invalid)?;
    if FileType::from_raw_mode(before.st_mode) != FileType::RegularFile
        || before.st_mode & 0o7777 != 0o600
        || before.st_nlink != 1
        || before.st_size <= 0
        || before.st_size > 65536
        || before.st_uid != effective_uid()?
    {
        return Err(invalid);
    }
    let fd = open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map_err(|_| invalid)?;
    let after = fstat(&fd).map_err(|_| invalid)?;
    if before.st_ino != after.st_ino || before.st_dev != after.st_dev {
        return Err(invalid);
    }
    let mut bytes = vec![];
    std::fs::File::from(fd)
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid)?;
    if bytes.len() != before.st_size as usize {
        return Err(invalid);
    }
    Ok(bytes)
}
#[cfg(target_os = "macos")]
fn effective_uid() -> Result<u32, LookupError> {
    libproc::proc_pid::pidinfo::<libproc::bsd_info::BSDInfo>(std::process::id() as i32, 0)
        .map(|i| i.pbi_uid)
        .map_err(|_| LookupError::InvalidConfiguration)
}
#[cfg(target_os = "linux")]
fn effective_uid() -> Result<u32, LookupError> {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("Uid:"))
                .and_then(|l| l.split_whitespace().nth(2))
                .and_then(|v| v.parse().ok())
        })
        .ok_or(LookupError::InvalidConfiguration)
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn private_ca(_: &std::path::Path) -> Result<Vec<u8>, LookupError> {
    Err(LookupError::InvalidConfiguration)
}
#[cfg(test)]
mod tests;
