//! Original structured invoice lifecycle; all amounts, selectors, proof and authority are server-owned.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::{Value, json};
#[derive(Clone, Copy, Debug)]
pub enum InvoiceLifecycleKind {
    Issue,
    Create,
    Render,
    Post,
    SettleBank,
    CreditNote,
    RefundBank,
    ApplyPayment,
}
fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn strings(keys: &[&str]) -> Value {
    let mut fields = json!({});
    for key in keys {
        fields[*key] = json!({"type":"string"});
    }
    fields
}
fn strict(schema: &mut Value) {
    if let Some(types) = schema.get_mut("type").and_then(Value::as_array_mut) {
        types.retain(|v| v != "null");
        if types.len() == 1 {
            schema["type"] = types[0].clone();
        }
    }
    if let Some(values) = schema.get_mut("enum").and_then(Value::as_array_mut) {
        values.retain(|v| !v.is_null());
    }
    if let Some(fields) = schema.get_mut("properties").and_then(Value::as_object_mut) {
        for field in fields.values_mut() {
            strict(field);
        }
    }
    if let Some(items) = schema.get_mut("items") {
        strict(items);
    }
}
fn issue_payload() -> Value {
    let party = object(strings(&["name", "address", "vatOrCvr"]), &[]);
    let mut buyer = party.clone();
    buyer["properties"]["eanNumber"] = json!({"type":"string"});
    buyer["properties"]["publicRecipient"] = json!({"type":"boolean"});
    let mut line = strings(&["description"]);
    for key in ["quantity", "unitPriceExVat", "lineTotalExVat"] {
        line[key] = json!({"type":"number"});
    }
    let mut totals = strings(&["vatComputationBasis"]);
    for key in [
        "netAmount",
        "vatRate",
        "vatAmount",
        "grossAmount",
        "fxRateToDkk",
        "netAmountDkk",
        "vatAmountDkk",
        "grossAmountDkk",
    ] {
        totals[key] = json!({"type":"number"});
    }
    let mut fields = strings(&[
        "issueDate",
        "invoiceNumber",
        "currency",
        "dueDate",
        "deliveryDate",
        "deliveryPeriodStart",
        "deliveryPeriodEnd",
        "reverseChargeNote",
    ]);
    fields["invoiceType"] = json!({"type":"string","enum":["full","simplified"]});
    fields["vatTreatment"] = json!({"type":"string","enum":["standard","domestic_reverse_charge","foreign_reverse_charge"]});
    fields["reverseChargeBasis"] = json!({"type":"string","enum":["DK_MOMSLOVEN_§46_STK_1_NR_3","DK_MOMSLOVEN_§46_STK_1_NR_6","DK_MOMSLOVEN_§46_STK_1_NR_7","EU_MOMSDIREKTIV_ART_196","EU_MOMSDIREKTIV_ART_199"]});
    fields["seller"] = party;
    fields["buyer"] = buyer;
    fields["lines"] = json!({"type":"array","items":object(line,&["description"])});
    fields["totals"] = object(totals, &[]);
    object(fields, &["invoiceType"])
}
pub fn invoice_lifecycle_schema(kind: InvoiceLifecycleKind) -> Value {
    let id = json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
    let mut schema = match kind {
        InvoiceLifecycleKind::Issue => object(
            json!({"payload":issue_payload(),"customerId":id}),
            &["payload"],
        ),
        InvoiceLifecycleKind::Create => {
            let mut payload = crate::invoice_body_schema();
            strict(&mut payload);
            object(json!({"payload":payload,"customerId":id}), &["payload"])
        }
        InvoiceLifecycleKind::Render | InvoiceLifecycleKind::Post => {
            let mut fields = json!({"documentId":id,"invoiceNumber":{"type":"string"}});
            if matches!(kind, InvoiceLifecycleKind::Post) {
                fields["revenueAccountNo"] = json!({"type":"string"});
            }
            object(fields, &[])
        }
        InvoiceLifecycleKind::ApplyPayment => {
            let mut fields = strings(&[
                "invoiceNumber",
                "paymentDate",
                "bankAccountNo",
                "receivableAccountNo",
                "note",
            ]);
            fields["invoiceDocumentId"] = id.clone();
            fields["bankTransactionId"] = id.clone();
            fields["journalEntryId"] = id;
            fields["amount"] = json!({"type":"number"});
            object(
                json!({"payload":object(fields,&["paymentDate","amount"])}),
                &["payload"],
            )
        }
        InvoiceLifecycleKind::CreditNote => {
            let mut fields = strings(&[
                "originalInvoiceNumber",
                "issueDate",
                "reason",
                "creditNoteNumber",
            ]);
            fields["originalInvoiceDocumentId"] = id;
            fields["grossAmount"] = json!({"type":"number"});
            object(
                json!({"payload":object(fields,&["issueDate","reason"])}),
                &["payload"],
            )
        }
        InvoiceLifecycleKind::SettleBank | InvoiceLifecycleKind::RefundBank => {
            let mut fields = strings(&[
                "invoiceNumber",
                "bankTransactionReference",
                "paymentDate",
                "bankAccountNo",
                "receivableAccountNo",
            ]);
            fields["invoiceDocumentId"] = id.clone();
            fields["bankTransactionId"] = id;
            fields["amount"] = json!({"type":"number"});
            if matches!(kind, InvoiceLifecycleKind::RefundBank) {
                fields["refundDate"] = json!({"type":"string"});
            }
            object(json!({"payload":object(fields,&[])}), &["payload"])
        }
    };
    schema["properties"]["confirm"] = json!({"const":true});
    schema["required"]
        .as_array_mut()
        .unwrap()
        .push(json!("confirm"));
    schema
}
/// Shape-only validation shared with structured MCP/CLI; server validates financial/business rules.
pub fn validate_invoice_lifecycle(kind: InvoiceLifecycleKind, body: &[u8]) -> Result<(), Error> {
    crate::domain::body(body, &invoice_lifecycle_schema(kind))
}
impl ApiClient {
    pub async fn invoice_lifecycle(
        &self,
        company: &str,
        kind: InvoiceLifecycleKind,
        body: Vec<u8>,
    ) -> Result<Value, Error> {
        validate_invoice_lifecycle(kind, &body)?;
        let render = matches!(kind, InvoiceLifecycleKind::Render);
        let action = match kind {
            InvoiceLifecycleKind::Issue => "issue",
            InvoiceLifecycleKind::Create => "create",
            InvoiceLifecycleKind::Render => "render",
            InvoiceLifecycleKind::Post => "post",
            InvoiceLifecycleKind::SettleBank => "settle-bank",
            InvoiceLifecycleKind::CreditNote => "credit-note",
            InvoiceLifecycleKind::RefundBank => "refund-bank",
            InvoiceLifecycleKind::ApplyPayment => "apply-payment",
        };
        let path = company_path(company, &format!("invoices/original/{action}"))?;
        let unknown = || {
            if render {
                Error::Transport
            } else {
                Error::OutcomeUnknown
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
                if render && e == Error::OutcomeUnknown {
                    Error::Transport
                } else {
                    e
                }
            })?;
        let status = response.status();
        if !render && (status.is_server_error() || status.as_u16() == 408) {
            return Err(Error::OutcomeUnknown);
        }
        if !status.is_success() {
            return Err(Error::Refused {
                status: status.as_u16(),
            });
        }
        let mut bytes = vec![];
        while let Some(chunk) = response.chunk().await.map_err(|_| unknown())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(unknown());
            }
            bytes.extend_from_slice(&chunk);
        }
        let protocol = || {
            if render {
                Error::Protocol
            } else {
                Error::OutcomeUnknown
            }
        };
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
        if !valid {
            return Err(protocol());
        }
        Ok(value)
    }
}
