//! Trusted recovery URL policy and sensitive mail boundary; no HTTP client or logger.
use crate::auth::{AuthError, CookieProfile};
use url::{Origin, Url};
use zeroize::Zeroizing;
pub struct RecoveryLinks {
    base: Url,
    origins: Vec<Origin>,
}
pub struct SensitiveUrl(Zeroizing<String>);
impl SensitiveUrl {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
pub struct ResetMessage<'a> {
    recipient: &'a str,
    token: &'a str,
    url: SensitiveUrl,
}
impl ResetMessage<'_> {
    pub fn recipient(&self) -> &str {
        self.recipient
    }
    pub fn token(&self) -> &str {
        self.token
    }
    pub fn action_url(&self) -> &str {
        self.url.as_str()
    }
}
#[derive(Debug, Clone, Copy)]
pub struct DeliveryUnconfirmed;
/// Trusted bounded synchronous external adapter. A failure may still mean mail was sent.
/// The request use case never holds a SQLite transaction while calling this port.
pub trait ResetSender {
    fn enabled(&self) -> bool;
    fn send(&mut self, message: &ResetMessage<'_>) -> Result<(), DeliveryUnconfirmed>;
}
pub struct DisabledResetSender;
impl ResetSender for DisabledResetSender {
    fn enabled(&self) -> bool {
        false
    }
    fn send(&mut self, _: &ResetMessage<'_>) -> Result<(), DeliveryUnconfirmed> {
        Err(DeliveryUnconfirmed)
    }
}
fn clean(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 4096
        && s.trim() == s
        && !s.chars().any(char::is_control)
        && !s.contains('\\')
}
fn origin(s: &str, profile: CookieProfile) -> Result<Url, AuthError> {
    if !clean(s) {
        return Err(AuthError::InvalidConfig);
    }
    let u = Url::parse(s).map_err(|_| AuthError::InvalidConfig)?;
    if !u.has_host()
        || u.host_str().is_some_and(|h| h.contains('*'))
        || !u.username().is_empty()
        || u.password().is_some()
        || u.path() != "/"
        || u.query().is_some()
        || u.fragment().is_some()
        || !(u.scheme() == "https"
            || profile == CookieProfile::Development
                && u.scheme() == "http"
                && matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))
    {
        return Err(AuthError::InvalidConfig);
    }
    Ok(u)
}
impl RecoveryLinks {
    pub fn new(
        base_origin: &str,
        trusted_origins: &[&str],
        profile: CookieProfile,
    ) -> Result<Self, AuthError> {
        if trusted_origins.is_empty() || trusted_origins.len() > 32 {
            return Err(AuthError::InvalidConfig);
        }
        let base = origin(base_origin, profile)?;
        let origins: Vec<Origin> = trusted_origins
            .iter()
            .map(|s| origin(s, profile).map(|u| u.origin()))
            .collect::<Result<_, _>>()?;
        if !origins.contains(&base.origin()) {
            return Err(AuthError::InvalidConfig);
        }
        Ok(Self { base, origins })
    }
    fn destination(&self, value: &str) -> Result<Url, AuthError> {
        if !clean(value) {
            return Err(AuthError::InvalidCredentials);
        }
        let u = if let Some(path) = value.strip_prefix('/') {
            let lower = path.to_ascii_lowercase();
            let (path, query) = path
                .split_once('?')
                .map_or((path, None), |(p, q)| (p, Some(q)));
            let allowed = |b: u8| b.is_ascii_alphanumeric() || b"_-.+/@".contains(&b);
            if lower.starts_with('/')
                || lower.starts_with("%2f")
                || lower.starts_with("%5c")
                || !path.bytes().all(allowed)
                || query.is_some_and(|q| !q.bytes().all(|b| allowed(b) || b"=&%".contains(&b)))
            {
                return Err(AuthError::InvalidCredentials);
            }
            self.base
                .join(value)
                .map_err(|_| AuthError::InvalidCredentials)?
        } else {
            Url::parse(value).map_err(|_| AuthError::InvalidCredentials)?
        };
        if !["https", "http"].contains(&u.scheme())
            || !u.username().is_empty()
            || u.password().is_some()
            || !self.origins.contains(&u.origin())
        {
            return Err(AuthError::InvalidCredentials);
        }
        Ok(u)
    }
    pub fn validate_redirect(&self, value: Option<&str>) -> Result<(), AuthError> {
        match value {
            None | Some("") => Ok(()),
            Some(v) => self.destination(v).map(|_| ()),
        }
    }
    pub fn message<'a>(
        &self,
        recipient: &'a str,
        token: &'a str,
        redirect: Option<&str>,
    ) -> Result<ResetMessage<'a>, AuthError> {
        self.validate_redirect(redirect)?;
        if token.len() != 24 || !token.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(AuthError::InvalidCredentials);
        }
        const COMPONENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
            .remove(b'-')
            .remove(b'_')
            .remove(b'.')
            .remove(b'!')
            .remove(b'~')
            .remove(b'*')
            .remove(b'\'')
            .remove(b'(')
            .remove(b')');
        let encoded = percent_encoding::utf8_percent_encode(redirect.unwrap_or(""), COMPONENT);
        let url = SensitiveUrl(Zeroizing::new(format!(
            "{}api/auth/reset-password/{token}?callbackURL={encoded}",
            self.base
        )));
        Ok(ResetMessage {
            recipient,
            token,
            url,
        })
    }
    /// Current-row validity is decided by storage; this function never authorizes recovery.
    pub fn callback(
        &self,
        token: &str,
        redirect: Option<&str>,
        valid: bool,
    ) -> Result<SensitiveUrl, AuthError> {
        self.validate_redirect(redirect)?;
        let mut target = if let Some(v) = redirect.filter(|s| !s.is_empty()) {
            self.destination(v)?
        } else {
            self.base
                .join("/api/auth/error")
                .map_err(|_| AuthError::InvalidConfig)?
        };
        let good = valid && redirect.is_some_and(|s| !s.is_empty());
        let name = if good { "token" } else { "error" };
        let value = if good { token } else { "INVALID_TOKEN" };
        let mut found = false;
        let mut pairs: Vec<(String, String)> = Vec::new();
        for (k, v) in target.query_pairs() {
            if k == name {
                if !found {
                    pairs.push((name.to_owned(), value.to_owned()));
                    found = true;
                }
            } else {
                pairs.push((k.into_owned(), v.into_owned()));
            }
        }
        if !found {
            pairs.push((name.to_owned(), value.to_owned()));
        }
        target.set_query(None);
        {
            let mut query = target.query_pairs_mut();
            for (k, v) in pairs {
                query.append_pair(&k, &v);
            }
        }
        Ok(SensitiveUrl(Zeroizing::new(target.into())))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trusted_origins_and_sensitive_links_use_standard_url_boundaries() {
        let links = RecoveryLinks::new(
            "https://EXAMPLE.invalid:443",
            &["https://example.invalid", "https://other.invalid"],
            CookieProfile::Secure,
        )
        .unwrap();
        for bad in [
            "https://example.invalid.evil/reset",
            "https://example.invalid@evil.invalid/reset",
            "https://user@example.invalid/reset",
            "//evil.invalid/reset",
            "/%2Fattack",
            "/%5Cattack",
            "/reset#x",
            "javascript:alert(1)",
            "https://example.invalid/\\evil",
            " /reset",
            "/reset\n",
        ] {
            assert!(links.validate_redirect(Some(bad)).is_err(), "{bad}");
        }
        for good in [
            "/reset",
            "/reset?x=a+b&y=%20",
            "https://other.invalid/reset?q=a b#fragment",
        ] {
            assert!(links.validate_redirect(Some(good)).is_ok());
        }
        let token = "AbCdEfGhIjKlMnOpQrStUv01";
        assert_eq!(
            links
                .message(
                    "u@example.invalid",
                    token,
                    Some("https://other.invalid/reset?q=a b#fragment")
                )
                .unwrap()
                .action_url(),
            format!(
                "https://example.invalid/api/auth/reset-password/{token}?callbackURL=https%3A%2F%2Fother.invalid%2Freset%3Fq%3Da%20b%23fragment"
            )
        );
        assert_eq!(
            links
                .callback(
                    token,
                    Some("https://other.invalid/reset?token=old&x=a%20b&token=old2#fragment"),
                    true
                )
                .unwrap()
                .as_str(),
            format!("https://other.invalid/reset?token={token}&x=a+b#fragment")
        );
        assert_eq!(
            links.callback(token, None, true).unwrap().as_str(),
            "https://example.invalid/api/auth/error?error=INVALID_TOKEN"
        );
    }
    #[test]
    fn configuration_is_explicit_and_http_is_loopback_development_only() {
        for base in [
            "https://example.invalid/path",
            "https://user@example.invalid",
            "https://example.invalid?x",
            "https://example.invalid#x",
            "http://example.invalid",
            "file:///tmp/x",
            "https://*.example.invalid",
        ] {
            assert!(RecoveryLinks::new(base, &[base], CookieProfile::Secure).is_err());
        }
        assert!(RecoveryLinks::new("https://example.invalid", &[], CookieProfile::Secure).is_err());
        assert!(
            RecoveryLinks::new(
                "https://example.invalid",
                &["https://other.invalid"],
                CookieProfile::Secure
            )
            .is_err()
        );
        for base in [
            "http://localhost:3000",
            "http://127.0.0.1:3000",
            "http://[::1]:3000",
        ] {
            assert!(RecoveryLinks::new(base, &[base], CookieProfile::Development).is_ok());
        }
        assert!(
            RecoveryLinks::new(
                "http://example.invalid",
                &["http://example.invalid"],
                CookieProfile::Development
            )
            .is_err()
        );
    }
    #[test]
    fn recovery_bearers_have_source_grammar_and_no_observed_duplicate() {
        let mut values = std::collections::HashSet::new();
        for _ in 0..256 {
            let token = crate::auth::new_reset_token().unwrap();
            assert_eq!(token.as_str().len(), 24);
            assert!(token.as_str().bytes().all(|b| b.is_ascii_alphanumeric()));
            assert!(values.insert(token.as_str().to_owned()));
        }
    }
}
