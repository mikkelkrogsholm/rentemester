use crate::{Error, money::Money};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct Period {
    pub start: String,
    pub end: String,
}
impl Period {
    pub fn new(start: &str, end: &str) -> Result<Self, Error> {
        if !crate::calendar::valid_iso_date(start)
            || !crate::calendar::valid_iso_date(end)
            || start > end
        {
            return Err(Error::new(
                "invalid_input",
                "period must use real canonical YYYY-MM-DD dates in ascending order",
            ));
        }
        Ok(Self {
            start: start.into(),
            end: end.into(),
        })
    }
}
#[derive(Clone, Debug)]
pub struct Line {
    pub account_no: String,
    pub name: String,
    pub kind: String,
    pub normal_balance: String,
    pub entry_id: i64,
    pub debit: Money,
    pub credit: Money,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub account_no: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub normal_balance: String,
    pub debit: Money,
    pub credit: Money,
    pub balance: Money,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub ok: bool,
    pub applied_rules: Vec<&'static str>,
    pub period_start: String,
    pub period_end: String,
    pub accounts: Vec<Account>,
    pub total_debit: Money,
    pub total_credit: Money,
    pub balanced: bool,
    pub lines_considered: usize,
    pub journal_entry_count: usize,
    pub errors: Vec<String>,
}
pub fn build(period: Period, lines: impl IntoIterator<Item = Line>) -> Report {
    let mut accounts: BTreeMap<Vec<u16>, Account> = BTreeMap::new();
    let mut entries = BTreeSet::new();
    let mut total_debit = Money::default();
    let mut total_credit = Money::default();
    let mut count = 0;
    for row in lines {
        count += 1;
        entries.insert(row.entry_id);
        total_debit.add(&row.debit);
        total_credit.add(&row.credit);
        let key = row.account_no.encode_utf16().collect();
        let acc = accounts.entry(key).or_insert_with(|| Account {
            account_no: row.account_no,
            name: row.name,
            kind: row.kind,
            normal_balance: row.normal_balance,
            debit: Money::default(),
            credit: Money::default(),
            balance: Money::default(),
        });
        acc.debit.add(&row.debit);
        acc.credit.add(&row.credit);
    }
    let accounts = accounts
        .into_values()
        .map(|mut a| {
            a.balance = a.debit.subtract(&a.credit);
            a
        })
        .collect();
    Report {
        ok: true,
        applied_rules: vec!["financial-statements:trial-balance"],
        period_start: period.start,
        period_end: period.end,
        accounts,
        balanced: total_debit == total_credit,
        total_debit,
        total_credit,
        lines_considered: count,
        journal_entry_count: entries.len(),
        errors: vec![],
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calendar_boundaries_and_strict_input() {
        for d in ["0000-02-29", "0099-12-31", "2000-02-29", "2026-10-17"] {
            assert!(Period::new(d, d).is_ok());
        }
        for d in [
            "1900-02-29",
            "2026-02-30",
            "2026-00-01",
            "2026-01-00",
            " 2026-01-01",
            "2026-01-01 ",
            "２０２６-01-01",
        ] {
            assert!(Period::new(d, d).is_err());
        }
        assert!(Period::new("2026-02-01", "2026-01-01").is_err());
    }
    #[test]
    fn independently_rounded_lines_and_touched_accounts() {
        let lines = [
            ("A", "1.005", "0"),
            ("A", "1.005", "0"),
            ("B", "0", "2.01"),
            ("A", "0", "0.01"),
        ]
        .into_iter()
        .map(|(no, d, c)| Line {
            account_no: no.into(),
            name: no.into(),
            kind: "asset".into(),
            normal_balance: "debit".into(),
            entry_id: 1,
            debit: Money::parse(d).unwrap(),
            credit: Money::parse(c).unwrap(),
        });
        let r = build(Period::new("2026-01-01", "2026-12-31").unwrap(), lines);
        assert!(r.balanced);
        assert_eq!(r.total_debit.decimal(), "2.02");
        assert_eq!(r.accounts[0].balance.decimal(), "2.01");
        assert_eq!(r.journal_entry_count, 1);
        assert_eq!(r.lines_considered, 4);
        let r = build(Period::new("2026-01-01", "2026-01-01").unwrap(), []);
        assert!(r.balanced);
        assert!(r.accounts.is_empty());
    }
}
