//! Existing bank and supplier-bill endpoints; accounting/evidence stays on server.
use crate::{
    ApiClient, Error, company_path,
    domain::{body, object, optional_text},
};
use reqwest::Method;
use serde_json::{Value, json};
#[derive(Clone, Copy, Debug)]
pub enum BankMutation {
    CreateAccount,
    ImportStatement,
    RegisterPayable,
    PayPayable(i64),
    MatchExpense,
}
pub fn bank_body_schema(operation: BankMutation) -> Value {
    let id = json!({"type":"integer","minimum":1});
    let date = json!({"type":"string","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"});
    let account = json!({"type":"string","minLength":1,"maxLength":128});
    let treatment = json!({"type":["string","null"],"enum":["standard","exempt","non_deductible",null],"description":"Server verifies company VAT registration and immutable document amounts"});
    match operation {
        BankMutation::CreateAccount => object(
            json!({"name":{"type":"string","minLength":1,"maxLength":256},"slug":optional_text(),"bankName":optional_text(),"registrationNo":optional_text(),"accountNo":optional_text(),"iban":optional_text(),"bic":optional_text(),"accountOwner":optional_text(),"customerNo":optional_text(),"currency":optional_text(),"ledgerAccountNo":optional_text()}),
            &["name"],
        ),
        BankMutation::ImportStatement => object(
            json!({"confirm":{"const":true},"csvContent":{"type":"string","minLength":1,"maxLength":240000,"description":"Generic UTF-8 CSV, comma/semicolon/tab separated. Required headers date,text,amount; ISO dates. Optional currency,reference,booking_date,balance_after,amount_dkk,fx_rate_to_dkk. Example: date;text;amount;currency\\n2026-01-01;Synthetic payment;-125;DKK\\n. Server validates numbers and duplicate identity, and archives original CSV."},"account":{"type":["string","null"],"maxLength":128,"description":"Existing active bank account id or slug from bank_accounts; ledger mapping constrains later payment account"},"profile":{"type":["string","null"],"maxLength":0,"description":"Named bank profiles unavailable; omit this field"}}),
            &["csvContent", "confirm"],
        ),
        BankMutation::RegisterPayable => object(
            json!({"documentId":id,"billDate":date,"dueDate":date,"expenseAccountNo":account,"vatTreatment":treatment,"vendorId":{"type":["integer","null"],"minimum":1},"note":optional_text(),"confirm":{"const":true}}),
            &[
                "documentId",
                "billDate",
                "dueDate",
                "expenseAccountNo",
                "confirm",
            ],
        ),
        BankMutation::PayPayable(_) => object(
            json!({"bankTransactionId":id,"paymentDate":optional_text(),"amount":{"type":["number","null"],"exclusiveMinimum":0,"description":"If supplied, equals full magnitude of outgoing DKK bank row; server prevents overpayment"},"paymentAccountNo":optional_text(),"note":optional_text(),"confirm":{"const":true}}),
            &["bankTransactionId", "confirm"],
        ),
        BankMutation::MatchExpense => object(
            json!({"documentId":id,"bankTransactionId":id,"expenseAccountNo":account,"vatTreatment":treatment,"paymentAccountNo":optional_text(),"transactionDate":optional_text(),"text":optional_text(),"confirm":{"const":true}}),
            &[
                "documentId",
                "bankTransactionId",
                "expenseAccountNo",
                "confirm",
            ],
        ),
    }
}
impl ApiClient {
    pub async fn bank_accounts(&self, company: &str) -> Result<Value, Error> {
        self.get(&company_path(company, "bank-accounts")?, &[])
            .await
    }
    pub async fn bank(&self, company: &str, year: Option<&str>) -> Result<Value, Error> {
        if year.is_some_and(|s| s.len() > 128) {
            return Err(Error::InvalidInput("bank year selector too long"));
        }
        let query = year.map(|y| vec![("year", y)]).unwrap_or_default();
        self.get(&company_path(company, "bank")?, &query).await
    }
    pub async fn payables(
        &self,
        company: &str,
        status: Option<&str>,
        as_of: Option<&str>,
    ) -> Result<Value, Error> {
        if status.is_some_and(|s| !["open", "paid", "overdue", "all"].contains(&s))
            || as_of.is_some_and(|s| s.len() > 32)
        {
            return Err(Error::InvalidInput("invalid payable status/asOf filter"));
        }
        let mut query = vec![];
        if let Some(s) = status {
            query.push(("status", s));
        }
        if let Some(s) = as_of {
            query.push(("asOf", s));
        }
        self.get(&company_path(company, "payables")?, &query).await
    }
    pub async fn expense_booking_options(
        &self,
        company: &str,
        document_id: i64,
    ) -> Result<Value, Error> {
        if document_id <= 0 {
            return Err(Error::InvalidInput("positive document id required"));
        }
        self.get(
            &company_path(company, &format!("documents/{document_id}/booking-options"))?,
            &[],
        )
        .await
    }
    /// Exact body, explicit confirmation, one attempt. No generic idempotency keys.
    /// On uncertainty reconcile bank/payables/journal/documents before further action.
    pub async fn bank_mutation(
        &self,
        company: &str,
        operation: BankMutation,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput(
                "explicit bank/payable mutation confirmation required",
            ));
        }
        body(&bytes, &bank_body_schema(operation))?;
        let path = match operation {
            BankMutation::CreateAccount => "bank-accounts".to_owned(),
            BankMutation::ImportStatement => "bank/import".into(),
            BankMutation::RegisterPayable => "payables".into(),
            BankMutation::PayPayable(id) => {
                if id <= 0 {
                    return Err(Error::InvalidInput("positive payable id required"));
                }
                format!("payables/{id}/pay")
            }
            BankMutation::MatchExpense => {
                if bytes.len() > 16 * 1024 {
                    return Err(Error::InvalidInput(
                        "expense match exceeds native16384-byte body limit",
                    ));
                }
                "documents/book-expense".into()
            }
        };
        self.request(
            Method::POST,
            &company_path(company, &path)?,
            &[],
            Some(bytes),
        )
        .await
    }
}
