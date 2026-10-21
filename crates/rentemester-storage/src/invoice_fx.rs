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
/// One dated principal with explicit, separate original-currency and DKK units.
pub(crate) struct DatedPrincipal {
    pub currency: String,
    pub open: Money,
    pub carrying_dkk: Money,
}
pub(crate) fn dated_principal(
    tx: &Transaction<'_>,
    id: i64,
    as_of: &str,
) -> Result<DatedPrincipal, Error> {
    use rentemester_engine::{currency::Currency, fiscal_year::Date};
    use rusqlite::{OptionalExtension, params};
    Date::parse(as_of)?;
    let (currency, raw, gross): (String, String, Money) = tx.query_row(
        "SELECT currency,payload_json,amount_inc_vat FROM documents WHERE id=?1 AND document_type='issued_invoice' AND invoice_date<=?2",
        params![id, as_of],
        |r| Ok((r.get(0)?, r.get(1)?, legacy_money(r.get_ref(2)?).map_err(|_| rusqlite::Error::InvalidQuery)?)),
    ).map_err(invalid)?;
    Currency::parse(&currency)?;
    let payload: Value = serde_json::from_str(&raw).map_err(invalid)?;
    if payload["currency"] != currency || gross.is_negative() {
        return Err(refused("invoice source currency or gross differs"));
    }
    let debtor: Option<i64> = tx.query_row(
        "SELECT receivable_account_id FROM issued_invoice_postings WHERE invoice_document_id=?1",
        [id], |r| r.get(0),
    ).optional().map_err(invalid)?;
    let Some(debtor) = debtor else {
        // Retain existing unposted DKK presentation. A document without a posting
        // cannot have any linked principal or claim effects, in any currency.
        let linked: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM invoice_payments WHERE invoice_document_id=?1) OR EXISTS(SELECT 1 FROM invoice_refunds WHERE invoice_document_id=?1) OR EXISTS(SELECT 1 FROM credit_note_postings WHERE original_invoice_document_id=?1) OR EXISTS(SELECT 1 FROM invoice_bad_debt_writeoffs WHERE invoice_document_id=?1) OR EXISTS(SELECT 1 FROM journal_entries WHERE document_id=?1)",
            [id], |r| r.get(0),
        ).map_err(invalid)?;
        crate::credit_note::credited(tx, id)?;
        if linked {
            return Err(refused("unposted invoice has unexplained movements"));
        }
        let carrying_dkk = if currency == "DKK" {
            gross.clone()
        } else {
            let (no, date, hash): (String, String, String) = tx
                .query_row(
                    "SELECT invoice_no,invoice_date,sha256_hash FROM documents WHERE id=?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .map_err(invalid)?;
            if crate::invoice_email::sha(raw.as_bytes()) != hash
                || payload["invoiceNumber"] != no
                || payload["issueDate"] != date
                || sales_invoice::project(&payload)?.gross != gross
            {
                return Err(refused("unposted foreign invoice source differs"));
            }
            projection(&payload)?.gross
        };
        return Ok(DatedPrincipal {
            currency,
            open: gross,
            carrying_dkk,
        });
    };
    // Authenticate the full retained history before selecting a date. Today's
    // principal is a cross-check only, never the reported historical balance.
    let (_, current_currency) = crate::reminder_claims::principal_open(tx, id)?;
    if current_currency != currency {
        return Err(refused("invoice principal currency differs"));
    }
    let carrying_dkk =
        crate::invoice_bad_debt::adjusted_carrying_through(tx, id, debtor, Some(as_of))?;
    let mut open = gross;
    let mut q = tx.prepare(
        "SELECT amount,0 FROM invoice_payments WHERE invoice_document_id=?1 AND payment_date<=?2
         UNION ALL SELECT amount,1 FROM invoice_refunds WHERE invoice_document_id=?1 AND refund_date<=?2
         UNION ALL SELECT c.amount_inc_vat,0 FROM credit_note_postings p JOIN documents c ON c.id=p.credit_note_document_id WHERE p.original_invoice_document_id=?1 AND c.invoice_date<=?2
         UNION ALL SELECT gross_amount,0 FROM invoice_bad_debt_writeoffs WHERE invoice_document_id=?1 AND writeoff_date<=?2"
    ).map_err(invalid)?;
    let mut rows = q.query(params![id, as_of]).map_err(invalid)?;
    while let Some(row) = rows.next().map_err(invalid)? {
        let amount = legacy_money(row.get_ref(0).map_err(invalid)?)?;
        if row.get::<_, bool>(1).map_err(invalid)? {
            open.add(&amount);
        } else {
            open = open.subtract(&amount);
        }
    }
    if currency == "DKK" && open != carrying_dkk {
        return Err(refused(
            "dated DKK principal differs from canonical carrying",
        ));
    }
    Ok(DatedPrincipal {
        currency,
        open,
        carrying_dkk,
    })
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
    fn historical_projection_uses_frozen_principal_for_exact_cumulative_credit_carrying() {
        let mut source = rentemester_engine::sales_invoice::compute(&serde_json::from_value(serde_json::json!({"issueDate":"2025-08-01","currency":"EUR","fxRateToDkk":7.46,"vatTreatment":"foreign_reverse_charge","reverseChargeBasis":"EU_B2B_SERVICES","reverseChargeNote":"Synthetic reverse charge","lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":0.03}]})).unwrap()).unwrap();
        source.as_object_mut().unwrap().remove("fxValuationVersion");
        source["lines"][0]
            .as_object_mut()
            .unwrap()
            .remove("vatBaseDkk");
        source["lines"][0]
            .as_object_mut()
            .unwrap()
            .remove("vatAmountDkk");
        source["totals"]
            .as_object_mut()
            .unwrap()
            .remove("vatAmountDkk");
        let original = serde_json::to_vec(&source).unwrap();
        let projected = projection(&source).unwrap();
        let mut before = m("0");
        let mut total = m("0");
        for _ in 0..3 {
            total.add(&portion(&m("0.03"), &projected.gross, &before, &m("0.01")).unwrap());
            before.add(&m("0.01"));
        }
        assert_eq!(total, m("0.22"));
        assert_eq!(serde_json::to_vec(&source).unwrap(), original);
        source["totals"]["grossAmountDkk"] = serde_json::json!(0.23);
        assert!(projection(&source).is_err());
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
