//! Explicit human-reviewed delegation to the configured executor; never accepts executor credentials.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_HOSTED_RECURRING_BODY: usize = 16 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostedRecurringMutation {
    Consent,
    Revoke,
}
impl HostedRecurringMutation {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Consent => "hosted-consent",
            Self::Revoke => "hosted-revoke",
        }
    }
}
pub fn hosted_recurring_body_schema(action: HostedRecurringMutation) -> Value {
    let hash = json!({"type":"string","minLength":64,"maxLength":64,"pattern":"^[0-9a-f]{64}$"});
    let mut fields = json!({"templateId":{"type":"integer","minimum":1,"maximum":9007199254740991u64},"reason":{"type":"string","minLength":1,"description":"Nonblank after native ECMAScript trimming, max1000 UTF16 units; exact bytes forwarded"},"confirm":{"const":true}});
    let mut required = vec!["templateId", "reason", "confirm"];
    match action {
        HostedRecurringMutation::Consent => {
            fields["planHash"] = hash;
            required.push("planHash");
            for key in ["serviceAccountId", "credentialId"] {
                fields[key] = json!({"type":"string","minLength":1,"maxLength":4096});
                required.push(key);
            }
        }
        HostedRecurringMutation::Revoke => {
            fields["expectedStateHash"] = hash;
            required.push("expectedStateHash");
        }
    }
    object(fields, &required)
}
fn id(n: i64) -> Result<(), Error> {
    if (1..=9_007_199_254_740_991).contains(&n) {
        Ok(())
    } else {
        Err(Error::InvalidInput(
            "positive exact bounded templateId required",
        ))
    }
}
impl ApiClient {
    pub async fn hosted_recurring_plan(
        &self,
        company: &str,
        template_id: i64,
    ) -> Result<Value, Error> {
        id(template_id)?;
        self.get(
            &company_path(
                company,
                &format!("scheduled-recurring/{template_id}/hosted-plan"),
            )?,
            &[],
        )
        .await
    }
    pub async fn mutate_hosted_recurring(
        &self,
        company: &str,
        action: HostedRecurringMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed || bytes.len() > MAX_HOSTED_RECURRING_BODY {
            return Err(Error::InvalidInput(
                "explicit hosted confirmation and native16KiB body required",
            ));
        }
        body(&bytes, &hosted_recurring_body_schema(action))?;
        let v: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::InvalidInput("native hosted JSON required"))?;
        id(v["templateId"]
            .as_i64()
            .ok_or(Error::InvalidInput("exact templateId required"))?)?;
        let reason = v["reason"]
            .as_str()
            .ok_or(Error::InvalidInput("explicit reason required"))?
            .trim_matches(|c: char| c != '\u{0085}' && (c.is_whitespace() || c == '\u{feff}'));
        if reason.is_empty() || reason.encode_utf16().count() > 1000 {
            return Err(Error::InvalidInput(
                "native nonblank1000 UTF16 reason required",
            ));
        }
        let key = if action == HostedRecurringMutation::Consent {
            "planHash"
        } else {
            "expectedStateHash"
        };
        let hash = v[key]
            .as_str()
            .ok_or(Error::InvalidInput("exact current hash required"))?;
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(Error::InvalidInput("lowercase SHA256 required"));
        }
        for key in ["serviceAccountId", "credentialId"] {
            if v.get(key)
                .and_then(Value::as_str)
                .is_some_and(|s| s.trim().is_empty() || s.chars().any(char::is_control))
            {
                return Err(Error::InvalidInput(
                    "bounded explicit executor identity required",
                ));
            }
        }
        self.request(
            Method::POST,
            &company_path(company, &format!("scheduled-recurring/{}", action.suffix()))?,
            &[],
            Some(bytes),
        )
        .await
    }
}
