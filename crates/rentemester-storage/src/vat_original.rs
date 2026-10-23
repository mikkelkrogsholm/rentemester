//! Frozen original VAT read ABI. The admitted caller owns the snapshot and physical-source checks.
//! Historical imports deliberately retain the native fail-closed, unported source-validation boundary.
use crate::invalid;
use rentemester_engine::{Error, fiscal_year::Date, sales_invoice::wire, vat_period, vat_report};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};

fn cadence(tx: &Transaction<'_>) -> Result<Option<String>, Error> {
    tx.query_row(
        "SELECT vat_period_type FROM companies WHERE id=1",
        [],
        |r| r.get(0),
    )
    .map_err(invalid)
}
fn nonregistered() -> Value {
    json!({"ok":false,"errors":["selskabet er ikke momsregistreret"]})
}
fn range_errors(from: &str, to: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let start = vat_period::date(from);
    let end = vat_period::date(to);
    if start.is_err() {
        errors.push("periodStart must be YYYY-MM-DD".into());
    }
    if end.is_err() {
        errors.push("periodEnd must be YYYY-MM-DD".into());
    }
    if let (Ok(start), Ok(end)) = (start, end) {
        if start > end {
            errors.push("periodStart must be before or equal to periodEnd".into());
        } else if (end - start).whole_days() > 3660 {
            errors.push("VAT range must be ordered and at most ten years".into());
        }
    }
    errors
}
fn rubrics(r: &vat_report::Rubrics) -> Result<Value, Error> {
    Ok(
        json!({"salgsmoms":wire(&r.salgsmoms)?,"momsAfVarekobUdland":wire(&r.moms_af_varekob_udland)?,
        "momsAfYdelseskobUdland":wire(&r.moms_af_ydelseskob_udland)?,"kobsmoms":wire(&r.kobsmoms)?,
        "momstilsvar":wire(&r.momstilsvar)?,"rubrikA":wire(&r.rubrik_a)?,"rubrikB":wire(&r.rubrik_b)?,"rubrikC":wire(&r.rubrik_c)?}),
    )
}
/// All posted/reversed range rows form three disjoint metadata groups. Global reversed
/// status alone must never retroactively change an earlier reporting window.
fn counts(tx: &Transaction<'_>, from: &str, to: &str, result: &mut Value) -> Result<(), Error> {
    let mut query = tx.prepare("SELECT j.id,j.reversal_of_entry_id,COUNT(l.id) FROM journal_entries j JOIN journal_lines l ON l.journal_entry_id=j.id WHERE j.status IN ('posted','reversed') AND j.transaction_date>=?1 AND j.transaction_date<=?2 GROUP BY j.id,j.reversal_of_entry_id ORDER BY j.id").map_err(invalid)?;
    let rows = query
        .query_map(params![from, to], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<i64>>(1)?,
                r.get::<_, u64>(2)?,
            ))
        })
        .map_err(invalid)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(invalid)?;
    let reversed = rows
        .iter()
        .filter_map(|(_, original, _)| *original)
        .collect::<std::collections::BTreeSet<_>>();
    let mut entries = [0u64; 3];
    let mut lines = [0u64; 3];
    for (id, original, count) in rows {
        let group = if original.is_some() {
            2
        } else if reversed.contains(&id) {
            1
        } else {
            0
        };
        entries[group] += 1;
        lines[group] += count;
    }
    for (key, value) in [
        ("journalEntryCount", entries[0]),
        ("reversedJournalEntryCount", entries[1]),
        ("reversalJournalEntryCount", entries[2]),
        ("totalJournalEntryCount", entries.iter().sum()),
        ("linesConsidered", lines[0]),
        ("reversedLinesConsidered", lines[1]),
        ("reversalLinesConsidered", lines[2]),
        ("totalLinesConsidered", lines.iter().sum()),
    ] {
        result[key] = json!(value);
    }
    Ok(())
}

pub fn report(tx: &Transaction<'_>, from: &str, to: &str) -> Result<Value, Error> {
    let Some(cadence) = cadence(tx)? else {
        return Ok(nonregistered());
    };
    report_registered(tx, from, to, &cadence)
}
fn report_registered(
    tx: &Transaction<'_>,
    from: &str,
    to: &str,
    cadence: &str,
) -> Result<Value, Error> {
    let errors = range_errors(from, to);
    let mut result = json!({"ok":false,"appliedRules":["DK-VAT-REPORT-001"],"periodStart":from,"periodEnd":to,
        "vatPeriodType":cadence,"filingDeadline":null,"warnings":[],"errors":errors});
    // Invalid selectors never read financial facts; their complete report is deterministically zero.
    let empty = vat_report::build(&[])?;
    let native = if errors.is_empty() {
        crate::vat_report::range_report(tx, from, to)?
    } else {
        empty
    };
    let mut output = native.output_vat.clone();
    output.add(&native.output_vat_adjustment);
    for (key, amount) in [
        ("outputVat", &output),
        ("inputVat", &native.input_vat),
        ("netVatPayable", &native.payable),
        ("ossConsumerSalesBase", &native.oss_consumer_sales_base),
        (
            "euGoodsAcquisitionOutputVat",
            &native.rubrikker.moms_af_varekob_udland,
        ),
        (
            "reverseChargePurchaseOutputVat",
            &native.reverse_charge_purchase_output_vat,
        ),
    ] {
        result[key] = wire(amount)?;
    }
    for (key, amount) in &native.canonical_bases {
        result[key] = wire(amount)?;
    }
    let mut reverse_sales = native.canonical_bases["foreignReverseChargeSalesBase"].clone();
    reverse_sales.add(&native.canonical_bases["domesticReverseChargeSalesBase"]);
    result["reverseChargeSalesBase"] = wire(&reverse_sales)?;
    result["ossConsumerSalesEntryCount"] = json!(native.oss_consumer_sales_entry_count);
    result["rubrikker"] = rubrics(&native.rubrikker)?;
    for key in [
        "journalEntryCount",
        "reversedJournalEntryCount",
        "reversalJournalEntryCount",
        "totalJournalEntryCount",
        "linesConsidered",
        "reversedLinesConsidered",
        "reversalLinesConsidered",
        "totalLinesConsidered",
    ] {
        result[key] = json!(0);
    }
    if errors.is_empty() {
        let window = vat_period::window(from, cadence)?;
        if window.start == from && window.end == to {
            result["filingDeadline"] = json!(window.deadline);
        }
        counts(tx, from, to, &mut result)?;
        result["ok"] = json!(native.errors.is_empty());
        result["errors"] = json!(native.errors);
        result["warnings"] = json!(native.warnings);
    }
    Ok(result)
}

