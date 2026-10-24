//! Exact existing contact-import transport; CSV, enrichment and dedup stay server-side.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path, domain::object};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_CONTACT_IMPORT_BODY: usize = 12 * 1024 * 1024;
pub fn contact_import_body_schema() -> Value {
    object(
        json!({"fileName":{"type":"string","minLength":1},"content":{"type":"string","minLength":1},"defaultRole":{"type":"string","enum":["customer","vendor"],"description":"Explicit original contact CSV acceptance and fallback role; classification remains server-owned"},"enrichCvr":{"type":"boolean","description":"Explicit optional CVR enrichment; provider configuration and cache remain server-owned"},"confirm":{"const":true}}),
        &["fileName", "content", "confirm"],
    )
}
impl ApiClient {
    pub async fn import_contacts(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed || bytes.len() > MAX_CONTACT_IMPORT_BODY {
            return Err(Error::InvalidInput(
                "explicit confirmation and exact native12MiB import JSON required",
            ));
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::InvalidInput("expected native contact import JSON"))?;
        let allowed = ["fileName", "content", "enrichCvr", "confirm", "defaultRole"];
        if value
            .as_object()
            .is_none_or(|o| o.keys().any(|k| !allowed.contains(&k.as_str())))
            || value["fileName"].as_str().is_none_or(|s| s.is_empty())
            || value["content"].as_str().is_none_or(|s| s.is_empty())
            || value["confirm"] != true
            || value.get("enrichCvr").is_some_and(|v| !v.is_boolean())
            || value
                .get("defaultRole")
                .is_some_and(|v| !matches!(v.as_str(), Some("customer" | "vendor")))
        {
            return Err(Error::InvalidInput(
                "native import body differs from discoverable schema",
            ));
        }
        let mut response = self
            .send(
                Method::POST,
                &company_path(company, "import")?,
                &[],
                Some(bytes),
                self.cookie.as_ref().map(|v| v.as_str()),
            )
            .await?;
        let status = response.status();
        if status.is_server_error() || status.as_u16() == 408 {
            return Err(Error::OutcomeUnknown);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::OutcomeUnknown)? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(Error::OutcomeUnknown);
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            if status.is_success() {
                Error::OutcomeUnknown
            } else {
                Error::Refused {
                    status: status.as_u16(),
                }
            }
        })?;
        if value["ok"] == false && value["code"].as_str().is_some_and(|s| !s.is_empty()) {
            if matches!(
                value["code"].as_str(),
                Some("internal" | "outcome_unknown" | "outcome_unconfirmed")
            ) {
                return Err(Error::OutcomeUnknown);
            }
            return Ok(value);
        }
        if status.is_success() && value["ok"] == true {
            Ok(value)
        } else if status.is_success() {
            Err(Error::OutcomeUnknown)
        } else {
            Err(Error::Refused {
                status: status.as_u16(),
            })
        }
    }
}
