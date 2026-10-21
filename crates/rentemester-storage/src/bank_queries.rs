//! Original bank query DTOs over the admitted native sources. No fiscal-year selection.
use crate::{bank, invalid};
use rentemester_engine::{Error, calendar::valid_iso_date, money::Money};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const RULE: &str = "DK-BOOKKEEPING-RECONCILIATION-001";
const SAFE: u64 = 9_007_199_254_740_991;
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Filters {
    pub status: Option<String>,
    pub text_match: Option<String>,
    /// Original currency units, decimal notation. Quantized once by shared Money.
    pub amount: Option<String>,
    pub account: Option<String>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ListQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    pub filters: Filters,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ReportQuery {
    pub from: String,
    pub to: String,
    pub filters: Filters,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AccountsQuery {
    pub include_inactive: bool,
}
impl Default for AccountsQuery {
    fn default() -> Self {
        Self {
            include_inactive: true,
        }
    }
}
fn query_error() -> Error {
    Error::new("invalid_bank_query", "invalid bounded bank query")
}
fn bounded(s: &str) -> bool {
    s.len() <= 4096 && !s.chars().any(|c| c.is_control() || c == '\u{fffd}')
}
impl Filters {
    fn validate_shape(&self) -> Result<(), Error> {
        if [&self.status, &self.text_match, &self.amount, &self.account]
            .into_iter()
            .flatten()
            .any(|s| !bounded(s))
        {
            return Err(query_error());
        }
        if let Some(amount) = &self.amount {
            let n: f64 = amount.parse().map_err(|_| query_error())?;
            if !n.is_finite() {
                return Err(query_error());
            }
            Money::parse(amount).map_err(|_| query_error())?;
        }
        Ok(())
    }
    fn errors(&self) -> Vec<String> {
        if self
            .status
            .as_deref()
            .is_some_and(|s| !s.is_empty() && !matches!(s, "all" | "matched" | "unmatched"))
        {
            vec!["status must be one of all, matched, unmatched when present".into()]
        } else {
            vec![]
        }
    }
}
impl ListQuery {
    pub fn validate_shape(&self) -> Result<(), Error> {
        self.filters.validate_shape()?;
        if self.limit.is_some_and(|n| !(1..=5000).contains(&n))
            || self.offset.is_some_and(|n| n > SAFE)
            || [&self.from, &self.to]
                .into_iter()
                .flatten()
                .any(|s| !bounded(s))
        {
            return Err(query_error());
        }
        Ok(())
    }
}
impl ReportQuery {
    pub fn validate_shape(&self) -> Result<(), Error> {
        self.filters.validate_shape()?;
        if self.from.is_empty() || self.to.is_empty() || !bounded(&self.from) || !bounded(&self.to)
        {
            return Err(query_error());
        }
        Ok(())
    }
}
fn envelope(data: Value, errors: Vec<String>, report: bool) -> Value {
    let mut result = if errors.is_empty() {
        json!({"ok":true,"data":data,"errors":[]})
    } else {
        json!({"ok":false,"errors":errors})
    };
    if report {
        result["appliedRules"] = json!([RULE]);
    }
    if result["errors"].as_array().is_some_and(|errors| {
        errors
            .iter()
            .any(|e| e.as_str().is_some_and(|s| s.ends_with(" does not exist")))
    }) {
        result["code"] = json!("NOT_FOUND");
    }
    result
}
/// Numeric JSON must round-trip to exact cents, including aggregated values.
fn number(amount: &Money) -> Result<Value, Error> {
    let fail = || {
        Error::new(
            "bank_query_precision_refused",
            "exact bank amount cannot be represented by legacy numeric JSON",
        )
    };
    let n: f64 = amount.decimal().parse().map_err(|_| fail())?;
    if !n.is_finite() {
        return Err(fail());
    }
    let mut buffer = ryu_js::Buffer::new();
    let raw = buffer.format_finite(n);
    if Money::parse(raw).map_err(|_| fail())? != *amount {
        return Err(fail());
    }
    serde_json::from_str(raw).map_err(|_| fail())
}
fn resolve(tx: &Transaction<'_>, filters: &Filters) -> Result<Result<Option<i64>, String>, Error> {
    let Some(raw) = filters.account.as_deref().filter(|s| !s.trim().is_empty()) else {
        return Ok(Ok(None));
    };
    let value = raw.trim();
    // Numeric positive id takes precedence; inactive accounts remain readable.
    let numeric = value
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && *n > 0.0 && n.fract() == 0.0 && *n <= SAFE as f64)
        .map(|n| n as i64);
    let id = tx
        .query_row(
            "SELECT id FROM bank_accounts WHERE id=?1 OR slug=?2 ORDER BY (id=?1) DESC LIMIT 1",
            params![numeric, value],
            |r| r.get::<_, i64>(0),
        )
        .optional()
        .map_err(invalid)?;
    Ok(id
        .map(Some)
        .ok_or_else(|| format!("bank account '{raw}' does not exist")))
}
struct Row {
    wire: Value,
    amount: Money,
}
fn rows(
    tx: &Transaction<'_>,
    filters: &Filters,
    from: Option<&str>,
    to: Option<&str>,
    account: Option<i64>,
) -> Result<Vec<Row>, Error> {
    let mut q = tx.prepare("SELECT bt.id,bt.transaction_date,bt.booking_date,bt.text,bt.amount,bt.currency,bt.reference,bt.import_batch_id,bt.bank_account_id,bt.status,br.journal_entry_id,br.journal_entry_no,EXISTS(SELECT 1 FROM journal_entries reversal WHERE reversal.reversal_of_entry_id=br.journal_entry_id) FROM bank_transactions bt LEFT JOIN bank_journal_reconciliations br ON br.bank_transaction_id=bt.id WHERE (?1 IS NULL OR bt.transaction_date>=?1) AND (?2 IS NULL OR bt.transaction_date<=?2) AND (?3 IS NULL OR bt.bank_account_id=?3) ORDER BY bt.transaction_date DESC,bt.id DESC").map_err(invalid)?;
    let text = filters
        .text_match
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_lowercase();
    let amount = filters.amount.as_deref().map(Money::parse).transpose()?;
    let mut out = vec![];
    let mut iter = q.query(params![from, to, account]).map_err(invalid)?;
    let mut seen = std::collections::BTreeSet::new();
    while let Some(r) = iter.next().map_err(invalid)? {
        let id: i64 = r.get(0).map_err(invalid)?;
        if !seen.insert(id) {
            return Err(Error::new(
                "bank_query_source_invalid",
                "ambiguous bank reconciliation linkage",
            ));
        }
        let journal: Option<i64> = r.get(10).map_err(invalid)?;
        let reversed: bool = r.get(12).map_err(invalid)?;
        let matched = journal.is_some() && !reversed;
        let status = if matched { "matched" } else { "unmatched" };
        let value = bank::signed_money(r.get_ref(4).map_err(invalid)?)?;
        let row_text: String = r.get(3).map_err(invalid)?;
        if filters
            .status
            .as_deref()
            .is_some_and(|s| matches!(s, "matched" | "unmatched") && s != status)
            || !text.is_empty() && !row_text.to_lowercase().contains(&text)
            || amount.as_ref().is_some_and(|a| *a != value)
        {
            continue;
        }
        out.push(Row { wire: json!({"id":id,"transactionDate":r.get::<_,String>(1).map_err(invalid)?,"bookingDate":r.get::<_,Option<String>>(2).map_err(invalid)?,"text":row_text,"amount":number(&value)?,"currency":r.get::<_,String>(5).map_err(invalid)?,"reference":r.get::<_,Option<String>>(6).map_err(invalid)?,"importBatchId":r.get::<_,Option<String>>(7).map_err(invalid)?,"bankAccountId":r.get::<_,Option<i64>>(8).map_err(invalid)?,"ledgerStatus":r.get::<_,String>(9).map_err(invalid)?,"reconciliationStatus":status,"journalEntryId":if matched { journal } else {None},"journalEntryNo":if matched {r.get::<_,Option<String>>(11).map_err(invalid)?}else{None}}), amount:value });
    }
    Ok(out)
}
pub fn list(tx: &Transaction<'_>, query: &ListQuery) -> Result<Value, Error> {
    query.validate_shape()?;
    let account = match resolve(tx, &query.filters)? {
        Ok(id) => id,
        Err(e) => return Ok(envelope(Value::Null, vec![e], false)),
    };
    let mut errors = vec![];
    for (date, key) in [(&query.from, "from"), (&query.to, "to")] {
        if date
            .as_deref()
            .is_some_and(|s| !s.is_empty() && !valid_iso_date(s.trim()))
        {
            errors.push(format!("{key} must be YYYY-MM-DD when present"));
        }
    }
    errors.extend(query.filters.errors());
    if !errors.is_empty() {
        return Ok(envelope(Value::Null, errors, false));
    }
    let mut rows: Vec<Value> = rows(
        tx,
        &query.filters,
        query.from.as_deref().map(str::trim),
        query.to.as_deref().map(str::trim),
        account,
    )?
    .into_iter()
    .map(|r| r.wire)
    .collect();
    let total = rows.len();
    let data = if query.limit.is_some() || query.offset.is_some() {
        let limit = query.limit.unwrap_or(500);
        let offset = query.offset.unwrap_or(0);
        rows = rows
            .into_iter()
            .skip(usize::try_from(offset).unwrap_or(usize::MAX))
            .take(limit as usize)
            .collect();
        let count = rows.len();
        let more = offset.saturating_add(count as u64) < total as u64;
        let mut data = json!({"rows":rows,"total":total,"count":count,"limit":limit,"offset":offset,"hasMore":more});
        if more {
            data["nextOffset"] = json!(offset + count as u64);
        }
        data
    } else {
        json!({"rows":rows,"count":total})
    };
    Ok(envelope(data, vec![], false))
}
pub fn report(tx: &Transaction<'_>, query: &ReportQuery) -> Result<Value, Error> {
    query.validate_shape()?;
    let account = match resolve(tx, &query.filters)? {
        Ok(id) => id,
        Err(e) => return Ok(envelope(Value::Null, vec![e], true)),
    };
    let from = query.from.trim();
    let to = query.to.trim();
    let mut errors = vec![];
    if !valid_iso_date(from) {
        errors.push("periodStart must be YYYY-MM-DD".into());
    }
    if !valid_iso_date(to) {
        errors.push("periodEnd must be YYYY-MM-DD".into());
    }
    errors.extend(query.filters.errors());
    if errors.is_empty() && from > to {
        errors.push("periodStart must be before or equal to periodEnd".into());
    }
    if !errors.is_empty() {
        return Ok(envelope(Value::Null, errors, true));
    }
    let mut rows = rows(tx, &query.filters, Some(from), Some(to), account)?;
    rows.reverse();
    let (mut matched, mut unmatched) = (vec![], vec![]);
    let (mut matched_total, mut unmatched_total) = (Money::default(), Money::default());
    for row in rows {
        let v = row.wire;
        let mut item = json!({"bankTransactionId":v["id"],"transactionDate":v["transactionDate"],"text":v["text"],"amount":v["amount"]});
        if v["reconciliationStatus"] == "matched" {
            item["journalEntryId"] = v["journalEntryId"].clone();
            item["journalEntryNo"] = v["journalEntryNo"].clone();
            matched_total.add(&row.amount);
            matched.push(item);
        } else {
            item["importBatchId"] = v["importBatchId"].clone();
            unmatched_total.add(&row.amount);
            unmatched.push(item);
        }
    }
    Ok(envelope(
        json!({"periodStart":from,"periodEnd":to,"matchedCount":matched.len(),"unmatchedCount":unmatched.len(),"matchedAmountTotal":number(&matched_total)?,"unmatchedAmountTotal":number(&unmatched_total)?,"matched":matched,"unmatched":unmatched}),
        vec![],
        true,
    ))
}
pub fn accounts(tx: &Transaction<'_>, query: &AccountsQuery) -> Result<Value, Error> {
    let value = bank::accounts(tx, "")?;
    let rows: Vec<_> = value["accounts"]
        .as_array()
        .ok_or_else(query_error)?
        .iter()
        .filter(|r| query.include_inactive || r["active"] == true)
        .cloned()
        .collect();
    Ok(envelope(
        json!({"count":rows.len(),"accounts":rows}),
        vec![],
        false,
    ))
}
#[cfg(test)]
mod tests;
