//! One operator-selected delivery adapter for identity mail. Local files are loopback-only.
use rentemester_runtime::{
    auth_mail::{GatewayConfig, HttpJsonAuthSender, MailKind},
    recovery::{
        DeliveryUnconfirmed, ResetMessage, ResetSender, VerificationMessage, VerificationSender,
    },
};
use serde::Deserialize;
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use zeroize::Zeroize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct Config {
    endpoint: String,
    bearer_token: String,
    from: String,
    idempotency_secret: String,
    trusted_ca_file: Option<PathBuf>,
}
impl Drop for Config {
    fn drop(&mut self) {
        self.bearer_token.zeroize();
        self.idempotency_secret.zeroize();
    }
}
pub(crate) enum Delivery {
    Local(PathBuf),
    Gateway(HttpJsonAuthSender),
}
impl Delivery {
    pub(crate) fn new(
        config: Option<&Config>,
        local: bool,
        spool: &Path,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        if let Some(c) = config {
            let config = GatewayConfig {
                endpoint: &c.endpoint,
                bearer_token: &c.bearer_token,
                from: &c.from,
                idempotency_secret: &c.idempotency_secret,
            };
            let sender = if let Some(path) = &c.trusted_ca_file {
                if !path.is_absolute() {
                    return Err("authMail.trustedCaFile requires an absolute path".into());
                }
                // Bounded PEM trust input; does not disable hostname or certificate checks.
                use std::io::Read;
                let mut pem = Vec::new();
                std::fs::File::open(path)?
                    .take(65537)
                    .read_to_end(&mut pem)?;
                HttpJsonAuthSender::with_trusted_ca(config, &pem)?
            } else {
                HttpJsonAuthSender::new(config)?
            };
            Ok(Self::Gateway(sender))
        } else if local {
            Ok(Self::Local(spool.to_owned()))
        } else {
            Err("HTTPS server requires authMail delivery configuration".into())
        }
    }
    fn deliver(
        &mut self,
        kind: MailKind,
        recipient: &str,
        token: &str,
        url: &str,
    ) -> Result<(), DeliveryUnconfirmed> {
        match self {
            Self::Gateway(sender) => sender.deliver(kind, recipient, token, url),
            Self::Local(dir) => {
                let prefix = match kind {
                    MailKind::Verification => "verification",
                    MailKind::PasswordReset => "password-reset",
                    MailKind::WorkspaceInvitation => "invitation",
                };
                let id = rentemester_runtime::auth::new_token().map_err(|_| DeliveryUnconfirmed)?;
                let mut options = std::fs::OpenOptions::new();
                options.create_new(true).write(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut file = options
                    .open(dir.join(format!("{prefix}-{}.txt", id.as_str())))
                    .map_err(|_| DeliveryUnconfirmed)?;
                file.write_all(url.as_bytes())
                    .and_then(|_| file.sync_all())
                    .map_err(|_| DeliveryUnconfirmed)
            }
        }
    }
    pub(crate) fn invitation(
        &mut self,
        origin: &str,
        recipient: &str,
        token: &str,
    ) -> Result<(), ()> {
        self.deliver(
            MailKind::WorkspaceInvitation,
            recipient,
            token,
            &format!("{origin}/invite#token={token}"),
        )
        .map_err(|_| ())
    }
}
impl VerificationSender for Delivery {
    fn enabled(&self) -> bool {
        true
    }
    fn send(&mut self, m: &VerificationMessage) -> Result<(), DeliveryUnconfirmed> {
        self.deliver(
            MailKind::Verification,
            m.recipient(),
            m.token(),
            m.action_url(),
        )
    }
}
impl ResetSender for Delivery {
    fn enabled(&self) -> bool {
        true
    }
    fn send(&mut self, m: &ResetMessage<'_>) -> Result<(), DeliveryUnconfirmed> {
        self.deliver(
            MailKind::PasswordReset,
            m.recipient(),
            m.token(),
            m.action_url(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_delivery_requires_explicit_valid_https_gateway_before_workspace_mutation() {
        let unused = Path::new("/does-not-exist/rentemester-synthetic");
        assert!(Delivery::new(None, false, unused).is_err());
        assert!(matches!(
            Delivery::new(None, true, unused).unwrap(),
            Delivery::Local(_)
        ));
        let config:Config=serde_json::from_value(serde_json::json!({"endpoint":"http://example.invalid/send","bearerToken":"synthetic","from":"auth@example.invalid","idempotencySecret":"synthetic"})).unwrap();
        assert!(Delivery::new(Some(&config), false, unused).is_err());
        assert!(Delivery::new(Some(&config), true, unused).is_err());
        assert!(!unused.exists());
    }
}
