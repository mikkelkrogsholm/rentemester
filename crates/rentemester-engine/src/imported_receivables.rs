//! Explicit external receivable declarations. Hashes identify source declarations, not physical invoices.
use crate::{Error, fiscal_year::Date, money::Money};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidatedSchedule {
    pub source: Value,
    pub hash: String,
    pub invoices: Vec<Invoice>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Invoice {
    pub id: String,
    pub customer_id: Option<String>,
    pub customer_name: Option<String>,
    pub invoice_date: Date,
    pub due_date: Option<Date>,
    pub gross_amount: Money,
    pub control_account_no: String,
    pub recognition_ref: String,
    pub document_hash: String,
    pub events: Vec<Event>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    pub event_kind: String,
    pub payment_date: Date,
    pub amount: Money,
    pub payment_ref: String,
    pub document_hash: String,
}
fn refused(s: impl Into<String>) -> Error {
    Error::new("imported_receivables_refused", s)
}
fn trim_source(s: &str) -> &str {
    s.trim_matches(|c| matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}'))
}
fn text(v: &Value, key: &str) -> Result<String, Error> {
    let s = trim_source(v[key].as_str().unwrap_or(""));
    if s.is_empty() {
        Err(refused(format!("missing {key}")))
    } else {
        Ok(s.into())
    }
}
fn source_text(v: &Value, key: &str) -> Result<String, Error> {
    text(v, key)?;
    Ok(v[key].as_str().expect("validated string").to_owned())
}
fn hash(v: &Value, key: &str) -> Result<String, Error> {
    let s = source_text(v, key)?;
    if trim_source(&s).len() != 64 || !trim_source(&s).bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(refused(format!("invalid {key} SHA-256")));
    }
    Ok(s)
}
fn date(v: &Value, key: &str) -> Result<Date, Error> {
    Date::parse(v[key].as_str().unwrap_or("")).map_err(|_| refused(format!("invalid {key}")))
}
fn amount(v: &Value, key: &str) -> Result<Money, Error> {
    let n = v[key]
        .as_f64()
        .filter(|n| n.is_finite() && *n > 0.)
        .ok_or_else(|| refused(format!("invalid positive {key}")))?;
    let s = ryu_js::Buffer::new().format(n).to_owned();
    let m = Money::parse(&s)?;
    if m == Money::default() {
        return Err(refused(format!("{key} rounds to zero")));
    }
    Ok(m)
}
fn optional(v: &Value, key: &str) -> Option<String> {
    v[key]
        .as_str()
        .map(trim_source)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
/// ECMAScript number spelling and UTF-16 key ordering match the source canonical hash.
pub fn canonical(value: &Value) -> Result<String, Error> {
    Ok(match value {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::String(s) => serde_json::to_string(s).map_err(|_| refused("invalid string"))?,
        Value::Number(n) => {
            let n = n
                .as_f64()
                .filter(|n| n.is_finite())
                .ok_or_else(|| refused("nonfinite source number"))?;
            ryu_js::Buffer::new().format(n).into()
        }
        Value::Array(a) => format!(
            "[{}]",
            a.iter()
                .map(canonical)
                .collect::<Result<Vec<_>, _>>()?
                .join(",")
        ),
        Value::Object(o) => {
            let mut keys = o.keys().collect::<Vec<_>>();
            keys.sort_by_cached_key(|s| s.encode_utf16().collect::<Vec<_>>());
            format!(
                "{{{}}}",
                keys.into_iter()
                    .map(|k| Ok(format!(
                        "{}:{}",
                        serde_json::to_string(k).map_err(|_| refused("invalid key"))?,
                        canonical(&o[k])?
                    )))
                    .collect::<Result<Vec<_>, Error>>()?
                    .join(",")
            )
        }
    })
}
pub fn validate(source: &Value) -> Result<ValidatedSchedule, Error> {
    if source["contract"] != "rentemester-imported-receivables-v1" {
        return Err(refused("unsupported imported receivable contract"));
    }
    hash(source, "sourceDocumentHash")?;
    let invoices = source["invoices"]
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or_else(|| refused("schedule needs invoices"))?;
    let mut ids = BTreeSet::new();
    let mut result = vec![];
    for v in invoices {
        let id = source_text(v, "id")?;
        if !ids.insert(trim_source(&id).to_owned()) {
            return Err(refused("duplicate invoice id"));
        }
        let invoice_date = date(v, "invoiceDate")?;
        let due_date = if v["dueDate"].is_null() {
            None
        } else {
            Some(date(v, "dueDate")?)
        };
        if due_date.is_some_and(|d| d < invoice_date) {
            return Err(refused("due date precedes invoice"));
        }
        let gross_amount = amount(v, "grossAmount")?;
        let mut paid = Money::default();
        let mut events = vec![];
        let mut event_ids = BTreeSet::new();
        if !v["payments"].is_null() {
            for e in v["payments"]
                .as_array()
                .ok_or_else(|| refused("payments must be an array"))?
            {
                let id = source_text(e, "id")?;
                if !event_ids.insert(trim_source(&id).to_owned()) {
                    return Err(refused("duplicate event id"));
                }
                let kind = if e["eventKind"].is_null() {
                    "payment"
                } else {
                    e["eventKind"].as_str().unwrap_or("")
                };
                if !["payment", "credit_note"].contains(&kind) {
                    return Err(refused("unsupported event kind"));
                }
                let payment_date = date(e, "paymentDate")?;
                if payment_date < invoice_date {
                    return Err(refused("event precedes invoice"));
                }
                let amount = amount(e, "amount")?;
                paid.add(&amount);
                events.push(Event {
                    id,
                    event_kind: kind.into(),
                    payment_date,
                    amount,
                    payment_ref: source_text(e, "paymentRef")?,
                    document_hash: hash(e, "documentHash")?,
                });
            }
        }
        if gross_amount.subtract(&paid).is_negative() {
            return Err(refused("payments exceed invoice amount"));
        }
        result.push(Invoice {
            id,
            customer_id: optional(v, "customerId"),
            customer_name: optional(v, "customerName"),
            invoice_date,
            due_date,
            gross_amount,
            control_account_no: source_text(v, "controlAccountNo")?,
            recognition_ref: source_text(v, "recognitionRef")?,
            document_hash: hash(v, "documentHash")?,
            events,
        });
    }
    let hash = format!("{:x}", Sha256::digest(canonical(source)?.as_bytes()));
    Ok(ValidatedSchedule {
        source: source.clone(),
        hash,
        invoices: result,
    })
}
pub fn balance(schedule: &ValidatedSchedule, cutoff: Date, account: &str) -> Money {
    let mut total = Money::default();
    for i in &schedule.invoices {
        if i.invoice_date <= cutoff && i.control_account_no == account {
            total.add(&i.gross_amount);
            for e in &i.events {
                if e.payment_date <= cutoff {
                    total = total.subtract(&e.amount);
                }
            }
        }
    }
    total
}
#[cfg(test)]
mod tests;
