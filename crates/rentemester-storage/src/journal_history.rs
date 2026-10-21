//! Native append-only journal headers across all fiscal years. No accounting writes.
use crate::invalid;
use rentemester_engine::{Error, fiscal_year::Date};
use rusqlite::{Transaction, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const SAFE: u64 = 9_007_199_254_740_991;
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Query {
    pub from: Option<String>,
    pub to: Option<String>,
    pub status: Option<String>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    #[serde(default)]
    pub all: bool,
}
impl Query {
    pub fn validate(&self) -> Result<(), Error> {
        for date in [&self.from, &self.to].into_iter().flatten() {
            Date::parse(date).map_err(|_| query_error())?;
        }
        if self
            .status
            .as_deref()
            .is_some_and(|s| !matches!(s, "all" | "posted" | "reversed"))
            || self.limit.is_some_and(|n| !(1..=5000).contains(&n))
            || self.offset.is_some_and(|n| n > SAFE)
            || self.all && (self.limit.is_some() || self.offset.is_some())
        {
            return Err(query_error());
        }
        // Legacy reversed ranges are valid empty selections.
        Ok(())
    }
}
fn query_error() -> Error {
    Error::new(
        "invalid_journal_history_query",
        "invalid journal history filters",
    )
}

pub fn read(tx: &Transaction<'_>, query: &Query) -> Result<Value, Error> {
    query.validate()?;
    crate::journal_preview::require_current(tx)?;
    if !crate::journal_hash::verify(tx)?.ok {
        return Err(Error::new(
            "journal_history_unavailable",
            "journal hash integrity differs",
        ));
    }
    let status = query.status.as_deref().unwrap_or("all");
    let total: i64 = tx.query_row(
        "SELECT count(*) FROM journal_entries WHERE (?1 IS NULL OR transaction_date>=?1) AND (?2 IS NULL OR transaction_date<=?2) AND (?3='all' OR status=?3)",
        params![query.from, query.to, status], |r| r.get(0),
    ).map_err(invalid)?;
    let total = u64::try_from(total).map_err(invalid)?;
    if total > SAFE {
        return Err(invalid("journal history count exceeds safe integer range"));
    }
    let offset = query.offset.unwrap_or(0);
    let limit = if query.all {
        total
    } else {
        query.limit.unwrap_or(500)
    };
    let mut statement = tx.prepare(
        "SELECT id,entry_no,transaction_date,text,currency,amount_foreign,amount_dkk,fx_rate_to_dkk,document_id,source_bank_transaction_id,status,reversal_of_entry_id FROM journal_entries WHERE (?1 IS NULL OR transaction_date>=?1) AND (?2 IS NULL OR transaction_date<=?2) AND (?3='all' OR status=?3) ORDER BY id DESC LIMIT ?4 OFFSET ?5"
    ).map_err(invalid)?;
    let mut rows = statement
        .query(params![
            query.from,
            query.to,
            status,
            limit as i64,
            offset as i64
        ])
        .map_err(invalid)?;
    let mut entries = Vec::new();
    while let Some(row) = rows.next().map_err(invalid)? {
        let mut entry = json!({
            "id":row.get::<_,i64>(0).map_err(invalid)?,
            "entryNo":row.get::<_,String>(1).map_err(invalid)?,
            "transactionDate":row.get::<_,String>(2).map_err(invalid)?,
            "text":row.get::<_,String>(3).map_err(invalid)?,
            "currency":row.get::<_,String>(4).map_err(invalid)?,
            "amountForeign":row.get::<_,Option<f64>>(5).map_err(invalid)?,
            "amountDkk":row.get::<_,Option<f64>>(6).map_err(invalid)?,
            "fxRateToDkk":row.get::<_,Option<f64>>(7).map_err(invalid)?,
            "documentId":row.get::<_,Option<i64>>(8).map_err(invalid)?,
            "sourceBankTransactionId":row.get::<_,Option<i64>>(9).map_err(invalid)?,
            "status":row.get::<_,String>(10).map_err(invalid)?,
            "reversalOfEntryId":row.get::<_,Option<i64>>(11).map_err(invalid)?,
        });
        if query.all {
            let mut total = rentemester_engine::money::Money::default();
            let mut lines = tx
                .prepare(
                    "SELECT debit_amount FROM journal_lines WHERE journal_entry_id=?1 ORDER BY id",
                )
                .map_err(invalid)?;
            let mut debits = lines
                .query([row.get::<_, i64>(0).map_err(invalid)?])
                .map_err(invalid)?;
            while let Some(line) = debits.next().map_err(invalid)? {
                total.add(&crate::legacy_money(line.get_ref(0).map_err(invalid)?)?);
            }
            entry["bookedDebitTotal"] = rentemester_engine::sales_invoice::wire(&total)?;
        }
        entries.push(entry);
    }
    let count = entries.len() as u64;
    let has_more = offset + count < total;
    let mut result = json!({"entries":entries,"total":total,"count":count,"limit":limit,"offset":offset,"hasMore":has_more});
    if has_more {
        result["nextOffset"] = json!(offset + count);
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
