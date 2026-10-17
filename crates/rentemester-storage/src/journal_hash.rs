//! Byte compatibility for the existing persisted journal format, not full audit integrity.
use crate::{invalid, require_columns, safe_entry_id};
use rentemester_engine::{Error, money::Money};
use rusqlite::{Connection, Row, types::ValueRef};
use serde::Serialize;
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub ok: bool,
    pub entries_checked: usize,
    pub errors: Vec<String>,
}

// Declaration order is historical hash data. Do not replace with a sorted map.
#[derive(Serialize)]
struct Entry {
    id: i64,
    entry_no: String,
    transaction_date: String,
    text: String,
    source_bank_transaction_id: Option<i64>,
    document_id: Option<i64>,
    currency: String,
    amount_foreign: Option<Box<RawValue>>,
    amount_dkk: Option<Box<RawValue>>,
    fx_rate_to_dkk: Option<Box<RawValue>>,
    rule_version: String,
    created_by: String,
    created_by_program: String,
    status: String,
    reversal_of_entry_id: Option<i64>,
    lines: Vec<Line>,
}
#[derive(Serialize)]
struct Line {
    ordinal: usize,
    account_no: String,
    debit_amount: Box<RawValue>,
    credit_amount: Box<RawValue>,
    vat_code: Option<String>,
    text: Option<String>,
}

