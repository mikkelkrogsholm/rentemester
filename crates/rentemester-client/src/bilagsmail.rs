//! Native receive-only mailbox transport. Configuration secrets are request bytes, never diagnostics.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_BILAGSMAIL_BODY: usize = 16 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BilagsmailMutation {
    Configure,
    DeleteConfig,
    Alias,
    Poll,
}
pub fn bilagsmail_body_schema(action: BilagsmailMutation) -> Value {
    match action {
        BilagsmailMutation::Configure => object(
            json!({"host":{"type":"string","minLength":1},"port":{"type":"integer","minimum":1,"maximum":65535},"username":{"type":"string","minLength":1},"password":{"type":"string","minLength":1,"maxLength":8192,"writeOnly":true,"description":"Private mailbox credential supplied only inside bounded request body; never returned"},"secure":{"type":"boolean","description":"TLS on connect; source default true, explicit false is plaintext IMAP without invented STARTTLS"},"mailbox":{"type":"string"}}),
            &["host", "port", "username", "password"],
        ),
        BilagsmailMutation::DeleteConfig => {
            json!({"type":"null","description":"DELETE has no HTTP body; outer explicit confirmation required"})
        }
        BilagsmailMutation::Alias => object(
            json!({"alias":{"type":["string","null"],"description":"Explicit nullable alias; server normalizes and checks source3–64 character grammar"}}),
            &["alias"],
        ),
        BilagsmailMutation::Poll => object(json!({"confirm":{"const":true}}), &["confirm"]),
    }
}
impl ApiClient {
    pub async fn bilagsmail(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "bilagsmail")?, &[]).await
    }
    /// One exact native write, never regenerates inputs or retries uncertain effects.
    pub async fn mutate_bilagsmail(
        &self,
        company: &str,
        action: BilagsmailMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed || bytes.len() > MAX_BILAGSMAIL_BODY {
            return Err(Error::InvalidInput(
                "explicit mailbox confirmation and16KiB body bound required",
            ));
        }
        let (method, suffix) = match action {
            BilagsmailMutation::Configure => (Method::POST, "imap-config"),
            BilagsmailMutation::DeleteConfig => (Method::DELETE, "imap-config"),
            BilagsmailMutation::Alias => (Method::PATCH, "alias"),
            BilagsmailMutation::Poll => (Method::POST, "poll"),
        };
        let input = if action == BilagsmailMutation::DeleteConfig {
            if !bytes.is_empty() {
                return Err(Error::InvalidInput("mailbox deletion accepts no body"));
            }
            None
        } else {
            body(&bytes, &bilagsmail_body_schema(action))?;
            Some(bytes)
        };
        let result = self
            .request(
                method,
                &company_path(company, &format!("bilagsmail/{suffix}"))?,
                &[],
                input,
            )
            .await;
        if action == BilagsmailMutation::DeleteConfig {
            return result.map_err(|e| match e {
                Error::Transport | Error::Protocol => Error::OutcomeUnknown,
                Error::Refused { status } if status == 408 || status >= 500 => {
                    Error::OutcomeUnknown
                }
                other => other,
            });
        }
        result.map_err(|e| match e {
            Error::Refused { status: 408 } => Error::OutcomeUnknown,
            other => other,
        })
    }
}
