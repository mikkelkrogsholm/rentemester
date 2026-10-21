//! Source-facing payable intents and filtered projection over the native writers/dated rows.
use crate::{
    idempotency::{self, Operation, Outcome, PrincipalKind},
    payables,
};
use rentemester_engine::{Error, fiscal_year::Date, money::Money, sales_invoice::wire};
use rusqlite::Transaction;
use serde::Deserialize;
use serde_json::{Number, Value, json};
fn optional<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Register {
    pub document_id: i64,
    pub bill_date: String,
    pub due_date: String,
    pub expense_account: String,
    pub confirm: bool,
    #[serde(default, deserialize_with = "optional")]
    pub vat_treatment: Option<String>,
    #[serde(default, deserialize_with = "optional")]
    pub vendor_id: Option<i64>,
    #[serde(default, deserialize_with = "optional")]
    pub note: Option<String>,
    #[serde(default, deserialize_with = "optional")]
    pub idempotency_key: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Pay {
    pub payable_id: i64,
    pub bank_transaction_id: i64,
    pub confirm: bool,
    #[serde(default, deserialize_with = "optional")]
    pub amount: Option<Number>,
    #[serde(default, deserialize_with = "optional")]
    pub date: Option<String>,
    #[serde(default, deserialize_with = "optional")]
    pub payment_account: Option<String>,
    #[serde(default, deserialize_with = "optional")]
    pub note: Option<String>,
    #[serde(default, deserialize_with = "optional")]
    pub idempotency_key: Option<String>,
}
#[derive(Default)]
pub struct Query {
    pub status: Option<String>,
    pub as_of: Option<String>,
    pub as_of_date: Option<String>,
    pub supplier: Option<String>,
    pub vendor_id: Option<i64>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub min_days: Option<u64>,
}
#[derive(Clone, Copy)]
pub enum Action<'a> {
    Register(&'a Register),
    Pay(&'a Pay),
    List(&'a Query),
}
fn invalid() -> Error {
    Error::new(
        "payable_original_validation",
        "Confirmed safe references and valid original payable fields required",
    )
}
fn safe(id: i64) -> bool {
    (1..=9_007_199_254_740_991).contains(&id)
}
fn text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.len() <= max
}
impl Register {
    pub fn native(&self) -> payables::Register {
        payables::Register {
            document_id: self.document_id,
            bill_date: self.bill_date.clone(),
            due_date: self.due_date.clone(),
            expense_account_no: self.expense_account.clone(),
            vat_treatment: self.vat_treatment.clone(),
            vendor_id: self.vendor_id,
            note: self.note.clone(),
            confirm: self.confirm,
        }
    }
    pub fn validate(&self) -> Result<(), Error> {
        if !self.confirm
            || !safe(self.document_id)
            || self.vendor_id.is_some_and(|n| !safe(n))
            || !text(&self.expense_account, 256)
            || self.note.as_ref().is_some_and(|s| s.len() > 4096)
            || self
                .vat_treatment
                .as_deref()
                .is_some_and(|s| !matches!(s, "standard" | "exempt" | "non_deductible"))
        {
            return Err(invalid());
        }
        Date::parse(&self.bill_date)?;
        Date::parse(&self.due_date)?;
        if let Some(key) = &self.idempotency_key {
            idempotency::validate_key(key)?;
        }
        Ok(())
    }
}
impl Pay {
    pub fn native(&self) -> payables::Payment {
        payables::Payment {
            bank_transaction_id: self.bank_transaction_id,
            payment_date: self.date.clone(),
            payment_account_no: self.payment_account.clone(),
            amount: self.amount.clone(),
            note: self.note.clone(),
            confirm: self.confirm,
        }
    }
    pub fn validate(&self) -> Result<(), Error> {
        if !self.confirm
            || !safe(self.payable_id)
            || !safe(self.bank_transaction_id)
            || self.note.as_ref().is_some_and(|s| s.len() > 4096)
            || self
                .payment_account
                .as_deref()
                .is_some_and(|s| !text(s, 256))
        {
            return Err(invalid());
        }
        if let Some(amount) = &self.amount {
            let n = Money::parse(&amount.to_string())?;
            if n.is_negative() || n == Money::default() {
                return Err(invalid());
            }
        }
        if let Some(date) = &self.date {
            Date::parse(date)?;
        }
        if let Some(key) = &self.idempotency_key {
            idempotency::validate_key(key)?;
        }
        Ok(())
    }
    pub fn payload(&self) -> Value {
        json!({"payableId":self.payable_id,"bankTransactionId":self.bank_transaction_id,"amount":self.amount,"date":self.date,"paymentAccount":self.payment_account,"note":self.note})
    }
}
fn envelope(mut value: Value) -> Value {
    let rules = value.as_object_mut().unwrap().remove("appliedRules");
    value.as_object_mut().unwrap().remove("ok");
    value.as_object_mut().unwrap().remove("errors");
    let mut result = json!({"ok":true,"data":value,"errors":[]});
    if let Some(rules) = rules {
        result["appliedRules"] = rules;
    }
    result
}
pub fn mutate(
    tx: &Transaction<'_>,
    action: Action<'_>,
    subject: &str,
    kind: PrincipalKind,
    latest: Date,
    now: i64,
) -> Result<Value, Error> {
    let (key, operation, payload) = match action {
        Action::Register(i) => {
            i.validate()?;
            let mut payload = json!({"documentId":i.document_id,"billDate":i.bill_date,"dueDate":i.due_date,"expenseAccount":i.expense_account});
            if let Some(value) = &i.vat_treatment {
                payload["vatTreatment"] = json!(value);
            }
            if let Some(value) = i.vendor_id {
                payload["vendorId"] = json!(value);
            }
            if let Some(value) = &i.note {
                payload["note"] = json!(value);
            }
            (
                i.idempotency_key.as_deref(),
                Operation::PayableRegister,
                payload,
            )
        }
        Action::Pay(i) => {
            i.validate()?;
            (
                i.idempotency_key.as_deref(),
                Operation::PayablePay,
                i.payload(),
            )
        }
        _ => return Err(invalid()),
    };
    let write = |tx: &Transaction<'_>| {
        let mut value = match action {
            Action::Register(i) => {
                payables::register_original(tx, &i.native(), subject, latest, kind)?
            }
            Action::Pay(i) => {
                let mut value = payables::pay_with_principal(
                    tx,
                    i.payable_id,
                    &i.native(),
                    subject,
                    latest,
                    kind,
                )?;
                value["appliedRules"] = json!(["DK-PAYABLE-PAYMENT-001"]);
                value
            }
            _ => return Err(invalid()),
        };
        value["ok"] = json!(true);
        Ok(value)
    };
    let Some(key) = key else {
        return write(tx).map(envelope);
    };
    let actor = format!("user:{subject}");
    match idempotency::execute(
        tx,
        idempotency::Input {
            key,
            operation,
            principal_kind: kind,
            subject,
            payload: &payload,
            created_by: &actor,
            created_by_program: "rentemester-original-payable",
            now_ms: now,
        },
        write,
    )
    .map_err(|failure| failure.error)?
    {
        Outcome::Accepted {
            mut result,
            receipt,
        } => {
            result["idempotency"] = serde_json::to_value(receipt).map_err(crate::invalid)?;
            Ok(envelope(result))
        }
        Outcome::Refused { code } => Err(Error::new(
            code,
            "Payable request key refused; inspect its retained outcome",
        )),
    }
}
pub fn list(tx: &Transaction<'_>, query: &Query, today: &str) -> Result<Value, Error> {
    let status = query.status.as_deref().unwrap_or("all");
    let as_of = query
        .as_of
        .as_deref()
        .or(query.as_of_date.as_deref())
        .unwrap_or(today);
    let mut errors = vec![];
    for (label, value) in [
        ("asOfDate", Some(as_of)),
        ("from", query.from.as_deref()),
        ("to", query.to.as_deref()),
    ] {
        if value.is_some_and(|s| Date::parse(s).is_err()) {
            errors.push(format!("{label} must be YYYY-MM-DD when present"));
        }
    }
    if !matches!(status, "all" | "open" | "paid" | "overdue") {
        errors.push("status must be one of open, paid, overdue, all".into());
    }
    if query.vendor_id.is_some_and(|n| !safe(n)) {
        errors.push("vendorId must be a positive integer when present".into());
    }
    if !errors.is_empty() {
        return Ok(json!({"ok":false,"errors":errors}));
    }
    let dated = payables::dated_rows(tx, as_of, status)?;
    let needle = query
        .supplier
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let mut rows = dated["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| {
            let date = r["billDate"].as_str().unwrap();
            query.from.as_deref().is_none_or(|from| date >= from)
                && query.to.as_deref().is_none_or(|to| date <= to)
                && query
                    .vendor_id
                    .is_none_or(|v| r["vendorId"].as_i64() == Some(v))
                && (needle.is_empty()
                    || r["supplierName"]
                        .as_str()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&needle))
                && (status != "overdue"
                    || r["overdueDays"].as_u64().unwrap() >= query.min_days.unwrap_or(0))
        })
        .cloned()
        .collect::<Vec<_>>();
    rows.sort_by(|a, b| {
        b["overdueDays"]
            .as_i64()
            .cmp(&a["overdueDays"].as_i64())
            .then_with(|| a["dueDate"].as_str().cmp(&b["dueDate"].as_str()))
            .then_with(|| a["payableId"].as_i64().cmp(&b["payableId"].as_i64()))
    });
    let mut total = Money::default();
    let mut overdue = Money::default();
    for row in &rows {
        let open = Money::parse(&row["openBalance"].to_string())?;
        total.add(&open);
        if row["isOverdue"] == true {
            overdue.add(&open);
        }
    }
    Ok(envelope(
        json!({"ok":true,"count":rows.len(),"status":status,"asOfDate":as_of,"totalOpenBalance":wire(&total)?,"overdueOpenBalance":wire(&overdue)?,"notYetDueOpenBalance":wire(&total.subtract(&overdue))?,"rows":rows,"errors":[]}),
    ))
}
#[cfg(test)]
mod tests;
