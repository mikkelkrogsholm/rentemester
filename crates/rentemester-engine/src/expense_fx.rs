//! Exact valuation from the imported payment; no implicit exchange-rate lookup.
use crate::{
    Error,
    currency::Currency,
    document::{VatLine, money},
    fx::FxRate,
    money::Money,
};
pub struct Payment<'a> {
    pub currency: &'a str,
    pub amount: &'a Money,
    pub amount_dkk: Option<&'a Money>,
    pub rate: Option<&'a FxRate>,
}
pub struct Basis {
    pub gross_dkk: Money,
    pub rate: FxRate,
    pub rate_source: Option<&'static str>,
}
fn fail(message: &str) -> Error {
    Error::new("expense_fx_refused", message)
}
fn abs(amount: &Money) -> Money {
    if amount.is_negative() {
        Money::default().subtract(amount)
    } else {
        amount.clone()
    }
}
pub fn resolve(currency: &str, gross: &Money, payment: Payment<'_>) -> Result<Basis, Error> {
    Currency::parse(currency)?;
    Currency::parse(payment.currency)?;
    if gross.is_negative() || gross == &Money::default() || !payment.amount.is_negative() {
        return Err(fail(
            "positive voucher gross and an outgoing bank payment required",
        ));
    }
    let paid = abs(payment.amount);
    if currency == "DKK" {
        if payment.currency != "DKK" || &paid != gross {
            return Err(fail("DKK voucher requires equal outgoing DKK bank payment"));
        }
        return Ok(Basis {
            gross_dkk: paid,
            rate: FxRate::parse("1")?,
            rate_source: None,
        });
    }
    let (settlement, rate, source) = if payment.currency == "DKK" {
        if payment.amount_dkk.is_some_and(|value| abs(value) != paid) {
            return Err(fail("bank amount_dkk differs from DKK settlement"));
        }
        let rate = payment
            .rate
            .cloned()
            .map(Ok)
            .unwrap_or_else(|| FxRate::from_settlement(gross, &paid))?;
        let source = if payment.rate.is_some() {
            "imported_bank"
        } else {
            "derived_dkk_settlement"
        };
        (paid, rate, source)
    } else {
        if payment.currency != currency || &paid != gross {
            return Err(fail(
                "foreign payment currency and amount must match voucher",
            ));
        }
        let settlement = payment
            .amount_dkk
            .map(abs)
            .ok_or_else(|| fail("foreign bank payment requires amount_dkk"))?;
        let rate = payment
            .rate
            .cloned()
            .ok_or_else(|| fail("foreign bank payment requires positive imported FX rate"))?;
        (settlement, rate, "imported_bank")
    };
    if settlement == Money::default() || rate.apply(gross) != settlement {
        return Err(fail(
            "six-decimal FX rate cannot reconstruct documented DKK settlement",
        ));
    }
    Ok(Basis {
        gross_dkk: settlement,
        rate,
        rate_source: Some(source),
    })
}
/// Allocate conversion rounding to the largest exempt base, otherwise largest base.
/// This keeps every debit nonnegative and the mixed base + VAT equal to settlement.
pub fn scale_net(
    lines: &[VatLine],
    rate: &FxRate,
    target_net: &Money,
) -> Result<Vec<Money>, Error> {
    if lines.is_empty() {
        return Err(fail("purchase split is empty"));
    }
    let mut scaled = lines
        .iter()
        .map(|line| money(&line.net_amount).map(|net| rate.apply(&net)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut total = Money::default();
    for amount in &scaled {
        total.add(amount);
    }
    let residual = target_net.subtract(&total);
    if residual != Money::default() {
        let exempt: Vec<_> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.classification == "exempt")
            .map(|(i, _)| i)
            .collect();
        let candidates = if exempt.is_empty() {
            (0..lines.len()).collect()
        } else {
            exempt
        };
        let mut largest = candidates[0];
        for i in candidates {
            if scaled[largest].subtract(&scaled[i]).is_negative() {
                largest = i;
            }
        }
        scaled[largest].add(&residual);
        if scaled[largest].is_negative() {
            return Err(fail("FX split residual would make a base negative"));
        }
    }
    Ok(scaled)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn m(s: &str) -> Money {
        Money::parse(s).unwrap()
    }
    #[test]
    fn imported_and_derived_settlements_must_reconstruct() {
        let basis = resolve(
            "EUR",
            &m("100"),
            Payment {
                currency: "DKK",
                amount: &m("-746.25"),
                amount_dkk: None,
                rate: None,
            },
        )
        .unwrap();
        assert_eq!(basis.rate.decimal(), "7.462500");
        assert_eq!(basis.rate_source, Some("derived_dkk_settlement"));
        let rate = FxRate::parse("7.46").unwrap();
        assert_eq!(
            resolve(
                "EUR",
                &m("100"),
                Payment {
                    currency: "EUR",
                    amount: &m("-100"),
                    amount_dkk: Some(&m("-746")),
                    rate: Some(&rate)
                }
            )
            .unwrap()
            .gross_dkk,
            m("746")
        );
        for (foreign, paid) in [("30000", "-0.02"), ("100", "-745")] {
            let rate = if foreign == "100" { Some(&rate) } else { None };
            assert!(
                resolve(
                    "EUR",
                    &m(foreign),
                    Payment {
                        currency: "DKK",
                        amount: &m(paid),
                        amount_dkk: None,
                        rate
                    }
                )
                .is_err()
            );
        }
        assert!(
            resolve(
                "EUR",
                &m("100"),
                Payment {
                    currency: "USD",
                    amount: &m("-100"),
                    amount_dkk: Some(&m("-746")),
                    rate: Some(&rate)
                }
            )
            .is_err()
        );
    }
    #[test]
    fn mixed_rounding_residual_is_reconciled_to_exempt_base() {
        let lines=serde_json::from_value::<Vec<VatLine>>(serde_json::json!([{"classification":"dk_purchase_25","netAmount":0.01,"vatAmount":0},{"classification":"exempt","netAmount":0.01,"vatAmount":0}])).unwrap();
        let scaled = scale_net(&lines, &FxRate::parse("1.5").unwrap(), &m("0.03")).unwrap();
        assert_eq!(scaled, vec![m("0.02"), m("0.01")]);
    }
}
