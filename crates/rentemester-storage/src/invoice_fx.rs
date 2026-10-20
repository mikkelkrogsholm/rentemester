//! Original-currency counters and exact frozen DKK carrying amounts are separate units.
use crate::{invalid, legacy_money};
use rentemester_engine::{
    Error,
    money::Money,
    sales_invoice::{self, refused, wire},
};
use rusqlite::Transaction;
use serde_json::Value;

pub(crate) fn amount(value: &Value) -> Result<Money, Error> {
    if !value.is_number() {
        return Err(refused("numeric frozen amount required"));
    }
    let raw = value.to_string();
    let amount = Money::parse(&raw)?;
    if wire(&amount)?.as_f64() != value.as_f64() {
        return Err(refused("frozen amount must be exact cents"));
    }
    Ok(amount)
}
/// Bounded integer ratio, rounded once; no binary64 financial arithmetic.
pub(crate) fn portion(
    gross: &Money,
    dkk: &Money,
    before: &Money,
    delta: &Money,
) -> Result<Money, Error> {
    fn cents(m: &Money) -> Result<i128, Error> {
        wire(m)?;
        m.decimal()
            .replace('.', "")
            .parse()
            .map_err(|_| refused("invoice amount exceeds exact allocation range"))
    }
    let (g, d, b, a) = (cents(gross)?, cents(dkk)?, cents(before)?, cents(delta)?);
    if g <= 0 || d <= 0 || b < 0 || a <= 0 || b.checked_add(a).is_none_or(|v| v > g) {
        return Err(refused("allocation exceeds original invoice principal"));
    }
    let target = |n: i128| -> Result<i128, Error> {
        n.checked_mul(d)
            .and_then(|v| v.checked_add(g / 2))
            .map(|v| v / g)
            .ok_or_else(|| refused("invoice allocation exceeds exact range"))
    };
    let result = target(b + a)? - target(b)?;
    Money::parse(&format!("{}.{:02}", result / 100, result % 100))
}
pub(crate) fn projection(payload: &Value) -> Result<sales_invoice::TaxProjection, Error> {
    if payload["currency"] == "DKK" {
        sales_invoice::project(payload)
    } else {
        rentemester_engine::sales_fx::project(payload)
    }
}

pub(crate) fn credit_dkk(
    payload: &Value,
    currency: &str,
    original: &Money,
) -> Result<Money, Error> {
    if currency == "DKK" {
        return Ok(original.clone());
    }
    if payload["currency"] != currency
        || payload["fxValuationVersion"] != 1
        || amount(&payload["grossAmount"])? != *original
    {
        return Err(refused("foreign credit valuation identity differs"));
    }
    let gross = amount(&payload["grossAmountDkk"])?;
    let mut sum = amount(&payload["netAmountDkk"])?;
    sum.add(&amount(&payload["vatAmountDkk"])?);
    if sum != gross || gross.is_negative() {
        return Err(refused("foreign credit DKK totals differ"));
    }
    Ok(gross)
}
pub(crate) fn credited_dkk(tx: &Transaction<'_>, id: i64) -> Result<Money, Error> {
    crate::credit_note::credited(tx, id)?;
    let mut q=tx.prepare("SELECT booked_gross_dkk FROM credit_note_postings WHERE original_invoice_document_id=?1").map_err(invalid)?;
    let mut rows = q.query([id]).map_err(invalid)?;
    let mut total = Money::default();
    while let Some(row) = rows.next().map_err(invalid)? {
        total.add(&legacy_money(row.get_ref(0).map_err(invalid)?)?);
    }
    Ok(total)
}
/// Every prior cash movement must still identify its active principal journal.
pub(crate) fn movements(
    tx: &Transaction<'_>,
    id: i64,
    currency: &str,
    refund: bool,
) -> Result<Money, Error> {
    let sql = if refund {
        "SELECT p.amount,p.currency,j.status,EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id),j.document_id,j.source_bank_transaction_id,p.bank_transaction_id FROM invoice_refunds p JOIN journal_entries j ON j.id=p.journal_entry_id WHERE p.invoice_document_id=?1"
    } else {
        "SELECT p.amount,p.currency,j.status,EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id),j.document_id,j.source_bank_transaction_id,p.bank_transaction_id FROM invoice_payments p JOIN journal_entries j ON j.id=p.journal_entry_id WHERE p.invoice_document_id=?1"
    };
    let mut q = tx.prepare(sql).map_err(invalid)?;
    let mut rows = q.query([id]).map_err(invalid)?;
    let mut sum = Money::default();
    while let Some(r) = rows.next().map_err(invalid)? {
        let amount = legacy_money(r.get_ref(0).map_err(invalid)?)?;
        if r.get::<_, String>(1).map_err(invalid)? != currency
            || r.get::<_, String>(2).map_err(invalid)? != "posted"
            || r.get::<_, bool>(3).map_err(invalid)?
            || r.get::<_, Option<i64>>(4).map_err(invalid)? != Some(id)
            || r.get::<_, Option<i64>>(5).map_err(invalid)?
                != r.get::<_, Option<i64>>(6).map_err(invalid)?
            || amount.is_negative()
            || amount == Money::default()
        {
            return Err(refused(
                "previous invoice cash movement requires evidence resolution",
            ));
        }
        sum.add(&amount);
    }
    Ok(sum)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn m(s: &str) -> Money {
        Money::parse(s).unwrap()
    }
    #[test]
    fn cumulative_foreign_cent_allocation_preserves_final_original_dkk() {
        let mut before = m("0");
        let mut total = m("0");
        for _ in 0..3 {
            let part = portion(&m("0.03"), &m("0.22"), &before, &m("0.01")).unwrap();
            total.add(&part);
            before.add(&m("0.01"));
        }
        assert_eq!(total, m("0.22"));
        assert!(portion(&m("0.03"), &m("0.22"), &before, &m("0.01")).is_err());
    }
}
