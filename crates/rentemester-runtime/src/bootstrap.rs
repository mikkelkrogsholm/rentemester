//! Private operator/configuration boundary for first-owner bootstrap; never HTTP claims.
use crate::auth::{AuthError, CookieProfile, CookieVerifier};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use zeroize::Zeroizing;

/// Created only by a trusted adapter resolving the current local policy allowlist.
/// No Deserialize: request data cannot assert its own operator authorization.
/// This is not a sandbox against code that supplies a forged *trusted* policy.
pub struct Operator {
    workspace: String,
    actor: String,
}
impl Operator {
    pub fn from_policy(
        workspace: &str,
        actor: &str,
        allowed: &[String],
    ) -> Result<Self, AuthError> {
        let Some((kind, subject)) = actor.split_once(':') else {
            return Err(AuthError::InvalidCredentials);
        };
        if workspace.trim().is_empty()
            || workspace.len() > 1024
            || workspace.chars().any(char::is_control)
            || !matches!(kind, "user" | "agent" | "system")
            || subject.is_empty()
            || actor.len() > 1024
            || actor.chars().any(|c| c.is_control() || c.is_whitespace())
            || !allowed.iter().any(|s| s == actor)
        {
            return Err(AuthError::InvalidCredentials);
        }
        Ok(Self {
            workspace: workspace.to_owned(),
            actor: actor.to_owned(),
        })
    }
    pub fn workspace(&self) -> &str {
        &self.workspace
    }
    pub fn actor(&self) -> &str {
        &self.actor
    }
}
pub struct ReservationKey {
    workspace: String,
    secret: Zeroizing<String>,
}
impl ReservationKey {
    pub fn new(workspace: &str, secret: &str) -> Result<Self, AuthError> {
        CookieVerifier::new(workspace, CookieProfile::Development, secret)?;
        Ok(Self {
            workspace: workspace.to_owned(),
            secret: Zeroizing::new(secret.to_owned()),
        })
    }
    pub fn workspace(&self) -> &str {
        &self.workspace
    }
    /// Caller supplies the canonical, validated email. Compatible with the existing
    /// private bootstrap HMAC namespace; no email is persisted in saga/audit rows.
    pub fn hash_email(&self, canonical_email: &str) -> Result<String, AuthError> {
        if canonical_email.is_empty()
            || canonical_email.len() > 4096
            || canonical_email.chars().any(char::is_control)
        {
            return Err(AuthError::InvalidCredentials);
        }
        let mut mac = Hmac::<Sha256>::new_from_slice(self.secret.as_bytes())
            .map_err(|_| AuthError::CryptoUnavailable)?;
        mac.update(b"rentemester-workspace-bootstrap-email-v1\0");
        mac.update(canonical_email.as_bytes());
        Ok(mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policy_membership_and_workspace_are_required() {
        let allowed = vec!["user:installer".to_owned()];
        assert!(Operator::from_policy("w", "agent:other", &allowed).is_err());
        assert!(Operator::from_policy("w", "user:", &["user:".to_owned()]).is_err());
        let operator = Operator::from_policy("w", "user:installer", &allowed).unwrap();
        assert_eq!(operator.workspace(), "w");
        assert_eq!(operator.actor(), "user:installer");
    }
}
