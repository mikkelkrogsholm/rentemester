//! EU B2B service listing from immutable explicitly linked native sales, valued in DKK.
//! This is a preparation report, with credit adjustments dated at credit issuance, never a submission.
use crate::{invalid, legacy_money};
use rentemester_engine::{Error, fiscal_year::Date, money::Money, sales_invoice::wire, vies};
use rusqlite::{OptionalExtension, Transaction};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn refused(message: impl Into<String>) -> Error {
    Error::new("eu_sales_evidence_required", message)
}
fn money(value: &Value) -> Result<Money, Error> {
    match value {
        Value::Number(n) => Money::parse(&n.to_string()),
        Value::String(s) => Money::parse_canonical(s),
        _ => Err(refused("frozen DKK valuation missing")),
    }
}
struct Document {
    id: i64,
    kind: String,
    number: String,
    date: String,
    currency: String,
    payload: Value,
    hash: String,
}
fn document(tx: &Transaction<'_>, id: i64) -> Result<Document, Error> {
    let row=tx.query_row("SELECT document_type,invoice_no,invoice_date,currency,payload_json,sha256_hash FROM documents WHERE id=?1",[id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?))).map_err(invalid)?;
    let payload: Value = serde_json::from_str(&row.4).map_err(invalid)?;
    Date::parse(&row.2)?;
    Ok(Document {
        id,
        kind: row.0,
        number: row.1,
        date: row.2,
        currency: row.3,
        payload,
        hash: row.5,
    })
}
fn verify_snapshot(doc: &Document) -> Result<(), Error> {
    let serialized = serde_json::to_string_pretty(&doc.payload).map_err(invalid)?;
    if crate::invoice_email::sha(serialized.as_bytes()) != doc.hash {
        return Err(refused("immutable invoice snapshot hash differs"));
    }
    Ok(())
}
fn original(tx: &Transaction<'_>, doc: &Document) -> Result<(vies::Vat, Money), Error> {
    verify_snapshot(doc)?;
    let p = &doc.payload;
    if doc.kind != "issued_invoice"
        || p["invoiceNumber"].as_str() != Some(&doc.number)
        || p["issueDate"].as_str() != Some(&doc.date)
        || p["currency"].as_str() != Some(&doc.currency)
        || p["reverseChargeBasis"].as_str() != Some("EU_B2B_SERVICES")
    {
        return Err(refused(
            "explicit immutable EU B2B service invoice required",
        ));
    }
    let vat = p["buyer"]["vatOrCvr"]
        .as_str()
        .ok_or_else(|| refused("frozen foreign EU buyer VAT missing"))?;
    let parsed = vies::foreign(vat)?;
    vies::require_record(
        &p["viesValidation"],
        vat,
        p["issuedAt"]
            .as_str()
            .ok_or_else(|| refused("invoice issuance timestamp missing"))?,
    )?;
    let totals = &p["totals"];
    let net = if doc.currency == "DKK" {
        money(&totals["netAmount"])?
    } else {
        if p["fxValuationVersion"].as_i64() != Some(1) {
            return Err(refused("frozen foreign invoice DKK valuation required"));
        }
        money(&totals["netAmountDkk"])?
    };
    let vat = money(
        &totals[if doc.currency == "DKK" {
            "vatAmount"
        } else {
            "vatAmountDkk"
        }],
    )?;
    let gross = money(
        &totals[if doc.currency == "DKK" {
            "grossAmount"
        } else {
            "grossAmountDkk"
        }],
    )?;
    if net.is_negative() || net == Money::default() || vat != Money::default() || gross != net {
        return Err(refused("EU service DKK basis is inconsistent"));
    }
    let link:Option<(i64,Money)>=tx.query_row("SELECT journal_entry_id,booked_gross_dkk FROM issued_invoice_postings WHERE invoice_document_id=?1",[doc.id],|r|Ok((r.get(0)?,legacy_money(r.get_ref(1)?).map_err(|_|rusqlite::Error::InvalidQuery)?))).optional().map_err(invalid)?;
    let (journal, booked) = link.ok_or_else(|| refused("explicit posted invoice link required"))?;
    if booked != gross {
        return Err(refused("frozen DKK gross differs from posted invoice"));
    }
    verify_journal(tx, doc, journal, &net, false)?;
    Ok((parsed, net))
}
fn verify_journal(
    tx: &Transaction<'_>,
    doc: &Document,
    journal: i64,
    net: &Money,
    credit: bool,
) -> Result<(), Error> {
    let (status,document_id,date,currency,bank,reversed):(String,Option<i64>,String,String,Option<i64>,bool)=tx.query_row("SELECT status,document_id,transaction_date,currency,source_bank_transaction_id,EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id) FROM journal_entries j WHERE id=?1",[journal],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).map_err(invalid)?;
    if status != "posted"
        || document_id != Some(doc.id)
        || date != doc.date
        || currency != "DKK"
        || bank.is_some()
        || reversed
    {
        return Err(refused("EU sales source posting requires resolution"));
    }
    let mut q=tx.prepare("SELECT a.type,l.vat_code,l.debit_amount,l.credit_amount FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=?1 ORDER BY l.id").map_err(invalid)?;
    let mut rows = q.query([journal]).map_err(invalid)?;
    let mut income = Money::default();
    let mut balance = Money::default();
    let mut count = 0;
    while let Some(row) = rows.next().map_err(invalid)? {
        let debit = legacy_money(row.get_ref(2).map_err(invalid)?)?;
        let amount = legacy_money(row.get_ref(3).map_err(invalid)?)?;
        if debit.is_negative() || amount.is_negative() {
            return Err(refused("negative journal side"));
        }
        balance.add(&debit.subtract(&amount));
        if row.get::<_, String>(0).map_err(invalid)? == "income" {
            if row.get::<_, Option<String>>(1).map_err(invalid)?.as_deref()
                != Some("REVERSE_CHARGE_EXEMPT")
                || (if credit {
                    amount.clone()
                } else {
                    debit.clone()
                }) != Money::default()
            {
                return Err(refused("explicit EU service journal basis required"));
            }
            income.add(if credit { &debit } else { &amount });
            count += 1;
        } else if row.get::<_, Option<String>>(1).map_err(invalid)?.is_some() {
            return Err(refused("unexpected EU service VAT journal line"));
        }
    }
    if count == 0 || income != *net || balance != Money::default() {
        return Err(refused(
            "EU service frozen base differs from balanced journal",
        ));
    }
    Ok(())
}
#[derive(Default)]
struct Customer {
    country: String,
    name: Option<String>,
    invoices: Vec<String>,
    credits: Vec<String>,
    total: Money,
}
pub fn read(tx: &Transaction<'_>, from: Date, to: Date) -> Result<Value, Error> {
    if from > to
        || (rentemester_engine::vat_period::date(&to.to_string())?
            - rentemester_engine::vat_period::date(&from.to_string())?)
        .whole_days()
            > 3660
    {
        return Err(Error::new(
            "invalid_date_range",
            "EU sales range must be ordered and at most ten years",
        ));
    }
    let mut q=tx.prepare("SELECT id FROM documents WHERE document_type IN ('issued_invoice','credit_note') AND invoice_date>=?1 AND invoice_date<=?2 ORDER BY invoice_date,id").map_err(invalid)?;
    let ids = q
        .query_map([from.to_string(), to.to_string()], |r| r.get::<_, i64>(0))
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    let mut customers = BTreeMap::<String, Customer>::new();
    let mut sources = Vec::new();
    let mut total = Money::default();
    let mut invoices = 0;
    let mut credits = 0;
    for id in ids {
        let doc = document(tx, id)?;
        let (original_doc, linked_credit) = if doc.kind == "credit_note" {
            let original_id:Option<i64>=tx.query_row("SELECT original_invoice_document_id FROM credit_note_postings WHERE credit_note_document_id=?1",[id],|r|r.get(0)).optional().map_err(invalid)?;
            let Some(original_id) = original_id else {
                continue;
            };
            (document(tx, original_id)?, true)
        } else {
            let posted:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM issued_invoice_postings WHERE invoice_document_id=?1)",[id],|r|r.get(0)).map_err(invalid)?;
            if !posted {
                continue;
            }
            (document(tx, id)?, false)
        };
        if original_doc.payload["vatTreatment"].as_str() != Some("foreign_reverse_charge") {
            continue;
        }
        let (vat, original_net) = original(tx, &original_doc)?;
        let mut base = original_net.clone();
        if linked_credit {
            verify_snapshot(&doc)?;
            let p = &doc.payload;
            if p["type"].as_str() != Some("credit_note")
                || p["originalInvoiceDocumentId"].as_i64() != Some(original_doc.id)
                || p["originalInvoiceNumber"].as_str() != Some(&original_doc.number)
                || p["creditNoteNumber"].as_str() != Some(&doc.number)
                || p["issueDate"].as_str() != Some(&doc.date)
                || doc.date < original_doc.date
                || doc.currency != original_doc.currency
                || p["currency"].as_str() != Some(&doc.currency)
                || p["buyer"] != original_doc.payload["buyer"]
            {
                return Err(refused(
                    "credit snapshot differs from original EU service invoice",
                ));
            }
            base = money(
                &p[if doc.currency == "DKK" {
                    "netAmount"
                } else {
                    "netAmountDkk"
                }],
            )?;
            let gross = money(
                &p[if doc.currency == "DKK" {
                    "grossAmount"
                } else {
                    "grossAmountDkk"
                }],
            )?;
            if base.is_negative()
                || base == Money::default()
                || base != gross
                || money(
                    &p[if doc.currency == "DKK" {
                        "vatAmount"
                    } else {
                        "vatAmountDkk"
                    }],
                )? != Money::default()
                || original_net.subtract(&base).is_negative()
            {
                return Err(refused("credit DKK basis is inconsistent"));
            }
            let (journal,booked):(i64,Money)=tx.query_row("SELECT journal_entry_id,booked_gross_dkk FROM credit_note_postings WHERE credit_note_document_id=?1 AND original_invoice_document_id=?2",[id,original_doc.id],|r|Ok((r.get(0)?,legacy_money(r.get_ref(1)?).map_err(|_|rusqlite::Error::InvalidQuery)?))).map_err(invalid)?;
            if booked != gross {
                return Err(refused("credit DKK basis differs from linked posting"));
            }
            let mut credited = Money::default();
            let mut prior_query=tx.prepare("SELECT p.booked_gross_dkk FROM credit_note_postings p JOIN documents c ON c.id=p.credit_note_document_id WHERE p.original_invoice_document_id=?1 AND (c.invoice_date<?2 OR (c.invoice_date=?2 AND c.id<=?3)) ORDER BY c.invoice_date,c.id").map_err(invalid)?;
            let mut prior = prior_query
                .query(rusqlite::params![original_doc.id, doc.date, id])
                .map_err(invalid)?;
            while let Some(row) = prior.next().map_err(invalid)? {
                let amount = legacy_money(row.get_ref(0).map_err(invalid)?)?;
                if amount.is_negative() {
                    return Err(refused("negative linked credit DKK amount"));
                }
                credited.add(&amount);
            }
            if original_net.subtract(&credited).is_negative() {
                return Err(refused(
                    "cumulative credits exceed original EU service DKK basis",
                ));
            }
            verify_journal(tx, &doc, journal, &base, true)?;
            base = Money::default().subtract(&base);
            credits += 1;
        } else {
            invoices += 1;
        }
        let customer = customers.entry(vat.normalized.clone()).or_default();
        customer.country = vat.country_code;
        customer.name = original_doc.payload["buyer"]["name"]
            .as_str()
            .map(str::to_owned);
        customer.total.add(&base);
        if linked_credit {
            customer.credits.push(doc.number.clone())
        } else {
            customer.invoices.push(doc.number.clone())
        }
        total.add(&base);
        sources.push(json!({"documentId":id,"documentType":doc.kind,"documentNumber":doc.number,"issueDate":doc.date,"vatNumber":vat.normalized,"currency":doc.currency,"netAmountDkk":wire(&base)?}));
    }
    let customers=customers.into_iter().map(|(vat,c)|Ok(json!({"vatNumber":vat,"countryCode":c.country,"customerName":c.name,"invoiceCount":c.invoices.len(),"creditNoteCount":c.credits.len(),"totalValue":wire(&c.total)?,"invoiceNumbers":c.invoices,"creditNoteNumbers":c.credits}))).collect::<Result<Vec<Value>,Error>>()?;
    Ok(
        json!({"ok":true,"appliedRules":["DK-VAT-EU-SALES-LIST-001"],"periodStart":from.to_string(),"periodEnd":to.to_string(),"customers":customers,"totalValue":wire(&total)?,"invoiceCount":invoices,"creditNoteCount":credits,"sources":sources,"warnings":[],"errors":[]}),
    )
}
#[cfg(test)]
mod tests;
