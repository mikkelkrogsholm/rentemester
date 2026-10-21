//! Original payable transport; original payload names, server calculations and receipts.
use crate::{ApiClient, Error, company_path};
use reqwest::Method;
use serde_json::{Value, json};
#[derive(Clone, Copy, Debug)]
pub enum PayableOriginalMutation {
    Register,
    Pay,
}
#[derive(Clone, Default, Debug)]
pub struct PayableOriginalQuery {
    pub status: Option<String>,
    pub as_of: Option<String>,
    pub as_of_date: Option<String>,
    pub supplier: Option<String>,
    pub vendor_id: Option<u64>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub min_days: Option<u64>,
}
pub fn payable_original_body_schema(operation: PayableOriginalMutation) -> Value {
    let id = json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
    let mut schema = match operation {
        PayableOriginalMutation::Register => {
            json!({"type":"object","properties":{"documentId":id,"billDate":{"type":"string"},"dueDate":{"type":"string"},"expenseAccount":{"type":"string","minLength":1},"vatTreatment":{"type":"string","enum":["standard","exempt","non_deductible"]},"vendorId":id,"note":{"type":"string"}},"required":["documentId","billDate","dueDate","expenseAccount"],"additionalProperties":false})
        }
        PayableOriginalMutation::Pay => {
            json!({"type":"object","properties":{"payableId":id,"bankTransactionId":id,"amount":{"type":"number","exclusiveMinimum":0},"date":{"type":"string"},"paymentAccount":{"type":"string"},"note":{"type":"string"}},"required":["payableId","bankTransactionId"],"additionalProperties":false})
        }
    };
    schema["properties"]["confirm"] = json!({"const":true});
    schema["properties"]["idempotencyKey"] = json!({"type":"string","minLength":1,"description":"Optional exact raw caller key, at most128 UTF-16 code units; never generated or automatically retried."});
    schema["required"]
        .as_array_mut()
        .unwrap()
        .push(json!("confirm"));
    schema
}
fn envelope(value: Value, write: bool) -> Result<Value, Error> {
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
            value.get("data").is_none() && value["errors"].as_array().is_some_and(|a| !a.is_empty())
        };
    if valid {
        Ok(value)
    } else {
        Err(if write {
            Error::OutcomeUnknown
        } else {
            Error::Protocol
        })
    }
}
impl ApiClient {
    pub async fn payable_original_list(
        &self,
        company: &str,
        query: &PayableOriginalQuery,
    ) -> Result<Value, Error> {
        if query
            .vendor_id
            .is_some_and(|n| n == 0 || n > 9007199254740991)
            || query.min_days.is_some_and(|n| n > 9007199254740991)
        {
            return Err(Error::InvalidInput(
                "positive safe vendorId/nonnegative safe minDays required",
            ));
        }
        let mut pairs = vec![];
        for (key, value) in [
            ("status", query.status.as_deref()),
            (
                "asOf",
                query.as_of.as_deref().or(query.as_of_date.as_deref()),
            ),
            ("supplier", query.supplier.as_deref()),
            ("from", query.from.as_deref()),
            ("to", query.to.as_deref()),
        ] {
            if let Some(value) = value {
                pairs.push((key, value.to_owned()));
            }
        }
        if let Some(v) = query.vendor_id {
            pairs.push(("vendorId", v.to_string()));
        }
        if let Some(v) = query.min_days {
            pairs.push(("minDays", v.to_string()));
        }
        let refs = pairs
            .iter()
            .map(|(k, v)| (*k, v.as_str()))
            .collect::<Vec<_>>();
        envelope(
            self.request(
                Method::GET,
                &company_path(company, "payables/original/list")?,
                &refs,
                None,
            )
            .await?,
            false,
        )
    }
    /// One confirmed request; raw optional key and payload preserved. Never retry automatically.
    pub async fn payable_original_mutation(
        &self,
        company: &str,
        operation: PayableOriginalMutation,
        body: Vec<u8>,
    ) -> Result<Value, Error> {
        if body.len() > 16 * 1024 {
            return Err(Error::InvalidInput(
                "original payable mutation exceeds16KiB",
            ));
        }
        crate::domain::body(&body, &payable_original_body_schema(operation))?;
        let value: Value = serde_json::from_slice(&body)
            .map_err(|_| Error::InvalidInput("expected original payable JSON"))?;
        if value
            .get("idempotencyKey")
            .and_then(Value::as_str)
            .is_some_and(|s| s.encode_utf16().count() > 128)
        {
            return Err(Error::InvalidInput(
                "idempotencyKey exceeds128 UTF-16 code units",
            ));
        }
        let action = match operation {
            PayableOriginalMutation::Register => "register",
            PayableOriginalMutation::Pay => "pay",
        };
        envelope(
            self.request(
                Method::POST,
                &company_path(company, &format!("payables/original/{action}"))?,
                &[],
                Some(body),
            )
            .await?,
            true,
        )
    }
}
