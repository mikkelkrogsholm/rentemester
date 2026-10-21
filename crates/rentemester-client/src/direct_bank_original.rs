//! Original direct-bank payable correction transport; native proof/accounting stay server-owned.
use crate::{
    ApiClient, BankOriginalMutation, Error, MAX_RESPONSE, bank_original_body_schema, company_path,
};
use reqwest::Method;
use serde_json::{Value, json};
pub fn direct_bank_original_schema(apply: bool) -> Value {
    let mut schema = crate::payable_correction_body_schema(apply);
    for key in ["billDate", "dueDate", "note"] {
        schema["properties"][key] = json!({"type":"string"});
    }
    schema["properties"]["vatTreatment"] =
        json!({"type":"string","enum":["standard","exempt","non_deductible"]});
    schema["properties"]["vendorId"] =
        json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
    if apply {
        schema["properties"]["expectedPrincipal"] = bank_original_body_schema(
            BankOriginalMutation::CorrectionApply,
        )["properties"]["expectedPrincipal"]
            .clone();
    }
    schema
}
impl ApiClient {
    /// One POST. Plan is read-only; apply preserves exact raw key/reason and never retries.
    pub async fn direct_bank_original(
        &self,
        company: &str,
        body: Vec<u8>,
        apply: bool,
    ) -> Result<Value, Error> {
        if body.len() > 16 * 1024 {
            return Err(Error::InvalidInput(
                "original direct correction exceeds16KiB",
            ));
        }
        crate::domain::body(&body, &direct_bank_original_schema(apply))?;
        if apply {
            let value: Value = serde_json::from_slice(&body)
                .map_err(|_| Error::InvalidInput("expected direct correction JSON"))?;
            let hash = value["planHash"].as_str().unwrap_or("");
            if hash.len() != 64
                || !hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err(Error::InvalidInput("planHash must be lowercase64hex"));
            }
        }
        let path = company_path(
            company,
            if apply {
                "bank/original/direct-payable-apply"
            } else {
                "bank/original/direct-payable-plan"
            },
        )?;
        let unknown = || {
            if apply {
                Error::OutcomeUnknown
            } else {
                Error::Transport
            }
        };
        let mut response = self
            .send(
                Method::POST,
                &path,
                &[],
                Some(body),
                self.cookie.as_ref().map(|v| v.as_str()),
            )
            .await
            .map_err(|e| {
                if !apply && e == Error::OutcomeUnknown {
                    Error::Transport
                } else {
                    e
                }
            })?;
        let status = response.status();
        if apply && (status.is_server_error() || status.as_u16() == 408) {
            return Err(Error::OutcomeUnknown);
        }
        if !status.is_success() {
            return Err(Error::Refused {
                status: status.as_u16(),
            });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| unknown())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(unknown());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            if apply {
                Error::OutcomeUnknown
            } else {
                Error::Protocol
            }
        })?;
        let valid = value["ok"].is_boolean()
            && value["errors"]
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_string))
            && value.get("code").is_none_or(Value::is_string)
            && value
                .get("appliedRules")
                .is_none_or(|v| v.as_array().is_some_and(|a| a.iter().all(Value::is_string)))
            && if value["ok"] == true {
                value["data"].is_object() && value["errors"].as_array().is_some_and(Vec::is_empty)
            } else {
                value.get("data").is_none()
                    && value["errors"].as_array().is_some_and(|a| !a.is_empty())
            };
        if !valid {
            return Err(if apply {
                Error::OutcomeUnknown
            } else {
                Error::Protocol
            });
        }
        Ok(value)
    }
}
