//! Source-compatible built-in SMTP admission. This is deliberately not the native
//! real SMTP provider: no socket is constructed and no delivery is promised.
use super::{Handoff, MailError, Message, SenderIdentity, Transport, identity_valid};
use serde::Deserialize;
use zeroize::Zeroizing;

/// Private bytes must come from an admitted company config/smtp.json capability,
/// not from an HTTP request or an arbitrary filesystem path.
pub struct BuiltIn {
    identity: SenderIdentity,
    dry_run: bool,
    _username: Option<Zeroizing<String>>,
    _password: Option<Zeroizing<String>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Wire {
    host: String,
    port: serde_json::Value,
    from_address: String,
    from_name: Option<String>,
    username: Option<String>,
    password: Option<String>,
    dry_run: Option<bool>,
}
impl BuiltIn {
    pub fn from_admitted_config(bytes: &[u8]) -> Result<Self, MailError> {
        if bytes.is_empty() || bytes.len() > 16384 {
            return Err(MailError);
        }
        let w: Wire = serde_json::from_slice(bytes).map_err(|_| MailError)?;
        let port = match &w.port {
            serde_json::Value::String(s) => s.parse::<u16>().map_err(|_| MailError)?,
            _ => w
                .port
                .as_u64()
                .and_then(|n| u16::try_from(n).ok())
                .ok_or(MailError)?,
        };
        let identity = SenderIdentity {
            host: w.host.trim().into(),
            port,
            from_address: w.from_address.trim().into(),
            from_name: w.from_name,
        };
        if !identity_valid(&identity) {
            return Err(MailError);
        }
        Ok(Self {
            identity,
            dry_run: w.dry_run == Some(true),
            _username: w.username.map(Zeroizing::new),
            _password: w.password.map(Zeroizing::new),
        })
    }
    /// Exact source refusal, to be enclosed in the original core result.
    pub fn refusal(&self) -> Option<&'static str> {
        (!self.dry_run).then_some("the built-in SMTP transport runs in dryRun only; inject a live EmailTransport for real delivery")
    }
}
impl Transport for BuiltIn {
    fn identity(&self) -> &SenderIdentity {
        &self.identity
    }
    fn send(&mut self, message: &Message) -> Handoff {
        if self.dry_run
            && message.from == super::from(&self.identity)
            && super::email(&message.to)
            && !message.raw.is_empty()
            && message.raw.len() <= super::MAX_MIME_BYTES
            && super::digest(&message.raw) == message.body_sha256
        {
            // Acceptance of a dry-run receipt, not evidence of SMTP delivery.
            Handoff::Accepted
        } else {
            Handoff::Refused
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn config(dry: bool) -> BuiltIn {
        BuiltIn::from_admitted_config(
            &serde_json::to_vec(&serde_json::json!({
                "host":"smtp.example.invalid","port":"465","fromAddress":"billing@example.invalid",
                "dryRun":dry,"password":"synthetic-secret"
            }))
            .unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn original717_invoice_send_email_builtin_happy_duplicate_and_refusal() {
        let mut c = config(true);
        let m = super::super::build(
            c.identity(),
            "buyer@example.invalid",
            "invoice",
            "2026-1",
            b"%PDF-1.4\nsynthetic",
            "2026-1.pdf",
        )
        .unwrap();
        let again = super::super::build(
            c.identity(),
            "buyer@example.invalid",
            "invoice",
            "2026-1",
            b"%PDF-1.4\nsynthetic",
            "2026-1.pdf",
        )
        .unwrap();
        assert_eq!(m.message_id, again.message_id);
        assert_eq!(m.raw, again.raw);
        assert_eq!(c.send(&m), Handoff::Accepted);
        assert_eq!(c.refusal(), None);
        let mut live = config(false);
        assert_eq!(live.send(&m), Handoff::Refused);
        assert!(live.refusal().unwrap().contains("dryRun only"));
        let mut forged = again;
        forged.raw.push(0);
        assert_eq!(c.send(&forged), Handoff::Refused);
    }
}
