//! Contained VAT is taken from verified original counters, never guessed from rounded net.
use crate::{invalid, invoice_fx, legacy_money};
use rentemester_engine::{
    Error,
    credit_note::{self, Counter},
    money::Money,
    sales_invoice::refused,
};
use rusqlite::Transaction;
use serde_json::{Value, json};
use std::collections::BTreeMap;

struct SourceCounter {
    id: i64,
    account: String,
    code: Option<String>,
    amount: Money,
    revenue: bool,
}
#[allow(clippy::too_many_arguments)]
fn counters(
    tx: &Transaction<'_>,
    journal: i64,
    document: i64,
    date: &str,
    currency: &str,
    debtor: i64,
    gross: &Money,
    credit: bool,
    payload: &Value,
) -> Result<Vec<SourceCounter>, Error> {
    let (status,doc,day,unit,bank,reversed):(String,Option<i64>,String,String,Option<i64>,bool)=tx.query_row("SELECT status,document_id,transaction_date,currency,source_bank_transaction_id,EXISTS(SELECT 1 FROM journal_entries r WHERE r.reversal_of_entry_id=j.id) FROM journal_entries j WHERE id=?1",[journal],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).map_err(invalid)?;
    if status != "posted"
        || doc != Some(document)
        || day != date
        || unit != currency
        || bank.is_some()
        || reversed
    {
        return Err(refused(
            "simplified VAT source journal identity requires resolution",
        ));
    }
    let metadata = tx
        .query_row(
            "SELECT amount_foreign,amount_dkk,fx_rate_to_dkk FROM journal_entries WHERE id=?1",
            [journal],
            |r| {
                let amount = |i| -> rusqlite::Result<Option<Money>> {
                    match r.get_ref(i)? {
                        rusqlite::types::ValueRef::Null => Ok(None),
                        value => legacy_money(value)
                            .map(Some)
                            .map_err(|_| rusqlite::Error::InvalidQuery),
                    }
                };
                Ok((amount(0)?, amount(1)?, r.get::<_, Option<f64>>(2)?))
            },
        )
        .map_err(invalid)?;
    if currency == "DKK" {
        if metadata.0.is_some() || metadata.1.is_some() || metadata.2.is_some() {
            return Err(refused(
                "DKK simplified source must not carry foreign valuation metadata",
            ));
        }
    } else {
        let (source, quote) = if credit {
            (
                invoice_fx::amount(&payload["grossAmount"])?,
                &payload["journalFxRateToDkk"],
            )
        } else {
            (
                rentemester_engine::sales_invoice::project(payload)?.gross,
                &payload["totals"]["fxRateToDkk"],
            )
        };
        let rate = rentemester_engine::sales_fx::rate(currency, quote.as_number())?;
        if rate.apply(&source) != *gross
            || metadata.0.as_ref() != Some(&source)
            || metadata.1.as_ref() != Some(gross)
            || metadata.2 != Some(crate::journal_post::rate(&rate)?)
        {
            return Err(refused(
                "simplified source journal valuation differs from immutable evidence",
            ));
        }
    }
    let mut query=tx.prepare("SELECT l.id,l.account_id,a.account_no,a.type,l.vat_code,l.debit_amount,l.credit_amount FROM journal_lines l JOIN accounts a ON a.id=l.account_id WHERE l.journal_entry_id=?1 ORDER BY l.id").map_err(invalid)?;
    let mut rows = query.query([journal]).map_err(invalid)?;
    let mut result = vec![];
    let mut debtors = 0;
    while let Some(row) = rows.next().map_err(invalid)? {
        let debit = legacy_money(row.get_ref(5).map_err(invalid)?)?;
        let booked_credit = legacy_money(row.get_ref(6).map_err(invalid)?)?;
        let (amount, opposite) = if credit {
            (debit, booked_credit)
        } else {
            (booked_credit, debit)
        };
        let code: Option<String> = row.get(4).map_err(invalid)?;
        if row.get::<_, i64>(1).map_err(invalid)? == debtor {
            if amount != Money::default() || &opposite != gross || code.is_some() {
                return Err(refused("simplified VAT debtor differs from source gross"));
            }
            debtors += 1;
            continue;
        }
        let kind: String = row.get(3).map_err(invalid)?;
        let revenue = kind == "income" && code.as_deref() == Some("DK_SALE_25");
        if opposite != Money::default()
            || amount.is_negative()
            || amount == Money::default()
            || !(revenue || (kind == "vat" && code.is_none()))
        {
            return Err(refused(
                "simplified VAT counter classification or direction differs",
            ));
        }
        result.push(SourceCounter {
            id: row.get(0).map_err(invalid)?,
            account: row.get(2).map_err(invalid)?,
            code,
            amount,
            revenue,
        });
    }
    if debtors != 1 {
        return Err(refused(
            "simplified VAT requires exactly one debtor counter",
        ));
    }
    Ok(result)
}