fn number(value: ValueRef<'_>) -> Result<f64, Error> {
    match value {
        ValueRef::Integer(n) if n.unsigned_abs() <= 9_007_199_254_740_991 => Ok(n as f64),
        ValueRef::Real(n) if n.is_finite() => Ok(n),
        _ => Err(invalid(
            "unsupported legacy hash numeric storage or precision",
        )),
    }
}
fn json_number(value: f64) -> Result<Box<RawValue>, Error> {
    if !value.is_finite() {
        return Err(invalid("legacy hash numeric normalization overflow"));
    }
    RawValue::from_string(ryu_js::Buffer::new().format_finite(value).to_owned()).map_err(invalid)
}
fn optional_number(value: ValueRef<'_>) -> Result<Option<Box<RawValue>>, Error> {
    if value == ValueRef::Null {
        Ok(None)
    } else {
        json_number(number(value)?).map(Some)
    }
}
fn line_amount(value: ValueRef<'_>) -> Result<Box<RawValue>, Error> {
    let value = number(value)?;
    if value < 0.0 {
        return Err(invalid("negative legacy journal line amount"));
    }
    let cents = Money::parse(ryu_js::Buffer::new().format_finite(value))
        .map_err(invalid)?
        .decimal()
        .replace('.', "");
    // Legacy roundDkk is Number(integer cents) / 100, not parseFloat(exact decimal).
    json_number(cents.parse::<f64>().map_err(invalid)? / 100.0)
}
fn optional_id(row: &Row<'_>, index: usize) -> Result<Option<i64>, Error> {
    row.get::<_, Option<i64>>(index)
        .map_err(invalid)?
        .map(safe_entry_id)
        .transpose()
}
fn read_entry(row: &Row<'_>, db: &Connection) -> Result<Entry, Error> {
    let id = safe_entry_id(row.get(0).map_err(invalid)?)?;
    let mut statement = db.prepare("SELECT a.account_no,jl.debit_amount,jl.credit_amount,jl.vat_code,jl.text FROM journal_lines jl JOIN accounts a ON a.id=jl.account_id WHERE jl.journal_entry_id=?1 ORDER BY jl.id").map_err(invalid)?;
    let mut rows = statement.query([id]).map_err(invalid)?;
    let mut lines = Vec::new();
    while let Some(line) = rows.next().map_err(invalid)? {
        lines.push(Line {
            ordinal: lines.len(),
            account_no: line.get(0).map_err(invalid)?,
            debit_amount: line_amount(line.get_ref(1).map_err(invalid)?)?,
            credit_amount: line_amount(line.get_ref(2).map_err(invalid)?)?,
            vat_code: line.get(3).map_err(invalid)?,
            text: line.get(4).map_err(invalid)?,
        });
    }
    Ok(Entry {
        id,
        entry_no: row.get(1).map_err(invalid)?,
        transaction_date: row.get(2).map_err(invalid)?,
        text: row.get(3).map_err(invalid)?,
        source_bank_transaction_id: optional_id(row, 4)?,
        document_id: optional_id(row, 5)?,
        currency: row.get(6).map_err(invalid)?,
        amount_foreign: optional_number(row.get_ref(7).map_err(invalid)?)?,
        amount_dkk: optional_number(row.get_ref(8).map_err(invalid)?)?,
        fx_rate_to_dkk: optional_number(row.get_ref(9).map_err(invalid)?)?,
        rule_version: row.get(10).map_err(invalid)?,
        created_by: row.get(11).map_err(invalid)?,
        created_by_program: row.get(12).map_err(invalid)?,
        status: row.get(13).map_err(invalid)?,
        reversal_of_entry_id: optional_id(row, 14)?,
        lines,
    })
}
pub(super) fn verify(db: &Connection) -> Result<Report, Error> {
    require_columns(
        db,
        "journal_entries",
        &[
            ("entry_no", "TEXT", 1, 0),
            ("text", "TEXT", 1, 0),
            ("source_bank_transaction_id", "INTEGER", 0, 0),
            ("document_id", "INTEGER", 0, 0),
            ("currency", "TEXT", 1, 0),
            ("amount_foreign", "NUMERIC", 0, 0),
            ("amount_dkk", "NUMERIC", 0, 0),
            ("fx_rate_to_dkk", "NUMERIC", 0, 0),
            ("rule_version", "TEXT", 1, 0),
            ("created_by", "TEXT", 1, 0),
            ("created_by_program", "TEXT", 1, 0),
            ("status", "TEXT", 1, 0),
            ("reversal_of_entry_id", "INTEGER", 0, 0),
            ("previous_hash", "TEXT", 0, 0),
            ("entry_hash", "TEXT", 1, 0),
        ],
    )?;
    require_columns(
        db,
        "journal_lines",
        &[("vat_code", "TEXT", 0, 0), ("text", "TEXT", 0, 0)],
    )?;
    let mut statement = db.prepare("SELECT id,entry_no,transaction_date,text,source_bank_transaction_id,document_id,currency,amount_foreign,amount_dkk,fx_rate_to_dkk,rule_version,created_by,created_by_program,status,reversal_of_entry_id,previous_hash,entry_hash FROM journal_entries ORDER BY id").map_err(invalid)?;
    let mut rows = statement.query([]).map_err(invalid)?;
    let mut previous = "GENESIS".to_owned();
    let mut errors = Vec::new();
    let mut count = 0;
    while let Some(row) = rows.next().map_err(invalid)? {
        let entry = read_entry(row, db)?;
        let previous_hash: Option<String> = row.get(15).map_err(invalid)?;
        let stored: String = row.get(16).map_err(invalid)?;
        let bytes = serde_json::to_vec(&entry).map_err(invalid)?;
        let expected = format!(
            "{:x}",
            Sha256::new()
                .chain_update(bytes)
                .chain_update(previous.as_bytes())
                .finalize()
        );
        if entry.lines.is_empty() {
            errors.push(format!("{}: entry has no journal lines", entry.entry_no));
        }
        if previous_hash.as_deref() != Some(&previous) {
            errors.push(format!("{}: previous_hash mismatch", entry.entry_no));
        }
        if stored != expected {
            errors.push(format!("{}: entry_hash mismatch", entry.entry_no));
        }
        previous = stored;
        count += 1;
    }
    Ok(Report {
        ok: errors.is_empty(),
        entries_checked: count,
        errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_json_numbers_and_rounding_bytes() {
        for (value, text) in [
            (1e21, "1e+21"),
            (1e-6, "0.000001"),
            (1e-7, "1e-7"),
            (-0.0, "0"),
        ] {
            assert_eq!(json_number(value).unwrap().get(), text);
        }
        for (value, text) in [
            (1.005, "1.01"),
            (1.0049999999999997, "1"),
            (0.004999999999999999, "0"),
            (999999999999.9949, "999999999999.99"),
        ] {
            assert_eq!(line_amount(ValueRef::Real(value)).unwrap().get(), text);
        }
        for value in [
            ValueRef::Real(f64::INFINITY),
            ValueRef::Real(f64::NAN),
            ValueRef::Real(-1.0),
            ValueRef::Integer(9_007_199_254_740_993),
            ValueRef::Null,
            ValueRef::Text(b"1"),
        ] {
            assert!(line_amount(value).is_err());
        }
        assert!(line_amount(ValueRef::Real(f64::MAX)).is_err());
    }
}
