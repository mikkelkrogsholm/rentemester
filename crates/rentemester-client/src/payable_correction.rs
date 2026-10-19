//! Exact reviewed direct-bank purchase correction; all accounting and source proof stay server-side.
use crate::{
    ApiClient, Error, MAX_RESPONSE, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_PAYABLE_CORRECTION_BODY: usize = 16 * 1024;
pub fn payable_correction_body_schema(apply: bool) -> Value {
    let id = json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
    let mut schema = object(
        json!({"documentId":id,"bankTransactionId":id,"billDate":{"type":"string","minLength":10,"maxLength":10},"dueDate":{"type":"string","minLength":10,"maxLength":10},"expenseAccountNo":{"type":"string","minLength":1},"vatTreatment":{"type":["string","null"],"enum":["standard","exempt","non_deductible",null]},"vendorId":{"type":["integer","null"],"minimum":1,"maximum":9007199254740991u64},"note":{"type":["string","null"]}}),
        &[
            "documentId",
            "bankTransactionId",
            "billDate",
            "dueDate",
            "expenseAccountNo",
        ],
    );
    if apply {
        for (key, value) in [
            (
                "planHash",
                json!({"type":"string","minLength":64,"maxLength":64,"pattern":"^[0-9a-f]{64}$"}),
            ),
            ("reason", json!({"type":"string","minLength":1})),
            (
                "idempotencyKey",
                json!({"type":"string","minLength":1,"description":"Exact caller key; preserve with reviewed body after an unknown outcome. Never generated or retried by the client."}),
            ),
            ("confirm", json!({"const":true})),
        ] {
            schema["properties"][key] = value;
            schema["required"].as_array_mut().unwrap().push(json!(key));
        }
    }
    schema
}
impl ApiClient {
    pub async fn direct_bank_payable_correction(
        &self,
        company: &str,
        bytes: Vec<u8>,
        apply: bool,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if confirmed != apply || bytes.len() > MAX_PAYABLE_CORRECTION_BODY {
            return Err(Error::InvalidInput(
                "plan is read-only; apply requires explicit confirmation and exact native16KiB JSON",
            ));
        }
        body(&bytes, &payable_correction_body_schema(apply))?;
        let path = company_path(
            company,
            if apply {
                "payables/direct-bank-correction/apply"
            } else {
                "payables/direct-bank-correction/plan"
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
                Some(bytes),
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
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| unknown())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(unknown());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            if status.is_success() {
                unknown()
            } else {
                Error::Refused {
                    status: status.as_u16(),
                }
            }
        })?;
        if value["ok"] == false && value["code"].as_str().is_some_and(|s| !s.is_empty()) {
            if apply
                && matches!(
                    value["code"].as_str(),
                    Some("internal" | "outcome_unknown" | "outcome_unconfirmed")
                )
            {
                return Err(Error::OutcomeUnknown);
            }
            return Ok(value);
        }
        if status.is_success() && value["ok"] == true {
            Ok(value)
        } else if status.is_success() {
            Err(unknown())
        } else {
            Err(Error::Refused {
                status: status.as_u16(),
            })
        }
    }
}
