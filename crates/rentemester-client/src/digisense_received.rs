//! Confirmed inbound commands only; no local provider transport or automatic replay.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_DIGISENSE_RECEIVED_BODY: usize = 16 * 1024;
pub fn digisense_received_schema(workspace: bool) -> Value {
    if workspace {
        return object(json!({"confirm":{"const":true}}), &["confirm"]);
    }
    let mut metadata = crate::documents_original_schema(crate::DocumentsOriginalMutation::Ingest)["properties"]["metadata"].clone();
    metadata["required"] = json!([]);
    metadata["properties"]["documentType"] = json!({"const":"purchase_sale"});
    object(
        json!({"confirm":{"const":true},"companyKey":{"type":"string","minLength":1,"maxLength":512},"limit":{"type":"integer","minimum":1,"maximum":100},"maxPages":{"type":"integer","minimum":1,"maximum":100},"maxTimestamp":{"type":"string","minLength":1,"maxLength":128},"metadata":metadata,"ingestOptions":object(json!({"forceDuplicateLogicalIdentity":{"type":"boolean"}}), &[])}),
        &["confirm"],
    )
}
pub fn validate_digisense_received(bytes: &[u8], workspace: bool) -> Result<Value, Error> {
    if bytes.len() > MAX_DIGISENSE_RECEIVED_BODY {
        return Err(Error::InvalidInput("receive body exceeds16KiB"));
    }
    body(bytes, &digisense_received_schema(workspace))?;
    let v: Value =
        serde_json::from_slice(bytes).map_err(|_| Error::InvalidInput("receive JSON required"))?;
    for key in ["companyKey", "maxTimestamp"] {
        if let Some(s) = v[key].as_str() {
            if s.trim() != s || s.is_empty() || s.chars().any(char::is_control) {
                return Err(Error::InvalidInput("bounded receive assertion required"));
            }
        }
    }
    Ok(v)
}
impl ApiClient {
    /// Exact reviewed bytes once. On OutcomeUnknown inspect documents/dedup; never replay automatically.
    pub async fn digisense_received(
        &self,
        company: &str,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        let path = company_path(company, "efaktura/modtag")?;
        self.receive_request(&path, bytes, confirmed, false).await
    }
    pub async fn digisense_received_workspace(
        &self,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        self.receive_request("/api/workspace/efaktura/modtag", bytes, confirmed, true)
            .await
    }
    async fn receive_request(
        &self,
        path: &str,
        bytes: Vec<u8>,
        confirmed: bool,
        workspace: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit receive confirmation required",
            ));
        }
        validate_digisense_received(&bytes, workspace)?;
        self.request(Method::POST, path, &[], Some(bytes))
            .await
            .map_err(|e| match e {
                Error::Refused { status: 408 } => Error::OutcomeUnknown,
                other => other,
            })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn digisense_received_strict_bounds_confirmation() {
        for v in [
            json!({}),
            json!({"confirm":false}),
            json!({"confirm":true,"limit":0}),
            json!({"confirm":true,"maxPages":101}),
            json!({"confirm":true,"apiKey":"private"}),
            json!({"confirm":true,"metadata":{"unexpected":1}}),
        ] {
            assert!(validate_digisense_received(&serde_json::to_vec(&v).unwrap(), false).is_err());
        }
        assert!(
            validate_digisense_received(
                br#"{"confirm":true,"limit":100,"metadata":{"invoiceNo":"human"}}"#,
                false
            )
            .is_ok()
        );
        assert!(
            validate_digisense_received(br#"{"confirm":true,"workspace":"/tmp"}"#, true).is_err()
        );
    }
}
