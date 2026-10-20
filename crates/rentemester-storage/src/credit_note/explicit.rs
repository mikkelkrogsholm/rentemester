//! Per-original-line credit evidence; repeated account/code pairs remain distinct.
use super::*;
#[derive(Debug)]
struct OriginalCounter {
    id: i64,
    account: String,
    code: Option<String>,
    original: Money,
    revenue: bool,
}
fn journal_counters(
    tx: &Transaction<'_>,
    journal: i64,
    debtor: &str,
    gross: &Money,
    credit: bool,
) -> Result<Vec<OriginalCounter>, Error> {
    let mut q = tx.prepare("SELECT jl.id,a.account_no,a.type,jl.debit_amount,jl.credit_amount,jl.vat_code FROM journal_lines jl JOIN accounts a ON a.id=jl.account_id WHERE jl.journal_entry_id=?1 ORDER BY jl.id").map_err(invalid)?;
    let mut rows = q.query([journal]).map_err(invalid)?;
    let mut result = vec![];
    let mut receivable = Money::default();
    let mut debtor_count = 0;
    while let Some(row) = rows.next().map_err(invalid)? {
        let account: String = row.get(1).map_err(invalid)?;
        let kind: String = row.get(2).map_err(invalid)?;
        let debit = legacy_money(row.get_ref(3).map_err(invalid)?)?;
        let credit_amount = legacy_money(row.get_ref(4).map_err(invalid)?)?;
        let code: Option<String> = row.get(5).map_err(invalid)?;
        let (counter, opposite) = if credit {
            (&debit, &credit_amount)
        } else {
            (&credit_amount, &debit)
        };
        if account == debtor {
            if *counter != Money::default() || code.is_some() {
                return Err(refused("invalid original debtor counter"));
            }
            receivable.add(opposite);
            debtor_count += 1;
        } else {
            if *opposite != Money::default()
                || counter.is_negative()
                || *counter == Money::default()
                || !((kind == "income"
                    && matches!(
                        code.as_deref(),
                        Some("DK_SALE_25" | "DK_SALE_EXEMPT" | "DOMESTIC_REVERSE_CHARGE_EXEMPT")
                    ))
                    || (kind == "vat" && code.is_none()))
            {
                return Err(refused(
                    "invalid original invoice counter direction or VAT classification",
                ));
            }
            result.push(OriginalCounter {
                id: row.get(0).map_err(invalid)?,
                account,
                code,
                original: counter.clone(),
                revenue: kind == "income",
            });
        }
    }
    if debtor_count != 1 || &receivable != gross {
        return Err(refused("original debtor does not match linked gross"));
    }
    Ok(result)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn lines(
    tx: &Transaction<'_>,
    id: i64,
    journal: i64,
    debtor_id: i64,
    debtor: &str,
    number: &str,
    projection: &rentemester_engine::sales_invoice::TaxProjection,
    prior_total: &Money,
    amount: &Money,
) -> Result<(Vec<JournalLine>, Money, Value), Error> {
    let original = journal_counters(tx, journal, debtor, &projection.gross, false)?;
    let expected: Vec<_> = projection
        .lines
        .iter()
        .filter(|line| line.net != Money::default())
        .collect();
    let income: Vec<_> = original.iter().filter(|row| row.revenue).collect();
    if income.len() != expected.len()
        || income.iter().zip(expected).any(|(row, line)| {
            row.original != line.net || row.code.as_deref() != Some(line.vat_code)
        })
    {
        return Err(refused(
            "original journal bases do not match immutable invoice lines",
        ));
    }
    let mut original_vat = Money::default();
    for row in original.iter().filter(|row| !row.revenue) {
        original_vat.add(&row.original);
    }
    if original_vat != projection.vat {
        return Err(refused(
            "original VAT counters do not match immutable invoice",
        ));
    }
    let mut prior: Vec<Money> = original.iter().map(|_| Money::default()).collect();
    let mut q=tx.prepare("SELECT c.payload_json,p.journal_entry_id,p.receivable_account_id,p.booked_gross_dkk FROM credit_note_postings p JOIN documents c ON c.id=p.credit_note_document_id WHERE p.original_invoice_document_id=?1 ORDER BY p.journal_entry_id").map_err(invalid)?;
    let mut rows = q.query([id]).map_err(invalid)?;
    let mut verified_prior = Money::default();
    while let Some(row) = rows.next().map_err(invalid)? {
        if row.get::<_, i64>(2).map_err(invalid)? != debtor_id {
            return Err(refused("prior credit changed original debtor"));
        }
        let gross = legacy_money(row.get_ref(3).map_err(invalid)?)?;
        let payload: Value =
            serde_json::from_str(&row.get::<_, String>(0).map_err(invalid)?).map_err(invalid)?;
        let saved = payload["creditAllocations"]
            .as_array()
            .filter(|a| a.len() == original.len())
            .ok_or_else(|| refused("prior credit lacks original-line allocation evidence"))?;
        if payload["originalInvoiceDocumentId"].as_i64() != Some(id) {
            return Err(refused("prior credit refers to another invoice"));
        }
        let actual = journal_counters(tx, row.get(1).map_err(invalid)?, debtor, &gross, true)?;
        let mut actual = actual.iter();
        let mut sum = Money::default();
        for ((entry, source), before) in saved.iter().zip(&original).zip(prior.iter_mut()) {
            let parse = |name: &str| {
                Money::parse_canonical(
                    entry[name]
                        .as_str()
                        .ok_or_else(|| refused("invalid credit allocation amount"))?,
                )
            };
            let allocated = parse("allocatedAmount")?;
            if entry["journalLineId"].as_i64() != Some(source.id)
                || entry["accountNo"] != source.account
                || entry["vatCode"] != json!(source.code)
                || parse("originalAmount")? != source.original
                || parse("priorAmount")? != *before
                || allocated.is_negative()
            {
                return Err(refused(
                    "prior credit allocation identity or cumulative amount differs",
                ));
            }
            if allocated != Money::default() {
                let posted = actual
                    .next()
                    .ok_or_else(|| refused("missing posted credit counter"))?;
                if posted.account != source.account
                    || posted.code != source.code
                    || posted.original != allocated
                {
                    return Err(refused("posted credit differs from saved allocation"));
                }
            }
            before.add(&allocated);
            sum.add(&allocated);
            if source.original.subtract(before).is_negative() {
                return Err(refused("prior credit exceeds original counter"));
            }
        }
        if actual.next().is_some() || sum != gross {
            return Err(refused("prior credit counters do not reconcile"));
        }
        verified_prior.add(&gross);
    }
    if &verified_prior != prior_total {
        return Err(refused(
            "prior allocation totals differ from linked credits",
        ));
    }
    let counters: Vec<_> = original
        .iter()
        .zip(&prior)
        .map(|(row, prior)| Counter {
            original: row.original.clone(),
            prior: prior.clone(),
            revenue: row.revenue,
        })
        .collect();
    let allocated = credit_note::allocate(&projection.gross, prior_total, amount, &counters)?;
    let mut lines = vec![JournalLine {
        account_no: AccountNo::parse(debtor)?,
        debit_amount: Money::default(),
        credit_amount: amount.clone(),
        vat_code: None,
        text: Some(format!("Kreditering af {number}")),
    }];
    let mut vat = Money::default();
    let mut evidence = vec![];
    for ((source, prior), allocated) in original.iter().zip(prior).zip(allocated) {
        evidence.push(json!({"journalLineId":source.id,"accountNo":source.account,"vatCode":source.code,"originalAmount":source.original.decimal(),"priorAmount":prior.decimal(),"allocatedAmount":allocated.decimal()}));
        if !source.revenue {
            vat.add(&allocated);
        }
        if allocated != Money::default() {
            lines.push(JournalLine {
                account_no: AccountNo::parse(&source.account)?,
                debit_amount: allocated,
                credit_amount: Money::default(),
                vat_code: source.code.clone(),
                text: Some(format!("Kreditering af {number}")),
            });
        }
    }
    Ok((lines, vat, json!(evidence)))
}
