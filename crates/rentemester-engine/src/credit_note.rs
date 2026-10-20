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
        // A later rounded proportion must never retract an already credited
        // source counter. Keep each target inside its remaining original range.
        let target = Money(target.0.max(c.prior.0.clone()).min(c.original.0.clone()));
        total.add(&target);
        targets.push(target);
    }
    let residual = cumulative.subtract(&total);
    if residual != Money::default() {
        if !counters.iter().any(|counter| counter.revenue) {
            return Err(refused("revenue base required for rounding allocation"));
        }
        let decreasing = residual.is_negative();
        let mut remaining = if decreasing {
            Money(-residual.0)
        } else {
            residual
        };
        let mut order: Vec<_> = (0..counters.len()).collect();
        // Preserve the existing preference for the largest revenue counter,
        // but use another counter when that preference would retract prior
        // credit or exceed the original. This is bounded by the line count.
        order.sort_by(|a, b| {
            counters[*b]
                .revenue
                .cmp(&counters[*a].revenue)
                .then_with(|| targets[*b].0.cmp(&targets[*a].0))
                .then_with(|| b.cmp(a))
        });
        for index in order {
            let capacity = if decreasing {
                targets[index].subtract(&counters[index].prior)
            } else {
                counters[index].original.subtract(&targets[index])
            };
            let change = Money(capacity.0.min(remaining.0.clone()));
            if decreasing {
                targets[index] = targets[index].subtract(&change);
            } else {
                targets[index].add(&change);
            }
            remaining = remaining.subtract(&change);
            if remaining == Money::default() {
                break;
            }
        }
        if remaining != Money::default() {
            return Err(refused(
                "credit rounding exceeds remaining original counters",
            ));
        }
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
    #[test]
    fn repeated_one_cent_credits_never_retract_a_prior_line_allocation() {
        let mut counters = vec![
            Counter {
                original: m("0.02"),
                prior: m("0"),
                revenue: true,
            },
            Counter {
                original: m("0.02"),
                prior: m("0"),
                revenue: true,
            },
            Counter {
                original: m("0.02"),
                prior: m("0"),
                revenue: false,
            },
        ];
        let mut credited = m("0");
        for _ in 0..6 {
            let allocations = allocate(&m("0.06"), &credited, &m("0.01"), &counters).unwrap();
            for (counter, allocation) in counters.iter_mut().zip(allocations) {
                assert!(!allocation.is_negative());
                counter.prior.add(&allocation);
                assert!(!counter.original.subtract(&counter.prior).is_negative());
            }
            credited.add(&m("0.01"));
        }
        assert!(
            counters
                .iter()
                .all(|counter| counter.prior == counter.original)
        );
    }
    #[test]
    fn small_counter_partitions_conserve_all_cents_through_repeated_credits() {
        for first in 1..=6 {
            for second in 1..=6 {
                for vat in 0..=5 {
                    let gross = Money((first + second + vat).into());
                    let mut counters: Vec<_> = [first, second, vat]
                        .into_iter()
                        .enumerate()
                        .map(|(index, cents)| Counter {
                            original: Money(cents.into()),
                            prior: m("0"),
                            revenue: index < 2,
                        })
                        .collect();
                    let mut credited = m("0");
                    for _ in 0..first + second + vat {
                        let allocations =
                            allocate(&gross, &credited, &m("0.01"), &counters).unwrap();
                        let mut sum = m("0");
                        for (counter, allocation) in counters.iter_mut().zip(allocations) {
                            assert!(!allocation.is_negative());
                            sum.add(&allocation);
                            counter.prior.add(&allocation);
                            assert!(!counter.original.subtract(&counter.prior).is_negative());
                        }
                        assert_eq!(sum, m("0.01"));
                        credited.add(&sum);
                    }
                    assert!(
                        counters
                            .iter()
                            .all(|counter| counter.original == counter.prior)
                    );
                }
            }
        }
    }
}
