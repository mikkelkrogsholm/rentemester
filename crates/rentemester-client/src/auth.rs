//! Existing server authentication ceremony; no authentication policy lives here.
use crate::{ApiClient, Error};
use reqwest::{Method, StatusCode, header::SET_COOKIE};
use serde_json::Value;
use std::{io::Write, path::Path};
use zeroize::Zeroizing;

/// Secret-bearing states intentionally have no Debug/Serialize implementation.
pub struct PendingLogin {
    origin: String,
    cookie: Zeroizing<String>,
}
pub struct AuthenticatedSession {
    origin: String,
    cookie: Zeroizing<String>,
}
pub enum LoginProgress {
    MfaRequired(PendingLogin),
    EnrollmentRequired,
}
#[derive(Clone, Copy, Debug)]
pub enum MfaFactor {
    Totp,
    BackupCode,
}

impl ApiClient {
    /// Begin a human login. Existing client session cookies are deliberately omitted.
    /// Password is discarded after this one request; no retries on uncertain outcomes.
    pub async fn begin_login(&self, email: &str, password: String) -> Result<LoginProgress, Error> {
        let password = Zeroizing::new(password);
        if email.is_empty() || email.len() > 4096 || password.is_empty() || password.len() > 4096 {
            return Err(Error::InvalidInput(
                "email/password outside server input bounds",
            ));
        }
        let password_json = Zeroizing::new(
            serde_json::to_string(password.as_str())
                .map_err(|_| Error::InvalidInput("invalid password"))?,
        );
        let email_json =
            serde_json::to_string(email).map_err(|_| Error::InvalidInput("invalid email"))?;
        let body = Zeroizing::new(format!(
            "{{\"email\":{email_json},\"password\":{},\"rememberMe\":false}}",
            password_json.as_str()
        ));
        let response = self
            .send(
                Method::POST,
                "/api/auth/sign-in/email",
                &[],
                Some(body.as_bytes().to_vec()),
                None,
            )
            .await?;
        if response.status() == StatusCode::CONFLICT {
            let value = auth_body(response).await?;
            return if value["code"] == "MFA_ENROLLMENT_REQUIRED" {
                Ok(LoginProgress::EnrollmentRequired)
            } else {
                Err(Error::Refused { status: 409 })
            };
        }
        let response = auth_status(response)?;
        let cookie = issued_cookie(&response, &self.origin, "two_factor")?;
        let value = auth_body(response).await?;
        if value["twoFactorRedirect"] != true {
            return Err(Error::OutcomeUnknown);
        }
        Ok(LoginProgress::MfaRequired(PendingLogin {
            origin: self.origin.clone(),
            cookie,
        }))
    }
    /// Complete exactly the pending origin-bound challenge using TOTP or a backup code.
    /// A refused code leaves the pending state available for an explicit caller retry.
    pub async fn complete_login(
        &self,
        pending: &PendingLogin,
        factor: MfaFactor,
        code: String,
    ) -> Result<AuthenticatedSession, Error> {
        let code = Zeroizing::new(code);
        if pending.origin != self.origin {
            return Err(Error::InvalidInput(
                "login challenge belongs to another server origin",
            ));
        }
        if code.is_empty() || code.len() > 4096 {
            return Err(Error::InvalidInput(
                "factor code outside server input bounds",
            ));
        }
        let code_json = Zeroizing::new(
            serde_json::to_string(code.as_str())
                .map_err(|_| Error::InvalidInput("invalid factor code"))?,
        );
        let body = Zeroizing::new(format!(
            "{{\"code\":{},\"trustDevice\":false,\"disableSession\":false}}",
            code_json.as_str()
        ));
        let path = match factor {
            MfaFactor::Totp => "/api/auth/two-factor/verify-totp",
            MfaFactor::BackupCode => "/api/auth/two-factor/verify-backup-code",
        };
        let response = auth_status(
            self.send(
                Method::POST,
                path,
                &[],
                Some(body.as_bytes().to_vec()),
                Some(&pending.cookie),
            )
            .await?,
        )?;
        let cookie = issued_cookie(&response, &self.origin, "session_token")?;
        let value = auth_body(response).await?;
        if value["status"] != true {
            return Err(Error::OutcomeUnknown);
        }
        Ok(AuthenticatedSession {
            origin: self.origin.clone(),
            cookie,
        })
    }
    pub fn adopt_session(&mut self, session: AuthenticatedSession) -> Result<(), Error> {
        if session.origin != self.origin {
            return Err(Error::InvalidInput(
                "session belongs to another server origin",
            ));
        }
        self.cookie = Some(session.cookie);
        Ok(())
    }
}
impl AuthenticatedSession {
    /// Create a new private file compatible with read_cookie_file. Never overwrite a
    /// file/symlink or another profile. Parent directory must already be private.
    pub fn write_cookie_file_new(&self, path: &Path) -> Result<(), Error> {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        #[cfg(not(unix))]
        {
            return Err(Error::InvalidInput(
                "private session file creation requires Unix permissions; use in-memory adopt_session",
            ));
        }
        #[cfg(unix)]
        {
            let mut file = options
                .open(path)
                .map_err(|_| Error::InvalidInput("session output must be a new writable file"))?;
            file.write_all(self.cookie.as_bytes())
                .and_then(|_| file.sync_all())
                .map_err(|_| {
                    Error::InvalidInput(
                        "could not persist session cookie; output may be incomplete",
                    )
                })
        }
    }
}
fn auth_status(response: reqwest::Response) -> Result<reqwest::Response, Error> {
    if response.status().is_server_error() {
        return Err(Error::OutcomeUnknown);
    }
    if !response.status().is_success() {
        return Err(Error::Refused {
            status: response.status().as_u16(),
        });
    }
    Ok(response)
}
async fn auth_body(mut response: reqwest::Response) -> Result<Value, Error> {
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| Error::OutcomeUnknown)? {
        if bytes.len().saturating_add(chunk.len()) > 65_536 {
            return Err(Error::OutcomeUnknown);
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| Error::OutcomeUnknown)
}
fn issued_cookie(
    response: &reqwest::Response,
    origin: &str,
    suffix: &str,
) -> Result<Zeroizing<String>, Error> {
    let secure = origin.starts_with("https:");
    let expected = format!(
        "{}better-auth.{suffix}",
        if secure { "__Secure-" } else { "" }
    );
    let mut selected = None;
    for header in response.headers().get_all(SET_COOKIE) {
        let raw = header.to_str().map_err(|_| Error::OutcomeUnknown)?;
        let mut parts = raw.split(';');
        let Some((name, value)) = parts.next().and_then(|p| p.split_once('=')) else {
            return Err(Error::OutcomeUnknown);
        };
        if name != expected || value.is_empty() {
            continue;
        }
        if value.len() > 4096 || value.chars().any(|c| c.is_control() || c.is_whitespace()) {
            return Err(Error::OutcomeUnknown);
        }
        let attrs: Vec<_> = parts.map(str::trim).collect();
        if attrs.iter().any(|a| a.eq_ignore_ascii_case("Max-Age=0")) {
            continue;
        }
        if attrs
            .iter()
            .any(|a| a.to_ascii_lowercase().starts_with("domain="))
            || !attrs.iter().any(|a| a.eq_ignore_ascii_case("Path=/"))
            || !attrs.iter().any(|a| a.eq_ignore_ascii_case("HttpOnly"))
            || !attrs.iter().any(|a| a.eq_ignore_ascii_case("SameSite=Lax"))
            || (secure && !attrs.iter().any(|a| a.eq_ignore_ascii_case("Secure")))
            || selected.is_some()
        {
            return Err(Error::OutcomeUnknown);
        }
        selected = Some(Zeroizing::new(format!("{name}={value}")));
    }
    selected.ok_or(Error::OutcomeUnknown)
}
