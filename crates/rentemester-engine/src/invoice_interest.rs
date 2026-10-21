//! Pure DKK simple-interest mathematics. Inputs must be authenticated by storage;
//! a mathematical correction ceiling does not establish ledger/account capacity.
use crate::{Error, fiscal_year::Date, money::Money};
use num_bigint::BigInt;
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RateSource {
    StatutoryTable,
    ManualOverride,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventKind {
    Payment,
    Credit,
    Refund,
    WriteOff,
}
#[derive(Clone, Debug)]
pub struct PrincipalEvent {
    pub effective_date: Date,
    pub kind: EventKind,
    pub amount: Money,
    pub source_id: i64,
}
#[derive(Clone, Debug)]
pub struct InterestClaim {
    pub id: i64,
    pub claim_date: Date,
    pub reference_basis_points: u32,
    pub annual_basis_points: u32,
    pub rate_source: RateSource,
    pub amount: Money,
    pub posted_journal_id: Option<i64>,
}
#[derive(Clone, Debug)]
pub struct InterestCorrection {
    pub id: i64,
    pub correction_date: Date,
    pub amount: Money,
    pub journal_id: i64,
}
#[derive(Clone, Debug)]
pub struct History {
    pub currency: String,
    pub gross: Money,
    pub effective_due_date: Date,
    pub events: Vec<PrincipalEvent>,
    pub claims: Vec<InterestClaim>,
    pub corrections: Vec<InterestCorrection>,
}
#[derive(Clone, Debug)]
pub struct Calculate {
    pub as_of: Date,
    pub reference_basis_points: Option<u32>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Calculation {
    pub as_of_date: Date,
    pub effective_due_date: Date,
    pub overdue_days: u32,
    pub interest_from_date: Date,
    pub claimable_days: u32,
    pub principal_open_balance: Money,
    pub reference_rate_percent: Money,
    pub annual_interest_rate_percent: Money,
    pub reference_rate_source: RateSource,
    pub accrued_interest_amount: Money,
    pub prior_claimed_interest: Money,
    pub total_interest_to_date: Money,
    pub over_claimed_interest: Money,
    pub warnings: Vec<String>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimOverclaim {
    pub claim_id: i64,
    pub claim_date: Date,
    pub journal_entry_id: i64,
    pub over_claimed_amount: Money,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrectionProposal {
    pub has_proposal: bool,
    pub posted_interest: Money,
    pub lawful_interest: Money,
    pub already_corrected: Money,
    pub over_claimed_amount: Money,
    pub through_date: Option<Date>,
    pub claim_overclaims: Vec<ClaimOverclaim>,
}
fn refused(message: &str) -> Error {
    Error::new("interest_refused", message)
}
fn positive(m: &Money) -> bool {
    m.0 > BigInt::from(0)
}
fn nonnegative(m: Money) -> Money {
    if m.is_negative() { Money::default() } else { m }
}
fn day(d: Date) -> i64 {
    let s = d.to_string();
    let y = s[..4].parse::<i32>().unwrap();
    let m = time::Month::try_from(s[5..7].parse::<u8>().unwrap()).unwrap();
    i64::from(
        time::Date::from_calendar_date(y, m, s[8..].parse().unwrap())
            .unwrap()
            .to_julian_day(),
    )
}
fn days(a: Date, b: Date) -> u32 {
    (day(b) - day(a)).max(0) as u32
}
// Existing source table, deliberately no network/default currency conversion.
const TABLE: &[(&str, u32)] = &[
    ("2023-01-01", 190),
    ("2023-07-01", 325),
    ("2024-01-01", 375),
    ("2024-07-01", 350),
    ("2025-01-01", 275),
    ("2025-07-01", 175),
    ("2026-01-01", 175),
];
pub fn statutory_reference_basis_points(d: Date) -> Option<u32> {
    TABLE
        .iter()
        .rev()
        .find(|(date, _)| *date <= d.to_string().as_str())
        .map(|(_, rate)| *rate)
}
fn annual(reference: u32) -> Result<u32, Error> {
    reference
        .checked_add(800)
        .ok_or_else(|| refused("annual rate overflows"))
}
fn validate(h: &History) -> Result<Vec<&InterestClaim>, Error> {
    if h.currency != "DKK" {
        return Err(Error::new(
            "unsupported_foreign_interest",
            "foreign-interest currency/conversion contract is not implemented",
        ));
    }
    if !positive(&h.gross) {
        return Err(refused("positive invoice principal required"));
    }
    let mut events = BTreeSet::new();
    for e in &h.events {
        if e.source_id <= 0 || !positive(&e.amount) || !events.insert((e.kind, e.source_id)) {
            return Err(refused("invalid or duplicate principal event"));
        }
    }
    let mut ids = BTreeSet::new();
    let mut journals = BTreeSet::new();
    for c in &h.claims {
        if c.id <= 0
            || !ids.insert(c.id)
            || !positive(&c.amount)
            || c.claim_date <= h.effective_due_date
            || annual(c.reference_basis_points)? != c.annual_basis_points
        {
            return Err(refused("invalid interest claim"));
        }
        if c.rate_source == RateSource::StatutoryTable
            && statutory_reference_basis_points(c.claim_date) != Some(c.reference_basis_points)
        {
            return Err(refused("claim table rate differs from source table"));
        }
        if let Some(j) = c.posted_journal_id {
            if j <= 0 || !journals.insert(j) {
                return Err(refused("invalid or reused claim journal"));
            }
        }
    }
    let mut correction_ids = BTreeSet::new();
    for c in &h.corrections {
        if c.id <= 0
            || !correction_ids.insert(c.id)
            || c.journal_id <= 0
            || !journals.insert(c.journal_id)
            || !positive(&c.amount)
        {
            return Err(refused("invalid or reused correction"));
        }
    }
    let mut claims = h.claims.iter().collect::<Vec<_>>();
    claims.sort_by_key(|c| (c.claim_date, c.id));
    Ok(claims)
}
fn balance(h: &History, d: Date) -> Money {
    let mut result = h.gross.clone();
    for e in &h.events {
        if e.effective_date <= d {
            if e.kind == EventKind::Refund {
                result.add(&e.amount);
            } else {
                result = result.subtract(&e.amount);
            }
        }
    }
    result
}
/// Shared source-aware window builder for calculation AND correction. All
/// boundaries are half-open; events take effect at the start of their day.
fn numerator(
    h: &History,
    from: Date,
    to: Date,
    source: RateSource,
    rate: u32,
) -> Result<BigInt, Error> {
    if to <= from {
        return Ok(BigInt::from(0));
    }
    let mut points = BTreeSet::from([from, to]);
    for e in &h.events {
        if e.effective_date > from && e.effective_date < to {
            points.insert(e.effective_date);
        }
    }
    if source == RateSource::StatutoryTable {
        for (date, _) in TABLE {
            let d = Date::parse(date)?;
            if d > from && d < to {
                points.insert(d);
            }
        }
    }
    let points = points.into_iter().collect::<Vec<_>>();
    let mut n = BigInt::from(0);
    for pair in points.windows(2) {
        let principal = balance(h, pair[0]);
        let applied = if source == RateSource::StatutoryTable {
            annual(statutory_reference_basis_points(pair[0]).ok_or_else(|| {
                refused("no statutory rate for interest window; explicit manual rate required")
            })?)?
        } else {
            rate
        };
        if positive(&principal) {
            n += principal.0 * BigInt::from(applied) * BigInt::from(days(pair[0], pair[1]));
        }
    }
    Ok(n)
}
fn rounded(n: &BigInt) -> Result<Money, Error> {
    Ok(Money(crate::decimal::round_div(
        n,
        &BigInt::from(3_650_000u32),
    )?))
}
fn percent(bp: u32) -> Money {
    Money(BigInt::from(bp))
}
pub fn calculate(h: &History, input: &Calculate) -> Result<Calculation, Error> {
    let claims = validate(h)?;
    let table = statutory_reference_basis_points(input.as_of);
    let (reference, source) = match input.reference_basis_points {
        Some(v) => (v, RateSource::ManualOverride),
        None => (
            table.ok_or_else(|| {
                refused("no statutory reference rate; explicit manual rate required")
            })?,
            RateSource::StatutoryTable,
        ),
    };
    let rate = annual(reference)?;
    let mut warnings = vec![];
    if source == RateSource::ManualOverride && table.is_some_and(|v| v.abs_diff(reference) > 25) {
        warnings.push("manual reference rate differs from statutory table by more than 0.25 percentage points".into());
    }
    let anchor = claims.last().map_or(h.effective_due_date, |c| {
        c.claim_date.max(h.effective_due_date)
    });
    let from = anchor.min(input.as_of);
    let principal = balance(h, input.as_of);
    let claimable = if positive(&principal) {
        days(from, input.as_of)
    } else {
        0
    };
    let mut n = BigInt::from(0);
    let mut start = h.effective_due_date;
    let mut prior = Money::default();
    for c in claims {
        let end = c.claim_date.min(input.as_of);
        n += numerator(h, start, end, c.rate_source, c.annual_basis_points)?;
        start = end;
        if c.claim_date <= input.as_of {
            prior.add(&c.amount);
        }
    }
    if claimable > 0 {
        n += numerator(h, anchor, input.as_of, source, rate)?;
    }
    for c in &h.corrections {
        if c.correction_date <= input.as_of {
            prior = prior.subtract(&c.amount);
        }
    }
    let total = rounded(&n)?;
    let increment = if claimable > 0 {
        nonnegative(total.subtract(&prior))
    } else {
        Money::default()
    };
    let over = nonnegative(prior.subtract(&total));
    Ok(Calculation {
        as_of_date: input.as_of,
        effective_due_date: h.effective_due_date,
        overdue_days: if positive(&principal) {
            days(h.effective_due_date, input.as_of)
        } else {
            0
        },
        interest_from_date: from,
        claimable_days: claimable,
        principal_open_balance: principal,
        reference_rate_percent: percent(reference),
        annual_interest_rate_percent: percent(rate),
        reference_rate_source: source,
        accrued_interest_amount: increment,
        prior_claimed_interest: prior,
        total_interest_to_date: total,
        over_claimed_interest: over,
        warnings,
    })
}
pub fn correction(h: &History) -> Result<CorrectionProposal, Error> {
    let claims = validate(h)?;
    let mut start = h.effective_due_date;
    let mut n = BigInt::from(0);
    let mut posted = Money::default();
    let mut through = None;
    let mut candidates = vec![];
    for c in claims {
        if let Some(j) = c.posted_journal_id {
            let claim_n = numerator(h, start, c.claim_date, c.rate_source, c.annual_basis_points)?;
            let excess = nonnegative(c.amount.subtract(&rounded(&claim_n)?));
            n += claim_n;
            posted.add(&c.amount);
            through = Some(c.claim_date);
            if positive(&excess) {
                candidates.push(ClaimOverclaim {
                    claim_id: c.id,
                    claim_date: c.claim_date,
                    journal_entry_id: j,
                    over_claimed_amount: excess,
                });
            }
        }
        start = c.claim_date;
    }
    let lawful = rounded(&n)?;
    let gross = nonnegative(posted.subtract(&lawful));
    let mut remaining = gross.clone();
    let mut causal: Vec<ClaimOverclaim> = vec![];
    for mut c in candidates {
        if !positive(&remaining) {
            break;
        }
        if c.over_claimed_amount.0 > remaining.0 {
            c.over_claimed_amount = remaining.clone();
        }
        remaining = remaining.subtract(&c.over_claimed_amount);
        causal.push(c);
    }
    // Source algorithm assigns cumulative rounding residue to final causal claim.
    if positive(&remaining) {
        if let Some(last) = causal.last_mut() {
            last.over_claimed_amount.add(&remaining);
        } else {
            return Err(refused("global correction has no causal claim"));
        }
    }
    let mut corrected = Money::default();
    for c in &h.corrections {
        corrected.add(&c.amount);
    }
    if corrected.0 > gross.0 {
        return Err(refused("prior corrections exceed lawful aggregate ceiling"));
    }
    let over = gross.subtract(&corrected);
    Ok(CorrectionProposal {
        has_proposal: positive(&over),
        posted_interest: posted,
        lawful_interest: lawful,
        already_corrected: corrected,
        over_claimed_amount: over,
        through_date: through,
        claim_overclaims: causal,
    })
}
#[cfg(test)]
mod tests;
