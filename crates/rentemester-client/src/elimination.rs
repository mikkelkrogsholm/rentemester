//! Exact native elimination transport. Server owns proof, polarity, money and four-eyes authority.
use crate::{
    ApiClient, Error, MAX_RESPONSE, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_ELIMINATION_BODY: usize = 256 * 1024;
#[derive(Clone, Copy, Debug)]
pub enum EliminationMutation {
    Propose,
    Approve,
    Reject,
    Apply,
    Reverse,
}
impl EliminationMutation {
    fn suffix(self) -> &'static str {
        match self {
            Self::Propose => "propose",
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Apply => "apply",
            Self::Reverse => "reverse",
        }
    }
}
pub fn elimination_plan_schema() -> Value {
    object(
        json!({"id":{"type":"string","pattern":"^[a-z][a-z0-9-]{0,63}$"},"mappingId":{"type":"string","pattern":"^[a-z][a-z0-9-]{0,63}$"},"asOf":{"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"},"evidenceRefs":{"type":"array","minItems":1,"maxItems":32,"items":{"type":"string","minLength":1,"description":"Trimmed NFC, at most256 UTF16 code units, distinct; server validates canonical source evidence"}}}),
        &["id", "mappingId", "asOf", "evidenceRefs"],
    )
}
pub fn elimination_body_schema(action: EliminationMutation) -> Value {
    let hash = json!({"type":"string","pattern":"^[0-9a-f]{64}$","minLength":64,"maxLength":64});
    let mut schema = if matches!(action, EliminationMutation::Propose) {
        let mut v = elimination_plan_schema();
        v["properties"]["expectedPayloadHash"] = hash;
        v["required"]
            .as_array_mut()
            .unwrap()
            .push(json!("expectedPayloadHash"));
        v
    } else {
        object(
            json!({"eliminationId":{"type":"string","pattern":"^[a-z][a-z0-9-]{0,63}$"},"payloadHash":hash}),
            &["eliminationId", "payloadHash"],
        )
    };
    schema["properties"]["confirm"] = json!({"const":true});
    schema["required"]
        .as_array_mut()
        .unwrap()
        .push(json!("confirm"));
    schema
}
impl ApiClient {
    pub async fn eliminations(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "eliminations")?, &[]).await
    }
    pub async fn group_eliminations(&self, as_of: &str) -> Result<Value, Error> {
        if as_of.len() != 10
            || as_of.bytes().enumerate().any(|(i, b)| {
                if i == 4 || i == 7 {
                    b != b'-'
                } else {
                    !b.is_ascii_digit()
                }
            })
        {
            return Err(Error::InvalidInput(
                "explicit elimination asOf YYYY-MM-DD required",
            ));
        }
        self.get("/api/group-eliminations", &[("asOf", as_of)])
            .await
    }
    /// Read-only POST once, without confirmation or mutation-uncertainty semantics. No retries.
    pub async fn elimination_plan(&self, company: &str, bytes: Vec<u8>) -> Result<Value, Error> {
        body(&bytes, &elimination_plan_schema())?;
        let mut response = self
            .send(
                Method::POST,
                &company_path(company, "eliminations/plan")?,
                &[],
                Some(bytes),
                self.cookie.as_ref().map(|v| v.as_str()),
            )
            .await
            .map_err(|e| {
                if matches!(e, Error::OutcomeUnknown) {
                    Error::Transport
                } else {
                    e
                }
            })?;
        if !response.status().is_success() {
            return Err(Error::Refused {
                status: response.status().as_u16(),
            });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(Error::Protocol);
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| Error::Protocol)
    }
    /// Exact native confirmed body once. No generated IDs, actor, keys, amounts, defaults or retries.
    pub async fn mutate_elimination(
        &self,
        company: &str,
        action: EliminationMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed || bytes.len() > MAX_ELIMINATION_BODY {
            return Err(Error::InvalidInput(
                "explicit elimination confirmation and native256KiB body required",
            ));
        }
        body(&bytes, &elimination_body_schema(action))?;
        self.request(
            Method::POST,
            &company_path(company, &format!("eliminations/{}", action.suffix()))?,
            &[],
            Some(bytes),
        )
        .await
    }
}
