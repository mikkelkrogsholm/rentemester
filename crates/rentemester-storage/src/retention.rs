//! Retention read model: existing stored deadlines or native fiscal-year fallback, never backfill writes.
use crate::invalid;
use rentemester_engine::{
    Error, calendar,
    fiscal_year::{Date, StartMonth},
};
use rusqlite::Transaction;
use serde_json::{Value, json};
pub(crate) const RULE: &str = "DK-BOOKKEEPING-RETENTION-001";
pub(crate) fn effective(
    tx: &Transaction<'_>,
    stored: Option<&str>,
    basis: Option<&str>,
) -> Result<Option<String>, Error> {
    if let Some(date) = stored.filter(|d| Date::parse(d).is_ok()) {
        return Ok(Some(date.to_owned()));
    }
    let Some(basis) = basis.filter(|d| Date::parse(d).is_ok()) else {
        return Ok(None);
    };
    let start: i32 = tx
        .query_row(
            "SELECT fiscal_year_start_month FROM companies WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    StartMonth::new(start)?;
    Ok(Some(calendar::retention_deadline(basis, start as u32)?))
}
pub fn read(tx: &Transaction<'_>, slug: &str, today: &str) -> Result<Value, Error> {
    crate::validate(tx)?;
    Date::parse(today)?;
    let mut summaries = vec![];
    for (table, sql) in [
        (
            "documents",
            "SELECT retain_until,COALESCE(invoice_date,substr(upload_datetime,1,10)) FROM documents",
        ),
        (
            "journal_entries",
            "SELECT retain_until,transaction_date FROM journal_entries",
        ),
        (
            "bank_transactions",
            "SELECT retain_until,COALESCE(booking_date,transaction_date) FROM bank_transactions",
        ),
    ] {
        let mut q = tx.prepare(sql).map_err(invalid)?;
        let mut rs = q.query([]).map_err(invalid)?;
        let (mut total, mut unknown) = (0, 0);
        let (mut expired, mut future) = (vec![], vec![]);
        while let Some(r) = rs.next().map_err(invalid)? {
            total += 1;
            let deadline: Option<String> = r.get(0).map_err(invalid)?;
            let basis: Option<String> = r.get(1).map_err(invalid)?;
            match effective(tx, deadline.as_deref(), basis.as_deref())? {
                Some(date) if date.as_str() < today => expired.push(date),
                Some(date) => future.push(date),
                None => unknown += 1,
            }
        }
        expired.sort();
        future.sort();
        summaries.push(json!({"table":table,"total":total,"expired":expired.len(),"nextExpiry":future.first(),"oldestExpired":expired.first(),"unknown":unknown}));
    }
    Ok(
        json!({"slug":slug,"company":crate::company_setup::profile(tx)?,"report":{"ok":true,"asOf":today,"appliedRules":[RULE],"rows":summaries,"errors":[]},"legalCitation":{"sourceId":"DK-BOGFORINGSLOVEN-2022-700","note":"Bogføringsloven § 12, stk. 1 — bilag og bogføringsmateriale skal opbevares i 5 år efter udløbet af det regnskabsår, materialet vedrører."}}),
    )
}
