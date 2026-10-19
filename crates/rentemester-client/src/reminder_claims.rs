//! Explicit reminder/claim operations. Native rules, account allocation and current evidence remain server-owned.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object},
};
use reqwest::Method;
use serde_json::{Value, json};
pub const MAX_REMINDER_CLAIM_BODY: usize = 16 * 1024;
#[derive(Clone, Copy)]
pub enum ReminderClaimMutation {
    Register,
    Post,
    Settle,
}
pub fn reminder_claim_body_schema(op: ReminderClaimMutation) -> Value {
    let id = json!({"type":"integer","minimum":1,"maximum":9007199254740991u64});
    let date = json!({"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"});
    let text = json!({"type":"string","minLength":1});
    match op {
        ReminderClaimMutation::Register => object(
            json!({"invoiceDocumentId":id,"reminderDate":date,"feeAmount":{"type":["number","null"]},"note":{"type":["string","null"],"maxLength":4096},"confirm":{"const":true}}),
            &["invoiceDocumentId", "reminderDate", "confirm"],
        ),
        ReminderClaimMutation::Post => object(
            json!({"invoiceDocumentId":id,"reminderId":id,"transactionDate":date,"receivableAccountNo":text,"reminderIncomeAccountNo":text,"confirm":{"const":true}}),
            &[
                "invoiceDocumentId",
                "reminderId",
                "receivableAccountNo",
                "reminderIncomeAccountNo",
                "confirm",
            ],
        ),
        ReminderClaimMutation::Settle => object(
            json!({"invoiceDocumentId":id,"bankTransactionId":id,"paymentDate":date,"amount":{"type":["number","null"]},"receivableAccountNo":text,"confirm":{"const":true}}),
            &["invoiceDocumentId", "bankTransactionId", "confirm"],
        ),
    }
}
impl ApiClient {
    pub async fn invoice_reminder_claims(&self, company: &str, id: i64) -> Result<Value, Error> {
        if !(1..=9007199254740991).contains(&id) {
            return Err(Error::InvalidInput("positive exact invoice id required"));
        }
        self.get(
            &company_path(company, &format!("invoices/{id}/reminder-claims"))?,
            &[],
        )
        .await
    }
    /// Exact body once. Unknown outcomes require canonical claim/journal/bank reads, never automatic retry.
    pub async fn reminder_claim_mutation(
        &self,
        company: &str,
        op: ReminderClaimMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit reminder/claim confirmation required",
            ));
        }
        if bytes.len() > MAX_REMINDER_CLAIM_BODY {
            return Err(Error::InvalidInput(
                "reminder/claim body exceeds native16KiB",
            ));
        }
        body(&bytes, &reminder_claim_body_schema(op))?;
        let path = match op {
            ReminderClaimMutation::Register => "invoices/reminders/register",
            ReminderClaimMutation::Post => "invoices/reminders/post",
            ReminderClaimMutation::Settle => "invoices/claims/settle",
        };
        self.request(
            Method::POST,
            &company_path(company, path)?,
            &[],
            Some(bytes),
        )
        .await
    }
}
