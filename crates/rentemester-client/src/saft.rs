//! Audited bounded v3 package download; generation choices affect export metadata only.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_SAFT_BODY: usize = 4096;
pub const MAX_SAFT_PACKAGE: usize = 256 * 1024 * 1024;
pub fn saft_body_schema() -> Value {
    let date = json!({"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"});
    let mut schema = object(
        json!({"periodStart":date,"periodEnd":date,"confirm":{"const":true},"generationMode":{"type":"string","enum":["period_end","caller_supplied"]},"generatedAt":{"type":"string","description":"Explicit-zone timestamp; normalized and calendar-validated by server, only with caller_supplied mode."}}),
        &["periodStart", "periodEnd", "confirm"],
    );
    schema["oneOf"] = json!([
        {"not":{"anyOf":[{"required":["generationMode"]},{"required":["generatedAt"]}]}},
        {"required":["generationMode"],"properties":{"generationMode":{"const":"period_end"}},"not":{"required":["generatedAt"]}},
        {"required":["generationMode","generatedAt"],"properties":{"generationMode":{"const":"caller_supplied"}}}
    ]);
    schema
}
impl ApiClient {
    /// Exact confirmed native envelope once. Successful response is exact tar integer bytes.
    /// Audit may have committed if response is lost; never automatically retry.
    pub async fn saft_export(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit SAF-T export confirmation required",
            ));
        }
        if bytes.len() > MAX_SAFT_BODY {
            return Err(Error::InvalidInput("SAF-T body exceeds4096 bytes"));
        }
        body(&bytes, &saft_body_schema())?;
        let input: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::InvalidInput("invalid SAF-T body"))?;
        match (
            input.get("generationMode").and_then(Value::as_str),
            input.get("generatedAt"),
        ) {
            (None, None) | (Some("period_end"), None) => (),
            (Some("caller_supplied"), Some(value)) if value.is_string() => (),
            _ => {
                return Err(Error::InvalidInput(
                    "SAF-T generatedAt requires caller_supplied mode; period_end forbids generatedAt",
                ));
            }
        }
        let mut r = self
            .send(
                Method::POST,
                &company_path(company, "saft-export")?,
                &[],
                Some(bytes),
                self.cookie.as_ref().map(|v| v.as_str()),
            )
            .await?;
        if r.status().is_server_error() {
            return Err(Error::OutcomeUnknown);
        }
        if !r.status().is_success() {
            return Err(Error::Refused {
                status: r.status().as_u16(),
            });
        }
        if r.headers()
            .get("content-type")
            .and_then(|h| h.to_str().ok())
            != Some("application/x-tar")
            || r.content_length()
                .is_some_and(|n| n > MAX_SAFT_PACKAGE as u64)
        {
            return Err(Error::OutcomeUnknown);
        }
        let mut data = vec![];
        while let Some(chunk) = r.chunk().await.map_err(|_| Error::OutcomeUnknown)? {
            if data.len().saturating_add(chunk.len()) > MAX_SAFT_PACKAGE {
                return Err(Error::OutcomeUnknown);
            }
            data.extend_from_slice(&chunk);
        }
        if data.len() < 1024 || data.len() % 512 != 0 {
            return Err(Error::OutcomeUnknown);
        }
        Ok(json!({"ok":true,"export":{"contentType":"application/x-tar","bytes":data}}))
    }
}
