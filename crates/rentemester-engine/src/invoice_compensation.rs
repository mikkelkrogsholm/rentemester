//! Pure source-compatible fixed late-compensation policy; no posting or delivery.
use crate::{Error, money::Money, vat_period::date};
use serde_json::{Value, json};
pub const CALCULATE_RULE: &str = "DK-INVOICE-LATE-COMPENSATION-001";
pub const REGISTER_RULE: &str = "DK-INVOICE-LATE-COMPENSATION-REGISTER-001";
pub const POST_RULE: &str = "DK-INVOICE-LATE-COMPENSATION-BOOKKEEPING-001";
pub const START_DATE: &str = "2013-03-01";
pub const MAXIMUM_DKK: &str = "310";
pub fn refused(message: impl Into<String>) -> Error {
    Error::new("invoice_compensation_refused", message)
}
/// Preserve the existing commercial-party contract: CVR/EU-style VAT, explicit
/// public recipient, or a 13-digit EAN/GLN. This does not claim a registry lookup.
pub fn commercial(buyer: &Value) -> bool {
    if buyer["publicRecipient"] == true {
        return true;
    }
    if let Some(raw) = buyer["vatOrCvr"].as_str() {
        let s = raw
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .to_ascii_uppercase();
        let bytes = s.as_bytes();
        if bytes.len() == 8 && bytes.iter().all(u8::is_ascii_digit)
            || bytes.len() == 10 && s.starts_with("DK") && bytes[2..].iter().all(u8::is_ascii_digit)
            || (4..=14).contains(&bytes.len())
                && bytes[..2].iter().all(u8::is_ascii_alphabetic)
                && bytes[2..].iter().all(u8::is_ascii_alphanumeric)
        {
            return true;
        }
    }
    buyer["eanNumber"].as_str().is_some_and(|raw| {
        let s = raw
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>();
        s.len() == 13 && s.bytes().all(|b| b.is_ascii_digit())
    })
}
pub struct Assessment {
    pub eligible: bool,
    pub is_commercial: bool,
    pub overdue_days: i64,
    pub amount_dkk: Money,
    pub reason: &'static str,
}
/// Principal is in its own invoice currency; only its sign is used. The fixed
/// compensation is always a separate DKK amount and is never added to it here.
pub fn assess(
    issue: &str,
    due: &str,
    on: &str,
    buyer: &Value,
    principal: &Money,
    requested: Option<Money>,
) -> Result<Assessment, Error> {
    let issued = date(issue)?;
    let due = date(due)?;
    let on = date(on)?;
    if due < issued {
        return Err(refused("invoice due date cannot predate issuance"));
    }
    let requested = requested.unwrap_or(Money::parse(MAXIMUM_DKK)?);
    if requested.is_negative()
        || Money::parse(MAXIMUM_DKK)?
            .subtract(&requested)
            .is_negative()
    {
        return Err(refused(
            "compensation amount must be nonnegative and at most310 DKK",
        ));
    }
    let business = commercial(buyer);
    let collectible = !principal.is_negative() && principal != &Money::default();
    let overdue = if collectible {
        (on - due).whole_days().max(0)
    } else {
        0
    };
    let reason = if !business {
        "buyer commercial identity is not established"
    } else if !collectible {
        "invoice has no collectible open balance"
    } else if overdue == 0 {
        "invoice is not overdue as of the requested date"
    } else if issue < START_DATE {
        "invoice predates compensation policy start date2013-03-01"
    } else {
        "eligible"
    };
    let eligible = reason == "eligible";
    Ok(Assessment {
        eligible,
        is_commercial: business,
        overdue_days: overdue,
        amount_dkk: if eligible {
            requested
        } else {
            Money::default()
        },
        reason,
    })
}
impl Assessment {
    pub fn value(&self) -> Result<Value, Error> {
        Ok(
            json!({"eligible":self.eligible,"isCommercialTransaction":self.is_commercial,"overdueDays":self.overdue_days,"compensationAmountDkk":crate::sales_invoice::wire(&self.amount_dkk)?,"reason":self.reason,"appliedRules":[CALCULATE_RULE],"errors":[]}),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn m(v: &str) -> Money {
        Money::parse(v).unwrap()
    }
    #[test]
    fn commercial_source_identifiers_and_private_rejection() {
        for buyer in [
            json!({"vatOrCvr":"dk 12345678"}),
            json!({"vatOrCvr":"DEAB123"}),
            json!({"eanNumber":"5790 0000 00000"}),
            json!({"publicRecipient":true}),
        ] {
            assert!(commercial(&buyer));
        }
        for buyer in [
            json!({"name":"Business sounding name"}),
            json!({"vatOrCvr":"private person"}),
            json!({"eanNumber":"123"}),
            json!({"publicRecipient":"true"}),
        ] {
            assert!(!commercial(&buyer));
        }
    }
    #[test]
    fn exact_default_cap_zero_and_currency_independent_principal() {
        let b = json!({"vatOrCvr":"DK12345678"});
        assert_eq!(
            assess(
                "2026-01-01",
                "2026-01-31",
                "2026-02-01",
                &b,
                &m("0.01"),
                None
            )
            .unwrap()
            .amount_dkk,
            m("310")
        );
        assert_eq!(
            assess(
                "2026-01-01",
                "2026-01-31",
                "2026-02-01",
                &b,
                &m("5000"),
                Some(m("30.05"))
            )
            .unwrap()
            .amount_dkk,
            m("30.05")
        );
        assert!(
            assess(
                "2026-01-01",
                "2026-01-31",
                "2026-02-01",
                &b,
                &m("1"),
                Some(m("310.01"))
            )
            .is_err()
        );
        assert!(
            assess(
                "2026-01-01",
                "2026-01-31",
                "2026-02-01",
                &b,
                &m("1"),
                Some(m("-0.01"))
            )
            .is_err()
        );
        assert_eq!(
            assess(
                "2026-01-01",
                "2026-01-31",
                "2026-02-01",
                &b,
                &m("1"),
                Some(m("0"))
            )
            .unwrap()
            .amount_dkk,
            m("0")
        );
    }
    #[test]
    fn start_due_principal_and_date_boundaries() {
        let b = json!({"publicRecipient":true});
        assert!(
            !assess("2013-02-28", "2013-03-10", "2013-03-11", &b, &m("1"), None)
                .unwrap()
                .eligible
        );
        assert!(
            assess(START_DATE, "2013-03-10", "2013-03-11", &b, &m("1"), None)
                .unwrap()
                .eligible
        );
        for principal in [m("0"), m("-1")] {
            assert!(
                !assess(
                    "2026-01-01",
                    "2026-01-31",
                    "2026-02-01",
                    &b,
                    &principal,
                    None
                )
                .unwrap()
                .eligible
            );
        }
        assert!(
            !assess("2026-01-01", "2026-01-31", "2026-01-31", &b, &m("1"), None)
                .unwrap()
                .eligible
        );
        assert!(assess("2026-02-30", "2026-03-01", "2026-03-02", &b, &m("1"), None).is_err());
        assert!(assess("2026-02-01", "2026-01-31", "2026-03-02", &b, &m("1"), None).is_err());
    }
}
