//! Exact native domain contracts. These endpoints have no server idempotency keys.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_DOMAIN_BODY: usize = 262_144;
#[derive(Clone, Copy, Debug)]
pub enum DomainMutation {
    CreateCustomer,
    UpdateCustomer(i64),
    DeleteCustomer(i64),
    CreateVendor,
    UpdateVendor(i64),
    DeleteVendor(i64),
    IssueInvoice,
    PostInvoice,
}
/// Caller attestations, never injected into the native JSON body.
#[derive(Clone, Copy, Debug)]
pub struct Confirmation {
    pub confirm: bool,
    pub invoice_preview_reviewed: bool,
}
impl DomainMutation {
    fn route(self) -> Result<(Method, String), Error> {
        Ok(match self {
            Self::CreateCustomer => (Method::POST, "customers".into()),
            Self::CreateVendor => (Method::POST, "vendors".into()),
            Self::UpdateCustomer(id) | Self::UpdateVendor(id) => {
                if id <= 0 {
                    return Err(Error::InvalidInput("positive record id required"));
                }
                (
                    Method::PATCH,
                    format!(
                        "{}/{id}",
                        if matches!(self, Self::UpdateCustomer(_)) {
                            "customers"
                        } else {
                            "vendors"
                        }
                    ),
                )
            }
            Self::DeleteCustomer(id) | Self::DeleteVendor(id) => {
                if id <= 0 {
                    return Err(Error::InvalidInput("positive record id required"));
                }
                (
                    Method::DELETE,
                    format!(
                        "{}/{id}",
                        if matches!(self, Self::DeleteCustomer(_)) {
                            "customers"
                        } else {
                            "vendors"
                        }
                    ),
                )
            }
            Self::IssueInvoice => (Method::POST, "invoices/issue".into()),
            Self::PostInvoice => (Method::POST, "invoices/post".into()),
        })
    }
}
pub(crate) fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
pub(crate) fn optional_text() -> Value {
    json!({"type":["string","null"],"maxLength":4096})
}
/// Discoverable wire body schema; this validates shape, the server owns domain policy.
pub fn domain_body_schema(operation: DomainMutation) -> Value {
    if matches!(
        operation,
        DomainMutation::DeleteCustomer(_) | DomainMutation::DeleteVendor(_)
    ) {
        return object(json!({"confirm":{"const":true}}), &["confirm"]);
    }
    if matches!(operation, DomainMutation::PostInvoice) {
        return object(
            json!({"invoiceDocumentId":{"type":"integer","minimum":1},"revenueAccountNo":{"type":"string","minLength":1,"maxLength":128},"confirm":{"const":true}}),
            &["invoiceDocumentId", "revenueAccountNo", "confirm"],
        );
    }
    if matches!(operation, DomainMutation::IssueInvoice) {
        return invoice_body_schema();
    }
    let vendor = matches!(
        operation,
        DomainMutation::CreateVendor | DomainMutation::UpdateVendor(_)
    );
    let mut fields = json!({"name":{"type":"string","minLength":1,"maxLength":256},"address":optional_text(),"vatOrCvr":optional_text(),"email":optional_text(),"phone":optional_text(),"website":optional_text(),"notes":optional_text()});
    if vendor {
        for key in [
            "countryCode",
            "identifierKind",
            "defaultExpenseAccount",
            "defaultVatTreatment",
        ] {
            fields[key] = optional_text();
        }
    } else {
        fields["eanNumber"] = optional_text();
        fields["paymentTermsDays"] = json!({"type":["integer","null"],"minimum":1,"maximum":365});
        fields["defaultCurrency"] = if matches!(operation, DomainMutation::UpdateCustomer(_)) {
            json!({"type":"string","minLength":3,"maxLength":3,"description":"Uppercase ISO currency; null cannot clear a customer default"})
        } else {
            optional_text()
        };
    }
    let mut schema = object(
        fields,
        if matches!(
            operation,
            DomainMutation::CreateCustomer | DomainMutation::CreateVendor
        ) {
            &["name"]
        } else {
            &[]
        },
    );
    schema["minProperties"] = json!(1);
    schema
}
pub fn invoice_body_schema() -> Value {
    let party = object(
        json!({"name":optional_text(),"address":optional_text(),"vatOrCvr":optional_text()}),
        &[],
    );
    let mut nullable_party = party;
    nullable_party["type"] = json!(["object", "null"]);
    let line = object(
        json!({"description":{"type":"string","minLength":1,"maxLength":1000},"quantity":{"type":"number","exclusiveMinimum":0},"unitPriceExVat":{"type":"number","minimum":0}}),
        &["description", "quantity", "unitPriceExVat"],
    );
    object(
        json!({"issueDate":{"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"},"lines":{"type":"array","minItems":1,"maxItems":200,"items":line},"vatRatePercent":{"type":["number","null"],"description":"Native issuance supports25% only"},"customerId":{"type":["integer","null"],"minimum":1},"invoiceNumber":optional_text(),"dueDate":optional_text(),"currency":optional_text(),"seller":nullable_party,"buyer":nullable_party}),
        &["issueDate", "lines"],
    )
}
fn matches_type(value: &Value, kind: &str) -> bool {
    match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => false,
    }
}
fn validate(value: &Value, schema: &Value) -> bool {
    if schema["enum"]
        .as_array()
        .is_some_and(|allowed| !allowed.contains(value))
    {
        return false;
    }
    if let Some(expected) = schema.get("const") {
        return value == expected;
    }
    let types = &schema["type"];
    if !(types.as_str().is_some_and(|s| matches_type(value, s))
        || types.as_array().is_some_and(|types| {
            types
                .iter()
                .any(|t| t.as_str().is_some_and(|s| matches_type(value, s)))
        }))
    {
        return false;
    }
    if let Some(fields) = value.as_object() {
        if fields.len() < schema["minProperties"].as_u64().unwrap_or(0) as usize {
            return false;
        }
        if schema["required"].as_array().is_some_and(|required| {
            required
                .iter()
                .any(|key| !fields.contains_key(key.as_str().unwrap()))
        }) {
            return false;
        }
        return fields.iter().all(|(key, value)| {
            schema["properties"]
                .get(key)
                .is_some_and(|property| validate(value, property))
        });
    }
    if let Some(items) = value.as_array() {
        return items.len() >= schema["minItems"].as_u64().unwrap_or(0) as usize
            && items.len() <= schema["maxItems"].as_u64().unwrap_or(u64::MAX) as usize
            && items.iter().all(|v| validate(v, &schema["items"]));
    }
    if let Some(text) = value.as_str() {
        return text.len() >= schema["minLength"].as_u64().unwrap_or(0) as usize
            && text.len() <= schema["maxLength"].as_u64().unwrap_or(u64::MAX) as usize;
    }
    if let Some(number) = value.as_f64() {
        return schema["minimum"].as_f64().is_none_or(|n| number >= n)
            && schema["maximum"].as_f64().is_none_or(|n| number <= n)
            && schema["exclusiveMinimum"]
                .as_f64()
                .is_none_or(|n| number > n);
    }
    true
}
pub(crate) fn body(bytes: &[u8], schema: &Value) -> Result<(), Error> {
    if bytes.len() > MAX_DOMAIN_BODY {
        return Err(Error::InvalidInput("domain request exceeds262144 bytes"));
    }
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| Error::InvalidInput("expected native JSON object"))?;
    if !validate(&value, schema) {
        return Err(Error::InvalidInput(
            "native body shape differs from discoverable domain schema",
        ));
    }
    Ok(())
}
impl ApiClient {
    /// Canonical seller fields for an exact reviewed invoice source snapshot.
    pub async fn company_profile(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "company")?, &[]).await
    }
    pub async fn accounts(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "accounts")?, &[]).await
    }
    pub async fn contacts(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "contacts")?, &[]).await
    }
    pub async fn documents(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "documents")?, &[]).await
    }
    pub async fn invoices(&self, company: &str, year: Option<&str>) -> Result<Value, Error> {
        let query = year.map(|year| vec![("year", year)]).unwrap_or_default();
        self.get(&company_path(company, "invoices")?, &query).await
    }
    /// Send caller bytes once. No idempotency support exists here: on uncertainty,
    /// inspect contacts/invoices/journal/documents before taking any further action.
    /// IssueInvoice additionally requires caller attestation of reviewing the same
    /// exact invoice input using invoice_preview; no server preview token is invented.
    pub async fn domain_mutation(
        &self,
        company: &str,
        operation: DomainMutation,
        bytes: Vec<u8>,
        confirmation: Confirmation,
    ) -> Result<Value, Error> {
        if !confirmation.confirm
            || (matches!(operation, DomainMutation::IssueInvoice)
                && !confirmation.invoice_preview_reviewed)
        {
            return Err(Error::InvalidInput(
                "explicit confirmation required; invoice issuance also requires reviewed preview of identical input",
            ));
        }
        body(&bytes, &domain_body_schema(operation))?;
        let (method, path) = operation.route()?;
        self.request(method, &company_path(company, &path)?, &[], Some(bytes))
            .await
    }
    /// Nonmutating server PDF preview. Binary bytes are returned as JSON integers
    /// for CLI/MCP portability; no local files, invoice numbers or postings are created.
    pub async fn invoice_preview(&self, company: &str, bytes: Vec<u8>) -> Result<Value, Error> {
        body(&bytes, &invoice_body_schema())?;
        let response = self
            .send(
                Method::POST,
                &company_path(company, "invoices/preview")?,
                &[],
                Some(bytes),
                self.cookie.as_ref().map(|v| v.as_str()),
            )
            .await
            .map_err(|e| {
                if e == Error::OutcomeUnknown {
                    Error::Transport
                } else {
                    e
                }
            })?;
        let pdf = read_pdf(response).await?;
        Ok(json!({"ok":true,"preview":{"contentType":"application/pdf","bytes":pdf}}))
    }
    /// Reads the immutable issued original through the current company authorization.
    pub async fn invoice_pdf(&self, company: &str, id: i64) -> Result<Value, Error> {
        if id <= 0 {
            return Err(Error::InvalidInput("positive invoice document id required"));
        }
        let response = self
            .send(
                Method::GET,
                &company_path(company, &format!("invoices/{id}/pdf"))?,
                &[],
                None,
                self.cookie.as_ref().map(|v| v.as_str()),
            )
            .await?;
        let pdf = read_pdf(response).await?;
        Ok(json!({"ok":true,"pdf":{"contentType":"application/pdf","bytes":pdf}}))
    }
}

async fn read_pdf(mut response: reqwest::Response) -> Result<Vec<u8>, Error> {
    if !response.status().is_success() {
        return Err(Error::Refused {
            status: response.status().as_u16(),
        });
    }
    if response
        .headers()
        .get("content-type")
        .and_then(|h| h.to_str().ok())
        .is_none_or(|s| s.split(';').next() != Some("application/pdf"))
    {
        return Err(Error::Protocol);
    }
    let mut pdf = vec![];
    while let Some(chunk) = response.chunk().await.map_err(|_| Error::Transport)? {
        if pdf.len().saturating_add(chunk.len()) > MAX_RESPONSE {
            return Err(Error::Protocol);
        }
        pdf.extend_from_slice(&chunk);
    }
    if !pdf.starts_with(b"%PDF-") {
        return Err(Error::Protocol);
    }
    Ok(pdf)
}
