//! Port of DK-INVOICE-REMINDER-FEE-001 and deterministic claim receipt allocation.
//! No delivery, automatic reminders, interest or compensation policy.
use crate::{Error, money::Money, vat_period::date};
use std::collections::BTreeMap;
pub const REGISTER_RULE: &str = "DK-INVOICE-REMINDER-FEE-001";
pub const POST_RULE: &str = "DK-INVOICE-REMINDER-FEE-BOOKKEEPING-001";
pub const SETTLE_RULE: &str = "DK-INVOICE-CLAIM-SETTLEMENT-001";
pub fn refused(s: impl Into<String>) -> Error {
    Error::new("reminder_claim_refused", s)
}
/// Existing invoice-status compatibility fallback: issue date plus30 days only when no explicit due date persisted.
pub fn effective_due_date(issue: &str, explicit: Option<&str>) -> Result<String, Error> {
    if let Some(d) = explicit {
        date(d)?;
        return Ok(d.into());
    }
    let d = date(issue)?
        .checked_add(time::Duration::days(30))
        .ok_or_else(|| refused("invoice due date exceeds supported calendar"))?;
    if !(0..=9999).contains(&d.year()) {
        return Err(refused("invoice due date exceeds supported calendar"));
    }
    Ok(format!(
        "{:04}-{:02}-{:02}",
        d.year(),
        u8::from(d.month()),
        d.day()
    ))
}
pub fn registration(
    due: &str,
    on: &str,
    principal: &Money,
    requested: Option<Money>,
    previous: &[String],
) -> Result<Money, Error> {
    let day = date(on)?;
    if day <= date(due)? || principal.is_negative() || principal == &Money::default() {
        return Err(refused(
            "invoice must be overdue with positive principal on reminderDate",
        ));
    }
    let fee = requested.unwrap_or(Money::parse("100")?);
    if fee.is_negative()
        || fee == Money::default()
        || Money::parse("100")?.subtract(&fee).is_negative()
    {
        return Err(refused(
            "reminder fee must be positive and cannot exceed existing rule maximum100 DKK",
        ));
    }
    if previous.len() >= 3 {
        return Err(refused(
            "cannot register more than3 reminder fees for one invoice claim",
        ));
    }
    if let Some(last) = previous.iter().max()
        && (day - date(last)?).whole_days() < 10
    {
        return Err(refused(
            "reminderDate must be at least10 days after previous reminder",
        ));
    }
    Ok(fee)
}
pub fn allocate(
    balances: &BTreeMap<String, Money>,
    amount: &Money,
) -> Result<BTreeMap<String, Money>, Error> {
    if amount.is_negative() || amount == &Money::default() {
        return Err(refused("claim receipt amount must be positive"));
    }
    let mut remaining = amount.clone();
    let mut result = BTreeMap::new();
    for (account, available) in balances {
        if available.is_negative() {
            return Err(refused("claim receivable is over-cleared"));
        }
        if remaining == Money::default() {
            break;
        }
        let used = if available.subtract(&remaining).is_negative() {
            available.clone()
        } else {
            remaining.clone()
        };
        if used != Money::default() {
            remaining = remaining.subtract(&used);
            result.insert(account.clone(), used);
        }
    }
    if remaining != Money::default() {
        return Err(refused("claim receipt exceeds ledger-backed claim balance"));
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn existing_caps_spacing_dates_and_allocation() {
        assert_eq!(
            effective_due_date("2026-01-01", None).unwrap(),
            "2026-01-31"
        );
        let m = |s| Money::parse(s).unwrap();
        assert_eq!(
            registration("2026-01-01", "2026-01-02", &m("1"), None, &[]).unwrap(),
            m("100")
        );
        for fee in ["0", "100.01", "-1"] {
            assert!(registration("2026-01-01", "2026-01-02", &m("1"), Some(m(fee)), &[]).is_err());
        }
        assert!(registration("2026-01-01", "2026-01-01", &m("1"), None, &[]).is_err());
        assert!(
            registration(
                "2026-01-01",
                "2026-01-20",
                &m("1"),
                None,
                &["2026-01-11".into()]
            )
            .is_err()
        );
        assert!(
            registration(
                "2026-01-01",
                "2026-02-01",
                &m("1"),
                None,
                &[
                    "2026-01-02".into(),
                    "2026-01-12".into(),
                    "2026-01-22".into()
                ]
            )
            .is_err()
        );
        let b = BTreeMap::from([("A".into(), m("30.01")), ("B".into(), m("70"))]);
        assert_eq!(allocate(&b, &m("50")).unwrap()["B"], m("19.99"));
        assert!(allocate(&b, &m("100.02")).is_err());
    }
}
