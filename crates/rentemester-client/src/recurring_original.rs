//! Original structured recurring operations over the common server-owned engine.
use crate::{ApiClient, Error, company_path, domain::object};
use reqwest::Method;
use serde_json::{Value, json};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginalRecurringKind {
    Create,
    Generate,
}
impl OriginalRecurringKind {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Generate => "generate",
        }
    }
}
/// Original InvoicePayload: totals are source assertions, never recomputed by a client.
pub fn recurring_original_invoice_schema() -> Value {
    let text = json!({"type":"string"});
    let num = json!({"type":"number"});
    let party = object(
        json!({"name":text,"address":text,"vatOrCvr":text,"eanNumber":text,"publicRecipient":{"type":"boolean"},"buyerReference":text,"orderReference":text}),
        &[],
    );
    let line = object(
        json!({"description":text,"quantity":num,"unitPriceExVat":num,"lineTotalExVat":num,"unitCode":text,"taxClassification":text,"vatRate":num,"reverseChargeBasis":text}),
        &[],
    );
    let totals = object(
        json!({"netAmount":num,"vatRate":num,"vatAmount":num,"grossAmount":num,"fxRateToDkk":num,"netAmountDkk":num,"vatAmountDkk":num,"grossAmountDkk":num,"vatComputationBasis":text}),
        &[],
    );
    object(
        json!({"invoiceType":{"type":"string","enum":["full","simplified"]},"vatTreatment":{"type":"string","enum":["standard","domestic_reverse_charge","foreign_reverse_charge"]},"seller":party,"buyer":party,"lines":{"type":"array","items":line},"totals":totals,"reverseChargeBasis":text,"reverseChargeNote":text,"currency":text,"unitCode":text,"issueDate":text,"invoiceNumber":text,"dueDate":text,"deliveryDate":text,"deliveryPeriodStart":text,"deliveryPeriodEnd":text}),
        &["invoiceType"],
    )
}
pub fn recurring_original_schema(kind: OriginalRecurringKind) -> Value {
    let text = json!({"type":"string"});
    match kind {
        OriginalRecurringKind::Create => object(
            json!({"name":text,"interval":{"type":"string","enum":["weekly","monthly","quarterly","yearly"]},"intervalCount":{"type":"integer","minimum":1,"maximum":120},"deliveryChannel":{"type":"string","enum":["manual","email","digisense"]},"firstIssueDate":text,"invoice":recurring_original_invoice_schema(),"paymentTermsDays":{"type":"integer","minimum":0,"maximum":365},"deliveryPeriodMode":{"type":"string","enum":["issue_month","interval_window","none"]},"notes":text,"confirm":{"const":true}}),
            &["name", "interval", "firstIssueDate", "invoice", "confirm"],
        ),
        OriginalRecurringKind::Generate => object(
            json!({"templateId":{"type":"integer","minimum":1,"maximum":9007199254740991u64},"asOfDate":text,"confirm":{"const":true}}),
            &["templateId", "asOfDate", "confirm"],
        ),
    }
}
pub fn validate_recurring_original(kind: OriginalRecurringKind, bytes: &[u8]) -> Result<(), Error> {
    crate::domain::body(bytes, &recurring_original_schema(kind))
}
fn checked(value: Value, mutation: bool) -> Result<Value, Error> {
    let protocol = || {
        if mutation {
            Error::OutcomeUnknown
        } else {
            Error::Protocol
        }
    };
    if !value["ok"].is_boolean()
        || !value["errors"]
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string))
    {
        return Err(protocol());
    }
    if value["ok"] == false {
        if value["errors"].as_array().is_none_or(Vec::is_empty) {
            return Err(protocol());
        }
        return Ok(value);
    }
    if !value["errors"].as_array().is_some_and(Vec::is_empty) {
        return Err(protocol());
    }
    if mutation {
        if !value["templateId"].as_i64().is_some_and(|n| n > 0)
            || !value["appliedRules"]
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_string))
        {
            return Err(protocol());
        }
    } else if !value["rows"]
        .as_array()
        .is_some_and(|a| Some(a.len() as u64) == value["count"].as_u64())
    {
        return Err(protocol());
    }
    Ok(value)
}
impl ApiClient {
    pub async fn recurring_original_list(
        &self,
        company: &str,
        include_inactive: bool,
    ) -> Result<Value, Error> {
        let value = self
            .get(
                &company_path(company, "recurring-invoices/original/list")?,
                &[(
                    "includeInactive",
                    if include_inactive { "true" } else { "false" },
                )],
            )
            .await?;
        checked(value, false)
    }
    /// Exact validated source payload once. Unknown writes require canonical reads, never replay.
    pub async fn recurring_original(
        &self,
        company: &str,
        kind: OriginalRecurringKind,
        bytes: Vec<u8>,
    ) -> Result<Value, Error> {
        validate_recurring_original(kind, &bytes)?;
        let value = self
            .request(
                Method::POST,
                &company_path(
                    company,
                    &format!("recurring-invoices/original/{}", kind.suffix()),
                )?,
                &[],
                Some(bytes),
            )
            .await
            .map_err(|error| {
                if error == (Error::Refused { status: 408 }) {
                    Error::OutcomeUnknown
                } else {
                    error
                }
            })?;
        checked(value, true)
    }
}