pub(super) fn exact_vat(tx: &Transaction<'_>) -> Result<BTreeMap<(i64, String), Money>, Error> {
    let mut result = BTreeMap::new();
    let mut query=tx.prepare("SELECT d.id,d.payload_json,d.invoice_date,d.currency,p.journal_entry_id,p.receivable_account_id,p.booked_gross_dkk FROM issued_invoice_postings p JOIN documents d ON d.id=p.invoice_document_id ORDER BY p.journal_entry_id").map_err(invalid)?;
    let mut rows = query.query([]).map_err(invalid)?;
    while let Some(row) = rows.next().map_err(invalid)? {
        let payload: Value =
            serde_json::from_str(&row.get::<_, String>(1).map_err(invalid)?).map_err(invalid)?;
        // Complete ex-VAT simplified lines retain ordinary line VAT arithmetic.
        if payload["invoiceType"] != "simplified"
            || !payload["lines"].as_array().is_some_and(|lines| {
                !lines.is_empty()
                    && lines.iter().all(|line| {
                        line.get("quantity").is_none() && line.get("unitPriceExVat").is_none()
                    })
            })
        {
            continue;
        }
        let source = rentemester_engine::sales_invoice::project(&payload)?;
        let valued = rentemester_engine::sales_fx::project(&payload)?;
        let id: i64 = row.get(0).map_err(invalid)?;
        let date: String = row.get(2).map_err(invalid)?;
        let currency: String = row.get(3).map_err(invalid)?;
        let journal: i64 = row.get(4).map_err(invalid)?;
        let debtor: i64 = row.get(5).map_err(invalid)?;
        if payload["currency"] != currency
            || payload["issueDate"] != date
            || legacy_money(row.get_ref(6).map_err(invalid)?)? != valued.gross
        {
            return Err(refused("simplified VAT source metadata differs"));
        }
        let original = counters(
            tx,
            journal,
            id,
            &date,
            &currency,
            debtor,
            &valued.gross,
            false,
            &payload,
        )?;
        let income: Vec<_> = original.iter().filter(|c| c.revenue).collect();
        let mut vat = Money::default();
        for c in original.iter().filter(|c| !c.revenue) {
            vat.add(&c.amount);
        }
        if income.len() != 1 || income[0].amount != valued.net || vat != valued.vat {
            return Err(refused(
                "simplified VAT counters differ from immutable invoice",
            ));
        }
        result.insert((journal, income[0].account.clone()), vat);
        let mut prior: Vec<_> = original.iter().map(|_| Money::default()).collect();
        let mut prior_dkk = Money::default();
        let mut prior_source = Money::default();
        let mut credits=tx.prepare("SELECT c.id,c.payload_json,c.invoice_date,c.currency,p.journal_entry_id,p.receivable_account_id,p.booked_gross_dkk FROM credit_note_postings p JOIN documents c ON c.id=p.credit_note_document_id WHERE p.original_invoice_document_id=?1 ORDER BY p.journal_entry_id").map_err(invalid)?;
        let mut credits = credits.query([id]).map_err(invalid)?;
        while let Some(credit) = credits.next().map_err(invalid)? {
            let body: Value = serde_json::from_str(&credit.get::<_, String>(1).map_err(invalid)?)
                .map_err(invalid)?;
            let amount = invoice_fx::amount(&body["grossAmount"])?;
            let delta = invoice_fx::portion(&source.gross, &valued.gross, &prior_source, &amount)?;
            let entry: i64 = credit.get(4).map_err(invalid)?;
            let credit_id: i64 = credit.get(0).map_err(invalid)?;
            let day: String = credit.get(2).map_err(invalid)?;
            if body["originalInvoiceDocumentId"] != id
                || body["issueDate"] != day
                || body["currency"] != currency
                || credit.get::<_, String>(3).map_err(invalid)? != currency
                || credit.get::<_, i64>(5).map_err(invalid)? != debtor
                || legacy_money(credit.get_ref(6).map_err(invalid)?)? != delta
                || invoice_fx::credit_dkk(&body, &currency, &amount)? != delta
            {
                return Err(refused(
                    "simplified VAT credit identity or carrying amount differs",
                ));
            }
            let inputs: Vec<_> = original
                .iter()
                .zip(&prior)
                .map(|(c, p)| Counter {
                    original: c.amount.clone(),
                    prior: p.clone(),
                    revenue: c.revenue,
                })
                .collect();
            let allocated = credit_note::allocate(&valued.gross, &prior_dkk, &delta, &inputs)?;
            let expected:Vec<_>=original.iter().zip(&prior).zip(&allocated).map(|((c,p),a)|json!({"journalLineId":c.id,"accountNo":c.account,"vatCode":c.code,"originalAmount":c.amount.decimal(),"priorAmount":p.decimal(),"allocatedAmount":a.decimal()})).collect();
            if body["creditAllocations"] != json!(expected) {
                return Err(refused("simplified VAT credit original allocations differ"));
            }
            let actual = counters(
                tx, entry, credit_id, &day, &currency, debtor, &delta, true, &body,
            )?;
            let expected: Vec<_> = original
                .iter()
                .zip(&allocated)
                .filter(|(_, a)| **a != Money::default())
                .collect();
            if actual.len() != expected.len()
                || actual.iter().zip(&expected).any(|(a, (s, amount))| {
                    a.account != s.account || a.code != s.code || a.amount != **amount
                })
            {
                return Err(refused(
                    "simplified VAT credit journal differs from immutable allocation",
                ));
            }
            let mut credit_vat = Money::default();
            for (_, a) in original.iter().zip(&allocated).filter(|(c, _)| !c.revenue) {
                credit_vat.add(a);
            }
            let vat_key = if currency == "DKK" {
                "vatAmount"
            } else {
                "vatAmountDkk"
            };
            if invoice_fx::amount(&body[vat_key])? != credit_vat {
                return Err(refused(
                    "simplified VAT credit amount differs from actual counter",
                ));
            }
            for (c, a) in original.iter().zip(&allocated).filter(|(c, _)| c.revenue) {
                if *a != Money::default() {
                    result.insert(
                        (entry, c.account.clone()),
                        Money::default().subtract(&credit_vat),
                    );
                }
            }
            for (p, a) in prior.iter_mut().zip(allocated) {
                p.add(&a);
            }
            prior_dkk.add(&delta);
            prior_source.add(&amount);
        }
    }
    Ok(result)
}
