//! Fixed original VAT purchase writes. No accounting, identity selection or retries.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::{Value, json};

pub fn original_vat_purchase_schema(representation: bool) -> Value {
    let id = json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
    let mut properties = json!({"transactionDate":{"type":"string"},"text":{"type":"string"},"netAmount":{"type":"number"},"expenseAccountNo":{"type":"string"},"documentId":id,"paymentAccountNo":{"type":"string"},"sourceBankTransactionId":id,"currency":{"type":"string"},"amountForeign":{"type":"number"},"amountDkk":{"type":"number"},"fxRateToDkk":{"type":"number"}});
    if !representation {
        properties["invoiceNo"] = json!({"type":"string"});
    }
    json!({"type":"object","properties":properties,"required":["transactionDate","text","netAmount",if representation {"documentId"} else {"expenseAccountNo"}],"additionalProperties":false})
}
pub fn validate_original_vat_purchase(input: &Value, representation: bool) -> Result<(), Error> {
    let bytes =
        serde_json::to_vec(input).map_err(|_| Error::InvalidInput("invalid VAT purchase JSON"))?;
    if bytes.len() > MAX_RESPONSE {
        return Err(Error::InvalidInput("VAT purchase input exceeds 16MiB"));
    }
    let schema = original_vat_purchase_schema(representation);
    let object = input
        .as_object()
        .ok_or(Error::InvalidInput("VAT purchase object required"))?;
    let properties = schema["properties"].as_object().unwrap();
    if schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|k| !object.contains_key(k.as_str().unwrap()))
        || object.iter().any(|(key, value)| {
            let Some(field) = properties.get(key) else {
                return true;
            };
            match field["type"].as_str().unwrap() {
                "string" => !value.is_string(),
                "number" => !value.is_number(),
                "integer" => value
                    .as_u64()
                    .is_none_or(|n| !(1..=9_007_199_254_740_991).contains(&n)),
                _ => true,
            }
        })
    {
        return Err(Error::InvalidInput("invalid strict VAT purchase payload"));
    }
    Ok(())
}
fn receipt(value: &Value) -> bool {
    let shape = value.as_object().is_some_and(|object| {
        object.keys().all(|key| {
            matches!(
                key.as_str(),
                "ok" | "errors" | "appliedRules" | "entryId" | "entryNo" | "entryHash"
            )
        }) && value["ok"].is_boolean()
            && ["errors", "appliedRules"].iter().all(|key| {
                value[key]
                    .as_array()
                    .is_some_and(|a| a.iter().all(Value::is_string))
            })
            && value.get("entryId").is_none_or(|v| {
                v.as_u64()
                    .is_some_and(|n| (1..=9_007_199_254_740_991).contains(&n))
            })
    });
    shape
        && value["errors"]
            .as_array()
            .is_some_and(|a| a.is_empty() == (value["ok"] == true))
        && if value["ok"] == true {
            value["entryId"].as_u64().is_some_and(|id| id > 0)
                && value["entryNo"]
                    .as_str()
                    .is_some_and(|s| !s.trim().is_empty())
                && value["entryHash"]
                    .as_str()
                    .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        } else {
            ["entryId", "entryNo", "entryHash"]
                .iter()
                .all(|k| value.get(k).is_none())
        }
}
impl ApiClient {
    pub async fn original_vat_eu_service_purchase(
        &self,
        company: &str,
        input: &Value,
        actor: Option<&str>,
    ) -> Result<Value, Error> {
        self.original_vat_purchase(company, input, actor, false)
            .await
    }
    pub async fn original_vat_representation_purchase(
        &self,
        company: &str,
        input: &Value,
        actor: Option<&str>,
    ) -> Result<Value, Error> {
        self.original_vat_purchase(company, input, actor, true)
            .await
    }
    async fn original_vat_purchase(
        &self,
        company: &str,
        input: &Value,
        actor: Option<&str>,
        representation: bool,
    ) -> Result<Value, Error> {
        validate_original_vat_purchase(input, representation)?;
        let mut body = json!({"payload":input,"confirm":true});
        if let Some(actor) = actor {
            if actor.trim().is_empty()
                || actor != actor.trim()
                || actor.encode_utf16().count() > 160
                || actor.chars().any(char::is_control)
            {
                return Err(Error::InvalidInput("invalid actor assertion"));
            }
            body["actor"] = json!(actor);
        }
        let body = serde_json::to_vec(&body)
            .map_err(|_| Error::InvalidInput("invalid VAT purchase JSON"))?;
        if body.len() > MAX_RESPONSE {
            return Err(Error::InvalidInput("VAT purchase body exceeds 16MiB"));
        }
        let operation = if representation {
            "representation-purchase"
        } else {
            "eu-service-purchase"
        };
        let mut response = self
            .send(
                Method::POST,
                &company_path(company, &format!("original/vat/{operation}"))?,
                &[],
                Some(body),
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await?;
        let status = response.status().as_u16();
        if status >= 500 || status == 408 {
            return Err(Error::OutcomeUnknown);
        }
        if status != 200 && status != 422 {
            return Err(Error::Refused { status });
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Error::OutcomeUnknown)? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(Error::OutcomeUnknown);
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            if status == 422 {
                Error::Refused { status }
            } else {
                Error::OutcomeUnknown
            }
        })?;
        if receipt(&value)
            && ((status == 200 && value["ok"] == true) || (status == 422 && value["ok"] == false))
        {
            Ok(value)
        } else if status == 422 {
            Err(Error::Refused { status })
        } else {
            Err(Error::OutcomeUnknown)
        }
    }
}
