//! Original invoice reads; filters, histories, aging and currencies remain server-owned.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::{Number, Value, json};
#[derive(Clone, Copy, Debug)]
pub enum InvoiceOriginalKind {
    List,
    Find,
    Status,
    Overdue,
}
#[derive(Clone, Default, Debug)]
pub struct InvoiceOriginalQuery {
    pub status: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub customer_cvr: Option<String>,
    pub customer: Option<String>,
    pub invoice_number: Option<String>,
    pub query: Option<String>,
    pub as_of: Option<String>,
    pub min_amount: Option<Number>,
    pub max_amount: Option<Number>,
    pub amount: Option<Number>,
    pub min_days: Option<u64>,
    pub document_id: Option<i64>,
}
pub fn invoice_original_schema(kind: InvoiceOriginalKind) -> Value {
    let mut fields = json!({"asOf":{"type":"string"}});
    match kind {
        InvoiceOriginalKind::List => {
            fields["status"] = json!({"type":"string","enum":["open","paid","credited","refunded","overpaid","written_off","overdue","all"]});
            for key in ["from", "to", "customerCvr", "customer", "invoiceNumber"] {
                fields[key] = json!({"type":"string"});
            }
            for key in ["minAmount", "maxAmount"] {
                fields[key] = json!({"type":"number"});
            }
        }
        InvoiceOriginalKind::Find => {
            for key in ["query", "customer", "invoiceNumber"] {
                fields[key] = json!({"type":"string"});
            }
            for key in ["amount", "minAmount", "maxAmount"] {
                fields[key] = json!({"type":"number"});
            }
        }
        InvoiceOriginalKind::Status => {
            fields["documentId"] =
                json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
            fields["invoiceNumber"] = json!({"type":"string"});
        }
        InvoiceOriginalKind::Overdue => {
            fields["minDays"] = json!({"type":"integer","minimum":0,"maximum":9007199254740991u64});
        }
    }
    json!({"type":"object","properties":fields,"additionalProperties":false})
}
impl ApiClient {
    /// One GET; no read retries, local fallback, financial calculations or fiscal-year default.
    pub async fn invoice_original(
        &self,
        company: &str,
        kind: InvoiceOriginalKind,
        query: &InvoiceOriginalQuery,
    ) -> Result<Value, Error> {
        if query
            .document_id
            .is_some_and(|n| !(1..=9007199254740991).contains(&n))
            || query.min_days.is_some_and(|n| n > 9007199254740991)
        {
            return Err(Error::InvalidInput(
                "positive safe documentId/nonnegative safe minDays required",
            ));
        }
        let mut pairs = vec![];
        for (key, value) in [
            ("status", query.status.as_deref()),
            ("from", query.from.as_deref()),
            ("to", query.to.as_deref()),
            ("customerCvr", query.customer_cvr.as_deref()),
            ("customer", query.customer.as_deref()),
            ("invoiceNumber", query.invoice_number.as_deref()),
            ("query", query.query.as_deref()),
            ("asOf", query.as_of.as_deref()),
        ] {
            if let Some(value) = value {
                pairs.push((key, value.to_owned()));
            }
        }
        for (key, value) in [
            ("minAmount", query.min_amount.as_ref()),
            ("maxAmount", query.max_amount.as_ref()),
            ("amount", query.amount.as_ref()),
        ] {
            if let Some(value) = value {
                pairs.push((key, value.to_string()));
            }
        }
        if let Some(value) = query.min_days {
            pairs.push(("minDays", value.to_string()));
        }
        if let Some(value) = query.document_id {
            pairs.push(("documentId", value.to_string()));
        }
        let schema = invoice_original_schema(kind);
        if pairs
            .iter()
            .any(|(key, _)| schema["properties"].get(*key).is_none())
            || pairs
                .iter()
                .map(|(k, v)| k.len() + v.len() + 2)
                .sum::<usize>()
                > 4096
        {
            return Err(Error::InvalidInput(
                "irrelevant invoice selector or query exceeds4KiB",
            ));
        }
        let action = match kind {
            InvoiceOriginalKind::List => "list",
            InvoiceOriginalKind::Find => "find",
            InvoiceOriginalKind::Status => "status",
            InvoiceOriginalKind::Overdue => "overdue",
        };
        let refs = pairs
            .iter()
            .map(|(k, v)| (*k, v.as_str()))
            .collect::<Vec<_>>();
        let mut response = self
            .send(
                Method::GET,
                &company_path(company, &format!("invoices/original/{action}"))?,
                &refs,
                None,
                self.cookie.as_ref().map(|v| v.as_str()),
            )
            .await?;
        let status = response.status();
        let business_refusal = status.as_u16() == 422;
        if !status.is_success() && !business_refusal {
            return Err(Error::Refused {
                status: status.as_u16(),
            });
        }
        // Only complete, bounded business receipts may cross the HTTP error boundary.
        let protocol = || {
            if business_refusal {
                Error::Refused { status: 422 }
            } else {
                Error::Protocol
            }
        };
        let mut bytes = vec![];
        while let Some(chunk) = response.chunk().await.map_err(|_| {
            if business_refusal {
                Error::Refused { status: 422 }
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
        let valid_refusal = !business_refusal
            || (value["ok"] == false
                && value["code"]
                    .as_str()
                    .is_some_and(|code| !code.trim().is_empty()));
        if !valid || !valid_refusal {
            return Err(protocol());
        }
        Ok(value)
    }
}
