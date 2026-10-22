//! Original accrual defaults and source DTOs over the shared append-only implementation.
//! Transactions and physical original admission belong to the caller. Every Err requires rollback.
use crate::{accruals, idempotency::PrincipalKind};
use rentemester_engine::{Error, accrual, fiscal_year::Date, sales_invoice::refused};
use rusqlite::Transaction;
use serde::Deserialize;
use serde_json::{Number, Value, json};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Register {
    pub accrual_type: String,
    pub description: String,
    pub total_amount: Number,
    pub recognition_periods: u32,
    pub first_recognition_date: String,
    pub result_account_no: String,
    pub registration_date: Option<String>,
    pub period_step_months: Option<u32>,
    pub balance_account_no: Option<String>,
    pub settlement_account_no: Option<String>,
    pub document_id: Option<i64>,
    pub note: Option<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Recognize {
    pub period: u32,
    pub date: Option<String>,
    pub settlement_account_no: Option<String>,
}
fn rules(mut result: Value) -> Value {
    let rules = result["appliedRules"]
        .as_array_mut()
        .expect("shared journal receipt rules");
    let rule = json!("DK-BOOKKEEPING-ACCRUAL-001");
    if !rules.contains(&rule) {
        rules.insert(0, rule);
    }
    result
}
pub fn register(
    tx: &Transaction<'_>,
    input: &Register,
    subject: &str,
    kind: PrincipalKind,
    latest: Date,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let actor = accruals::principal(subject, kind)?;
    if input.note.as_ref().is_some_and(|note| note.len() > 4096) {
        return Err(refused("note exceeds supported length"));
    }
    let balance = match input.accrual_type.as_str() {
        "prepaid_expense" => "1300",
        "accrued_expense" => "7300",
        "deferred_revenue" => "7310",
        _ => return Err(refused("unsupported accrual type")),
    };
    let settlement = match &input.settlement_account_no {
        Some(no) => no.clone(),
        None => crate::expense::role(tx, "bank")?,
    };
    let native = accruals::Register {
        accrual_type: input.accrual_type.clone(),
        description: input.description.clone(),
        total_amount: input.total_amount.clone(),
        recognition_periods: input.recognition_periods,
        first_recognition_date: input.first_recognition_date.clone(),
        result_account_no: input.result_account_no.trim().to_owned(),
        registration_date: input
            .registration_date
            .as_ref()
            .unwrap_or(&input.first_recognition_date)
            .clone(),
        period_step_months: input.period_step_months.unwrap_or(1),
        balance_account_no: input
            .balance_account_no
            .clone()
            .unwrap_or_else(|| balance.into()),
        settlement_account_no: settlement,
        // The shared implementation uses the separately resolved nullable header, not this native-only field.
        document_id: input.document_id.unwrap_or(0),
        note: input
            .note
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
        confirm: true,
    };
    Ok(rules(accruals::register_resolved(
        tx,
        &native,
        &actor,
        latest,
        input.document_id,
        true,
    )?))
}
pub fn recognize(
    tx: &Transaction<'_>,
    accrual_id: i64,
    input: &Recognize,
    subject: &str,
    kind: PrincipalKind,
    latest: Date,
) -> Result<Value, Error> {
    crate::journal_post::require_current(tx)?;
    let actor = accruals::principal(subject, kind)?;
    let a = accruals::load(tx, accrual_id)?;
    let scheduled = accrual::period(&a.total, a.periods, &a.first, a.step, input.period)?.0;
    let settlement = if a.kind == "accrued_expense" {
        Some(match &input.settlement_account_no {
            Some(no) => no.clone(),
            None => crate::expense::role(tx, "bank")?,
        })
    } else {
        input.settlement_account_no.clone()
    };
    let native = accruals::Recognize {
        period_index: input.period,
        transaction_date: input.date.clone().unwrap_or(scheduled),
        settlement_account_no: settlement,
        confirm: true,
    };
    Ok(rules(accruals::recognize_resolved(
        tx, accrual_id, &native, &actor, latest, true,
    )?))
}
pub fn report(tx: &Transaction<'_>) -> Result<Value, Error> {
    accruals::report(tx)
}
pub fn document_id(tx: &Transaction<'_>, accrual_id: i64) -> Result<Option<i64>, Error> {
    Ok(accruals::load(tx, accrual_id)?.document)
}

#[cfg(test)]
mod tests;
