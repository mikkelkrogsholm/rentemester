use super::*;
use rusqlite::types::{Value as Cell, ValueRef};
pub(super) fn value(x: &ArchiveDecimal) -> Result<Cell, Error> {
    let mut n = x.decimal().parse::<f64>().map_err(sql)?;
    if !n.is_finite()
        || n.fract() == 0.0 && n.abs() > 9_007_199_254_740_991f64
        || !x.matches_numeric_decimal(ryu_js::Buffer::new().format_finite(n))
    {
        return Err(policy::refused(
            "source amount cannot roundtrip exact six-decimal/native Money storage",
        ));
    }
    if n == 0.0 {
        n = 0.0;
    }
    if n.fract() == 0.0 {
        Ok(Cell::Integer(n as i64))
    } else {
        Ok(Cell::Real(n))
    }
}
pub(super) fn verify(cell: ValueRef<'_>, x: &ArchiveDecimal) -> Result<(), Error> {
    let expected = value(x)?;
    let valid = match (cell, &expected) {
        (ValueRef::Integer(a), Cell::Integer(b)) => a == *b,
        (ValueRef::Real(a), Cell::Real(b)) => a.is_finite() && a.to_bits() == b.to_bits(),
        (ValueRef::Real(a), Cell::Integer(b)) => a.is_finite() && a == *b as f64,
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(policy::refused(
            "archive numeric cell differs from exact source",
        ))
    }
}
pub(super) fn check_year(year: &Year) -> Result<(), Error> {
    for p in &year.postings {
        value(&p.amount)?;
        if let Some(v) = &p.running_balance {
            value(v)?;
        }
    }
    for b in &year.balances {
        value(&b.amount)?;
    }
    Ok(())
}