pub fn eu_list(tx: &Transaction<'_>, from: &str, to: &str) -> Result<Value, Error> {
    if cadence(tx)?.is_none() {
        return Ok(nonregistered());
    }
    let errors = range_errors(from, to);
    if !errors.is_empty() {
        return Ok(
            json!({"ok":false,"appliedRules":["DK-VAT-EU-SALES-LIST-001"],"periodStart":from,"periodEnd":to,"customers":[],"totalValue":0,"invoiceCount":0,"creditNoteCount":0,"sources":[],"warnings":[],"errors":errors}),
        );
    }
    // Preserve native evidence failures (and their recognized codes), not guessed success/FX values.
    crate::eu_sales::read(tx, Date::parse(from)?, Date::parse(to)?)
}

pub fn oss(tx: &Transaction<'_>, from: &str, to: &str) -> Result<Value, Error> {
    if cadence(tx)?.is_none() {
        return Ok(nonregistered());
    }
    let vat = report(tx, from, to)?;
    if vat["ok"] != true {
        return Ok(
            json!({"ok":false,"appliedRules":["DK-VAT-OSS-001"],"periodStart":from,"periodEnd":to,"ossConsumerSalesBase":0,"entryCount":0,"submission":false,"warnings":[],"errors":vat["errors"],"vatReport":vat}),
        );
    }
    // Reuse native preview warnings and semantics; replace only the abbreviated report.
    let mut result = crate::oss_report::read(tx, Date::parse(from)?, Date::parse(to)?)?;
    result["vatReport"] = vat;
    Ok(result)
}

pub fn filing(tx: &Transaction<'_>, from: &str, to: &str) -> Result<Value, Error> {
    let Some(cadence) = cadence(tx)? else {
        return Ok(nonregistered());
    };
    let vat = report_registered(tx, from, to, &cadence)?;
    let mut result = json!({"ok":false,"appliedRules":["DK-VAT-FILING-001"],"periodStart":from,"periodEnd":to,"vatPeriodType":cadence,
        "periodStatus":"open","periodReference":null,"filingDeadline":vat["filingDeadline"],"rubrikker":rubrics(&vat_report::Rubrics::default())?,"warnings":[],"errors":[],"vatReport":vat});
    let mut errors = range_errors(from, to);
    if errors.is_empty() {
        let window = vat_period::window(from, &cadence)?;
        if window.start != from || window.end != to {
            errors.push(format!("VAT period {from}..{to} does not match the company's registered {cadence} cadence; the canonical period containing {from} is {}..{}",window.start,window.end));
        } else {
            let row: Option<(i64,String,Option<String>)> = tx.query_row("SELECT id,status,reference FROM accounting_periods WHERE period_start=?1 AND period_end=?2 AND kind IN ('vat_period','vat_quarter') ORDER BY CASE kind WHEN 'vat_period' THEN 0 ELSE 1 END,id DESC LIMIT 1",params![from,to],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(invalid)?;
            let state = if let Some((id, state, reference)) = row {
                result["periodReference"] = json!(reference);
                crate::periods::effective(tx, id, state)?
            } else {
                "open".into()
            };
            if !["open", "closed", "reported"].contains(&state.as_str()) {
                return Err(invalid("unknown VAT period state"));
            }
            result["periodStatus"] = json!(state);
            errors.extend(
                result["vatReport"]["errors"]
                    .as_array()
                    .ok_or_else(|| invalid("VAT errors missing"))?
                    .iter()
                    .map(|v| v.as_str().unwrap_or("invalid VAT error").to_owned()),
            );
            if state == "open" {
                errors.push(format!("VAT period {from}..{to} is not closed: a momsangivelse requires a closed or reported vat_period accounting period covering exactly this period — run 'period close --kind vat_period' first"));
            }
        }
    }
    if errors.is_empty() {
        result["ok"] = json!(true);
        result["rubrikker"] = result["vatReport"]["rubrikker"].clone();
        result["warnings"] = result["vatReport"]["warnings"].clone();
    }
    result["errors"] = json!(errors);
    Ok(result)
}

#[cfg(test)]
mod tests;
