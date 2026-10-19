//! Human-owner lifecycle only. One-time successful receipts may contain a secret; never replay issuance.
use crate::{
    ApiClient, Error,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_SERVICE_PRINCIPAL_BODY: usize = 8192;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServicePrincipalMutation {
    Create,
    Rotate,
    Revoke,
    Recover,
}
impl ServicePrincipalMutation {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Create => "",
            Self::Rotate => "/rotate",
            Self::Revoke => "/revoke",
            Self::Recover => "/recover",
        }
    }
}
pub fn service_principal_body_schema(action: ServicePrincipalMutation) -> Value {
    let mut fields = json!({"confirm":{"const":true},"operationId":{"type":"string","description":"Existing native UUID syntax, optional for issuance/revoke; retain caller operationId for explicit recovery, never generate a new one"}});
    let mut required = vec!["confirm"];
    match action {
        ServicePrincipalMutation::Create => {
            fields["displayName"] = json!({"type":"string","minLength":1,"description":"Native trimmed nonblank display name, at most120 Unicode scalar values and1024 UTF8 bytes"});
            required.push("displayName");
        }
        ServicePrincipalMutation::Rotate | ServicePrincipalMutation::Revoke => {
            for key in ["serviceAccountId", "credentialId"] {
                fields[key] = json!({"type":"string","minLength":1,"maxLength":4096});
                required.push(key);
            }
        }
        ServicePrincipalMutation::Recover => required.push("operationId"),
    }
    if matches!(
        action,
        ServicePrincipalMutation::Create | ServicePrincipalMutation::Rotate
    ) {
        fields["expiresInSeconds"] = json!({"type":"integer","minimum":86400,"maximum":31536000});
    }
    object(fields, &required)
}
fn uuid(s: &str) -> bool {
    let s = s.trim().to_ascii_lowercase();
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                *b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
        && matches!(b[14], b'1'..=b'8')
        && matches!(b[19], b'8' | b'9' | b'a' | b'b')
}
fn text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}
impl ApiClient {
    pub async fn service_principals(&self) -> Result<Value, Error> {
        self.get("/api/workspace/service-principals", &[]).await
    }
    /// Caller-chosen operationId permits explicit recovery, never secret redisclosure or automatic replay.
    pub async fn mutate_service_principal(
        &self,
        action: ServicePrincipalMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed || bytes.len() > MAX_SERVICE_PRINCIPAL_BODY {
            return Err(Error::InvalidInput(
                "explicit service lifecycle confirmation and native8192-byte body required",
            ));
        }
        body(&bytes, &service_principal_body_schema(action))?;
        let v: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::InvalidInput("native lifecycle JSON required"))?;
        if v.get("operationId")
            .and_then(Value::as_str)
            .is_some_and(|s| !uuid(s))
        {
            return Err(Error::InvalidInput("native operation UUID required"));
        }
        for key in ["serviceAccountId", "credentialId"] {
            if v.get(key)
                .and_then(Value::as_str)
                .is_some_and(|s| !text(s, 4096))
            {
                return Err(Error::InvalidInput(
                    "bounded native service identifier required",
                ));
            }
        }
        if let Some(name) = v.get("displayName").and_then(Value::as_str) {
            let name = name.trim();
            if !text(name, 1024) || name.chars().count() > 120 {
                return Err(Error::InvalidInput("native displayName bound required"));
            }
        }
        if v.get("expiresInSeconds")
            .is_some_and(|n| !n.as_u64().is_some_and(|s| (86400..=31536000).contains(&s)))
        {
            return Err(Error::InvalidInput("native expiry interval required"));
        }
        self.request(
            Method::POST,
            &format!("/api/workspace/service-principals{}", action.suffix()),
            &[],
            Some(bytes),
        )
        .await
    }
}
