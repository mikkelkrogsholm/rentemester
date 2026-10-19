//! Deterministic invoice MIME and one bounded SMTP handoff. No ledger, retry or credential persistence.
use base64::{Engine, engine::general_purpose::STANDARD};
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Tokio1Executor,
    address::Envelope,
    transport::smtp::{
        authentication::Credentials,
        client::{Certificate, CertificateStore, Tls, TlsParameters},
    },
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{io::Read, path::PathBuf, time::Duration};
use zeroize::Zeroize;

pub const MAX_PDF_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_MIME_BYTES: usize = 20 * 1024 * 1024;
const DEADLINE: Duration = Duration::from_secs(8);
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SenderIdentity {
    pub host: String,
    pub port: u16,
    pub from_address: String,
    pub from_name: Option<String>,
}
/// Original prepared MIME, intentionally not printable or serializable by an adapter.
pub struct Message {
    pub to: String,
    pub from: String,
    pub subject: String,
    pub message_id: String,
    pub raw: Vec<u8>,
    pub body_sha256: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handoff {
    Accepted,
    Refused,
    Unknown,
}
pub trait Transport {
    fn identity(&self) -> &SenderIdentity;
    fn send(&mut self, message: &Message) -> Handoff;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MailError;
impl MailError {
    pub fn code(&self) -> &'static str {
        "invoice_mail_invalid"
    }
}
impl std::fmt::Display for MailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for MailError {}
#[derive(Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Mode {
    #[default]
    Implicit,
    Starttls,
}
/// Private operator configuration only. Never accept this through a product mutation.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    host: String,
    port: u16,
    from_address: String,
    from_name: Option<String>,
    username: Option<String>,
    password: Option<String>,
    #[serde(default)]
    tls: Mode,
    trusted_ca_file: Option<PathBuf>,
}
impl Drop for Config {
    fn drop(&mut self) {
        self.username.zeroize();
        self.password.zeroize();
    }
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn email(s: &str) -> bool {
    if s.is_empty() || s.len() > 254 || !s.is_ascii() || s.trim() != s {
        return false;
    }
    let Some((local, domain)) = s.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && local.len() <= 64
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && local
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".!#$%&'*+-/=?^_`{|}~".contains(&b))
        && domain.contains('.')
        && domain.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}
