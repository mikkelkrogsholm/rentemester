//! Original bank account master-data transport. Identity and patch semantics stay server-owned.
use crate::{ApiClient, Error, company_path};
use reqwest::Method;
use serde_json::{Value, json};
#[derive(Clone, Copy, Debug)]
pub enum BankAccountOriginalMutation {
    Create,
    Update,
}
pub fn bank_account_original_schema(operation: BankAccountOriginalMutation) -> Value {
    let mut properties = json!({"confirm":{"const":true},"name":{"type":"string"},"bankName":{"type":"string"},"registrationNo":{"type":"string"},"accountNo":{"type":"string"},"iban":{"type":"string"},"bic":{"type":"string"},"accountOwner":{"type":"string"},"customerNo":{"type":"string"},"currency":{"type":"string"},"ledgerAccountNo":{"type":"string"}});
    let required = match operation {
        BankAccountOriginalMutation::Create => {
            properties["slug"] = json!({"type":"string"});
            vec!["confirm", "name"]
        }
        BankAccountOriginalMutation::Update => {
            properties["account"] = json!({"type":"string","minLength":1});
            properties["active"] = json!({"type":"boolean"});
            vec!["confirm", "account"]
        }
    };
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
impl ApiClient {
    /// Exactly one authenticated request, no retry.5xx/lost or malformed replies retain mutation uncertainty.
    pub async fn bank_account_original(
        &self,
        company: &str,
        operation: BankAccountOriginalMutation,
        body: Vec<u8>,
    ) -> Result<Value, Error> {
        crate::domain::body(&body, &bank_account_original_schema(operation))?;
        let action = match operation {
            BankAccountOriginalMutation::Create => "create",
            BankAccountOriginalMutation::Update => "update",
        };
        let value = self
            .request(
                Method::POST,
                &company_path(company, &format!("bank/accounts-original/{action}"))?,
                &[],
                Some(body),
            )
            .await?;
        let valid = value["ok"].is_boolean()
            && value["errors"]
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_string))
            && value.get("code").is_none_or(Value::is_string)
            && if value["ok"] == true {
                value["data"]["account"].is_object()
                    && value["errors"].as_array().is_some_and(Vec::is_empty)
            } else {
                value.get("data").is_none()
                    && value["errors"].as_array().is_some_and(|a| !a.is_empty())
            };
        if !valid {
            return Err(Error::OutcomeUnknown);
        }
        Ok(value)
    }
}
