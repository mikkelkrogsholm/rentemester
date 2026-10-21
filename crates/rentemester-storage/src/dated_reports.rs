//! Original flat numeric financial reports over the shared exact native reducers.
use crate::invalid;
use rentemester_engine::{
    Error,
    calendar::valid_iso_date,
    financial_statements::{self, Line, Section},
    money::Money,
    trial_balance::{Period, Report},
};
use rusqlite::Transaction;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Kind {
    TrialBalance,
    ProfitLoss,
    Balance,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub kind: Kind,
    pub from: Option<String>,
    pub to: Option<String>,
    pub as_of: Option<String>,
}
impl Input {
    pub fn validate_shape(&self) -> Result<(), Error> {
        let present = match self.kind {
            Kind::Balance => self.as_of.is_some() && self.from.is_none() && self.to.is_none(),
            _ => self.from.is_some() && self.to.is_some() && self.as_of.is_none(),
        };
        if !present
            || [&self.from, &self.to, &self.as_of]
                .into_iter()
                .flatten()
                .any(|s| s.is_empty() || s.len() > 128 || s.chars().any(char::is_control))
        {
            return Err(Error::new(
                "invalid_dated_report_input",
                "exact bounded report date selectors required",
            ));
        }
        Ok(())
    }
}
fn rule(kind: Kind) -> &'static str {
    match kind {
        Kind::TrialBalance => "financial-statements:trial-balance",
        Kind::ProfitLoss => "financial-statements:profit-loss",
        Kind::Balance => "financial-statements:balance-sheet",
    }
}
/// Legacy numbers are permitted only when their JSON number round-trips to
/// the exact cent amount. Versioned exact-string report wires are unaffected.
fn number(money: &Money) -> Result<Value, Error> {
    let n: f64 = money.decimal().parse().map_err(|_| precision())?;
    if !n.is_finite() {
        return Err(precision());
    }
    let mut buffer = ryu_js::Buffer::new();
    let raw = buffer.format_finite(n);
    if Money::parse(raw).map_err(|_| precision())? != *money {
        return Err(precision());
    }
    serde_json::from_str(raw).map_err(|_| precision())
}
fn precision() -> Error {
    Error::new(
        "dated_report_precision_refused",
        "exact report amount cannot be represented by legacy numeric JSON",
    )
}
fn empty(input: &Input, errors: Vec<&str>) -> Value {
    match input.kind {
        Kind::TrialBalance => {
            json!({"ok":false,"appliedRules":[rule(input.kind)],"periodStart":input.from,"periodEnd":input.to,"accounts":[],"totalDebit":0,"totalCredit":0,"balanced":false,"linesConsidered":0,"journalEntryCount":0,"errors":errors})
        }
        Kind::ProfitLoss => {
            json!({"ok":false,"appliedRules":[rule(input.kind)],"periodStart":input.from,"periodEnd":input.to,"income":[],"expense":[],"totalIncome":0,"totalExpense":0,"result":0,"errors":errors})
        }
        Kind::Balance => {
            json!({"ok":false,"appliedRules":[rule(input.kind)],"asOfDate":input.as_of,"assets":{"lines":[],"total":0},"liabilities":{"lines":[],"total":0},"equity":{"lines":[],"total":0},"periodResult":0,"totalAssets":0,"totalLiabilitiesAndEquity":0,"balanced":false,"errors":errors})
        }
    }
}
fn lines(lines: &[Line], report: &Report) -> Result<Vec<Value>, Error> {
    let kinds: BTreeMap<_, _> = report
        .accounts
        .iter()
        .map(|a| (a.account_no.as_str(), a.kind.as_str()))
        .collect();
    lines.iter().map(|line| Ok(json!({"accountNo":line.account_no,"name":line.name,"type":kinds.get(line.account_no.as_str()).ok_or_else(||invalid("report line has no source account"))?,"amount":number(&line.amount)?}))).collect()
}
fn section(section: &Section, report: &Report) -> Result<Value, Error> {
    Ok(json!({"lines":lines(&section.lines,report)?,"total":number(&section.total)?}))
}

