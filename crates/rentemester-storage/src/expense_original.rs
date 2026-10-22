//! Source expense ABI mapped onto the shared native allocation and journal writer.
use crate::{
    expense,
    idempotency::{self, Failure, Operation, Outcome, PrincipalKind},
};
use rentemester_engine::{Error, fiscal_year::Date};
use rusqlite::Transaction;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub document_id: i64,
    pub bank_transaction_id: i64,
    pub expense_account: String,
    pub vat_treatment: Option<String>,
    pub payment_account: Option<String>,
    pub date: Option<String>,
    pub text: Option<String>,
    pub idempotency_key: Option<String>,
}
impl Input {
    fn native(&self) -> expense::Input {
        expense::Input {
            document_id: self.document_id,
            bank_transaction_id: self.bank_transaction_id,
            expense_account_no: self.expense_account.clone(),
            vat_treatment: self.vat_treatment.clone(),
            payment_account_no: self.payment_account.clone(),
            transaction_date: self.date.clone(),
            text: self.text.clone(),
        }
    }
    fn payload(&self) -> Value {
        let mut p = json!({"documentId":self.document_id,"bankTransactionId":self.bank_transaction_id,"expenseAccount":self.expense_account});
        for (key, value) in [
            ("vatTreatment", &self.vat_treatment),
            ("paymentAccount", &self.payment_account),
            ("date", &self.date),
            ("text", &self.text),
        ] {
            if let Some(value) = value {
                p[key] = json!(value);
            }
        }
        p
    }
}
pub fn book(
    tx: &Transaction<'_>,
    input: &Input,
    subject: &str,
    principal_kind: PrincipalKind,
    latest_date: Date,
    now_ms: i64,
) -> Result<Value, Failure> {
    crate::journal_post::require_current(tx)?;
    if subject.trim().is_empty() || subject.len() > 256 || subject.chars().any(char::is_control) {
        return Err(
            Error::new("expense_refused", "authenticated stable principal required").into(),
        );
    }
    let native = input.native();
    // Revalidate retained original, selected bank row, immutable extraction and
    // allocation before looking up a receipt, including on replay/conflict.
    expense::prepare_for_receipt(tx, &native, latest_date)?;
    let write = |tx: &Transaction<'_>| {
        expense::post_with_principal(tx, &native, subject, latest_date, principal_kind)
    };
    let Some(key) = input.idempotency_key.as_deref() else {
        return write(tx).map_err(Failure::from);
    };
    let payload = input.payload();
    let actor = format!(
        "{}:{subject}",
        match principal_kind {
            PrincipalKind::User => "user",
            PrincipalKind::ServiceAccount => "service_account",
        }
    );
    match idempotency::execute(
        tx,
        idempotency::Input {
            key,
            operation: Operation::ExpenseBook,
            principal_kind,
            subject,
            payload: &payload,
            created_by: &actor,
            created_by_program: "rentemester-original-expense",
            now_ms,
        },
        write,
    )? {
        Outcome::Accepted {
            mut result,
            receipt,
        } => {
            result["idempotency"] = serde_json::to_value(receipt).map_err(|_| {
                Error::new("storage_failure", "expense receipt serialization failed")
            })?;
            Ok(result)
        }
        Outcome::Refused { code } => Err(Error::new(
            code,
            "Expense request key refused; inspect its retained outcome",
        )
        .into()),
    }
}

#[cfg(test)]
mod tests;
