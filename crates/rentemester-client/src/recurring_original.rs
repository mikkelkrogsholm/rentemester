//! Original structured recurring operations over the common server-owned engine.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path, domain::object};
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
        &["description"],
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
        if value.get("code").is_some_and(|v| !v.is_string()) {
            return Err(protocol());
        }
        return Ok(value);
    }
    if !value["errors"].as_array().is_some_and(Vec::is_empty) {
        return Err(protocol());
    }
    if mutation {
        if value["templateId"].as_i64().is_none_or(|n| n <= 0)
            || !value["appliedRules"]
                .as_array()
                .is_some_and(|a| a.iter().all(Value::is_string))
        {
            return Err(protocol());
        }
    } else if value["rows"]
        .as_array()
        .is_none_or(|a| Some(a.len() as u64) != value["count"].as_u64())
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
        self.recurring_original_request(
            Method::GET,
            &company_path(company, "recurring-invoices/original/list")?,
            &[(
                "includeInactive",
                if include_inactive { "true" } else { "false" },
            )],
            None,
        )
        .await
    }
    /// Exact validated source payload once. Unknown writes require canonical reads, never replay.
    pub async fn recurring_original(
        &self,
        company: &str,
        kind: OriginalRecurringKind,
        bytes: Vec<u8>,
    ) -> Result<Value, Error> {
        validate_recurring_original(kind, &bytes)?;
        self.recurring_original_request(
            Method::POST,
            &company_path(
                company,
                &format!("recurring-invoices/original/{}", kind.suffix()),
            )?,
            &[],
            Some(bytes),
        )
        .await
    }

    // Family-local bounded transport: never reflect an unvalidated HTTP error body.
    async fn recurring_original_request(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<Vec<u8>>,
    ) -> Result<Value, Error> {
        let mutation = body.is_some();
        let mut response = self
            .send(
                method,
                path,
                query,
                body,
                self.cookie.as_ref().map(|v| v.as_str()),
            )
            .await?;
        let status = response.status();
        if mutation && (status.is_server_error() || status.as_u16() == 408) {
            return Err(Error::OutcomeUnknown);
        }
        let refusal = status.as_u16() == 422;
        if !status.is_success() && !refusal {
            return Err(Error::Refused {
                status: status.as_u16(),
            });
        }
        let protocol = || {
            if refusal {
                Error::Refused { status: 422 }
            } else if mutation {
                Error::OutcomeUnknown
            } else {
                Error::Protocol
            }
        };
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| {
            if refusal {
                Error::Refused { status: 422 }
            } else if mutation {
                Error::OutcomeUnknown
            } else {
                Error::Transport
            }
        })? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(protocol());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| protocol())?;
        if refusal {
            let valid = value["ok"] == false
                && value["code"].as_str().is_some_and(|s| !s.trim().is_empty())
                && value["errors"]
                    .as_array()
                    .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_string))
                && value
                    .get("appliedRules")
                    .is_none_or(|v| v.as_array().is_some_and(|a| a.iter().all(Value::is_string)))
                && [
                    "data",
                    "rows",
                    "count",
                    "created",
                    "documentId",
                    "invoiceNumber",
                    "periodIndex",
                    "issueDate",
                    "dueDate",
                    "deliveryPeriodStart",
                    "deliveryPeriodEnd",
                ]
                .iter()
                .all(|key| value.get(*key).is_none());
            if !valid {
                return Err(protocol());
            }
        }
        checked(value, mutation).map_err(|_| protocol())
    }
}

/// Explicit workspace assertion only; never a caller-selected server filesystem path.
pub fn recurring_workspace_schema() -> Value {
    object(
        json!({"workspace":{"type":"string","minLength":1},"asOfDate":{"type":"string","minLength":1},"maxGenerations":{"type":"integer","minimum":1,"maximum":5000},"confirm":{"const":true}}),
        &["workspace", "asOfDate", "confirm"],
    )
}
pub fn validate_recurring_workspace(bytes: &[u8]) -> Result<(), Error> {
    crate::domain::body(bytes, &recurring_workspace_schema())
}
fn checked_workspace(value: Value) -> Result<Value, Error> {
    let errors = value["errors"]
        .as_array()
        .filter(|a| a.iter().all(Value::is_string));
    let companies = value["companies"].as_array();
    if !value["ok"].is_boolean()
        || !value["hasMore"].is_boolean()
        || value["remainingGenerations"].as_u64().is_none()
        || errors.is_none_or(|a| a.is_empty() != (value["ok"] == true))
        || companies.is_none_or(|a| {
            a.iter().any(|v| {
                v["slug"].as_str().is_none_or(str::is_empty)
                    || !matches!(
                        v["status"].as_str(),
                        Some("processed" | "skipped" | "failed")
                    )
            })
        })
    {
        return Err(Error::OutcomeUnknown);
    }
    if let Some(continuation) = value.get("continuation")
        && !continuation.is_object()
    {
        return Err(Error::OutcomeUnknown);
    }
    Ok(value)
}
impl ApiClient {
    /// One explicit capped run. Partial failure is data, never a reason to retry.
    pub async fn recurring_run_workspace(&self, bytes: Vec<u8>) -> Result<Value, Error> {
        validate_recurring_workspace(&bytes)?;
        if self.service_key.is_some() {
            return Err(Error::InvalidInput(
                "workspace recurring run requires current human cookie authority",
            ));
        }
        let value = self
            .request(
                Method::POST,
                "/api/recurring-invoices/run-workspace",
                &[],
                Some(bytes),
            )
            .await
            .map_err(|e| {
                if e == (Error::Refused { status: 408 }) {
                    Error::OutcomeUnknown
                } else {
                    e
                }
            })?;
        checked_workspace(value)
    }
}
