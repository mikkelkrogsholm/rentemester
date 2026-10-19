//! Purpose-separated invitation digests. Bearers are never projected or logged.
use crate::auth::{AuthError, CookieProfile, CookieVerifier, new_token};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use zeroize::Zeroizing;
pub struct InvitationKey {
    workspace: String,
    key: Zeroizing<String>,
}
impl InvitationKey {
    pub fn new(workspace: &str, key: &str) -> Result<Self, AuthError> {
        CookieVerifier::new(workspace, CookieProfile::Development, key)?;
        Ok(Self {
            workspace: workspace.into(),
            key: Zeroizing::new(key.into()),
        })
    }
    pub fn workspace(&self) -> &str {
        &self.workspace
    }
    pub fn digest(&self, domain: &str, value: &str) -> Result<String, AuthError> {
        if !["token", "email"].contains(&domain) {
            return Err(AuthError::InvalidConfig);
        }
        let mut mac = Hmac::<Sha256>::new_from_slice(self.key.as_bytes())
            .map_err(|_| AuthError::InvalidConfig)?;
        mac.update(format!("rentemester-workspace-invitation-{domain}-v1\0{value}").as_bytes());
        Ok(mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect())
    }
    pub fn generate(&self) -> Result<(String, crate::auth::OpaqueToken), AuthError> {
        let token = new_token()?;
        let raw = self.digest("token", new_token()?.as_str())?;
        let id = format!(
            "{}-{}-4{}-a{}-{}",
            &raw[..8],
            &raw[8..12],
            &raw[13..16],
            &raw[17..20],
            &raw[20..32]
        );
        Ok((id, token))
    }
}
