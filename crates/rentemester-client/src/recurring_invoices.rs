//! Explicit manual invoice templates. The server owns periods, issuance and deduplication.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, invoice_body_schema, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_RECURRING_BODY: usize = 256 * 1024;
#[derive(Clone, Copy, Debug)]
pub enum RecurringInvoiceMutation {
    Create,
    Generate(i64),
    Retire(i64),
}
impl RecurringInvoiceMutation {
    fn path(self) -> Result<String, Error> {
        match self {
            Self::Create => Ok("recurring-invoices".into()),
            Self::Generate(id) | Self::Retire(id) => {
                if !(1..=9_007_199_254_740_991).contains(&id) {
                    return Err(Error::InvalidInput("positive exact template id required"));
                }
                Ok(format!(
                    "recurring-invoices/{id}/{}",
                    if matches!(self, Self::Generate(_)) {
                        "generate"
                    } else {
                        "retire"
                    }
                ))
            }
        }
    }
}
pub fn recurring_invoice_body_schema(op: RecurringInvoiceMutation) -> Value {
    let date = json!({"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"});
    match op {
        RecurringInvoiceMutation::Generate(_) => object(
            json!({"asOfDate":date,"confirm":{"const":true}}),
            &["asOfDate", "confirm"],
        ),
        RecurringInvoiceMutation::Retire(_) => object(
            json!({"reason":{"type":["string","null"],"maxLength":4096},"confirm":{"const":true}}),
            &["confirm"],
        ),
        RecurringInvoiceMutation::Create => {
            let invoice = invoice_body_schema();
            object(
                json!({"name":{"type":"string","minLength":1,"maxLength":256},"interval":{"type":"string","enum":["weekly","monthly","quarterly","yearly"]},"intervalCount":{"type":["integer","null"],"minimum":1,"maximum":120},"deliveryChannel":{"type":["string","null"],"enum":["manual",null]},"firstIssueDate":date,"paymentTermsDays":{"type":"integer","minimum":0,"maximum":365},"deliveryPeriodMode":{"type":["string","null"],"enum":["none","interval_window","issue_month",null]},"notes":{"type":["string","null"],"maxLength":4096},"vatRatePercent":{"type":"number"},"currency":invoice["properties"]["currency"],"customerId":invoice["properties"]["customerId"],"buyer":invoice["properties"]["buyer"],"lines":invoice["properties"]["lines"],"confirm":{"const":true}}),
                &[
                    "name",
                    "interval",
                    "firstIssueDate",
                    "paymentTermsDays",
                    "vatRatePercent",
                    "lines",
                    "confirm",
                ],
            )
        }
    }
}
impl ApiClient {
    pub async fn recurring_invoices(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "recurring-invoices")?, &[])
            .await
    }
    /// Exact bytes once, no caller keys or retries. Generate has only the server's template/period
    /// deduplication contract; after uncertainty read canonical templates/invoices before further action.
    pub async fn recurring_invoice_mutation(
        &self,
        company: &str,
        op: RecurringInvoiceMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit recurring invoice confirmation required",
            ));
        }
        let path = op.path()?;
        if bytes.len() > MAX_RECURRING_BODY {
            return Err(Error::InvalidInput(
                "recurring invoice body exceeds native 256KiB",
            ));
        }
        body(&bytes, &recurring_invoice_body_schema(op))?;
        self.request(
            Method::POST,
            &company_path(company, &path)?,
            &[],
            Some(bytes),
        )
        .await
    }
}