fn identity_valid(i: &SenderIdentity) -> bool {
    !i.host.is_empty()
        && i.host.len() <= 253
        && i.host.trim() == i.host
        && i.host.is_ascii()
        && i.host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-:".contains(&b))
        && i.port > 0
        && email(&i.from_address)
        && i.from_name.as_ref().is_none_or(|n| {
            !n.trim().is_empty()
                && n.len() <= 200
                && n.trim() == n
                && !n.chars().any(char::is_control)
                && !n.contains(['<', '>', '"'])
        })
}
fn header(s: &str) -> String {
    if s.is_ascii() {
        s.to_owned()
    } else {
        format!("=?UTF-8?B?{}?=", STANDARD.encode(s.as_bytes()))
    }
}
fn wrapped(bytes: &[u8]) -> String {
    STANDARD
        .encode(bytes)
        .as_bytes()
        .chunks(76)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect::<Vec<_>>()
        .join("\r\n")
}
fn display_name(name: &str) -> String {
    if name.is_ascii() && name.bytes().any(|b| b"(),:;@[]\\".contains(&b)) {
        format!("\"{}\"", name.replace('\\', "\\\\"))
    } else {
        header(name)
    }
}
fn from(i: &SenderIdentity) -> String {
    match &i.from_name {
        Some(n) => format!("{} <{}>", display_name(n), i.from_address),
        None => i.from_address.clone(),
    }
}
/// Matches the source TS fingerprint, boundary, headers and body without a generated Date header.
pub fn build(
    identity: &SenderIdentity,
    to: &str,
    kind: &str,
    invoice_number: &str,
    pdf: &[u8],
    pdf_filename: &str,
) -> Result<Message, MailError> {
    let to = to.trim();
    if !identity_valid(identity)
        || !email(to)
        || !matches!(kind, "invoice" | "reminder")
        || invoice_number.is_empty()
        || invoice_number.len() > 100
        || !invoice_number
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        || pdf_filename.len() > 150
        || !pdf_filename.ends_with(".pdf")
        || !pdf_filename
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        || pdf.is_empty()
        || pdf.len() > MAX_PDF_BYTES
        || !pdf.starts_with(b"%PDF-")
    {
        return Err(MailError);
    }
    let subject = if kind == "reminder" {
        format!("Betalingspaamindelse for faktura {invoice_number}")
    } else {
        format!("Faktura {invoice_number}")
    };
    let body = if kind == "reminder" {
        format!(
            "Hej\r\n\r\nVi kan se at faktura {invoice_number} endnu ikke er betalt. Fakturaen er vedhaeftet som PDF. Kontakt os hvis betalingen allerede er gennemfoert.\r\n\r\nVenlig hilsen\r\nRentemester"
        )
    } else {
        format!(
            "Hej\r\n\r\nHermed faktura {invoice_number}, vedhaeftet som PDF.\r\n\r\nVenlig hilsen\r\nRentemester"
        )
    };
    let fingerprint = digest(
        format!(
            "{kind}|{invoice_number}|{to}|{}|{}",
            identity.from_address,
            digest(pdf)
        )
        .as_bytes(),
    );
    let message_id = format!("<{}@rentemester.local>", &fingerprint[..32]);
    let boundary = format!("=_rentemester_{}", &fingerprint[..24]);
    let from = from(identity);
    let raw = [
        format!("From: {from}"),
        format!("To: {to}"),
        format!("Subject: {}", header(&subject)),
        format!("Message-ID: {message_id}"),
        "MIME-Version: 1.0".into(),
        format!("Content-Type: multipart/mixed; boundary=\"{boundary}\""),
        String::new(),
        format!("--{boundary}"),
        "Content-Type: text/plain; charset=UTF-8".into(),
        "Content-Transfer-Encoding: base64".into(),
        String::new(),
        wrapped(body.as_bytes()),
        format!("--{boundary}"),
        "Content-Type: application/pdf".into(),
        "Content-Transfer-Encoding: base64".into(),
        format!("Content-Disposition: attachment; filename=\"{pdf_filename}\""),
        String::new(),
        wrapped(pdf),
        format!("--{boundary}--"),
        String::new(),
    ]
    .join("\r\n")
    .into_bytes();
    if raw.len() > MAX_MIME_BYTES {
        return Err(MailError);
    }
    let body_sha256 = digest(&raw);
    Ok(Message {
        to: to.into(),
        from,
        subject,
        message_id,
        raw,
        body_sha256,
    })
}
/// Called on the blocking owner thread; its isolated runtime cannot leave a sender task running.
pub struct SmtpSender {
    identity: SenderIdentity,
    transport: AsyncSmtpTransport<Tokio1Executor>,
}
impl SmtpSender {
    pub fn new(config: &Config) -> Result<Self, MailError> {
        let identity = SenderIdentity {
            host: config.host.clone(),
            port: config.port,
            from_address: config.from_address.clone(),
            from_name: config.from_name.clone(),
        };
        if !identity_valid(&identity)
            || config.username.is_some() != config.password.is_some()
            || config
                .username
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.len() > 4096 || s.chars().any(char::is_control))
            || config
                .password
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.len() > 4096 || s.chars().any(char::is_control))
        {
            return Err(MailError);
        }
        let mut tls = TlsParameters::builder(config.host.clone());
        if let Some(path) = &config.trusted_ca_file {
            if !path.is_absolute() {
                return Err(MailError);
            }
            let metadata = std::fs::metadata(path).map_err(|_| MailError)?;
            if !metadata.is_file() || metadata.len() > 65536 {
                return Err(MailError);
            }
            let mut pem = Vec::new();
            let mut file = std::fs::File::open(path).map_err(|_| MailError)?;
            if !file.metadata().map_err(|_| MailError)?.is_file() {
                return Err(MailError);
            }
            (&mut file)
                .take(65537)
                .read_to_end(&mut pem)
                .map_err(|_| MailError)?;
            if pem.is_empty() || pem.len() > 65536 {
                return Err(MailError);
            }
            tls = tls
                .certificate_store(CertificateStore::None)
                .add_root_certificate(Certificate::from_pem(&pem).map_err(|_| MailError)?);
        }
        let tls = tls.build().map_err(|_| MailError)?;
        let mut transport = AsyncSmtpTransport::<Tokio1Executor>::relay(&config.host)
            .map_err(|_| MailError)?
            .port(config.port)
            .timeout(Some(DEADLINE))
            .tls(match config.tls {
                Mode::Implicit => Tls::Wrapper(tls),
                Mode::Starttls => Tls::Required(tls),
            });
        if let (Some(user), Some(password)) = (&config.username, &config.password) {
            transport = transport.credentials(Credentials::new(user.clone(), password.clone()));
        }
        Ok(Self {
            identity,
            transport: transport.build(),
        })
    }
}
impl Transport for SmtpSender {
    fn identity(&self) -> &SenderIdentity {
        &self.identity
    }
    fn send(&mut self, message: &Message) -> Handoff {
        // Do not accept a forged Message as permission to change the configured envelope sender.
        if message.from != from(&self.identity)
            || !email(&message.to)
            || message.raw.is_empty()
            || message.raw.len() > MAX_MIME_BYTES
            || digest(&message.raw) != message.body_sha256
        {
            return Handoff::Refused;
        }
        let envelope = match Envelope::new(
            Some(match self.identity.from_address.parse() {
                Ok(a) => a,
                Err(_) => return Handoff::Refused,
            }),
            vec![match message.to.parse() {
                Ok(a) => a,
                Err(_) => return Handoff::Refused,
            }],
        ) {
            Ok(e) => e,
            Err(_) => return Handoff::Refused,
        };
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(r) => r,
            Err(_) => return Handoff::Refused,
        };
        let result = runtime.block_on(async {
            tokio::time::timeout(DEADLINE, self.transport.send_raw(&envelope, &message.raw)).await
        });
        // The inline send future (and its socket) has completed or been dropped. Never spawn
        // a message task. Avoid waiting beyond the deadline for an OS DNS lookup to exit.
        runtime.shutdown_background();
        match result {
            Ok(Ok(response)) if response.has_code(250) => Handoff::Accepted,
            Ok(Err(error))
                if error.status().is_some_and(|code| {
                    let code = u16::from(code);
                    (400..600).contains(&code)
                }) =>
            {
                Handoff::Refused
            }
            _ => Handoff::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sender() -> SenderIdentity {
        SenderIdentity {
            host: "smtp.example.invalid".into(),
            port: 465,
            from_address: "billing@example.invalid".into(),
            from_name: Some("Synthetic Å".into()),
        }
    }
    fn message(kind: &str) -> Message {
        build(
            &sender(),
            "customer@example.invalid",
            kind,
            "2026-0001",
            b"%PDF-1.4\nsynthetic immutable\n",
            "invoice-2026-0001.pdf",
        )
        .unwrap()
    }
    #[test]
    fn byte_exact_source_ts_invoice_and_reminder_goldens() {
        // Produced independently by src/core/email.ts buildInvoiceEmailMessage through Bun.
        for (kind, id, hash, len) in [
            (
                "invoice",
                "<d531852e088ddc1ac234b25f6945b721@rentemester.local>",
                "c8e46b248fb60503742a9b3bc0398f33833ef6d1cf8fe24d6d05c2fd3fd43af7",
                788,
            ),
            (
                "reminder",
                "<fe98f4f1de58dd89e87500b96c29f1ed@rentemester.local>",
                "738e8747c3bbdf70299183abdc11e6dca04d8e6002fca8f91843a21f9bd07314",
                937,
            ),
        ] {
            let m = message(kind);
            assert_eq!(m.message_id, id);
            assert_eq!(m.body_sha256, hash);
            assert_eq!(m.raw.len(), len);
            assert_eq!(m.raw, message(kind).raw);
            assert!(!String::from_utf8_lossy(&m.raw).contains("\r\nDate:"));
        }
        let a = message("invoice");
        let mut changed = sender();
        changed.from_name = Some("Different name".into());
        let b = build(
            &changed,
            "customer@example.invalid",
            "invoice",
            "2026-0001",
            b"%PDF-1.4\nsynthetic immutable\n",
            "invoice-2026-0001.pdf",
        )
        .unwrap();
        let mut punctuation = sender();
        punctuation.from_name = Some("Synthetic, Inc".into());
        assert!(from(&punctuation).starts_with("\"Synthetic, Inc\" <"));
        assert_eq!(a.message_id, b.message_id);
        assert_ne!(a.body_sha256, b.body_sha256); // core must refuse this collision
    }
    #[test]
    fn injection_bounds_and_private_config_refuse_before_network() {
        for to in [
            "a@b.invalid\r\nBcc:other@example.invalid",
            "a b@example.invalid",
            "a@example.invalid,other@example.invalid",
            "ü@example.invalid",
        ] {
            assert!(
                build(
                    &sender(),
                    to,
                    "invoice",
                    "2026-0001",
                    b"%PDF-1.4",
                    "invoice.pdf"
                )
                .is_err()
            );
        }
        for name in ["A\nB", "A\0B", "A <other@example.invalid>"] {
            let mut s = sender();
            s.from_name = Some(name.into());
            assert!(
                build(
                    &s,
                    "to@example.invalid",
                    "invoice",
                    "2026-0001",
                    b"%PDF-1.4",
                    "invoice.pdf"
                )
                .is_err()
            );
        }
        for (number, file) in [
            ("2026\r\nBcc:x", "invoice.pdf"),
            ("2026-0001", "../invoice.pdf"),
            ("2026-0001", "invoice\".pdf"),
        ] {
            assert!(
                build(
                    &sender(),
                    "to@example.invalid",
                    "invoice",
                    number,
                    b"%PDF-1.4",
                    file
                )
                .is_err()
            );
        }
        assert!(
            build(
                &sender(),
                "to@example.invalid",
                "invoice",
                "2026-0001",
                &vec![b'x'; MAX_PDF_BYTES + 1],
                "invoice.pdf"
            )
            .is_err()
        );
        for value in [
            serde_json::json!({"host":"localhost","port":465,"fromAddress":"a@example.invalid","tls":"none"}),
            serde_json::json!({"host":"localhost","port":465,"fromAddress":"a@example.invalid","key":"secret"}),
        ] {
            assert!(serde_json::from_value::<Config>(value).is_err());
        }
        for value in [
            serde_json::json!({"host":"localhost","port":0,"fromAddress":"a@example.invalid"}),
            serde_json::json!({"host":"localhost","port":465,"fromAddress":"a@example.invalid","username":"user"}),
            serde_json::json!({"host":"localhost","port":465,"fromAddress":"a@example.invalid","trustedCaFile":"relative.pem"}),
        ] {
            let c: Config = serde_json::from_value(value).unwrap();
            assert!(SmtpSender::new(&c).is_err());
        }
        assert!(
            !serde_json::to_string(&sender())
                .unwrap()
                .contains("password")
        );
    }
    #[test]
    fn blocking_owner_context_and_unclassified_io_never_claim_acceptance() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        // Mirrors the server's spawn_blocking call: a Tokio handle may exist on its blocking thread.
        let outer = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        outer.block_on(async move {tokio::task::spawn_blocking(move||{
            let c:Config=serde_json::from_value(serde_json::json!({"host":"127.0.0.1","port":port,"fromAddress":"billing@example.invalid","fromName":"Synthetic Å"})).unwrap();
            let mut s=SmtpSender::new(&c).unwrap();assert_eq!(s.send(&message("invoice")),Handoff::Unknown);
        }).await.unwrap();});
    }
    #[test]
    #[ignore = "explicit synthetic loopback TLS SMTP harness only; root actual qualification supplies config/expected state"]
    fn synthetic_tls_smtp_probe() {
        let text = zeroize::Zeroizing::new(
            std::fs::read_to_string(std::env::var("RM_SYNTHETIC_SMTP_CONFIG").unwrap()).unwrap(),
        );
        let c: Config = serde_json::from_str(&text).unwrap();
        assert!(matches!(c.host.as_str(), "127.0.0.1" | "localhost"));
        assert!(c.trusted_ca_file.is_some());
        let mut smtp = SmtpSender::new(&c).unwrap();
        let m = build(
            smtp.identity(),
            "synthetic@example.invalid",
            "invoice",
            "2026-0001",
            b"%PDF-1.4\nsynthetic\n",
            "invoice-2026-0001.pdf",
        )
        .unwrap();
        let expected = match std::env::var("RM_SYNTHETIC_SMTP_EXPECT").unwrap().as_str() {
            "accepted" => Handoff::Accepted,
            "refused" => Handoff::Refused,
            "unknown" => Handoff::Unknown,
            _ => panic!("explicit expected handoff required"),
        };
        let start = std::time::Instant::now();
        assert_eq!(smtp.send(&m), expected);
        assert!(start.elapsed() < Duration::from_secs(10));
    }
}