pub fn read(tx: &Transaction<'_>, input: &Input) -> Result<Value, Error> {
    input.validate_shape()?;
    crate::journal_preview::require_current(tx)?;
    if !crate::journal_hash::verify(tx)?.ok {
        return Err(Error::new(
            "dated_report_source_unavailable",
            "journal hash integrity differs",
        ));
    }
    let mut errors = vec![];
    let (start, end) = match input.kind {
        Kind::Balance => {
            let date = input.as_of.as_deref().unwrap();
            if !valid_iso_date(date) {
                errors.push("asOfDate must be YYYY-MM-DD");
            }
            ("0000-01-01", date)
        }
        _ => {
            let from = input.from.as_deref().unwrap();
            let to = input.to.as_deref().unwrap();
            if !valid_iso_date(from) {
                errors.push("periodStart must be YYYY-MM-DD");
            }
            if !valid_iso_date(to) {
                errors.push("periodEnd must be YYYY-MM-DD");
            }
            if errors.is_empty() && from > to {
                errors.push("periodStart must be before or equal to periodEnd");
            }
            (from, to)
        }
    };
    if !errors.is_empty() {
        return Ok(empty(input, errors));
    }
    match input.kind {
        Kind::Balance => {
            crate::opening_balance::require_report_as_of(tx, end)?;
        }
        _ => {
            crate::opening_balance::require_report_range(tx, start, end)?;
        }
    }
    let report = crate::trial_balance_owned(tx, Period::new(start, end)?)?;
    match input.kind {
        Kind::TrialBalance => {
            let accounts=report.accounts.iter().map(|a| Ok(json!({"accountNo":a.account_no,"name":a.name,"type":a.kind,"normalBalance":a.normal_balance,"debit":number(&a.debit)?,"credit":number(&a.credit)?,"balance":number(&a.balance)?}))).collect::<Result<Vec<_>,Error>>()?;
            Ok(
                json!({"ok":true,"appliedRules":[rule(input.kind)],"periodStart":start,"periodEnd":end,"accounts":accounts,"totalDebit":number(&report.total_debit)?,"totalCredit":number(&report.total_credit)?,"balanced":report.balanced,"linesConsidered":report.lines_considered,"journalEntryCount":report.journal_entry_count,"errors":[]}),
            )
        }
        Kind::ProfitLoss => {
            let pnl = financial_statements::profit_and_loss(&report);
            Ok(
                json!({"ok":true,"appliedRules":[rule(input.kind)],"periodStart":start,"periodEnd":end,"income":lines(&pnl.income.lines,&report)?,"expense":lines(&pnl.expense.lines,&report)?,"totalIncome":number(&pnl.income.total)?,"totalExpense":number(&pnl.expense.total)?,"result":number(&pnl.result)?,"errors":[]}),
            )
        }
        Kind::Balance => {
            let mut sheet = financial_statements::balance_sheet(&report);
            // The shared reducer appends its synthetic result as the final equity
            // line. Original CLI presents that amount separately, without any
            // name-based guessing or change to the existing web presentation.
            let appended = sheet
                .equity
                .lines
                .pop()
                .ok_or_else(|| invalid("shared balance result line missing"))?;
            if appended.amount != sheet.period_result {
                return Err(invalid("shared balance result presentation differs"));
            }
            sheet.equity.total = sheet.equity.total.subtract(&sheet.period_result);
            Ok(
                json!({"ok":true,"appliedRules":[rule(input.kind)],"asOfDate":end,"assets":section(&sheet.assets,&report)?,"liabilities":section(&sheet.liabilities,&report)?,"equity":section(&sheet.equity,&report)?,"periodResult":number(&sheet.period_result)?,"totalAssets":number(&sheet.assets.total)?,"totalLiabilitiesAndEquity":number(&sheet.total_liabilities_and_equity)?,"balanced":sheet.balanced,"errors":[]}),
            )
        }
    }
}
#[cfg(test)]
mod tests;
