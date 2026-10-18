//! Exact cumulative allocation: repeated partial credits finish on every original cent.
use crate::{Error, money::Money, sales_invoice::refused};
pub struct Counter {
    pub original: Money,
    pub prior: Money,
    pub revenue: bool,
}
pub fn allocate(
    gross: &Money,
    credited: &Money,
    amount: &Money,
    counters: &[Counter],
) -> Result<Vec<Money>, Error> {
    if gross.is_negative()
        || *gross == Money::default()
        || amount.is_negative()
        || *amount == Money::default()
        || credited.is_negative()
    {
        return Err(refused(
            "positive credit amount and original invoice gross required",
        ));
    }
    let mut cumulative = credited.clone();
    cumulative.add(amount);
    if gross.subtract(&cumulative).is_negative() {
        return Err(refused("credit amount exceeds remaining creditable amount"));
    }
    let mut original = Money::default();
    let mut prior = Money::default();
    for c in counters {
        if c.original.is_negative()
            || c.prior.is_negative()
            || c.original.subtract(&c.prior).is_negative()
        {
            return Err(refused("invalid original or prior credit allocation"));
        }
        original.add(&c.original);
        prior.add(&c.prior);
    }
    if original != *gross || prior != *credited {
        return Err(refused(
            "credit allocations do not reconcile to their document totals",
        ));
    }
    let mut targets = Vec::new();
    let mut total = Money::default();
    for c in counters {
        let target = if cumulative == *gross {
            c.original.clone()
        } else {
            Money(crate::decimal::round_div(
                &(&c.original.0 * &cumulative.0),
                &gross.0,
            )?)
        };
        total.add(&target);
        targets.push(target);
    }
    let residual = cumulative.subtract(&total);
    if residual != Money::default() {
        let index = counters
            .iter()
            .enumerate()
            .filter(|(_, c)| c.revenue)
            .max_by(|(a, _), (b, _)| targets[*a].0.cmp(&targets[*b].0))
            .map(|(i, _)| i)
            .ok_or_else(|| refused("revenue base required for rounding allocation"))?;
        targets[index].add(&residual);
    }
    let mut result = Vec::new();
    let mut sum = Money::default();
    for (target, c) in targets.into_iter().zip(counters) {
        let current = target.subtract(&c.prior);
        if current.is_negative() || c.original.subtract(&target).is_negative() {
            return Err(refused(
                "credit would exceed an original account allocation",
            ));
        }
        sum.add(&current);
        result.push(current);
    }
    if sum != *amount {
        return Err(refused("credit journal is not balanced"));
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn m(s: &str) -> Money {
        Money::parse(s).unwrap()
    }
    #[test]
    fn final_partial_credit_restores_every_original_cent() {
        let mut counters = vec![
            Counter {
                original: m("0.04"),
                prior: m("0"),
                revenue: true,
            },
            Counter {
                original: m("0.01"),
                prior: m("0"),
                revenue: false,
            },
        ];
        let mut credited = m("0");
        for amount in ["0.01", "0.01", "0.03"] {
            let amount = m(amount);
            let allocation = allocate(&m("0.05"), &credited, &amount, &counters).unwrap();
            for (c, a) in counters.iter_mut().zip(allocation) {
                c.prior.add(&a);
            }
            credited.add(&amount);
        }
        assert_eq!(counters[0].prior, m("0.04"));
        assert_eq!(counters[1].prior, m("0.01"));
        assert!(allocate(&m("0.05"), &credited, &m("0.01"), &counters).is_err());
    }
}
