//! Dated OSS base preview from the native VAT projection; no foreign tax or submission.
use crate::invalid;
use rentemester_engine::{Error, fiscal_year::Date, vat_period};
use rusqlite::Transaction;
use serde_json::{Value, json};

pub fn read(tx: &Transaction<'_>, from: Date, to: Date) -> Result<Value, Error> {
    let start = from.to_string();
    let end = to.to_string();
    if from > to || (vat_period::date(&end)? - vat_period::date(&start)?).whole_days() > 3660 {
        return Err(Error::new(
            "invalid_date_range",
            "OSS range must be ordered and at most ten years",
        ));
    }
    let cadence: Option<String> = tx
        .query_row(
            "SELECT vat_period_type FROM companies WHERE id=1",
            [],
            |r| r.get(0),
        )
        .map_err(invalid)?;
    let Some(cadence) = cadence else {
        return Err(Error::new(
            "vat_registration_required",
            "OSS report requires a VAT-registered company",
        ));
    };
    vat_period::span(&cadence)?;
    let mut vat = crate::vat_report::range(tx, &start, &end)?;
    let errors = vat["errors"]
        .as_array()
        .ok_or_else(|| invalid("native VAT report errors missing"))?
        .clone();
    let ok = errors.is_empty();
    vat["ok"] = json!(ok);
    let mut warnings = if ok {
        vat["warnings"]
            .as_array()
            .ok_or_else(|| invalid("native VAT report warnings missing"))?
            .clone()
    } else {
        Vec::new()
    };
    let base = if ok {
        vat["ossConsumerSalesBase"].clone()
    } else {
        json!(0)
    };
    let count = if ok {
        vat["ossConsumerSalesEntryCount"].clone()
    } else {
        json!(0)
    };
    if ok
        && base
            .as_f64()
            .ok_or_else(|| invalid("native OSS base missing"))?
            > 0.0
    {
        warnings.push(json!("Rapporten viser kun OSS-grundlaget. Den fordeler ikke på destinationsland eller momssats, beregner ikke udenlandsk moms og indsender ikke en OSS-angivelse. Indberetning er virksomhedens ansvar."));
    }
    Ok(json!({
        "ok": ok,
        "appliedRules": ["DK-VAT-OSS-001"],
        "periodStart": start,
        "periodEnd": end,
        "ossConsumerSalesBase": base,
        "entryCount": count,
        "submission": false,
        "vatReport": vat,
        "warnings": warnings,
        "errors": errors
    }))
}

#[cfg(test)]
mod tests;
