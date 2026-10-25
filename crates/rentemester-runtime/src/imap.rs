//! Fixed, bounded IMAP intake transport. No database, mailbox flags, or retry policy.
use hickory_resolver::TokioResolver;
use rustls::{
    ClientConfig, RootCertStore,
    pki_types::{CertificateDer, ServerName, pem::PemObject},
};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use zeroize::Zeroizing;

pub const MAX_MESSAGE: usize = 25 * 1024 * 1024;
const MAX_BATCH: usize = 64 * 1024 * 1024;
const MAX_MESSAGES: usize = 32;
const MAX_LINE: usize = 64 * 1024;
const MAX_UIDS: usize = 100_000;

/// Company-owned secret configuration. Never Debug/Serialize or included in backups.
pub struct Config {
    host: String,
    port: u16,
    secure: bool,
    username: String,
    password: Zeroizing<String>,
    mailbox: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireConfig {
    host: String,
    port: u16,
    username: String,
    password: String,
    #[serde(default = "yes")]
    secure: bool,
    #[serde(default = "inbox")]
    mailbox: String,
}
fn yes() -> bool {
    true
}
fn inbox() -> String {
    "INBOX".into()
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub host: String,
    pub port: u16,
    pub secure: bool,
    pub username: String,
    pub mailbox: String,
}
impl Config {
    pub fn parse(bytes: &[u8]) -> Result<Self, Failure> {
        if bytes.is_empty() || bytes.len() > 16 * 1024 {
            return Err(Failure::Configuration);
        }
        let w: WireConfig = serde_json::from_slice(bytes).map_err(|_| Failure::Configuration)?;
        let password = Zeroizing::new(w.password);
        let host = w.host.trim().to_owned();
        let username = w.username.trim().to_owned();
        let mailbox = if w.mailbox.trim().is_empty() {
            inbox()
        } else {
            w.mailbox.trim().to_owned()
        };
        if host.is_empty()
            || host.len() > 253
            || host.chars().any(|c| c.is_whitespace() || c.is_control())
            || w.port == 0
            || username.is_empty()
            || username.len() > 4096
            || password.trim().is_empty()
            || password.len() > 8192
            || mailbox.len() > 1024
            || [username.as_str(), password.as_str(), mailbox.as_str()]
                .iter()
                .any(|s| s.chars().any(char::is_control))
        {
            return Err(Failure::Configuration);
        }
        ServerName::try_from(host.clone()).map_err(|_| Failure::Configuration)?;
        Ok(Self {
            host,
            port: w.port,
            secure: w.secure,
            username,
            password,
            mailbox,
        })
    }
    pub fn status(&self) -> Status {
        Status {
            host: self.host.clone(),
            port: self.port,
            secure: self.secure,
            username: self.username.clone(),
            mailbox: self.mailbox.clone(),
        }
    }
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperatorConfig {
    #[serde(default)]
    pub trusted_ca_pem: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}
fn default_timeout() -> u64 {
    30_000
}
impl Default for OperatorConfig {
    fn default() -> Self {
        Self {
            trusted_ca_pem: None,
            timeout_ms: default_timeout(),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Cursor {
    pub uid_validity: u32,
    pub last_uid: u32,
}
#[derive(Debug)]
pub struct Message {
    pub uid: u32,
    pub raw: Vec<u8>,
}
#[derive(Debug)]
pub struct Refusal {
    pub uid: u32,
    pub declared_size: u64,
    pub code: String,
}
#[derive(Debug)]
pub struct Batch {
    pub uid_validity: u32,
    pub messages: Vec<Message>,
    pub has_more: bool,
    pub refusals: Vec<Refusal>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Configuration,
    Unavailable,
    Authentication,
    Timeout,
    Protocol,
    Limit,
}
impl Failure {
    pub fn code(self) -> &'static str {
        match self {
            Self::Configuration => "imap_config_invalid",
            Self::Unavailable => "imap_unavailable",
            Self::Authentication => "imap_authentication_refused",
            Self::Timeout => "imap_timeout",
            Self::Protocol => "imap_protocol_invalid",
            Self::Limit => "imap_limit",
        }
    }
}
pub trait Client {
    fn fetch(&mut self, config: &Config, cursor: Option<Cursor>) -> Result<Batch, Failure>;
}
pub struct NetworkClient {
    tls: Arc<ClientConfig>,
    timeout: Duration,
}
impl NetworkClient {
    pub fn new(operator: OperatorConfig) -> Result<Self, Failure> {
        if !(1..=30_000).contains(&operator.timeout_ms) {
            return Err(Failure::Configuration);
        }
        let mut roots = RootCertStore::empty();
        for der in webpki_root_certs::TLS_SERVER_ROOT_CERTS {
            roots
                .add(CertificateDer::from(der.as_ref().to_vec()))
                .map_err(|_| Failure::Configuration)?;
        }
        if let Some(pem) = &operator.trusted_ca_pem {
            if pem.len() > 1024 * 1024 {
                return Err(Failure::Configuration);
            }
            let certs = CertificateDer::pem_slice_iter(pem.as_bytes())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| Failure::Configuration)?;
            if certs.is_empty() {
                return Err(Failure::Configuration);
            }
            for cert in certs {
                roots.add(cert).map_err(|_| Failure::Configuration)?;
            }
        }
        let tls =
            ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .map_err(|_| Failure::Configuration)?
                .with_root_certificates(roots)
                .with_no_client_auth();
        Ok(Self {
            tls: Arc::new(tls),
            timeout: Duration::from_millis(operator.timeout_ms),
        })
    }
    async fn run(&self, c: &Config, cursor: Option<Cursor>) -> Result<Batch, Failure> {
        let address = if let Ok(ip) = c.host.parse::<std::net::IpAddr>() {
            ip
        } else {
            let mut builder = TokioResolver::builder_tokio().map_err(|_| Failure::Unavailable)?;
            builder.options_mut().attempts = 1;
            builder.options_mut().num_concurrent_reqs = 1;
            let resolver = builder.build().map_err(|_| Failure::Unavailable)?;
            resolver
                .lookup_ip(c.host.as_str())
                .await
                .map_err(|_| Failure::Unavailable)?
                .iter()
                .next()
                .ok_or(Failure::Unavailable)?
        };
        // One connection attempt; later polling is an explicit separate service pass.
        let socket = tokio::net::TcpStream::connect((address, c.port))
            .await
            .map_err(|_| Failure::Unavailable)?;
        if c.secure {
            let name = ServerName::try_from(c.host.clone()).map_err(|_| Failure::Configuration)?;
            let socket = tokio_rustls::TlsConnector::from(self.tls.clone())
                .connect(name, socket)
                .await
                .map_err(|_| Failure::Unavailable)?;
            session(socket, c, cursor).await
        } else {
            session(socket, c, cursor).await
        }
    }
}
impl Client for NetworkClient {
    fn fetch(&mut self, c: &Config, cursor: Option<Cursor>) -> Result<Batch, Failure> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| Failure::Unavailable)?;
        rt.block_on(async {
            tokio::time::timeout(self.timeout, self.run(c, cursor))
                .await
                .map_err(|_| Failure::Timeout)?
        })
    }
}
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}
struct Response {
    lines: Vec<String>,
    literal: Option<Vec<u8>>,
    oversized: Option<u64>,
    budget: bool,
}
async fn line<S: AsyncRead + Unpin>(s: &mut BufReader<S>) -> Result<String, Failure> {
    let mut out = Vec::new();
    loop {
        let chunk = s.fill_buf().await.map_err(|_| Failure::Unavailable)?;
        if chunk.is_empty() {
            return Err(Failure::Protocol);
        }
        let n = chunk
            .iter()
            .position(|b| *b == b'\n')
            .map_or(chunk.len(), |n| n + 1);
        if out.len() + n > MAX_LINE {
            return Err(Failure::Limit);
        }
        let done = chunk[n - 1] == b'\n';
        out.extend_from_slice(&chunk[..n]);
        s.consume(n);
        if done {
            break;
        }
    }
    if !out.ends_with(b"\r\n") {
        return Err(Failure::Protocol);
    }
    out.truncate(out.len() - 2);
    String::from_utf8(out).map_err(|_| Failure::Protocol)
}
async fn command<S: AsyncRead + AsyncWrite + Unpin>(
    s: &mut BufReader<S>,
    tag: usize,
    cmd: &str,
    budget: usize,
    fetch: Option<u32>,
) -> Result<Response, Failure> {
    let tag = format!("A{tag:04}");
    let request = Zeroizing::new(format!("{tag} {cmd}\r\n"));
    s.get_mut()
        .write_all(request.as_bytes())
        .await
        .map_err(|_| Failure::Unavailable)?;
    s.get_mut()
        .flush()
        .await
        .map_err(|_| Failure::Unavailable)?;
    let mut out = Response {
        lines: vec![],
        literal: None,
        oversized: None,
        budget: false,
    };
    let mut total = 0;
    loop {
        let l = line(s).await?;
        total += l.len();
        if total > 1024 * 1024 {
            return Err(Failure::Limit);
        }
        if let Some(rest) = l.strip_prefix(&format!("{tag} ")) {
            if !rest
                .split_whitespace()
                .next()
                .is_some_and(|v| v.eq_ignore_ascii_case("OK"))
            {
                return Err(if cmd.starts_with("LOGIN ") {
                    Failure::Authentication
                } else {
                    Failure::Protocol
                });
            }
            return Ok(out);
        }
        if l.starts_with('*')
            && l.split_whitespace()
                .nth(1)
                .is_some_and(|v| v.eq_ignore_ascii_case("BYE"))
        {
            return Err(Failure::Unavailable);
        }
        if l.ends_with('}') {
            let pos = l.rfind('{').ok_or(Failure::Protocol)?;
            let digits = &l[pos + 1..l.len() - 1];
            if fetch.is_none()
                || out.literal.is_some()
                || digits.is_empty()
                || !digits.bytes().all(|b| b.is_ascii_digit())
            {
                return Err(Failure::Protocol);
            }
            let expected_uid = fetch.ok_or(Failure::Protocol)?;
            if !l.to_ascii_uppercase().contains(" FETCH ")
                || !l.to_ascii_uppercase().contains("BODY[] {")
                || !has_uid(&l, expected_uid)
            {
                return Err(Failure::Protocol);
            }
            let size = digits.parse::<u64>().map_err(|_| Failure::Limit)?;
            if size > MAX_MESSAGE as u64 {
                out.oversized = Some(size);
                return Ok(out);
            }
            if size > budget as u64 {
                out.budget = true;
                return Ok(out);
            }
            let mut raw = vec![0; size as usize];
            s.read_exact(&mut raw)
                .await
                .map_err(|_| Failure::Unavailable)?;
            out.literal = Some(raw);
        }
        out.lines.push(l);
    }
}
fn has_uid(line: &str, uid: u32) -> bool {
    let tokens = line
        .split([' ', '(', ')'])
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    tokens
        .windows(2)
        .any(|w| w[0].eq_ignore_ascii_case("UID") && w[1].parse::<u32>() == Ok(uid))
}
async fn session<S: AsyncRead + AsyncWrite + Unpin>(
    socket: S,
    c: &Config,
    cursor: Option<Cursor>,
) -> Result<Batch, Failure> {
    let mut s = BufReader::new(socket);
    let greeting = line(&mut s).await?;
    if !greeting.starts_with("* OK ") && greeting != "* OK" {
        return Err(Failure::Protocol);
    }
    let login = Zeroizing::new(format!(
        "LOGIN {} {}",
        quote(&c.username),
        quote(&c.password)
    ));
    command(&mut s, 1, &login, 0, None).await?;
    let selected = command(&mut s, 2, &format!("SELECT {}", quote(&c.mailbox)), 0, None).await?;
    let mut validity = None;
    for l in selected.lines {
        if let Some((_, rest)) = l.split_once("[UIDVALIDITY ") {
            let n = rest
                .split_once(']')
                .ok_or(Failure::Protocol)?
                .0
                .parse::<u32>()
                .map_err(|_| Failure::Protocol)?;
            if n == 0 || validity.replace(n).is_some() {
                return Err(Failure::Protocol);
            }
        }
    }
    let validity = validity.ok_or(Failure::Protocol)?;
    let start = match cursor {
        Some(old) if old.uid_validity == validity || old.uid_validity == 0 => {
            old.last_uid.checked_add(1)
        }
        _ => Some(1),
    };
    let mut batch = Batch {
        uid_validity: validity,
        messages: vec![],
        has_more: false,
        refusals: vec![],
    };
    let Some(start) = start else { return Ok(batch) };
    let searched = command(&mut s, 3, &format!("UID SEARCH UID {start}:*"), 0, None).await?;
    let mut uids = std::collections::BTreeSet::new();
    let mut search_seen = false;
    for l in searched.lines {
        if let Some(rest) = l.strip_prefix("* SEARCH") {
            if search_seen {
                return Err(Failure::Protocol);
            }
            search_seen = true;
            if !rest.is_empty() && !rest.starts_with(' ') {
                return Err(Failure::Protocol);
            }
            for n in rest.split_whitespace() {
                let uid = n.parse::<u32>().map_err(|_| Failure::Protocol)?;
                if uid == 0 {
                    return Err(Failure::Protocol);
                }
                if uid >= start {
                    uids.insert(uid);
                }
                if uids.len() > MAX_UIDS {
                    return Err(Failure::Limit);
                }
            }
        }
    }
    if !search_seen {
        return Err(Failure::Protocol);
    }
    let mut bytes = 0;
    for (index, uid) in uids.iter().enumerate() {
        if batch.messages.len() == MAX_MESSAGES {
            batch.has_more = true;
            break;
        }
        let r = command(
            &mut s,
            index + 4,
            &format!("UID FETCH {uid} (UID BODY.PEEK[])"),
            MAX_BATCH - bytes,
            Some(*uid),
        )
        .await?;
        if let Some(size) = r.oversized {
            batch.refusals.push(Refusal {
                uid: *uid,
                declared_size: size,
                code: "imap_message_too_large".into(),
            });
            batch.has_more = true;
            return Ok(batch);
        }
        if r.budget {
            batch.has_more = true;
            return Ok(batch);
        }
        if !r.lines.iter().any(|l| has_uid(l, *uid)) {
            return Err(Failure::Protocol);
        }
        let raw = r.literal.ok_or(Failure::Protocol)?;
        if raw.is_empty() {
            return Err(Failure::Protocol);
        }
        bytes += raw.len();
        batch.messages.push(Message { uid: *uid, raw });
    }
    // LOGOUT failure cannot invalidate retained read-only bytes. Deadline still applies.
    let _ = command(&mut s, MAX_UIDS + 10, "LOGOUT", 0, None).await;
    Ok(batch)
}
#[cfg(test)]
mod tests;

pub mod original717;
