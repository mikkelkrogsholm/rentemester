//! Fixed public-invoice handoff over immutable issued snapshots; never local provider or accounting logic.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_PUBLIC_INVOICE_BODY: usize = 16 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicInvoiceMutation {
    Register,
    Send,
    RefreshStatus,
}
pub fn public_invoice_body_schema(action: PublicInvoiceMutation) -> Value {
    let mut fields = json!({"confirm":{"const":true}});
    let mut required = vec!["confirm"];
    if action != PublicInvoiceMutation::Register {
        fields["invoiceDocumentId"] =
            json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
        required.push("invoiceDocumentId");
    }
    object(fields, &required)
}
impl ApiClient {
    pub async fn public_invoice_config(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "invoices/public-config")?, &[])
            .await
    }
    pub async fn public_invoice_submissions(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "invoices/public-submissions")?, &[])
            .await
    }
    /// Sends exact reviewed body once. HTTP success is not a delivery assertion:
    /// retain canonical state/transmissionId/redeliveryBlocked and read before any further action.
    pub async fn mutate_public_invoice(
        &self,
        company: &str,
        action: PublicInvoiceMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed || bytes.len() > MAX_PUBLIC_INVOICE_BODY {
            return Err(Error::InvalidInput(
                "explicit public-invoice confirmation and16KiB body bound required",
            ));
        }
        body(&bytes, &public_invoice_body_schema(action))?;
        if action != PublicInvoiceMutation::Register {
            let value: Value = serde_json::from_slice(&bytes)
                .map_err(|_| Error::InvalidInput("native public-invoice JSON required"))?;
            if value["invoiceDocumentId"]
                .as_i64()
                .is_none_or(|id| !(1..=9_007_199_254_740_991).contains(&id))
            {
                return Err(Error::InvalidInput(
                    "positive safe invoiceDocumentId required",
                ));
            }
        }
        let path = match action {
            PublicInvoiceMutation::Register => "invoices/public-register",
            PublicInvoiceMutation::Send => "invoices/send-public",
            PublicInvoiceMutation::RefreshStatus => "invoices/send-public/status",
        };
        self.request(
            Method::POST,
            &company_path(company, path)?,
            &[],
            Some(bytes),
        )
        .await
        .map_err(|e| match e {
            Error::Refused { status: 408 } => Error::OutcomeUnknown,
            other => other,
        })
    }
}
