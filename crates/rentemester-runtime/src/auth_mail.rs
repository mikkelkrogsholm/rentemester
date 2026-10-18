//! Bounded HTTPS auth-mail gateway. No logger, database, redirect or provider-body parser.
use crate::{
    auth::AuthError,
    recovery::{DeliveryUnconfirmed, ResetMessage, ResetSender},
};
use hickory_resolver::{TokioResolver, config::LookupIpStrategy};
use hmac::{Hmac, Mac};
use reqwest::{
    blocking::{Client, ClientBuilder},
    header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue},
};
use serde::Serialize;
use sha2::Sha256;
use std::time::{Duration, Instant};
use std::{net::SocketAddr, sync::Arc};
use url::Url;
use zeroize::Zeroizing;
const BUDGET: Duration = Duration::from_secs(8);
#[derive(Clone, Copy)]
pub enum MailKind {
    Verification,
    PasswordReset,
    WorkspaceInvitation,
}
impl MailKind {
    fn content(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Verification => (
                "verification",
                "Bekræft din e-mailadresse i Rentemester",
                "Bekræft din e-mailadresse ved at åbne linket.",
            ),
            Self::PasswordReset => (
                "password-reset",
                "Nulstil din adgangskode til Rentemester",
                "Nulstil din adgangskode ved at åbne linket.",
            ),
            Self::WorkspaceInvitation => (
                "workspace-invitation",
                "Du er inviteret til Rentemester",
                "Åbn invitationen for at oprette eller forbinde din bruger.",
            ),
        }
    }
}
/// Injected operator configuration, deliberately not Debug/Clone/Serde.
pub struct GatewayConfig<'a> {
    pub endpoint: &'a str,
    pub bearer_token: &'a str,
    pub from: &'a str,
    pub idempotency_secret: &'a str,
}
pub struct HttpJsonAuthSender {
    endpoint: Url,
    authorization: HeaderValue,
    from: String,
    key: Zeroizing<String>,
    client: Client,
}
fn bounded(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}
fn email(s: &str) -> bool {
    crate::recovery::mail_recipient(s)
}
struct GatewayDns(Option<TokioResolver>);
impl reqwest::dns::Resolve for GatewayDns {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let resolver = self.0.clone();
        Box::pin(async move {
            let Some(resolver) = resolver else {
                return Err(std::io::Error::other("gateway DNS unavailable").into());
            };
            let lookup = resolver
                .lookup_ip(name.as_str())
                .await
                .map_err(|_| std::io::Error::other("gateway DNS unavailable"))?;
            let addresses: Vec<SocketAddr> =
                lookup.iter().map(|ip| SocketAddr::new(ip, 0)).collect();
            Ok(Box::new(addresses.into_iter()) as reqwest::dns::Addrs)
        })
    }
}
fn client_builder() -> ClientBuilder {
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
        .timeout(BUDGET)
        .connect_timeout(BUDGET)
}
impl HttpJsonAuthSender {
    pub fn new(config: GatewayConfig<'_>) -> Result<Self, AuthError> {
        let roots = webpki_root_certs::TLS_SERVER_ROOT_CERTS
            .iter()
            .map(|c| {
                reqwest::Certificate::from_der(c.as_ref()).map_err(|_| AuthError::InvalidConfig)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::build(config, client_builder().tls_certs_only(roots))
    }
    fn build(config: GatewayConfig<'_>, builder: ClientBuilder) -> Result<Self, AuthError> {
        if !bounded(config.endpoint, 4096)
            || config.endpoint.trim() != config.endpoint
            || config.endpoint.contains('\\')
            || !bounded(config.bearer_token, 4096)
            || config.bearer_token.trim() != config.bearer_token
            || !config.bearer_token.is_ascii()
            || config.from.len() > 4096
            || !email(config.from.trim())
            || !bounded(config.idempotency_secret, 4096)
        {
            return Err(AuthError::InvalidConfig);
        }
        let endpoint = Url::parse(config.endpoint).map_err(|_| AuthError::InvalidConfig)?;
        if endpoint.scheme() != "https"
            || !endpoint.has_host()
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.fragment().is_some()
            || endpoint.query().is_some()
            || endpoint.host_str().is_some_and(|h| h.contains('*'))
        {
            return Err(AuthError::InvalidConfig);
        }
        let bearer = Zeroizing::new(format!("Bearer {}", config.bearer_token));
        let mut authorization =
            HeaderValue::from_str(&bearer).map_err(|_| AuthError::InvalidConfig)?;
        authorization.set_sensitive(true);
        // Load system resolver configuration/hosts once at construction, outside send's
        // deadline. Never lazily fall back to a public resolver or blocking getaddrinfo.
        let dns = if matches!(endpoint.host(), Some(url::Host::Domain(_))) {
            let mut resolver =
                TokioResolver::builder_tokio().map_err(|_| AuthError::InvalidConfig)?;
            resolver.options_mut().ip_strategy = LookupIpStrategy::Ipv4AndIpv6;
            Some(resolver.build().map_err(|_| AuthError::InvalidConfig)?)
        } else {
            None
        };
        Ok(Self {
            endpoint,
            authorization,
            from: config.from.trim().to_owned(),
            key: Zeroizing::new(config.idempotency_secret.to_owned()),
            client: builder
                .dns_resolver(Arc::new(GatewayDns(dns)))
                .build()
                .map_err(|_| AuthError::InvalidConfig)?,
        })
    }
    /// Gateway 2xx acknowledges handoff only. A timeout/error may follow actual delivery;
    /// callers must not manufacture a new token or assert that nothing was delivered.
    pub fn deliver(
        &mut self,
        kind: MailKind,
        recipient: &str,
        token: &str,
        action_url: &str,
    ) -> Result<(), DeliveryUnconfirmed> {
        let start = Instant::now();
        if !email(recipient) || !bounded(token, 8192) || !bounded(action_url, 16384) {
            return Err(DeliveryUnconfirmed);
        }
        let url = Url::parse(action_url).map_err(|_| DeliveryUnconfirmed)?;
        if !["https", "http"].contains(&url.scheme())
            || !url.has_host()
            || !url.username().is_empty()
            || url.password().is_some()
            || action_url.contains('\\')
        {
            return Err(DeliveryUnconfirmed);
        }
        let (kind, subject, text) = kind.content();
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire<'a> {
            version: &'static str,
            kind: &'static str,
            to: &'a str,
            from: &'a str,
            subject: &'static str,
            text: &'static str,
            action_url: &'a str,
        }
        let body = Zeroizing::new(
            serde_json::to_vec(&Wire {
                version: "rentemester-auth-email-v1",
                kind,
                to: recipient,
                from: &self.from,
                subject,
                text,
                action_url,
            })
            .map_err(|_| DeliveryUnconfirmed)?,
        );
        if body.len() > 32768 {
            return Err(DeliveryUnconfirmed);
        }
        let mut mac =
            Hmac::<Sha256>::new_from_slice(self.key.as_bytes()).map_err(|_| DeliveryUnconfirmed)?;
        mac.update(b"rentemester-auth-email-v1\0");
        mac.update(kind.as_bytes());
        mac.update(&[0]);
        mac.update(token.as_bytes());
        let idempotency: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        for attempt in 0..3 {
            let remaining = BUDGET
                .checked_sub(start.elapsed())
                .filter(|d| !d.is_zero())
                .ok_or(DeliveryUnconfirmed)?;
            let result = self
                .client
                .post(self.endpoint.clone())
                .header(AUTHORIZATION, self.authorization.clone())
                .header(CONTENT_TYPE, "application/json")
                .header("idempotency-key", &idempotency)
                .header("x-rentemester-auth-event", kind)
                .timeout(remaining)
                .body(body.to_vec())
                .send();
            let retry = match result {
                Ok(response) => {
                    let status = response.status().as_u16();
                    if (200..300).contains(&status) {
                        return if start.elapsed() < BUDGET {
                            Ok(())
                        } else {
                            Err(DeliveryUnconfirmed)
                        };
                    }
                    (status == 429) || (500..600).contains(&status)
                }
                Err(_) => true,
            };
            if !retry || attempt == 2 {
                return Err(DeliveryUnconfirmed);
            }
            let remaining = BUDGET
                .checked_sub(start.elapsed())
                .filter(|d| !d.is_zero())
                .ok_or(DeliveryUnconfirmed)?;
            std::thread::sleep(Duration::from_millis(100 << attempt).min(remaining));
        }
        Err(DeliveryUnconfirmed)
    }
}
impl ResetSender for HttpJsonAuthSender {
    fn enabled(&self) -> bool {
        true
    }
    fn send(&mut self, message: &ResetMessage<'_>) -> Result<(), DeliveryUnconfirmed> {
        self.deliver(
            MailKind::PasswordReset,
            message.recipient(),
            message.token(),
            message.action_url(),
        )
    }
}
impl crate::recovery::VerificationSender for HttpJsonAuthSender {
    fn enabled(&self) -> bool {
        true
    }
    fn send(
        &mut self,
        message: &crate::recovery::VerificationMessage,
    ) -> Result<(), DeliveryUnconfirmed> {
        self.deliver(
            MailKind::Verification,
            message.recipient(),
            message.token(),
            message.action_url(),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    #[test]
    fn gateway_input_refuses_credential_and_url_ambiguity() {
        let base = |endpoint, bearer_token, from, idempotency_secret| GatewayConfig {
            endpoint,
            bearer_token,
            from,
            idempotency_secret,
        };
        for u in [
            "http://gateway.invalid/send",
            "https://user@gateway.invalid/send",
            "https://gateway.invalid/send?token=private",
            "https://gateway.invalid/send#x",
            "https://*.invalid/send",
            "https://gateway.invalid/\\x",
        ] {
            assert!(
                HttpJsonAuthSender::new(base(
                    u,
                    "synthetic-key",
                    "auth@example.invalid",
                    "synthetic-idempotency"
                ))
                .is_err()
            );
        }
        for (b, f, k) in [
            ("", "auth@example.invalid", "key"),
            ("x\r\nprivate", "auth@example.invalid", "key"),
            ("key", "not-email", "key"),
            ("key", "auth@example.invalid", ""),
        ] {
            assert!(
                HttpJsonAuthSender::new(base("https://gateway.invalid/send", b, f, k)).is_err()
            );
        }
    }
    #[test]
    #[ignore = "synthetic TLS gateway process probe selected by independent harness"]
    fn probe() {
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        let r: Value = serde_json::from_str(&input).unwrap();
        let config = GatewayConfig {
            endpoint: r["endpoint"].as_str().unwrap_or(""),
            bearer_token: r["bearer"].as_str().unwrap_or(""),
            from: r["from"].as_str().unwrap_or(""),
            idempotency_secret: r["key"].as_str().unwrap_or(""),
        };
        let built = if let Some(pem) = r["rootPem"].as_str() {
            let root = reqwest::Certificate::from_pem(pem.as_bytes()).unwrap();
            HttpJsonAuthSender::build(config, client_builder().tls_certs_only([root]))
        } else {
            HttpJsonAuthSender::new(config)
        };
        let start = Instant::now();
        let output = match built {
            Err(e) => json!({"ok":false,"code":e.code()}),
            Ok(mut sender) => {
                if let Some(port) = r["dnsPort"].as_u64() {
                    use hickory_resolver::{
                        config::{NameServerConfig, ResolveHosts, ResolverConfig},
                        net::runtime::TokioRuntimeProvider,
                    };
                    let mut config = ResolverConfig::default();
                    let mut server = NameServerConfig::udp("127.0.0.1".parse().unwrap());
                    server.connections[0].port = port as u16;
                    config.name_servers.clear();
                    config.name_servers.push(server);
                    let mut dns =
                        TokioResolver::builder_with_config(config, TokioRuntimeProvider::default());
                    dns.options_mut().ip_strategy = LookupIpStrategy::Ipv4Only;
                    dns.options_mut().use_hosts_file = ResolveHosts::Never;
                    let root =
                        reqwest::Certificate::from_pem(r["rootPem"].as_str().unwrap().as_bytes())
                            .unwrap();
                    sender.client = client_builder()
                        .tls_certs_only([root])
                        .dns_resolver(Arc::new(GatewayDns(Some(dns.build().unwrap()))))
                        .build()
                        .unwrap();
                }
                let kind = match r["kind"].as_str().unwrap_or("password-reset") {
                    "verification" => MailKind::Verification,
                    "workspace-invitation" => MailKind::WorkspaceInvitation,
                    _ => MailKind::PasswordReset,
                };
                let result = if r["asVerificationSender"] == true {
                    let links = crate::recovery::RecoveryLinks::new(
                        "https://example.invalid",
                        &["https://example.invalid"],
                        crate::auth::CookieProfile::Secure,
                    )
                    .unwrap();
                    let message = links
                        .verification_message(
                            r["recipient"].as_str().unwrap(),
                            r["token"].as_str().unwrap(),
                            Some("/login"),
                        )
                        .unwrap();
                    crate::recovery::VerificationSender::send(&mut sender, &message)
                } else if r["asResetSender"] == true {
                    let links = crate::recovery::RecoveryLinks::new(
                        "https://example.invalid",
                        &["https://example.invalid"],
                        crate::auth::CookieProfile::Secure,
                    )
                    .unwrap();
                    let message = links
                        .message(
                            r["recipient"].as_str().unwrap(),
                            r["token"].as_str().unwrap(),
                            Some("/reset"),
                        )
                        .unwrap();
                    sender.send(&message)
                } else {
                    sender.deliver(
                        kind,
                        r["recipient"].as_str().unwrap_or(""),
                        r["token"].as_str().unwrap_or(""),
                        r["actionUrl"].as_str().unwrap_or(""),
                    )
                };
                json!({"ok":result.is_ok(),"code":if result.is_ok(){Value::Null}else{json!("auth_delivery_unconfirmed")},"elapsedMs":start.elapsed().as_millis()})
            }
        };
        println!("RM_AUTH_MAIL_RESULT={output}");
    }
}
