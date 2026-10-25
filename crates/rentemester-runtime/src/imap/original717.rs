//! Ephemeral original MCP connection settings. Resolution never saves or replaces
//! the configured-native mailbox grant. Callers must admit authority before I/O.
use super::{Config, Failure};
use serde::Deserialize;
use zeroize::Zeroizing;

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionInput {
    pub imap_host: Option<String>,
    pub imap_port: Option<u16>,
    pub imap_username: Option<String>,
    pub imap_mailbox: Option<String>,
    pub since_uid: Option<u32>,
}
/// Environment is injected for deterministic tests; no process-global mutation.
/// The password is never an original tool argument and is never serializable.
pub fn resolve(
    input: &ConnectionInput,
    env: impl Fn(&str) -> Option<String>,
) -> Result<Config, Vec<String>> {
    let host = input
        .imap_host
        .clone()
        .or_else(|| env("RENTEMESTER_IMAP_HOST"));
    let username = input
        .imap_username
        .clone()
        .or_else(|| env("RENTEMESTER_IMAP_USERNAME"));
    let password = Zeroizing::new(env("RENTEMESTER_IMAP_PASSWORD").unwrap_or_default());
    let secure = !matches!(env("RENTEMESTER_IMAP_TLS").as_deref(), Some("false" | "0"));
    let port = input
        .imap_port
        .map(u32::from)
        .or_else(|| env("RENTEMESTER_IMAP_PORT").map(|s| s.parse::<u32>().unwrap_or(0)))
        .unwrap_or(if secure { 993 } else { 143 });
    let mailbox = input
        .imap_mailbox
        .clone()
        .or_else(|| env("RENTEMESTER_IMAP_MAILBOX"))
        .unwrap_or_else(|| "INBOX".into());
    let mut errors = Vec::new();
    if host.as_deref().is_none_or(str::is_empty) {
        errors.push("IMAP host missing (--imap-host or RENTEMESTER_IMAP_HOST)".into());
    }
    if username.as_deref().is_none_or(str::is_empty) {
        errors.push("IMAP username missing (--imap-username or RENTEMESTER_IMAP_USERNAME)".into());
    }
    if password.is_empty() {
        errors.push("IMAP password missing (RENTEMESTER_IMAP_PASSWORD)".into());
    }
    if port == 0 || port > u16::MAX as u32 {
        errors.push("IMAP port must be a positive integer".into());
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    // Reuse native credential/header bounds and zeroizing configuration ownership.
    let bytes = Zeroizing::new(
        serde_json::to_vec(&serde_json::json!({
            "host":host,"port":port,"username":username,"password":password.as_str(),
            "secure":secure,"mailbox":mailbox
        }))
        .map_err(|_| vec!["IMAP configuration refused".into()])?,
    );
    Config::parse(&bytes).map_err(|e| {
        vec![match e {
            Failure::Configuration => "IMAP configuration refused".into(),
            _ => "IMAP configuration unavailable".into(),
        }]
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original717_imap_intake_poll_ephemeral_defaults_overrides_and_refusals() {
        let env = |key: &str| match key {
            "RENTEMESTER_IMAP_HOST" => Some("mail.example.invalid".into()),
            "RENTEMESTER_IMAP_USERNAME" => Some("synthetic-user".into()),
            "RENTEMESTER_IMAP_PASSWORD" => Some("synthetic-secret".into()),
            _ => None,
        };
        let defaults = resolve(&ConnectionInput::default(), env).ok().unwrap();
        assert_eq!(defaults.status().port, 993);
        assert_eq!(defaults.status().mailbox, "INBOX");
        let input = ConnectionInput {
            imap_host: Some("other.example.invalid".into()),
            imap_mailbox: Some("Receipts".into()),
            since_uid: Some(0),
            ..Default::default()
        };
        let overridden = resolve(&input, env).ok().unwrap();
        assert_eq!(overridden.status().host, "other.example.invalid");
        assert_eq!(overridden.status().mailbox, "Receipts");
        assert_eq!(defaults.status().mailbox, "INBOX");
        assert!(
            !serde_json::to_string(&overridden.status())
                .unwrap()
                .contains("secret")
        );
        assert_eq!(
            resolve(&ConnectionInput::default(), |_| None)
                .err()
                .unwrap()
                .len(),
            3
        );
        assert!(
            serde_json::from_value::<ConnectionInput>(serde_json::json!({"password":"forged"}))
                .is_err()
        );
        assert!(
            resolve(
                &ConnectionInput {
                    imap_port: Some(0),
                    ..Default::default()
                },
                env
            )
            .is_err()
        );
    }
}

pub fn decode_eml(text: &str) -> Result<Vec<u8>, Failure> {
    use base64::Engine;
    if text.len() > 36 * 1024 * 1024 {
        return Err(Failure::Configuration);
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text)
        .map_err(|_| Failure::Configuration)?;
    if bytes.len() > super::MAX_MESSAGE {
        return Err(Failure::Configuration);
    }
    Ok(bytes)
}
pub fn encode_eml(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
