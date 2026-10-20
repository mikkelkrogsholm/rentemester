//! Actual bank movement in exact DKK cents; no ledger, forecast or clock inference.
use crate::{Error, currency::Currency, fiscal_year::Date, fx::FxRate, money::Money};
use serde_json::{Value, json};

pub fn refused(message: impl Into<String>) -> Error {
    Error::new("cashflow_unavailable", message)
}
/// A retained row whose valuation has already been checked against its own FX evidence.
pub struct Movement {
    pub date: String,
    pub amount_dkk: Option<Money>,
}
/// Missing foreign valuation is unavailable, whereas contradictory retained proof is invalid.
pub fn valued_amount(
    currency: &str,
    amount: &Money,
    amount_dkk: Option<&Money>,
    rate: Option<&FxRate>,
) -> Result<Option<Money>, Error> {
    Currency::parse(currency)?;
    if currency == "DKK" {
        if amount_dkk.is_some_and(|dkk| dkk != amount) {
            return Err(refused(
                "DKK bank settlement differs from its retained valuation",
            ));
        }
        return Ok(Some(amount.clone()));
    }
    match (amount_dkk, rate) {
        (Some(dkk), Some(rate)) if rate.apply(amount) == *dkk => Ok(Some(dkk.clone())),
        (Some(_), Some(_)) => Err(refused(
            "foreign bank settlement and retained FX valuation differ",
        )),
        _ => Ok(None),
    }
}
fn iso(date: time::Date) -> String {
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        date.month() as u8,
        date.day()
    )
}
pub fn preceding_day(date: &str) -> Result<String, Error> {
    let date = crate::vat_period::date(date)?
        .previous_day()
        .ok_or_else(|| refused("cashflow opening date is outside the supported calendar"))?;
    let value = iso(date);
    Date::parse(&value)?;
    Ok(value)
}
struct Bucket {
    year: u16,
    month: u8,
    start: String,
    end: String,
    incoming: Money,
    outgoing: Money,
    unavailable: usize,
}
/// Twelve chronological fiscal months. Numeric wire is checked for exact cent roundtrip.
pub fn aggregate(start: &str, end: &str, movements: &[Movement]) -> Result<Value, Error> {
    Date::parse(start)?;
    Date::parse(end)?;
    if &start[8..] != "01" || start > end {
        return Err(refused(
            "cashflow requires an explicit complete fiscal-year descriptor",
        ));
    }
    let first_year: u16 = start[..4]
        .parse()
        .map_err(|_| refused("invalid fiscal year"))?;
    let first_month: u8 = start[5..7]
        .parse()
        .map_err(|_| refused("invalid fiscal month"))?;
    let mut buckets = Vec::with_capacity(12);
    for offset in 0..12u8 {
        let index = first_month - 1 + offset;
        let year = first_year
            .checked_add(u16::from(index / 12))
            .ok_or_else(|| refused("fiscal year exceeds the supported calendar"))?;
        let month = index % 12 + 1;
        let next_year = year
            .checked_add(u16::from(month == 12))
            .ok_or_else(|| refused("fiscal year exceeds the supported calendar"))?;
        let next_month = if month == 12 { 1 } else { month + 1 };
        let next = Date::new(next_year, next_month, 1)?.to_string();
        buckets.push(Bucket {
            year,
            month,
            start: Date::new(year, month, 1)?.to_string(),
            end: preceding_day(&next)?,
            incoming: Money::default(),
            outgoing: Money::default(),
            unavailable: 0,
        });
    }
    if buckets.last().unwrap().end != end {
        return Err(refused(
            "cashflow fiscal bounds do not cover exactly twelve months",
        ));
    }
    let mut incoming = Money::default();
    let mut outgoing = Money::default();
    let mut unavailable = 0;
    for movement in movements {
        Date::parse(&movement.date)?;
        let bucket = buckets
            .iter_mut()
            .find(|b| movement.date >= b.start && movement.date <= b.end)
            .ok_or_else(|| refused("bank movement lies outside the selected fiscal year"))?;
        if let Some(amount) = &movement.amount_dkk {
            if amount.is_negative() {
                let amount = Money::default().subtract(amount);
                bucket.outgoing.add(&amount);
                outgoing.add(&amount);
            } else {
                bucket.incoming.add(amount);
                incoming.add(amount);
            }
        } else {
            bucket.unavailable += 1;
            unavailable += 1;
        }
    }
    let months = buckets.into_iter().map(|b| {
        let known = b.unavailable == 0;
        Ok(json!({"year":b.year,"month":b.month,"label":format!("{:04}-{:02}",b.year,b.month),
            "periodStart":b.start,"periodEnd":b.end,"status":if known {"complete"} else {"unavailable"},
            "indbetalinger":if known {crate::sales_invoice::wire(&b.incoming)?} else {Value::Null},
            "udbetalinger":if known {crate::sales_invoice::wire(&b.outgoing)?} else {Value::Null},
            "netto":if known {crate::sales_invoice::wire(&b.incoming.subtract(&b.outgoing))?} else {Value::Null}}))
    }).collect::<Result<Vec<Value>,Error>>()?;
    Ok(
        json!({"months":months,"hasTransactions":!movements.is_empty(),
        "totalIn":if unavailable==0 {crate::sales_invoice::wire(&incoming)?} else {Value::Null},
        "totalOut":if unavailable==0 {crate::sales_invoice::wire(&outgoing)?} else {Value::Null},
        "status":if unavailable==0 {"complete"} else if unavailable==movements.len() {"unavailable"} else {"partial"},
        "coverage":{"transactionCount":movements.len(),"valuedTransactionCount":movements.len()-unavailable,"unvaluedTransactionCount":unavailable}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn movement(date: &str, amount: Option<&str>) -> Movement {
        Movement {
            date: date.into(),
            amount_dkk: amount.map(|s| Money::parse(s).unwrap()),
        }
    }
    #[test]
    fn shifted_months_and_exact_signed_cents_follow_the_fiscal_descriptor() {
        let value = aggregate(
            "2025-07-01",
            "2026-06-30",
            &[
                movement("2025-07-01", Some("0.01")),
                movement("2025-07-02", Some("0.02")),
                movement("2026-01-01", Some("-1.23")),
                movement("2026-06-30", Some("2.34")),
            ],
        )
        .unwrap();
        assert_eq!(value["months"].as_array().unwrap().len(), 12);
        assert_eq!(value["months"][0]["label"], "2025-07");
        assert_eq!(value["months"][6]["label"], "2026-01");
        assert_eq!(value["months"][11]["periodEnd"], "2026-06-30");
        assert_eq!(value["months"][0]["netto"], 0.03);
        assert_eq!(value["months"][6]["udbetalinger"], 1.23);
        assert_eq!(value["totalIn"], 2.37);
        assert_eq!(value["totalOut"], 1.23);
        assert_eq!(preceding_day("2024-03-01").unwrap(), "2024-02-29");
        assert!(aggregate("2025-07-01", "2025-12-31", &[]).is_err());
        assert!(
            aggregate(
                "2025-07-01",
                "2026-06-30",
                &[movement("2025-06-30", Some("1"))]
            )
            .is_err()
        );
    }
    #[test]
    fn unknown_valuation_never_becomes_zero_or_an_incomplete_total() {
        let value = aggregate(
            "2026-01-01",
            "2026-12-31",
            &[
                movement("2026-01-01", None),
                movement("2026-02-01", Some("2")),
            ],
        )
        .unwrap();
        assert_eq!(value["status"], "partial");
        assert!(value["months"][0]["indbetalinger"].is_null());
        assert_eq!(value["months"][1]["indbetalinger"], 2.0);
        assert!(value["totalIn"].is_null());
        assert_eq!(value["coverage"]["unvaluedTransactionCount"], 1);
        let empty = aggregate("2026-01-01", "2026-12-31", &[]).unwrap();
        assert_eq!(empty["hasTransactions"], false);
        assert_eq!(empty["totalIn"], 0.0);
    }
    #[test]
    fn fx_requires_actual_matching_evidence_and_numeric_wire_stays_exact() {
        let amount = Money::parse("-2").unwrap();
        let dkk = Money::parse("-15").unwrap();
        let rate = FxRate::parse("7.5").unwrap();
        assert_eq!(
            valued_amount("EUR", &amount, Some(&dkk), Some(&rate)).unwrap(),
            Some(dkk)
        );
        assert!(
            valued_amount("EUR", &amount, None, Some(&rate))
                .unwrap()
                .is_none()
        );
        assert!(
            valued_amount(
                "EUR",
                &amount,
                Some(&Money::parse("-14").unwrap()),
                Some(&rate)
            )
            .is_err()
        );
        assert!(valued_amount("DKK", &amount, Some(&Money::parse("2").unwrap()), None).is_err());
        assert!(
            aggregate(
                "2026-01-01",
                "2026-12-31",
                &[movement("2026-01-01", Some("999999999999999999999.99"))]
            )
            .is_err()
        );
    }
}
