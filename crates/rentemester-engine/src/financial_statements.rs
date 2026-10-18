//! Exact derived financial statements over the shared trial balance. No file or SQL access.
use crate::{money::Money, trial_balance::Report};

#[derive(Clone)]
pub struct Line {
    pub account_no: String,
    pub name: String,
    pub amount: Money,
}
#[derive(Default)]
pub struct Section {
    pub lines: Vec<Line>,
    pub total: Money,
}
pub struct ProfitAndLoss {
    pub income: Section,
    pub expense: Section,
    pub result: Money,
}
pub struct BalanceSheet {
    pub assets: Section,
    pub liabilities: Section,
    pub equity: Section,
    pub period_result: Money,
    pub total_liabilities_and_equity: Money,
    pub balanced: bool,
}
fn negative(value: &Money) -> Money {
    Money::default().subtract(value)
}
impl Section {
    fn push(&mut self, no: &str, name: &str, amount: Money) {
        self.total.add(&amount);
        self.lines.push(Line {
            account_no: no.into(),
            name: name.into(),
            amount,
        });
    }
}
pub fn profit_and_loss(report: &Report) -> ProfitAndLoss {
    let mut income = Section::default();
    let mut expense = Section::default();
    for account in &report.accounts {
        match account.kind.as_str() {
            "income" => income.push(
                &account.account_no,
                &account.name,
                negative(&account.balance),
            ),
            "expense" => expense.push(&account.account_no, &account.name, account.balance.clone()),
            _ => {}
        }
    }
    let result = income.total.subtract(&expense.total);
    ProfitAndLoss {
        income,
        expense,
        result,
    }
}
/// Input must cover all journal history through the requested balance date.
pub fn balance_sheet(cumulative: &Report) -> BalanceSheet {
    let mut assets = Section::default();
    let mut liabilities = Section::default();
    let mut equity = Section::default();
    let pnl = profit_and_loss(cumulative);
    for account in &cumulative.accounts {
        let section = match account.kind.as_str() {
            "vat" if account.normal_balance == "debit" => "asset",
            "vat" => "liability",
            kind => kind,
        };
        match section {
            "asset" => assets.push(&account.account_no, &account.name, account.balance.clone()),
            "liability" => liabilities.push(
                &account.account_no,
                &account.name,
                negative(&account.balance),
            ),
            "equity" => equity.push(
                &account.account_no,
                &account.name,
                negative(&account.balance),
            ),
            _ => {}
        }
    }
    // As in the existing live report: unclosed cumulative results remain visible in equity.
    equity.push("—", "Årets resultat", pnl.result.clone());
    let mut total_liabilities_and_equity = liabilities.total.clone();
    total_liabilities_and_equity.add(&equity.total);
    let balanced = assets.total == total_liabilities_and_equity;
    BalanceSheet {
        assets,
        liabilities,
        equity,
        period_result: pnl.result,
        total_liabilities_and_equity,
        balanced,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::trial_balance::{self, Line as Posting, Period};
    #[test]
    fn revenue_expenses_vat_and_equity_use_exact_signs_and_balance() {
        let rows = [
            ("1", "asset", "debit", "88.00", "0.00"),
            ("2", "vat", "debit", "8.00", "0.00"),
            ("3", "expense", "debit", "40.00", "0.00"),
            ("4", "income", "credit", "0.00", "100.00"),
            ("5", "vat", "credit", "0.00", "20.00"),
            ("6", "equity", "credit", "0.00", "16.00"),
        ];
        let tb = trial_balance::build(
            Period::new("2026-01-01", "2026-12-31").unwrap(),
            rows.into_iter()
                .map(|(no, kind, normal, debit, credit)| Posting {
                    account_no: no.into(),
                    name: format!("Synthetic {no}"),
                    kind: kind.into(),
                    normal_balance: normal.into(),
                    entry_id: 1,
                    debit: Money::parse_canonical(debit).unwrap(),
                    credit: Money::parse_canonical(credit).unwrap(),
                }),
        );
        assert!(tb.balanced);
        let pnl = profit_and_loss(&tb);
        assert_eq!(pnl.income.total.decimal(), "100.00");
        assert_eq!(pnl.expense.total.decimal(), "40.00");
        assert_eq!(pnl.result.decimal(), "60.00");
        let sheet = balance_sheet(&tb);
        assert!(sheet.balanced);
        assert_eq!(sheet.assets.total.decimal(), "96.00");
        assert_eq!(sheet.liabilities.total.decimal(), "20.00");
        assert_eq!(sheet.equity.total.decimal(), "76.00");
        assert_eq!(sheet.total_liabilities_and_equity.decimal(), "96.00");
    }
}
